"""Private per-directory desktop arrangement metadata; never modifies desktop files."""
import fcntl,hashlib,json,math,os,re,stat,sys,tempfile
from pathlib import Path

MAX_ITEMS=5000
GROUPS=('none','kind','modified','tags')

def owned(info,directory=False):
    if info.st_uid!=os.getuid() or info.st_mode & 0o077 or not (stat.S_ISDIR(info.st_mode) if directory else stat.S_ISREG(info.st_mode)):
        raise ValueError('Desktop arrangement requires owned private storage')

def key(value):
    if not isinstance(value,str) or not re.fullmatch(r'[0-9]{1,24}:[0-9]{1,24}(?:-[0-9a-f]{16})?',value):raise ValueError('Invalid desktop item identity')
    return value

def validate(data):
    if not isinstance(data,dict) or not {'grouping','positions'}<=set(data) or set(data)-{'grouping','positions','stacks'} or data['grouping'] not in GROUPS or not isinstance(data['positions'],dict) or len(data['positions'])>MAX_ITEMS:raise ValueError('Invalid desktop arrangement')
    positions={}
    for identity,point in data['positions'].items():
        key(identity)
        if not isinstance(point,list) or len(point)!=2 or any(isinstance(number,bool) or not isinstance(number,(int,float)) or not math.isfinite(number) or number<0 or number>1 for number in point):raise ValueError('Invalid desktop item position')
        positions[identity]=point
    stacks=data.get('stacks',{})
    if not isinstance(stacks,dict) or len(stacks)>64:raise ValueError('Invalid desktop stack arrangement')
    for identity,point in stacks.items():
        if not isinstance(identity,str) or not re.fullmatch(r'stack:(kind|modified|tags):[A-Za-z0-9 ,]{1,128}',identity):raise ValueError('Invalid desktop stack identity')
        if not isinstance(point,list) or len(point)!=2 or any(isinstance(number,bool) or not isinstance(number,(int,float)) or not math.isfinite(number) or number<0 or number>1 for number in point):raise ValueError('Invalid desktop stack position')
    result={'grouping':data['grouping'],'positions':positions}
    if stacks:result['stacks']=stacks
    return result

def handle(request):
    if not isinstance(request,dict) or request.get('action') not in ('load','save','reset'):raise ValueError('Invalid desktop arrangement request')
    if set(request)-{'action','directory','items','layout'}:raise ValueError('Unknown desktop arrangement field')
    directory=request.get('directory');items=request.get('items')
    if not isinstance(directory,str) or not Path(directory).is_absolute() or '\0' in directory:raise ValueError('An absolute desktop directory is required')
    if not isinstance(items,list) or len(items)>MAX_ITEMS or len(set(map(key,items)))!=len(items):raise ValueError('Invalid desktop item identities')
    folder=os.stat(directory)
    if not stat.S_ISDIR(folder.st_mode):raise ValueError('Desktop location is not a directory')
    identity=hashlib.sha256(f'{folder.st_dev}:{folder.st_ino}'.encode()).hexdigest()
    root=Path(os.environ.get('XDG_STATE_HOME',str(Path.home()/'.local/state')))/'lumen-desktop/desktop-layouts';root.mkdir(mode=0o700,parents=True,exist_ok=True);owned(root.lstat(),True)
    lock=os.open(root/(identity+'.lock'),os.O_CREAT|os.O_RDWR|os.O_NOFOLLOW|os.O_CLOEXEC,0o600)
    try:
        owned(os.fstat(lock));fcntl.flock(lock,fcntl.LOCK_EX);path=root/(identity+'.json');layout={'grouping':'none','positions':{}}
        if request['action']=='save':layout=validate(request.get('layout'))
        elif request['action']!='reset':
            try:descriptor=os.open(path,os.O_RDONLY|os.O_NOFOLLOW|os.O_CLOEXEC)
            except FileNotFoundError:descriptor=None
            if descriptor is not None:
                with os.fdopen(descriptor) as stream:
                    info=os.fstat(stream.fileno());owned(info)
                    if info.st_size>2_000_000:raise ValueError('Desktop arrangement exceeds its size limit')
                    layout=validate(json.load(stream))
        # The current authoritative listing removes stale metadata. Identity
        # follows a rename. An observed removal clears its placement before reuse.
        layout['positions']={identity:point for identity,point in layout['positions'].items() if identity in items}
        descriptor,temporary=tempfile.mkstemp(prefix='.layout-',dir=root)
        try:
            with os.fdopen(descriptor,'w') as stream:json.dump(layout,stream);stream.flush();os.fsync(stream.fileno())
            os.replace(temporary,path)
        finally:Path(temporary).unlink(missing_ok=True)
        return {'layout':layout}
    finally:os.close(lock)

if __name__=='__main__':
    try:
        line=sys.stdin.readline(2_000_001)
        if len(line)>2_000_000:raise ValueError('Desktop arrangement request is too large')
        print(json.dumps(handle(json.loads(line))))
    except (OSError,ValueError,TypeError) as error:print(json.dumps({'error':str(error)}));sys.exit(1)
