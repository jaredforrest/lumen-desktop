"""Read native recent groups and commit captured, visibly scoped selections."""
import json
import os
from pathlib import Path
import select
import socket
import sys
sys.path.insert(0,str(Path(__file__).resolve().parents[1]/'runtime'))
from window_groups import Groups, integer
from state import runtime_epoch
from native_file_drag import receive, send


def project(tree, outputs, workspaces, version, epoch):
    if not isinstance(version,dict) or version.get('native_window_group_selector') is not True:
        raise ValueError('The compositor does not support visibly scoped group selection')
    groups=Groups.read(tree.get('window_groups'))
    lock=version.get('lumen_session_lock')
    if not isinstance(lock,dict) or type(lock.get('version')) is not int or lock['version']!=1 or lock.get('available') is not True or type(lock.get('locked')) is not bool:
        raise ValueError('Native session lock facts are unavailable')
    if not isinstance(outputs,list) or not isinstance(workspaces,list) or any(not isinstance(row,dict) for row in outputs+workspaces):raise ValueError('Invalid output facts')
    rows=[]
    for output in outputs:
        if not isinstance(output,dict) or output.get('active') is not True:continue
        name=output.get('name')
        if not isinstance(name,str) or not name or len(name)>256:raise ValueError('Invalid output identity')
        visible=[ws for ws in workspaces if ws.get('output')==name and ws.get('visible') is True]
        if len(visible)!=1:continue
        workspace=integer(visible[0].get('id'))
        items=sorted((g for g in groups.groups if g.workspace==workspace),key=lambda g:(-g.recent,g.id))
        rows.append(dict(name=name,workspace=workspace,suspended=any(g.suspended for g in items),groups=[dict(id=g.id,selected=g.selected,recent=g.recent,count=len(g.members)) for g in items]))
    return dict(available=groups.separate_displays,membershipSupported=groups.separate_displays and version.get('native_window_group_membership') is True,enabled=groups.enabled and groups.separate_displays,locked=groups.locked or lock['locked'],epoch=epoch,revision=groups.revision,
                focusedOutput=next((o['name'] for o in outputs if o.get('active') is True and o.get('focused') is True),''),outputs=rows)


class Native:
    def __enter__(self):
        self.path=os.environ['SWAYSOCK'];self.epoch=runtime_epoch(self.path)
        self.stream=socket.socket(socket.AF_UNIX);self.stream.settimeout(2)
        try:self.stream.connect(self.path)
        except OSError:self.stream.close();raise
        return self
    def __exit__(self,*args):self.stream.close()
    def query(self,kind,value=''):
        send(self.stream,kind,value);reply,data=receive(self.stream)
        if reply!=kind:raise ValueError('Unexpected compositor reply')
        return data
    def snapshot(self):
        result=project(self.query(4),self.query(3),self.query(1),self.query(7),self.epoch)
        if runtime_epoch(self.path)!=self.epoch:raise ValueError('The compositor session changed')
        return result
    def activate(self,data):
        snapshot=self.snapshot()
        if data.get('epoch')!=self.epoch or type(data.get('revision')) is not int or data['revision']!=snapshot['revision']:
            raise ValueError('The group list or compositor session changed; refresh before acting')
        if not snapshot['enabled'] or snapshot['locked']:raise ValueError('Window Groups is unavailable')
        wid=integer(data.get('workspace'));gid=integer(data.get('id'))
        output=next((o for o in snapshot['outputs'] if o['name']==data.get('output') and o['workspace']==wid),None)
        if output is None or output['suspended'] or not any(g['id']==gid for g in output['groups']):
            raise ValueError('The group is no longer on the captured visible desktop')
        if runtime_epoch(self.path)!=self.epoch:raise ValueError('The compositor session changed')
        command=f'lumen_window groups {snapshot["revision"]} select-scoped {gid} {wid} '+json.dumps(output['name'])
        result=self.query(0,command)
        if not isinstance(result,list) or not result or not all(row.get('success') is True for row in result):
            raise ValueError('The compositor rejected the captured group selection')
        return dict(ok=True)


def watch():
    empty=dict(available=False,enabled=False,locked=True,outputs=[])
    def publish(value):print(json.dumps(value),flush=True)
    publish(empty)
    try:
        with socket.socket(socket.AF_UNIX) as events:
            events.settimeout(2);events.connect(os.environ['SWAYSOCK']);send(events,2,'["window","workspace","output","tick","shutdown"]')
            _,ack=receive(events)
            if not isinstance(ack,dict) or ack.get('success') is not True:return
            with Native() as native:
                while True:
                    value=native.snapshot()
                    # Heartbeats allow the UI to close after an unresponsive observer.
                    publish(value)
                    readable,_,_=select.select([events],[],[],.6)
                    if readable:
                        # Event payloads, including public ticks, are only invalidations.
                        kind,_=receive(events)
                        if kind==0x80000006:return
    except (ValueError,TypeError,KeyError,OSError,EOFError):pass
    finally:publish(empty)


if __name__=='__main__':
    if len(sys.argv)>1 and sys.argv[1]=='watch':watch()
    else:
        try:
            with Native() as native:print(json.dumps(native.activate(json.loads(sys.stdin.readline()))))
        except Exception as exc:print(json.dumps(dict(error=str(exc))))
