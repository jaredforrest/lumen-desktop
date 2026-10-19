"""Identity-based Sway workspace/window operations; no title-based commands."""
import json
import subprocess
import sys
from pathlib import Path
import os
sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "runtime"))
from spaces import SpacePolicy
from state import runtime_epoch

def policy():
    return SpacePolicy(Path(os.environ.get("XDG_STATE_HOME", Path.home() / ".local/state")) / "lumen-desktop/spaces.json")


def run(*args):
    return json.loads(subprocess.check_output(['swaymsg','-r',*args],timeout=4))


def snapshot(*, include_overview=False):
    windows=[]
    def walk(node,workspace='',output=''):
        if node.get('type')=='output':output=node['name']
        if node.get('type')=='workspace':workspace=node['name']
        if node.get('pid') and node.get('type') in ('con','floating_con'):
            app=node.get('app_id') or node.get('window_properties',{}).get('class','Application')
            title=node.get('name') or app
            group='Files' if app=='org.quickshell' and title.startswith('Files') else title if app=='org.quickshell' else app
            if include_overview or not (app=='org.quickshell' and node.get('name')=='Window Overview'):
                windows.append(dict(id=node['id'],pid=node['pid'],app=app,application=SpacePolicy.application(node),group=group,title=title,workspace=workspace,output=output,focused=node.get('focused',False),fullscreen=node.get('fullscreen_mode',0),paired=bool((node.get('lumen_pair') or {}).get('id') or (node.get('lumen_pair') or {}).get('survivor')),pairRestore=(node.get('lumen_pair') or {}).get('restore_workspace'),restores=(node.get('lumen_state') or {}).get('restore_workspaces',[(node.get('lumen_state') or {}).get('restore_workspace')])))
        for child in node.get('nodes',[])+node.get('floating_nodes',[]):walk(child,workspace,output)
    walk(run('-t','get_tree'))
    return dict(windows=windows,workspaces=run('-t','get_workspaces'),outputs=[dict(name=o['name'],label=(o['model'] if o.get('model') not in (None,'','Unknown') else 'Display '+str(i+1))) for i,o in enumerate(run('-t','get_outputs')) if o.get('active')])


def checked(command):
    result=run(command)
    if not result or not all(item.get('success') for item in result):
        raise ValueError('The window manager rejected this action')


def endpoint():
    path=os.environ.get('SWAYSOCK')
    if not path:raise ValueError('No compositor endpoint')
    return runtime_epoch(path)


def check_epoch(data, epoch):
    if data.get('sessionEpoch') != epoch or endpoint() != epoch:
        raise ValueError('The desktop session changed; refresh and try again')


def activate_desktop(model, desktop):
    # Names and connector identities come from the captured logical target.
    checked('focus output '+json.dumps(desktop['output'])+'; workspace '+json.dumps(desktop['name'])+'; move workspace to output '+json.dumps(desktop['output']))


def remove_desktop(model, data, epoch):
    model.require_revision(data.get('revision'));check_epoch(data,epoch)
    if run('-t','get_version').get('desktop_rehome') is not True:
        raise ValueError('This compositor cannot safely migrate all saved window placements; restart into the updated compositor')
    source=model.desktop(data.get('source'));target=model.desktop(data.get('destination'))
    if source['logicalId']==target['logicalId'] or source['outputId']!=target['outputId']:
        raise ValueError('Choose a different surviving desktop on the same display')
    captured=snapshot(include_overview=True)
    occupants=[window for window in captured['windows'] if window['workspace']==source['name'] or source['name'] in window['restores']]
    if any(window['paired'] and window.get('pairRestore') in (None,source['name']) for window in captured['windows']):
        raise ValueError('Leave paired layout before removing its saved desktop')
    if any(source['name'] in window['restores'] for window in occupants) and run('-t','get_version').get('desktop_rehome') is not True:
        raise ValueError('This compositor cannot safely migrate saved window placements')
    if any(window['fullscreen'] or window['paired'] for window in occupants):
        raise ValueError('Leave full screen or paired layout before removing this desktop')
    for window in occupants:
        check_epoch(data,epoch)
        fresh=snapshot(include_overview=True)
        current=next((item for item in fresh['windows'] if item['id']==window['id'] and item['pid']==window['pid']),None)
        if current is None:continue
        belongs=current['workspace']==source['name']
        saved=source['name'] in current['restores']
        if not belongs and not saved:continue
        outputs=run('-t','get_outputs')
        if not any(SpacePolicy.display(output)==target['outputId'] and output.get('active') for output in outputs):
            raise ValueError('The destination display disconnected; desktop retained')
        if saved:
            checked('[con_id='+str(window['id'])+'] lumen_window rehome '+json.dumps(source['name'])+' '+json.dumps(target['name'])+' '+json.dumps(target['output']))
        if belongs:
            checked('[con_id='+str(window['id'])+'] move container to workspace '+json.dumps(target['name']))
            checked('[con_id='+str(window['id'])+'] move workspace to output '+json.dumps(target['output']))
    fresh=snapshot(include_overview=True)
    if any(window['workspace']==source['name'] or source['name'] in window['restores'] for window in fresh['windows']):
        raise ValueError('The desktop changed during removal; desktop retained and moved windows preserved')
    focused=next((workspace for workspace in fresh['workspaces'] if workspace.get('focused')),None)
    visible_source=next((workspace for workspace in fresh['workspaces'] if workspace['name']==source['name'] and (workspace.get('visible') or workspace.get('focused'))),None)
    if visible_source:
        # Every connected output keeps one visible native desktop, even empty.
        # Select the survivor on that output, then restore another output's focus.
        activate_desktop(model,target)
        if focused and focused['name']!=source['name']:
            remaining=run('-t','get_workspaces')
            if any(workspace['id']==focused['id'] and workspace['name']==focused['name'] for workspace in remaining):
                checked('workspace '+json.dumps(focused['name']))
    check_epoch(data,epoch)
    final=snapshot(include_overview=True)
    if any(window['workspace']==source['name'] or source['name'] in window['restores']
            or (window['paired'] and window.get('pairRestore') in (None,source['name']))
            for window in final['windows']):
        raise ValueError('The desktop changed during removal; desktop retained and moved windows preserved')
    if any(workspace['name']==source['name'] for workspace in final['workspaces']):
        raise ValueError('The compositor still contains this desktop; desktop retained')
    model.reconcile(final['workspaces'],run('-t','get_outputs'))
    model.remove(source['logicalId'],target['logicalId'],model.state['revision'])
    return dict(ok=True,moved=[window['id'] for window in occupants],removed=source['logicalId'])


