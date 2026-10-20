"""Disposable filesystem/network/process boundary for optional application tests."""
import ctypes,errno,json,os,socket,stat,struct,subprocess,sys,tempfile
from pathlib import Path

MS_RDONLY=1;MS_NOSUID=2;MS_NODEV=4;MS_NOEXEC=8;MS_REMOUNT=32;MS_BIND=4096;MS_REC=16384;MS_PRIVATE=1<<18
libc=ctypes.CDLL(None,use_errno=True)

def mount(source,target,kind=None,flags=0,data=None):
    values=[None if value is None else os.fsencode(value) for value in (source,target,kind)]
    if libc.mount(*values,ctypes.c_ulong(flags),None if data is None else os.fsencode(data)):
        code=ctypes.get_errno();raise OSError(code,os.strerror(code),str(target))

def bind(source,target,readonly=False):
    source,target=Path(source),Path(target)
    if source.is_dir():target.mkdir(parents=True,exist_ok=True)
    else:target.parent.mkdir(parents=True,exist_ok=True);target.touch()
    mount(source,target,flags=MS_BIND|MS_REC)
    if readonly:mount(None,target,flags=MS_BIND|MS_REMOUNT|MS_RDONLY|MS_NOSUID|MS_NODEV)

def loopback():
    import fcntl
    with socket.socket(socket.AF_INET,socket.SOCK_DGRAM) as descriptor:
        request=struct.pack('16sH14s',b'lo',0,b'')
        current=fcntl.ioctl(descriptor,0x8913,request)
        flags=struct.unpack('16sH14s',current)[1]
        fcntl.ioctl(descriptor,0x8914,struct.pack('16sH14s',b'lo',flags|1,b''))

def drop_capabilities():
    class Header(ctypes.Structure):_fields_=[('version',ctypes.c_uint32),('pid',ctypes.c_int)]
    class Data(ctypes.Structure):_fields_=[('effective',ctypes.c_uint32),('permitted',ctypes.c_uint32),('inheritable',ctypes.c_uint32)]
    if libc.prctl(47,4,0,0,0):raise OSError(ctypes.get_errno(),'Could not clear ambient capabilities')
    for capability in range(64):
        if libc.prctl(24,capability,0,0,0) and ctypes.get_errno()!=errno.EINVAL:raise OSError(ctypes.get_errno(),'Could not drop capability bound')
    header=Header(0x20080522,0);data=(Data*2)()
    if libc.capset(ctypes.byref(header),data):raise OSError(ctypes.get_errno(),'Could not drop namespace capabilities')
    if libc.prctl(38,1,0,0,0):raise OSError(ctypes.get_errno(),'Could not set no-new-privileges')

def runtime_environment(environment):
    exact={'PATH','GLOBAL_MENU_PYTHON','LOCALE_ARCHIVE','TZDIR','LANG','LC_ALL','GI_TYPELIB_PATH','GIO_EXTRA_MODULES','LD_LIBRARY_PATH','FONTCONFIG_FILE','FONTCONFIG_PATH'}
    return {key:value for key,value in environment.items() if key.startswith(('QT_','QML')) or key in exact}

def graphics_policy(value):
    if type(value) is not bool:raise ValueError('software_graphics must be a boolean')
    return value

def software_device():
    """Fixed optional software allocator; never provision or expose GPU devices."""
    path=Path('/dev/udmabuf');info=path.lstat()
    if not stat.S_ISCHR(info.st_mode):raise ValueError('Software allocator must be a non-symlink character device')
    expected='%d:%d'%(os.major(info.st_rdev),os.minor(info.st_rdev))
    fields=dict(line.split('=',1) for line in Path('/sys/class/misc/udmabuf/uevent').read_text().splitlines() if '=' in line)
    if Path('/sys/class/misc/udmabuf/dev').read_text().strip()!=expected or fields.get('DEVNAME')!='udmabuf':raise ValueError('Software allocator identity does not match sysfs')
    if not os.access(path,os.R_OK|os.W_OK):raise PermissionError('Software allocator is not accessible')
    return info.st_dev,info.st_ino,info.st_rdev

def bind_software_device(target):
    expected=software_device();bind('/dev/udmabuf',target)
    bound=Path(target).lstat()
    if not stat.S_ISCHR(bound.st_mode) or (bound.st_dev,bound.st_ino,bound.st_rdev)!=expected or software_device()!=expected:raise ValueError('Software allocator changed during isolation')

