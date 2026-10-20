"""Read-only projection of authoritative display policy and native application."""
import json
import os
import re
from pathlib import Path
import select
import socket
import sys
import time
sys.path.insert(0,str(Path(__file__).resolve().parents[1]/'runtime'))
from display_space_runtime import configured
from display_space_policy import display_identity, identity_record, identity, primary
from native_file_drag import receive, send
from state import runtime_epoch


def project(native, outputs, desired, remembered=None):
    legacy=native is None
    if not legacy and (not isinstance(native,dict) or type(native.get('version')) is not int or native['version']!=1 or not isinstance(native.get('session'),str) or not re.fullmatch('[0-9a-f]{32}',native['session']) or type(native.get('available')) is not bool or type(native.get('separateDisplays')) is not bool or not isinstance(native.get('generation'),str) or not re.fullmatch('0|[1-9][0-9]{0,19}',native['generation']) or int(native['generation'])>2**64-1):
        raise ValueError('Updated display policy is unavailable')
    if not isinstance(outputs,list):raise ValueError('Invalid connected displays')
    rows=[]
    for row in outputs:
        if row.get('active') is not True:continue
        key=display_identity(row)
        rows.append(dict(name=row['name'],identity=identity_record(key),label=row.get('model') or row['name']))
    if legacy:
        return dict(protocolVersion=1,nativeSupported=False,session=None,generation=None,available=False,applied=False,independent=True,separateDisplays=desired['separateDisplays'],primary='',primaryDisplay=identity_record(desired['primaryDisplay']),outputs=rows,error='Display policy requires the updated compositor; using independent Dock routing')
    names=[row['name'] for row in rows]
    available=native.get('available') is True and native.get('primary') in names
    chosen=primary(desired['primaryDisplay'],remembered,outputs)
    applied=available and native.get('separateDisplays') is desired['separateDisplays'] and native.get('primary')==chosen['name']
    return dict(protocolVersion=1,nativeSupported=True,session=native['session'],generation=native.get('generation'),available=available,applied=applied,independent=desired['separateDisplays'] is True and native.get('separateDisplays') is True,separateDisplays=desired['separateDisplays'],
                primary=native.get('primary') if available else '',primaryDisplay=identity_record(desired['primaryDisplay']),
                outputs=rows,error='' if applied else 'Display policy is waiting for a current native registration')


def remembered_primary():
    path=Path(os.environ.get('XDG_STATE_HOME',str(Path.home()/'.local/state')))/'lumen-desktop/spaces.json'
    if not path.exists():return None
    with path.open('rb') as stream:data=stream.read(4*1024*1024+1)
    if len(data)>4*1024*1024:raise ValueError('Display state exceeds the observation budget')
    value=json.loads(data)
    if not isinstance(value,dict):raise ValueError('Invalid saved display state')
    chosen=value.get('automatic_primary')
    return identity(chosen) if chosen is not None else None


def watch():
    empty=dict(available=False,applied=False,separateDisplays=True,primary='',primaryDisplay=None,outputs=[],error='Display policy observer is unavailable')
    def publish(value):print(json.dumps(value),flush=True)
    publish(empty)
    try:
        path=os.environ['SWAYSOCK'];epoch=runtime_epoch(path)
        with socket.socket(socket.AF_UNIX) as events, socket.socket(socket.AF_UNIX) as commands:
            for stream in (events,commands):stream.settimeout(2);stream.connect(path)
            send(events,2,'["workspace","output","tick","shutdown"]');_,ack=receive(events)
            if not isinstance(ack,dict) or ack.get('success') is not True:return
            def query(kind):
                send(commands,kind,'');reply,data=receive(commands)
                if reply!=kind:raise ValueError('Unexpected native reply')
                return data
            while True:
                started=time.clock_gettime(time.CLOCK_BOOTTIME)
                tree=query(4);outputs=query(3)
                desired=configured()
                value=project(tree.get('display_spaces'),outputs,desired,remembered_primary() if desired['primaryDisplay'] is None else None)
                if runtime_epoch(path)!=epoch:raise ValueError('Desktop session changed')
                if time.clock_gettime(time.CLOCK_BOOTTIME)-started>2:
                    raise ValueError('Display observation expired')
                publish(value)
                ready,_,_=select.select([events],[],[],.6)
                if ready:
                    kind,_=receive(events)
                    if kind==0x80000006:return
    except (ValueError,TypeError,KeyError,OSError,EOFError):pass
    finally:publish(empty)


if __name__=='__main__':watch()
