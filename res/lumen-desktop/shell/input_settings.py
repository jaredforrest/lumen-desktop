"""Per-device presentation preferences over the compositor's input IPC."""
import fcntl
import hashlib
import json
import math
import os
from pathlib import Path
import re
import socket
import struct
import sys
import time
from nix_settings import read_settings, set_section

MAGIC=b'i3-ipc'
FIELDS={
    'repeat_rate':dict(label='Key repeat rate',kind='number',minimum=0,maximum=60,step=1,types=('keyboard',)),
    'repeat_delay':dict(label='Key repeat delay (ms)',kind='number',minimum=100,maximum=2000,step=50,types=('keyboard',)),
    'pointer_accel':dict(label='Tracking speed',kind='number',minimum=-1,maximum=1,step=.1,types=('pointer','touchpad'),source='accel_speed'),
    'scroll_factor':dict(label='Scroll speed',kind='number',minimum=.1,maximum=5,step=.1,types=('pointer','touchpad')),
    'natural_scroll':dict(label='Natural scrolling',kind='bool',types=('pointer','touchpad')),
    'left_handed':dict(label='Swap primary and secondary buttons',kind='bool',types=('pointer','touchpad')),
    'tap':dict(label='Tap to click',kind='bool',types=('touchpad',)),
    'dwt':dict(label='Disable trackpad while typing',kind='bool',types=('touchpad',)),
}


def receive(stream,count):
    result=b''
    while len(result)<count:
        value=stream.recv(count-len(result))
        if not value:raise ValueError('The compositor disconnected')
        result+=value
    return result


def response(stream):
    magic,length,kind=struct.unpack('<6sII',receive(stream,14))
    if magic!=MAGIC or length>4*1024*1024:raise ValueError('Invalid compositor response')
    return kind,json.loads(receive(stream,length))


def connect():
    path=os.environ.get('SWAYSOCK')
    if not path:raise ValueError('The compositor input provider is unavailable.')
    stream=socket.socket(socket.AF_UNIX,socket.SOCK_STREAM);stream.settimeout(5)
    try:stream.connect(path);return stream
    except OSError:stream.close();raise


def ipc(kind,payload=''):
    encoded=payload.encode()
    with connect() as stream:
        stream.sendall(struct.pack('<6sII',MAGIC,len(encoded),kind)+encoded)
        actual,value=response(stream)
        if actual!=kind:raise ValueError('Unexpected compositor response')
        return value


def identity(device):
    return dict(identifier=device.get('identifier',''),name=device.get('name',''),type=device.get('type',''))


def key_for(value):
    return hashlib.sha256(json.dumps(value,sort_keys=True,ensure_ascii=False,separators=(',',':')).encode()).hexdigest()


def safe_identifier(value):
    return isinstance(value,str) and re.fullmatch(r'[0-9]+:[0-9]+:[A-Za-z0-9_.:+()-]{1,512}',value) is not None


def valid(key,value):
    if key not in FIELDS:return False
    field=FIELDS[key]
    if field['kind']=='bool':return type(value) is bool
    if type(value) not in (int,float) or not math.isfinite(value):return False
    return field['minimum']<=value<=field['maximum'] and (key not in ('repeat_rate','repeat_delay') or type(value) is int)


def current_value(device,key):
    field=FIELDS[key]
    if device.get('type') not in field['types']:return None
    if key in ('repeat_rate','repeat_delay','scroll_factor'):value=device.get(key)
    else:value=device.get('libinput',{}).get(field.get('source',key))
    if field['kind']=='bool':return {'enabled':True,'disabled':False}.get(value) if isinstance(value,str) else None
    return value if valid(key,value) else None


def values(record,section='values'):
    return {key:value for key,value in record.get(section,{}).items() if value is not None}


def same_value(actual,expected):
    if type(expected) is bool:return actual is expected
    return type(actual) in (int,float) and type(expected) in (int,float) and abs(actual-expected)<.0001


def devices_snapshot(devices,saved):
    rows=[];connected=set()
    for device in devices:
        ident=identity(device);key=key_for(ident);connected.add(key)
        matches=sum(other.get('identifier')==ident['identifier'] for other in devices)
        editable=safe_identifier(ident['identifier']) and matches==1
        controls=[]
        for option,field in FIELDS.items():
            if ident['type'] not in field['types']:continue
            value=current_value(device,option)
            controls.append(dict(key=option,**{k:v for k,v in field.items() if k not in ('types','source')},value=value,
                                 supported=editable and value is not None,saved=option in values(saved.get(key,{})),
                                 inSync=option not in values(saved.get(key,{})) or same_value(value,values(saved.get(key,{}))[option])))
        rows.append(dict(key=key,identity=ident,connected=True,editable=editable,controls=controls,
                         saved=bool(values(saved.get(key,{}))),
                         detail='Two connected devices share this identifier; separate control is unsafe.' if matches>1 else
                                'This identifier cannot be safely addressed by the supported adapter.' if not editable else
                                'Changes apply only to this compositor identifier, not every device of this type.'))
    for key,record in saved.items():
        if key not in connected:
            rows.append(dict(key=key,identity=record['identity'],connected=False,editable=False,controls=[],saved=True,
                             detail='Disconnected. Saved preferences apply only when this exact identity returns.'))
    return sorted(rows,key=lambda row:(not row['connected'],row['identity']['type'],row['identity']['name'],row['key']))