def setup(root,source,output,environment,command,software_graphics=False,private_pty=False):
    """Called only after unshare; no mount or network operation on the host."""
    graphics_policy(software_graphics)
    if type(private_pty) is not bool:raise ValueError('private_pty must be a boolean')
    uid,gid=os.getuid(),os.getgid()
    if uid==0:raise ValueError('Browser fixture must retain a non-root identity')
    root,source,output=map(Path,(root,source,output))
    mount(None,'/',flags=MS_PRIVATE|MS_REC)
    mount('tmpfs',root,'tmpfs',MS_NOSUID|MS_NODEV,'mode=0755,size=2G')
    bind('/nix/store',root/'nix/store',True);bind(source,root/'source',True);bind(output,root/'artifacts')
    for name in ('etc','proc','sys','dev','tmp','run','home/fixture'):(root/name).mkdir(parents=True,exist_ok=True)
    (root/'tmp').chmod(0o1777);(root/'home/fixture').chmod(0o700)
    for name in ('null','zero','random','urandom'):bind('/dev/'+name,root/'dev'/name)
    if software_graphics:bind_software_device(root/'dev/udmabuf')
    if private_pty:
        (root/'dev/pts').mkdir()
        mount('devpts',root/'dev/pts','devpts',MS_NOSUID|MS_NOEXEC,'newinstance,ptmxmode=0666,mode=0620,max=64,gid='+str(gid))
        (root/'dev/ptmx').symlink_to('pts/ptmx')
    for name,target in [('fd','/proc/self/fd'),('stdin','/proc/self/fd/0'),('stdout','/proc/self/fd/1'),('stderr','/proc/self/fd/2')]: (root/'dev'/name).symlink_to(target)
    (root/'dev/shm').mkdir();mount('tmpfs',root/'dev/shm','tmpfs',MS_NOSUID|MS_NODEV,'mode=1777,size=256M')
    mount('proc',root/'proc','proc',MS_NOSUID|MS_NODEV|MS_NOEXEC)
    (root/'etc/passwd').write_text('fixture:x:%d:%d:Disposable fixture:/home/fixture:/bin/sh\n'%(uid,gid));(root/'etc/group').write_text('fixture:x:%d:\n'%gid)
    (root/'etc/hosts').write_text('127.0.0.1 localhost fixture.test\n::1 localhost\n');(root/'etc/nsswitch.conf').write_text('passwd: files\ngroup: files\nhosts: files\n');(root/'etc/resolv.conf').write_text('');(root/'etc/machine-id').write_text('0123456789abcdef0123456789abcdef\n')
    loopback()
    if libc.sethostname(b'fixture',7):raise OSError(ctypes.get_errno(),'Could not set private fixture hostname')
    env=runtime_environment(environment)
    env['PATH']=':'.join(value for value in env.get('PATH','').split(':') if value.startswith('/nix/store/'))
    env.update(HOME='/home/fixture',USER='fixture',LOGNAME='fixture',XDG_RUNTIME_DIR='/run/fixture',XDG_CONFIG_HOME='/home/fixture/.config',XDG_DATA_HOME='/home/fixture/.local/share',XDG_STATE_HOME='/home/fixture/.local/state',XDG_CACHE_HOME='/home/fixture/.cache',XDG_DATA_DIRS='/nonexistent',DBUS_SESSION_BUS_ADDRESS='unix:path=/run/fixture/inert-session',DBUS_SYSTEM_BUS_ADDRESS='unix:path=/run/fixture/inert-system',PULSE_SERVER='unix:/run/fixture/inert-audio',PIPEWIRE_REMOTE='inert-pipewire',GTK_A11Y='none',NO_AT_BRIDGE='1',PYTHONWARNINGS='error')
    for key in ('HOME','XDG_RUNTIME_DIR','XDG_CONFIG_HOME','XDG_DATA_HOME','XDG_STATE_HOME','XDG_CACHE_HOME'):(root/env[key].lstrip('/')).mkdir(parents=True,exist_ok=True,mode=0o700)
    old=root/'.old-root';old.mkdir()
    if libc.pivot_root(os.fsencode(root),os.fsencode(old)):raise OSError(ctypes.get_errno(),'Could not pivot disposable filesystem root')
    os.chdir('/')
    if libc.umount2(b'/.old-root',2):raise OSError(ctypes.get_errno(),'Could not detach original filesystem root')
    os.rmdir('/.old-root');os.chdir('/source');drop_capabilities()
    os.execve(command[0],command,env)

def run(source,output,command,environment=None,software_graphics=False,private_pty=False):
    graphics_policy(software_graphics)
    if type(private_pty) is not bool:raise ValueError('private_pty must be a boolean')
    source,output=Path(source).resolve(),Path(output).resolve()
    if not source.is_dir() or not output.is_dir():raise ValueError('Existing source and fresh artifact directories are required')
    info=output.stat()
    if info.st_uid!=os.getuid() or stat.S_IMODE(info.st_mode)&0o022 or any(output.iterdir()):raise ValueError('Artifact directory must be empty, owned and writable only by this user')
    with tempfile.TemporaryDirectory(prefix='lumen-browser-root-') as private:
        descriptor=Path(private)/'launch.json';descriptor.write_text(json.dumps(dict(root=str(Path(private)/'root'),source=str(source),output=str(output),environment=runtime_environment(os.environ if environment is None else environment),command=command,software_graphics=software_graphics,private_pty=private_pty)));(Path(private)/'root').mkdir()
        return subprocess.call(['unshare','--user','--map-current-user','--keep-caps','--mount','--pid','--net','--ipc','--uts','--fork','--kill-child','--mount-proc',sys.executable,str(Path(__file__).resolve()),str(descriptor)])

if __name__=='__main__':setup(**json.loads(Path(sys.argv[1]).read_text()))
