"""Opt-in text clipboard history. Ephemeral entries; persistent consent only."""
import argparse
from contextlib import contextmanager
import fcntl
import hashlib
import json
import math
import signal
import selectors
import os
from pathlib import Path
import stat
import subprocess
import sys
import tempfile
import time
import uuid

MAX_BYTES=256*1024
def boot_time():
    return time.clock_gettime(time.CLOCK_BOOTTIME) if hasattr(time,'CLOCK_BOOTTIME') else time.monotonic()


WARNING='Copied text may contain secrets. Marked sensitive offers are excluded, but unmarked secrets cannot always be detected. History is local, expires automatically and is cleared at logout.'


def private_directory(path):
    path=Path(path);path.mkdir(parents=True,exist_ok=True,mode=0o700)
    info=path.lstat()
    if not stat.S_ISDIR(info.st_mode) or info.st_uid!=os.getuid():
        raise ValueError('History directory is not a private owned directory')
    path.chmod(0o700)
    return path


def atomic(path,value):
    fd,tmp=tempfile.mkstemp(dir=path.parent,prefix='.pending-')
    try:
        with os.fdopen(fd,'w') as stream:
            os.fchmod(stream.fileno(),0o600);json.dump(value,stream,allow_nan=False);stream.flush();os.fsync(stream.fileno())
        os.replace(tmp,path)
    finally:
        if os.path.exists(tmp):os.unlink(tmp)


def process_identity(pid):
    if type(pid) is not int or pid<=0:return None
    try:
        directory=Path('/proc')/str(pid)
        if directory.stat().st_uid!=os.getuid():return None
        fields=(directory/'stat').read_text().rsplit(') ',1)[1].split()
        if fields[0]=='Z':return None
        return dict(pid=pid,start=int(fields[19]))
    except (OSError,ValueError,IndexError):return None


