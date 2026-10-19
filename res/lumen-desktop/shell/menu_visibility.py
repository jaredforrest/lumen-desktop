"""Read-only, output-scoped fullscreen observation from authoritative compositor IPC."""
import json,os,socket,struct,subprocess,sys

def snapshot(tree,outputs):
    if not isinstance(tree,dict) or not isinstance(outputs,list):raise ValueError('Invalid compositor snapshot')
    active={row['name']:row.get('current_workspace') for row in outputs if row.get('active') is True and isinstance(row.get('name'),str)}
    result={name:False for name in active};global_fullscreen=False
    def walk(node,output=None,workspace=None):
        nonlocal global_fullscreen
        if not isinstance(node,dict):raise ValueError('Invalid compositor node')
        if node.get('type')=='output':output=node.get('name')
        if node.get('type')=='workspace':workspace=node.get('name')
        mode=node.get('fullscreen_mode',0)
        if type(mode) is not int or mode not in (0,1,2):raise ValueError('Invalid fullscreen state')
        # Workspace nodes use a structural fullscreen marker even on an empty desktop.
        if node.get('type') in ('con','floating_con'):
            if mode==2:global_fullscreen=True
            if mode==1 and output in active and workspace==active[output]:result[output]=True
        for child in node.get('nodes',[])+node.get('floating_nodes',[]):walk(child,output,workspace)
    walk(tree)
    return {name:True for name in active} if global_fullscreen else result

def query(kind):return json.loads(subprocess.check_output(['swaymsg','-r','-t',kind],timeout=4,stderr=subprocess.DEVNULL))
def packet(stream):
    def read(count):
        value=b''
        while len(value)<count:
            chunk=stream.recv(count-len(value))
            if not chunk:raise OSError('Compositor disconnected')
            value+=chunk
        return value
    magic,length,kind=struct.unpack('<6sII',read(14))
    if magic!=b'i3-ipc' or length>16*1024*1024:raise ValueError('Invalid compositor packet')
    return kind,json.loads(read(length))

def watch(token):
    revision=0
    def emit(available,outputs,error=''):
        nonlocal revision
        revision+=1;print(json.dumps(dict(transport=token,revision=revision,available=available,outputs=outputs,error=error)),flush=True)
    try:
        with socket.socket(socket.AF_UNIX,socket.SOCK_STREAM) as stream:
            stream.settimeout(4);stream.connect(os.environ['SWAYSOCK'])
            payload=b'["window","workspace","output","shutdown"]'
            stream.sendall(struct.pack('<6sII',b'i3-ipc',len(payload),2)+payload)
            kind,acknowledgement=packet(stream)
            if kind!=2 or acknowledgement.get('success') is not True:raise ValueError('Subscription unavailable')
            stream.settimeout(None)
            emit(True,snapshot(query('get_tree'),query('get_outputs')))
            while True:
                kind,event=packet(stream)
                if kind==0x80000006:break
                emit(True,snapshot(query('get_tree'),query('get_outputs')))
    except (OSError,ValueError,KeyError,TypeError,subprocess.SubprocessError):pass
    finally:emit(False,{},'Fullscreen menu observation is unavailable')
if __name__=='__main__':watch(sys.argv[1] if len(sys.argv)>1 else '0')
