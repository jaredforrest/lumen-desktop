"""Bounded immutable preview resources with explicit cooperative leases.

The owner supplies identity(path, optional_stat) and validate(path, token).
Resources are never evicted while a lease or worker pin remains. The owner
releases only after cooperating UI sources retire; internal decoder completion
is not inferred. Exhaustion returns unavailable rather than evicting a lease.
"""
import atexit,contextlib,os,re,secrets,shutil,stat,tempfile,threading,time
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
        self.changed=threading.Condition(self.lock)
        self.leases={};self.references={};self.sizes={}
        self.provider_lease=self.begin()
    def begin(self):
        with self.lock:
            lease=secrets.token_hex(24)
            self.leases[lease]=dict(tokens=set(),pins=0,retired=False)
            return lease
    def live_locked(self,lease):
        value=self.leases.get(lease)
        if value is None or value['retired']:raise ValueError('The preview request was retired')
        return value
    @contextlib.contextmanager
    def pin(self,lease):
        with self.lock:self.live_locked(lease)['pins']+=1
        try:yield
        finally:
            with self.lock:
                value=self.leases[lease];value['pins']-=1
                if value['retired'] and not value['pins']:self.remove_locked(lease)
                self.changed.notify_all()
    def attach_locked(self,lease,token):
        value=self.live_locked(lease)
        value['tokens'].add(token);self.references.setdefault(token,set()).add(lease)
    def detach_locked(self,lease,token):
        self.leases[lease]['tokens'].discard(token)
        references=self.references[token];references.discard(lease)
        if not references:
            self.preview_snapshots.pop(token).unlink(missing_ok=True)
            self.preview_bytes-=self.sizes.pop(token);self.references.pop(token)
    def remove_locked(self,lease):
        for token in list(self.leases[lease]['tokens']):self.detach_locked(lease,token)
        self.leases.pop(lease)
    def retain(self,lease,token):
        with self.lock:
            if token not in self.preview_snapshots:raise ValueError('The preview resource was retired')
            self.attach_locked(lease,token)
    def keep(self,lease,paths):
        """Drop completed helper inputs; keep only this lease's published paths."""
        paths={Path(path) for path in paths}
        with self.lock:
            value=self.live_locked(lease)
            for token in list(value['tokens']):
                if self.preview_snapshots[token] not in paths:self.detach_locked(lease,token)
    def release(self,lease):
        """Caller has retired every UI source; active worker pins drain first."""
        with self.lock:
            value=self.leases.get(lease)
            if value is None:return True
            value['retired']=True
            if not value['pins']:self.remove_locked(lease)
            self.changed.notify_all()
            return lease not in self.leases
    def wait_released(self,lease,timeout):
        with self.changed:return self.changed.wait_for(lambda:lease not in self.leases,timeout)
    def capture(self,path,token,lease=None):
        with self.lock:return self.capture_locked(Path(path),token,lease or self.provider_lease)
    def close(self):
        """Owner calls only after its published resources have been retired."""
        with self.lock:
            if any(value['pins'] for value in self.leases.values()):raise ValueError('Preview workers are still using resources')
            if self.preview_directory is not None:shutil.rmtree(self.preview_directory,ignore_errors=True)
            self.preview_directory=None;self.preview_snapshots={};self.preview_bytes=0
            self.leases={};self.references={};self.sizes={}

    def capture_locked(self, path, token, lease):
        self.live_locked(lease)
        if token in self.preview_snapshots:
            self.attach_locked(lease,token)
            return self.preview_snapshots[token]
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
                raise ValueError('The private preview working set is full')
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
            self.sizes[token]=size;self.attach_locked(lease,token)
            return temporary
        except BaseException:
            if temporary is not None:temporary.unlink(missing_ok=True)
            raise
        finally:
            if fd is not None:os.close(fd)
            os.close(path_fd)

class PreviewRequests:
    """Provider-scoped request leases and retained immutable source ownership.

    Intake registers requests before dispatch. A cooperative release acknowledges
    retired UI sources and waits for that request's worker pin to drain. Keeping
    a source across paging reserves only that source, never the previous raster.
    """
    def __init__(self,resources):
        self.resources=resources;self.generation=secrets.token_hex(24)
        self.lock=threading.RLock();self.jobs={};self.sources={};self.reserved=set();self.last_id=0
    def drop_source_locked(self,token):
        source=self.sources.get(token)
        if source and not source['owners'] and token not in self.reserved:
            self.resources.release(source['lease']);self.sources.pop(token)
    def register(self,request_id,token=None):
        with self.lock:
            if type(request_id) is not int or request_id<=self.last_id:raise ValueError('Invalid preview request identity')
            if len(self.jobs)>=64:raise ValueError('Too many outstanding preview requests')
            self.last_id=request_id
            for old in list(self.reserved):
                self.reserved.remove(old)
                if old!=token:self.drop_source_locked(old)
            source=token if token in self.sources else None
            self.jobs[request_id]=dict(lease=self.resources.begin(),source=source,retired=False)
            if source:self.sources[source]['owners'].add(request_id)
    @contextlib.contextmanager
    def work(self,request_id):
        with self.lock:
            job=self.jobs.get(request_id)
            if job is None or job['retired']:raise ValueError('The preview request was retired')
            lease=job['lease']
        with self.resources.pin(lease):yield lease
    def retain_source(self,request_id,token):
        with self.lock:
            job=self.jobs.get(request_id)
            if job is None or job['retired']:raise ValueError('The preview request was retired')
            source=self.sources.get(token)
            if source is None:
                lease=self.resources.begin()
                try:self.resources.retain(lease,token)
                except BaseException:self.resources.release(lease);raise
                source=self.sources[token]=dict(lease=lease,owners=set())
            if job['source'] and job['source']!=token:raise ValueError('Preview source identity changed')
            source['owners'].add(request_id);job['source']=token
    def finish(self,request_id,published_paths):
        with self.lock:
            job=self.jobs.get(request_id)
            if job is None or job['retired']:raise ValueError('The preview request was retired')
            self.resources.keep(job['lease'],published_paths)
    def retire(self,request_ids,keep_source_token=None,timeout=9,wait=True):
        leases=[]
        with self.lock:
            for request_id in request_ids:
                if type(request_id) is not int:raise ValueError('Invalid preview release identity')
                job=self.jobs.get(request_id)
                if job is None:continue
                leases.append((request_id,job['lease']))
                if job['retired']:continue
                job['retired']=True
                token=job['source']
                if token:
                    source=self.sources[token];source['owners'].discard(request_id)
                    if keep_source_token and token==keep_source_token:self.reserved.add(token)
                    else:self.reserved.discard(token)
                    self.drop_source_locked(token)
                self.resources.release(job['lease'])
        if not wait:return
        deadline=time.monotonic()+timeout
        for request_id,lease in leases:
            if not self.resources.wait_released(lease,max(0,deadline-time.monotonic())):raise ValueError('Preview retirement is still waiting for its worker')
            with self.lock:
                if self.jobs.get(request_id,{}).get('lease')==lease:self.jobs.pop(request_id)
    def close(self):
        # Provider teardown is legal only after UI retirement and worker join.
        self.resources.close()
        with self.lock:self.jobs={};self.sources={};self.reserved=set()