class History:
    def __init__(self,runtime,state,limit=100,retention=3600,clock=boot_time,wall=time.time):
        if type(limit) is not int or not 1<=limit<=1000 or type(retention) is not int or not 60<=retention<=86400:
            raise ValueError('Invalid history policy')
        self.runtime=private_directory(runtime);self.state=private_directory(state)
        self.data=self.runtime/'entries.json';self.consent=self.state/'consent'
        self.limit,self.retention,self.clock,self.wall=limit,retention,clock,wall

    @contextmanager
    def locked(self):
        fd=os.open(self.runtime/'lock',os.O_CREAT|os.O_RDWR|os.O_NOFOLLOW,0o600)
        try:
            if os.fstat(fd).st_uid!=os.getuid():raise ValueError('Invalid lock owner')
            fcntl.flock(fd,fcntl.LOCK_EX);yield
        finally:os.close(fd)

    def enabled(self):
        try:
            info=self.consent.lstat()
            if not stat.S_ISREG(info.st_mode) or info.st_uid!=os.getuid() or info.st_size>1024 or info.st_mode&0o077:
                return False
            fd=os.open(self.consent,os.O_RDONLY|os.O_NOFOLLOW)
            with os.fdopen(fd) as stream:value=json.load(stream)
            return isinstance(value,dict) and set(value)=={'version','enabled'} and type(value['version']) is int and value['version']==1 and value['enabled'] is True
        except (FileNotFoundError,ValueError,OSError):return False

    def entries(self,include_expired=False):
        try:
            fd=os.open(self.data,os.O_RDONLY|os.O_NOFOLLOW)
            with os.fdopen(fd) as stream:
                info=os.fstat(stream.fileno())
                if not stat.S_ISREG(info.st_mode) or info.st_uid!=os.getuid() or info.st_mode&0o077 or info.st_size>(MAX_BYTES*6+1024)*self.limit:
                    raise ValueError('Invalid history storage')
                value=json.load(stream)
        except FileNotFoundError:return []
        if not isinstance(value,list) or len(value)>self.limit:raise ValueError('Invalid history data')
        ids=set()
        for row in value:
            if not isinstance(row,dict) or set(row)!={'id','text','timestamp','created_monotonic'}:
                raise ValueError('Invalid history row')
            if not isinstance(row['id'],str) or len(row['id'])!=32 or any(c not in '0123456789abcdef' for c in row['id']) or row['id'] in ids:
                raise ValueError('Invalid history identity')
            ids.add(row['id'])
            if not isinstance(row['text'],str) or not row['text'] or '\x00' in row['text'] or len(row['text'].encode('utf-8'))>MAX_BYTES:
                raise ValueError('Invalid history text')
            if any(type(row[k]) not in (int,float) or not math.isfinite(row[k]) or row[k]<0 for k in ('timestamp','created_monotonic')):
                raise ValueError('Invalid history time')
        now=self.clock()
        return value if include_expired else [v for v in value if 0<=now-v['created_monotonic']<self.retention][:self.limit]

    def prune(self):
        with self.locked():
            raw=self.entries(include_expired=True)
            now=self.clock()
            items=[v for v in raw if 0<=now-v['created_monotonic']<self.retention] if self.enabled() else []
            if items!=raw:atomic(self.data,items)
            return len(items)

    def set_enabled(self,enabled):
        with self.locked():
            if enabled:
                if not self.enabled():self.advance_barrier()
                atomic(self.consent,{'version':1,'enabled':True})
            else:
                self.advance_barrier()
                try:self.consent.unlink()
                except FileNotFoundError:pass
                atomic(self.data,[])
        return self.status()

    def observer_status(self):
        try:
            fd=os.open(self.runtime/'observer.json',os.O_RDONLY|os.O_NOFOLLOW)
            with os.fdopen(fd) as stream:
                info=os.fstat(stream.fileno())
                if not stat.S_ISREG(info.st_mode) or info.st_uid!=os.getuid() or info.st_mode&0o077 or info.st_size>1024:return None
                value=json.load(stream)
            if not isinstance(value,dict) or set(value)!={'supervisor','observer','ready','epoch'} or type(value['ready']) is not bool or not isinstance(value['epoch'],str):return None
            for role in ('supervisor','observer'):
                identity=value[role]
                if not isinstance(identity,dict) or set(identity)!={'pid','start'} or type(identity['pid']) is not int or type(identity['start']) is not int:return None
            return value
        except (OSError,ValueError):return None

    def collecting(self):
        value=self.observer_status()
        return bool(value and value['ready'] and value['epoch']==self.barrier()['epoch'] and all(process_identity(value[role]['pid'])==value[role] for role in ('supervisor','observer')))

    def publish_observer(self,owner,child,ready,epoch):
        with self.locked():atomic(self.runtime/'observer.json',dict(supervisor=owner,observer=child,ready=ready,epoch=epoch))

    def retire_observer(self,owner):
        with self.locked():
            value=self.observer_status()
            if value and value['supervisor']==owner:
                try:(self.runtime/'observer.json').unlink()
                except FileNotFoundError:pass

    def await_observer(self):
        deadline=time.monotonic()+3
        while self.enabled() and not self.collecting() and time.monotonic()<deadline:time.sleep(.03)
        return self.collecting()

    def status(self):
        return dict(enabled=self.enabled(),collecting=self.enabled() and self.collecting(),warning=WARNING,entries=[],retentionSeconds=self.retention,maxItems=self.limit,ephemeral=True)

    def advance_barrier(self):
        atomic(self.runtime/'clear-state.json',dict(time=self.clock(),epoch=uuid.uuid4().hex))

    def barrier(self):
        try:fd=os.open(self.runtime/'clear-state.json',os.O_RDONLY|os.O_NOFOLLOW)
        except FileNotFoundError:return dict(time=0,epoch="")
        with os.fdopen(fd) as stream:
            info=os.fstat(stream.fileno())
            if not stat.S_ISREG(info.st_mode) or info.st_uid!=os.getuid() or info.st_mode&0o077 or info.st_size>128:raise ValueError('Invalid clear barrier')
            value=json.load(stream)
        if not isinstance(value,dict) or set(value)!={'time','epoch'} or type(value['time']) not in (int,float) or not math.isfinite(value['time']) or value['time']<0 or not isinstance(value['epoch'],str) or len(value['epoch'])!=32 or any(c not in '0123456789abcdef' for c in value['epoch']):raise ValueError('Invalid clear barrier')
        return value

    def ingest(self,text,sensitive=False,captured_at=None,epoch=None):
        with self.locked():
            now=self.clock();barrier=self.barrier()
            if epoch is not None and epoch!=barrier['epoch']:return dict(recorded=False)
            if captured_at is not None and (not math.isfinite(captured_at) or captured_at<barrier['time'] or not 0<=now-captured_at<self.retention):return dict(recorded=False)
            if not self.enabled() or sensitive or not isinstance(text,str) or not text or len(text.encode('utf-8'))>MAX_BYTES or '\x00' in text:
                return dict(recorded=False)
            items=self.entries()
            # Consecutive duplicate selections do not create a new historical copy.
            if items and items[0]['text']==text:return dict(recorded=False)
            items=[dict(id=uuid.uuid4().hex,text=text,timestamp=self.wall(),created_monotonic=now if captured_at is None else captured_at)]+items
            atomic(self.data,items[:self.limit]);return dict(recorded=True)

    def list(self,query=''):
        with self.locked():
            if not self.enabled():return self.status()
            items=self.entries();atomic(self.data,items)
            folded=query.casefold()
            return dict(self.status(),entries=[dict(id=v['id'],text=v['text'][:4096],timestamp=v['timestamp'],truncated=len(v['text'])>4096) for v in items if folded in v['text'].casefold()])

    def clear(self):
        with self.locked():
            self.advance_barrier();atomic(self.data,[])
        return self.status()

    def copy(self,item_id,writer):
        with self.locked():
            if not self.enabled():raise ValueError('Clipboard history is disabled')
            item=next((v for v in self.entries() if v['id']==item_id),None)
            if not item:raise ValueError('Clipboard item expired or was cleared')
            # Hold the lock through copy so a concurrent Clear cannot replay stale text.
            writer(item['text'])
        return dict(self.status(),copied=True)


