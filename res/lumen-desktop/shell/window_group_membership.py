"""Commit one captured owned membership intent; no preview-side mutations."""
import json
import sys
from window_group_selector import Native
from window_groups import Groups,integer
from state import runtime_epoch


def command(state,groups,data):
    if data.get('epoch')!=state['epoch'] or type(data.get('revision')) is not int or data['revision']!=state['revision'] or groups.revision!=state['revision']:
        raise ValueError('The captured group list or session changed')
    if state['locked'] or not state['enabled']:raise ValueError('Window Groups is unavailable')
    wid=integer(data.get('workspace'));cid=integer(data.get('id'));pid=integer(data.get('pid'));sid=integer(data.get('sourceGroup'))
    output=next((o for o in state['outputs'] if o['name']==data.get('output') and o['workspace']==wid),None)
    source=next((g for g in groups.groups if g.id==sid and g.workspace==wid),None)
    if output is None or output['suspended'] or source is None or not any(m.id==cid and m.pid==pid for m in source.members):
        raise ValueError('The captured source window or visible desktop changed')
    action=data.get('kind')
    if action=='new':gid=sid;operation='remove-scoped'
    elif action=='group':
        gid=integer(data.get('groupId'));operation='add-scoped'
        if gid==sid or not any(g.id==gid and g.workspace==wid and not g.suspended for g in groups.groups):
            raise ValueError('Choose a different live group on the same desktop')
    else:raise ValueError('Choose an explicit group destination')
    value=f'lumen_window groups {groups.revision} {operation} {gid} {wid} '+json.dumps(output['name'])+f' {cid} {pid}'
    return value+(f' {sid}' if action=='group' else '')


def activate(data):
    with Native() as native:
        if native.query(7).get('native_window_group_membership') is not True:
            raise ValueError('The compositor cannot safely commit scoped membership intents')
        state=native.snapshot();groups=Groups.read(native.query(4).get('window_groups'))
        value=command(state,groups,data)
        if runtime_epoch(native.path)!=native.epoch:raise ValueError('The compositor session changed')
        result=native.query(0,value)
        if not result or not all(row.get('success') is True for row in result):raise ValueError('The compositor rejected the captured membership intent')
        return dict(ok=True)


if __name__=='__main__':
    try:print(json.dumps(activate(json.loads(sys.stdin.readline()))))
    except Exception as exc:print(json.dumps(dict(error=str(exc))))
