"""Exact-identity adapter for native paired placement; no focused-window fallback."""
import json,math,subprocess,sys

def run(*args):
    result=subprocess.run(['swaymsg','-r',*args],text=True,capture_output=True,timeout=5)
    try:value=json.loads(result.stdout)
    except ValueError:raise ValueError('Unable to read native paired placement')
    if result.returncode or isinstance(value,list) and any(not item.get('success') for item in value):
        raise ValueError(next((item.get('error') for item in value if isinstance(item,dict) and item.get('error')),'The native placement request was rejected') if isinstance(value,list) else 'The native placement request was rejected')
    return value

def identity(value):
    if type(value) is not int or not 0<value<=2**53-1:raise ValueError('Choose an exact window identity')
    return value

def windows():
    result=[]
    def walk(node,output=""):
        if node.get("type")=="output":output=node["name"]
        properties=node.get('window_properties') or {}
        app=node.get('app_id') or properties.get('class') or properties.get('instance')
        if node.get('pid') and app and node.get('type') in ('con','floating_con'):
            pair=node.get('lumen_pair',{})
            result.append(dict(id=node['id'],pid=node['pid'],title=node.get('name') or app,app=app,pair=pair,output=output))
        for child in node.get('nodes',[])+node.get('floating_nodes',[]):walk(child,output)
    walk(run('-t','get_tree'));return result

def handle(data):
    if not isinstance(data,dict):raise ValueError('Invalid paired placement request')
    if run('-t','get_version').get('native_paired_layouts') is not True:raise ValueError('Paired placement requires the updated compositor at next login')
    current=windows();source_id=identity(data.get('id'));source=next((w for w in current if w['id']==source_id),None)
    if not source or data.get('pid') not in (None,0,source['pid']):raise ValueError('The captured window is no longer available')
    action=data.get('action','status')
    if action=='status':
        candidates=[w for w in current if w['id']!=source_id and w['pair'].get('eligible') is True and not w['pair'].get('id') and not w['pair'].get('survivor') and not(w['app']=='org.quickshell' and w['title'] in ('Paired Windows','Window Overview','Quick Look'))]
        return dict(source=source,candidates=candidates)
    if type(data.get('pid')) is not int or data['pid']!=source['pid']:raise ValueError('The captured window identity changed')
    if action in ('create','replace'):
        candidate_id=identity(data.get('partner'));candidate=next((w for w in current if w['id']==candidate_id and w['pid']==data.get('partnerPid')),None)
        if not candidate or candidate_id==source_id:raise ValueError('The selected partner is no longer available')
        if action=='create':
            if data.get('side') not in ('left','right'):raise ValueError('Choose the left or right side')
            left,right=(source_id,candidate_id) if data['side']=='left' else (candidate_id,source_id)
            run(f'lumen_pair create {left} {right}')
        else:run(f'lumen_pair replace {source_id} {candidate_id}')
    elif action in ('swap','leave'):run(f'lumen_pair {action} {source_id}')
    elif action=='ratio':
        value=data.get('ratio')
        if type(value) not in (int,float) or not math.isfinite(value) or not 0<value<1:raise ValueError('Choose a valid division')
        run(f'lumen_pair ratio {source_id} {value}')
    else:raise ValueError('Unknown paired placement action')
    return dict(ok=True)

if __name__=='__main__':
    try:print(json.dumps(handle(json.loads(sys.stdin.readline()))))
    except (OSError,ValueError,subprocess.SubprocessError) as error:print(json.dumps(dict(error=str(error))))