def history():
    runtime=os.environ.get('XDG_RUNTIME_DIR')
    if not runtime:raise ValueError('A private user runtime directory is required')
    info=Path(runtime).lstat()
    if not stat.S_ISDIR(info.st_mode) or info.st_uid!=os.getuid() or info.st_mode&0o077:raise ValueError('User runtime root must be private and owned')
    state=Path(os.environ.get('XDG_STATE_HOME',str(Path.home()/'.local/state')))
    return History(Path(runtime)/'lumen-desktop/clipboard',state/'lumen-desktop/clipboard',
                   int(os.environ.get('LUMEN_CLIPBOARD_MAX_ITEMS','100')),int(os.environ.get('LUMEN_CLIPBOARD_RETENTION_SECONDS','3600')))


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('action',choices=['status','list','enable','disable','clear','copy','ingest','watch'])
    parser.add_argument('id',nargs='?');parser.add_argument('--query',default='');args=parser.parse_args();store=history()
    if args.action=='watch':
        if not store.enabled():store.prune();return
        executable=os.environ.get('LUMEN_CLIPBOARD_OBSERVER','lumen-clipboard-observer')
        child=subprocess.Popen([executable],stdout=subprocess.PIPE,stderr=subprocess.DEVNULL)
        def stop(*_):raise SystemExit(0)
        signal.signal(signal.SIGTERM,stop)
        # Only the owned observer writes this bounded private binary pipe.
        received=bytearray();ready=False;deadline=time.monotonic()+3
        owner=process_identity(os.getpid());observer=process_identity(child.pid);epoch=store.barrier()['epoch']
        try:
            if not owner or not observer:raise ValueError('Clipboard process identity unavailable')
            store.publish_observer(owner,observer,False,epoch)
            with selectors.DefaultSelector() as events:
                events.register(child.stdout,selectors.EVENT_READ)
                while child.poll() is None and store.enabled():
                    store.prune()
                    if not ready and time.monotonic()>deadline:raise ValueError('Clipboard observer unavailable')
                    if not events.select(.5):continue
                    data=os.read(child.stdout.fileno(),65536)
                    if not data:break
                    received.extend(data)
                    while len(received)>=5:
                        kind,size=received[0],int.from_bytes(received[1:5],'big')
                        if size>MAX_BYTES+8 or kind not in (1,2) or (kind==1 and (ready or size)) or (kind==2 and (not ready or size<8)):raise ValueError('Invalid clipboard transport')
                        if len(received)<5+size:break
                        payload=bytes(received[5:5+size]);del received[:5+size]
                        if kind==1:
                            ready=True;store.publish_observer(owner,observer,True,epoch);print(json.dumps(dict(watching=True)),flush=True)
                        else:
                            captured_at=int.from_bytes(payload[:8],'big')/1000000000
                            try:text=payload[8:].decode('utf-8')
                            except UnicodeDecodeError:continue
                            print(json.dumps(store.ingest(text,captured_at=captured_at,epoch=epoch)),flush=True)
            if store.enabled():raise ValueError('Clipboard observer stopped')
        finally:
            try:
                if child.poll() is None:
                    child.terminate()
                    try:child.wait(timeout=3)
                    except subprocess.TimeoutExpired:child.kill();child.wait()
            finally:
                child.stdout.close();store.retire_observer(owner)
            if not store.enabled():store.prune()
        return
    elif args.action=='enable':
        new_consent=not store.enabled();store.set_enabled(True)
        try:
            if new_consent:subprocess.run(['systemctl','--user','stop','lumen-clipboard-history.service'],check=True,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL,timeout=8)
            subprocess.run(['systemctl','--user','start','lumen-clipboard-history.service'],check=True,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL,timeout=8)
            if store.enabled() and not store.await_observer():raise ValueError('Clipboard observer unavailable')
        except (OSError,ValueError,subprocess.SubprocessError):
            store.set_enabled(False);raise ValueError('Clipboard integration is unavailable; history remains disabled')
        result=store.status()
    elif args.action=='disable':
        result=store.set_enabled(False)
        subprocess.run(['systemctl','--user','stop','lumen-clipboard-history.service'],capture_output=True,timeout=8)
    elif args.action=='clear':
        enabled=store.enabled()
        try:
            if enabled:subprocess.run(['systemctl','--user','stop','lumen-clipboard-history.service'],check=True,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL,timeout=8)
            result=store.clear()
            # Revocation during the transaction must never be turned back on.
            if enabled and store.enabled():subprocess.run(['systemctl','--user','start','lumen-clipboard-history.service'],check=True,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL,timeout=8)
            if enabled and store.enabled():store.await_observer()
            result=store.status()
        except (OSError,subprocess.SubprocessError):
            store.set_enabled(False);store.clear()
            raise ValueError('Clipboard collection unavailable; history cleared and disabled')
    elif args.action=='copy':
        result=store.copy(args.id,lambda text:subprocess.run(['wl-copy','--type','text/plain;charset=utf-8'],input=text.encode('utf-8'),check=True,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL,timeout=5))
    elif args.action=='ingest':
        # Offer state and password-manager MIME markers are best-effort protection.
        text=sys.stdin.buffer.read(MAX_BYTES+1)
        types=subprocess.run(['wl-paste','--list-types'],capture_output=True,text=True,timeout=3)
        sensitive=types.returncode!=0 or os.environ.get('CLIPBOARD_STATE') in ('sensitive','clear','nil') or any(marker in types.stdout for marker in ('x-kde-passwordManagerHint','application/x-keepassxc'))
        try:value=text.decode('utf-8')
        except UnicodeDecodeError:value=None
        result=store.ingest(value,sensitive)
    elif args.action=='list':result=store.list(args.query)
    else:result=store.status()
    print(json.dumps(result,allow_nan=False))


if __name__=='__main__':
    try:main()
    except (OSError,ValueError,KeyError,TypeError,UnicodeError,subprocess.SubprocessError):
        # Never log the copied content or external command's output.
        try:state=history().status()
        except (OSError,ValueError,TypeError):state=dict(entries=[])
        print(json.dumps(dict(state,error='Clipboard operation unavailable; retry or disable history')))
        sys.exit(1)
