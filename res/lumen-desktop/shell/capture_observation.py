"""Read-only, bounded observations of PipeWire-visible capture links.

These are graph observations, not a permission authority or proof that other
capture paths are idle. Client-provided names are labelled as reported metadata.
"""
import json
import os
import selectors
import subprocess
import time

MAX_BYTES=8*1024*1024
MAX_OBJECTS=16384
MAX_ROWS=64
DEADLINE=2.0
COVERAGE='PipeWire-visible graph only. Direct device access, hidden nodes and other capture paths are not assessed.'

def unavailable(reason):
    return dict(available=False,rows=[],coverage=COVERAGE,error=reason)

def identifier(value):
    if isinstance(value,str) and value.isascii() and value.isdecimal() and len(value)<=10:value=int(value)
    if type(value) is not int or not 0<=value<2**32:raise ValueError('Invalid graph identity')
    return value

def label(value,fallback):
    if not isinstance(value,str) or not value.strip():return fallback
    return ''.join(c for c in value if c.isprintable())[:160] or fallback

def describe(objects):
    if not isinstance(objects,list) or len(objects)>MAX_OBJECTS:raise ValueError('Unsupported graph size')
    indexed={}
    for obj in objects:
        if not isinstance(obj,dict):raise ValueError('Invalid graph object')
        key=identifier(obj.get('id'))
        if key in indexed:raise ValueError('Duplicate graph identity')
        indexed[key]=obj
    rows={}
    for obj in indexed.values():
        if obj.get('type')!='PipeWire:Interface:Link':continue
        info=obj.get('info')
        if not isinstance(info,dict):raise ValueError('Missing link evidence')
        if info.get('state')!='active':continue
        source=identifier(info.get('output-node-id'));target=identifier(info.get('input-node-id'))
        node=indexed.get(target);origin=indexed.get(source)
        if not node or not origin:raise ValueError('Incomplete active graph')
        if node.get('type')!='PipeWire:Interface:Node' or origin.get('type')!='PipeWire:Interface:Node':raise ValueError('Invalid link endpoint')
        ni=node.get('info',{})
        if not isinstance(ni,dict):raise ValueError('Invalid node information')
        props=ni.get('props',{})
        if not isinstance(props,dict):raise ValueError('Invalid node metadata')
        media=props.get('media.class');kind={'Stream/Input/Audio':'audio-input','Stream/Input/Video':'video-input'}.get(media)
        if kind is None:continue
        client=None
        if 'client.id' in props:client=indexed.get(identifier(props['client.id']))
        ci=client.get('info',{}) if client and client.get('type')=='PipeWire:Interface:Client' else {}
        if not isinstance(ci,dict):raise ValueError('Invalid client information')
        cp=ci.get('props',{})
        if not isinstance(cp,dict):raise ValueError('Invalid client metadata')
        key=(target,kind)
        if key not in rows:
            if len(rows)>=MAX_ROWS:raise ValueError('Capture observation count exceeds supported limit')
            rows[key]=dict(nodeId=str(target),kind=kind,reportedApplication=label(cp.get('application.name') or props.get('application.name'),'Unidentified client'),sources=[],attribution='Reported PipeWire client metadata; not authenticated application identity')
        rows[key]['sources'].append(str(source))
    result=sorted(rows.values(),key=lambda r:(r['kind'],int(r['nodeId'])))
    for row in result:row['sources']=sorted(set(row['sources']),key=int)
    return dict(available=True,rows=result,coverage=COVERAGE,error='')

def read_graph(command=('pw-dump','--no-colors'),deadline=DEADLINE,max_bytes=MAX_BYTES):
    """Never activate a provider, change routing or retain graph data on disk."""
    end=time.monotonic()+deadline
    with subprocess.Popen(command,stdout=subprocess.PIPE,stdin=subprocess.DEVNULL) as child:
        try:
            with selectors.DefaultSelector() as ready:
                ready.register(child.stdout,selectors.EVENT_READ);parts=[];size=0
                while ready.get_map():
                    remaining=end-time.monotonic()
                    if remaining<=0:raise TimeoutError('Capture observation timed out')
                    events=ready.select(remaining)
                    if not events:raise TimeoutError('Capture observation timed out')
                    chunk=os.read(child.stdout.fileno(),min(65536,max_bytes-size+1))
                    if not chunk:ready.unregister(child.stdout);break
                    size+=len(chunk)
                    if size>max_bytes:raise ValueError('Capture graph exceeds supported byte limit')
                    parts.append(chunk)
            if child.wait(timeout=max(.001,end-time.monotonic()))!=0:raise ValueError('PipeWire graph is unavailable')
            return json.loads(b''.join(parts))
        finally:
            if child.poll() is None:child.kill()
            child.wait()

def snapshot(command=('pw-dump','--no-colors')):
    try:return describe(read_graph(command))
    except (OSError,ValueError,TypeError,TimeoutError,subprocess.SubprocessError,RecursionError):return unavailable('Capture observations are unavailable; no idle or permission conclusion can be drawn.')

if __name__=='__main__':print(json.dumps(snapshot()))
