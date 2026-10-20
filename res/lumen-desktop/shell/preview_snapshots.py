"""Bounded immutable owned-preview resources, retained for the provider lifetime.

The owner supplies identity(path, optional_stat) and validate(path, token).
Published resources are never evicted while a client could still be loading them;
exhaustion returns unavailable rather than replacing a retained resource.
"""
import atexit,os,re,shutil,stat,tempfile,threading
from pathlib import Path
def parse_process_start(value):
    start=value.rsplit(') ',1)[1].split()[19]
    if not start.isdigit():raise ValueError('Invalid process start identity')
    return start

def process_start(pid):
    try:return parse_process_start(Path('/proc/'+str(pid)+'/stat').read_text())
    except FileNotFoundError:return None

class PreviewSnapshots:
    def __init__(self,identity,validate):
        self.identity,self.validate=identity,validate
        self.preview_directory=None;self.preview_snapshots={};self.preview_bytes=0
        self.lock=threading.Lock()
    def capture(self,path,token):
        with self.lock:return self.capture_locked(Path(path),token)
    def close(self):
        """Owner calls only after its published resources have been retired."""
        with self.lock:
            if self.preview_directory is not None:shutil.rmtree(self.preview_directory,ignore_errors=True)
            self.preview_directory=None;self.preview_snapshots={};self.preview_bytes=0

    def capture_locked(self, path, token):
        if token in self.preview_snapshots:return self.preview_snapshots[token]
        limit=64*1024*1024
        path_fd=os.open(path,os.O_PATH|os.O_NOFOLLOW)
        fd=None;temporary=None
        try:
            info=os.fstat(path_fd)
            if not stat.S_ISREG(info.st_mode) or info.st_size>limit or self.identity(path,info)!=token:
                raise ValueError('The selected preview is unavailable or too large')
            # Only this retained, proven regular descriptor is reopened for data.
            fd=os.open('/proc/self/fd/'+str(path_fd),os.O_RDONLY|os.O_NONBLOCK)
            if self.identity(path,os.fstat(fd))!=token:raise ValueError('The selected preview changed')
            if self.preview_bytes+info.st_size>128*1024*1024:
                raise ValueError('The private preview budget is full; close and reopen this preview provider')
            if self.preview_directory is None:
                runtime_value=os.environ.get('XDG_RUNTIME_DIR')
                if not runtime_value:raise ValueError('A private session runtime directory is required')
                runtime=Path(runtime_value);runtime_info=runtime.lstat()
                if not stat.S_ISDIR(runtime_info.st_mode) or runtime_info.st_uid!=os.getuid() or runtime_info.st_mode&0o077:
                    raise ValueError('A private session runtime directory is required')
                base=runtime/('lumen-preview-snapshots-'+str(os.getuid()))
                base.mkdir(mode=0o700,exist_ok=True)
                facts=base.lstat()
                if not stat.S_ISDIR(facts.st_mode) or facts.st_uid!=os.getuid() or facts.st_mode&0o077:
                    raise ValueError('Private preview storage is unavailable')
                # Retired subprocesses can leave runtime snapshots after SIGKILL.
                # Only this feature's dead-owner directories are removed.
                for old in base.iterdir():
                    match=re.fullmatch(r'([0-9]+)-([0-9]+)-[a-z0-9_]{8}',old.name)
                    if not match or not old.is_dir() or old.is_symlink():continue
                    try:
                        if process_start(int(match[1]))!=match[2]:shutil.rmtree(old,ignore_errors=True)
                    except (OSError,ValueError):pass
                start=process_start(os.getpid())
                if start is None:raise ValueError('Preview owner identity is unavailable')
                directory=Path(tempfile.mkdtemp(prefix=str(os.getpid())+'-'+start+'-',dir=base))
                atexit.register(shutil.rmtree,directory,ignore_errors=True)
                self.preview_directory=directory
            handle,tempname=tempfile.mkstemp(dir=self.preview_directory,suffix=path.suffix)
            temporary=Path(tempname)
            with os.fdopen(handle,'wb') as destination:
                remaining=limit+1
                while remaining:
                    chunk=os.read(fd,min(1024*1024,remaining))
                    if not chunk:break
                    destination.write(chunk);remaining-=len(chunk)
                size=destination.tell()
                if size>limit or size!=info.st_size or self.identity(path,os.fstat(fd))!=token:
                    raise ValueError('The selected preview changed')
            self.validate(str(path),token)
            temporary.chmod(0o400)
            self.preview_snapshots[token]=temporary;self.preview_bytes+=size
            return temporary
        except BaseException:
            if temporary is not None:temporary.unlink(missing_ok=True)
            raise
        finally:
            if fd is not None:os.close(fd)
            os.close(path_fd)