def snapshot():
    return dict(available=True,devices=devices_snapshot(ipc(100),read_settings().get('inputs',{})))


def apply_value(ident,key,value):
    if not safe_identifier(ident['identifier']) or not valid(key,value):raise ValueError('Unsupported device option or value')
    encoded=('enabled' if value else 'disabled') if type(value) is bool else str(value)
    result=ipc(0,'input '+json.dumps(ident['identifier'])+' '+key+' '+encoded)
    if not isinstance(result,list) or not result or not all(item.get('success') for item in result):raise ValueError('The compositor rejected this device setting')


def exact_device(ident,devices):
    matches=[device for device in devices if device.get('identifier')==ident['identifier']]
    if len(matches)!=1 or identity(matches[0])!=ident:raise ValueError('The device disconnected, changed identity or is ambiguous. Refresh before applying.')
    return matches[0]


def confirmed_value(ident,key,value):
    device=exact_device(ident,ipc(100));actual=current_value(device,key)
    confirmed=same_value(actual,value)
    if not confirmed:raise ValueError('The compositor did not confirm the requested setting')


def handle(request):
    if not isinstance(request,dict):raise ValueError('Expected an input settings request')
    action=request.get('action','status')
    if action=='status':return snapshot()
    folder=Path(os.environ['XDG_RUNTIME_DIR'])/'lumen-input-settings';folder.mkdir(mode=0o700,parents=True,exist_ok=True)
    with (folder/'edit.lock').open('a') as lock:
        fcntl.flock(lock,fcntl.LOCK_EX)
        saved=read_settings().get('inputs',{})
        devices=ipc(100);rows=devices_snapshot(devices,saved)
        row=next((item for item in rows if item['key']==request.get('device')),None)
        if row is None:raise ValueError('Select a reported device or saved profile')
        if action=='forget':
            if request.get('confirmed') is not True:raise ValueError('Confirm forgetting this device profile')
            updated=dict(saved);updated.pop(row['key'],None);set_section('inputs',updated)
            result=snapshot();result['message']='Saved profile forgotten. Current device values were not changed.';return result
        if action not in ('set','restore') or not row['editable']:raise ValueError('This device cannot be edited independently')
        option=request.get('key')
        if option not in FIELDS:raise ValueError('Unsupported input setting')
        device=exact_device(row['identity'],devices);previous=current_value(device,option)
        if previous is None:raise ValueError('The device does not report this capability')
        record=saved.get(row['key'],dict(identity=row['identity'],values={},baseline={}))
        overrides=values(record);baseline=values(record,'baseline')
        if action=='restore':
            if option not in overrides or option not in baseline:raise ValueError('There is no previous value saved for this option')
            value=baseline.pop(option);overrides.pop(option)
        else:
            value=request.get('value')
            if not valid(option,value):raise ValueError('Unsupported input option value')
            baseline.setdefault(option,previous);overrides[option]=value
        # Never send a wildcard or type selector. Recheck identity at mutation time.
        exact_device(row['identity'],ipc(100));apply_value(row['identity'],option,value)
        committed=False
        try:
            confirmed_value(row['identity'],option,value)
            updated=dict(saved)
            if overrides:updated[row['key']]=dict(identity=row['identity'],values=overrides,baseline=baseline)
            else:updated.pop(row['key'],None)
            set_section('inputs',updated)
            committed=True
            # Configuration activation may reload compositor defaults. Reapply
            # only the confirmed device option after the durable transaction.
            exact_device(row['identity'],ipc(100));apply_value(row['identity'],option,value)
            confirmed_value(row['identity'],option,value)
        except Exception as original:
            failures=[]
            if committed:
                try:set_section('inputs',saved)
                except Exception:failures.append('saved profile')
            # Roll back only if the same unique device is still connected.
            try:
                exact_device(row['identity'],ipc(100));apply_value(row['identity'],option,previous)
                confirmed_value(row['identity'],option,previous)
            except (OSError,ValueError):failures.append('previous device value')
            if failures:raise ValueError('Input update failed and could not restore '+', '.join(failures)+'. Refresh to inspect current and saved values.') from original
            raise
        result=snapshot();result['message']='Device preference saved.' if action=='set' else 'Previous device value restored.';return result


def restore_saved():
    devices=ipc(100)
    for record in read_settings().get('inputs',{}).values():
        try:
            exact_device(record['identity'],devices)
            for key,value in values(record).items():
                device=exact_device(record['identity'],ipc(100))
                if current_value(device,key) is not None and current_value(device,key)!=value:apply_value(record['identity'],key,value)
        except (OSError,ValueError):continue


def watch():
    with connect() as stream:
        payload=b'["input"]';stream.sendall(struct.pack('<6sII',MAGIC,len(payload),2)+payload)
        _,accepted=response(stream)
        if not accepted.get('success'):raise ValueError('Input event subscription failed')
        restore_saved()
        stream.settimeout(None)
        while True:
            kind,event=response(stream)
            if kind==0x80000015 and event.get('change')=='added':time.sleep(.1);restore_saved()


def main():
    if len(sys.argv)>1 and sys.argv[1]=='watch':watch();return
    try:print(json.dumps(handle(json.loads(sys.stdin.readline()))))
    except (OSError,ValueError,KeyError) as error:print(json.dumps(dict(available=False,devices=[],error=str(error))))


if __name__=='__main__':main()