def handle(data):
    if not isinstance(data,dict):raise ValueError('Expected an action object')
    epoch=endpoint();state=snapshot();action=data.get('action','status')
    model=policy()
    with model.transaction():
        model.reconcile(state['workspaces'],run('-t','get_outputs'))
        native=state['workspaces'];state['workspaces']=model.catalog()
        labels={item['name']:item['label'] for item in state['workspaces']}
        for window in state['windows']:window['workspaceLabel']=labels.get(window['workspace'],window['workspace'])
        for output in state['outputs']:
            output['logicalId']=next((key for key,name in model.outputs.items() if name==output['name']),None)
        state.update(autoReorder=model.state['auto_reorder'],auto_reorder=model.state['auto_reorder'],
            assignments=model.state['assignments'],revision=model.state['revision'],sessionEpoch=epoch,
            capabilities=dict(desktop_lifecycle=True,desktop_removal=run('-t','get_version').get('desktop_rehome') is True,all_desktops=False))
        if action=='status':return state
        if action in ('desktop-create','desktop-reorder','desktop-remove'):
            model.require_revision(data.get('revision'));check_epoch(data,epoch)
            if action=='desktop-create':return dict(ok=True,created=model.create(data.get('outputId'),data['revision']))
            if action=='desktop-reorder':
                model.reorder(data.get('outputId'),data.get('order'),data['revision']);return dict(ok=True)
            return remove_desktop(model,data,epoch)
        if action=='ordering':
            model.configure(auto_reorder=data['enabled']);return dict(ok=True)
        if action=='assign':
            window=next((w for w in state['windows'] if w['id']==data.get('id') and w['pid']==data.get('pid')),None)
            if not window:raise ValueError('This window is no longer available')
            model.assign(window['application'],data['kind'],data.get('target'));return dict(ok=True)
        if action=='workspace':
            if data.get('logicalId'):
                check_epoch(data,epoch);desktop=model.desktop(data['logicalId'])
            else:
                desktop=next((item for item in model.catalog() if item['name']==str(data.get('workspace'))),None)
                if desktop is None:raise ValueError('This workspace no longer exists')
            activate_desktop(model,desktop);return dict(ok=True)
        window=next((w for w in state['windows'] if w['id']==data.get('id') and w['pid']==data.get('pid')),None)
        if not window:raise ValueError('This window is no longer available')
        prefix='[con_id='+str(window['id'])+'] '
        if action=='focus':
            checked(prefix+('scratchpad show, ' if window['workspace']=='__i3_scratch' else '')+'focus')
        elif action=='move':
            if data.get('logicalId'):
                check_epoch(data,epoch);desktop=model.desktop(data['logicalId'])
                checked(prefix+'move container to workspace '+json.dumps(desktop['name']))
                checked(prefix+'move workspace to output '+json.dumps(desktop['output']))
            else:
                workspace=str(data['workspace'])
                if workspace not in {w['name'] for w in native} and workspace not in [str(i) for i in range(1,10)]:
                    raise ValueError('Choose an existing desktop')
                checked(prefix+'move container to workspace '+json.dumps(workspace))
        elif action=='output':
            output=data['output']
            if output not in {o['name'] for o in state['outputs']}:raise ValueError('Display is no longer connected')
            checked(prefix+'move container to output '+json.dumps(output))
        else:raise ValueError('Unknown window action')
    return dict(ok=True)


if __name__=='__main__':
    try:print(json.dumps(handle(json.loads(sys.stdin.readline()))))
    except Exception as exc:print(json.dumps(dict(error=str(exc))))
