"""Identity-based Sway workspace/window operations; no title-based commands."""
import json
import subprocess
import sys
from pathlib import Path
import os
sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "runtime"))
from spaces import SpacePolicy

def policy():
    return SpacePolicy(Path(os.environ.get("XDG_STATE_HOME", Path.home() / ".local/state")) / "lumen-desktop/spaces.json")


def run(*args):
    return json.loads(subprocess.check_output(['swaymsg','-r',*args],timeout=4))


def snapshot():
    windows=[]
    def walk(node,workspace='',output=''):
        if node.get('type')=='output':output=node['name']
        if node.get('type')=='workspace':workspace=node['name']
        if node.get('pid') and node.get('type') in ('con','floating_con'):
            app=node.get('app_id') or node.get('window_properties',{}).get('class','Application')
            title=node.get('name') or app
            group='Files' if app=='org.quickshell' and title.startswith('Files') else title if app=='org.quickshell' else app
            if not (app=='org.quickshell' and node.get('name')=='Window Overview'):
                windows.append(dict(id=node['id'],pid=node['pid'],app=app,application=SpacePolicy.application(node),group=group,title=title,workspace=workspace,output=output,focused=node.get('focused',False)))
        for child in node.get('nodes',[])+node.get('floating_nodes',[]):walk(child,workspace,output)
    walk(run('-t','get_tree'))
    return dict(windows=windows,workspaces=run('-t','get_workspaces'),outputs=[dict(name=o['name'],label=(o.get('model') or 'Display '+str(i+1))) for i,o in enumerate(run('-t','get_outputs')) if o.get('active')])


def handle(data):
    state=snapshot();action=data.get('action','status')
    model=policy()
    with model.transaction():
        model.reconcile(state['workspaces'],run('-t','get_outputs'))
        for workspace in state['workspaces']:
            workspace['logicalId']=model.current.get(workspace['id'])
        order=model.order()
        state['workspaces'].sort(key=lambda w:order.index(w['logicalId']) if w['logicalId'] in order else len(order))
        for output in state['outputs']:
            output['logicalId']=next((key for key,name in model.outputs.items() if name==output['name']),None)
        state['autoReorder']=model.state['auto_reorder']
        state['assignments']=model.state['assignments']
        if action=='ordering':
            model.configure(auto_reorder=data['enabled'])
            return dict(ok=True)
        if action=='assign':
            window=next((w for w in state['windows'] if w['id']==data.get('id') and w['pid']==data.get('pid')),None)
            if not window:raise ValueError('This window is no longer available')
            model.assign(window['application'],data['kind'],data.get('target'))
            return dict(ok=True)
    if action=='status':return state
    if action=='workspace':
        name=str(data['workspace'])
        if name not in {w['name'] for w in state['workspaces']}:raise ValueError('This workspace no longer exists')
        result=run('workspace '+json.dumps(name))
    else:
        window=next((w for w in state['windows'] if w['id']==data.get('id') and w['pid']==data.get('pid')),None)
        if not window:raise ValueError('This window is no longer available')
        prefix='[con_id='+str(window['id'])+'] '
        if action=='focus':
            result=run(prefix+('scratchpad show, ' if window['workspace']=='__i3_scratch' else '')+'focus')
        elif action=='move':
            workspace=str(data['workspace'])
            if workspace not in {w['name'] for w in state['workspaces']} and workspace not in [str(i) for i in range(1,10)]:
                raise ValueError('Choose an existing workspace or a numbered workspace')
            result=run(prefix+'move container to workspace '+json.dumps(workspace))
        elif action=='output':
            output=data['output']
            if output not in {o['name'] for o in state['outputs']}:raise ValueError('Display is no longer connected')
            result=run(prefix+'move container to output '+json.dumps(output))
        else:raise ValueError('Unknown window action')
    if not all(r.get('success') for r in result):raise ValueError('The window manager rejected this action')
    return dict(ok=True)


if __name__=='__main__':
    try:print(json.dumps(handle(json.loads(sys.stdin.readline()))))
    except Exception as exc:print(json.dumps(dict(error=str(exc))))
