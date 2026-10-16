"""Opt-in text clipboard history. Ephemeral entries; persistent consent only."""
import argparse
from contextlib import contextmanager
import fcntl
import hashlib
import json
import math
import signal
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
            if enabled:atomic(self.consent,{'version':1,'enabled':True})
            else:
                try:self.consent.unlink()
                except FileNotFoundError:pass
                atomic(self.data,[])
        return self.status()

    def status(self):
        return dict(enabled=self.enabled(),warning=WARNING,entries=[],retentionSeconds=self.retention,maxItems=self.limit,ephemeral=True)

    def ingest(self,text,sensitive=False):
        with self.locked():
            if not self.enabled() or sensitive or not isinstance(text,str) or not text or len(text.encode('utf-8'))>MAX_BYTES or '\x00' in text:
                return dict(recorded=False)
            items=self.entries()
            # Consecutive duplicate selections do not create a new historical copy.
            if items and items[0]['text']==text:return dict(recorded=False)
            items=[dict(id=uuid.uuid4().hex,text=text,timestamp=self.wall(),created_monotonic=self.clock())]+items
            atomic(self.data,items[:self.limit]);return dict(recorded=True)

    def list(self,query=''):
        with self.locked():
            if not self.enabled():return self.status()
            items=self.entries();atomic(self.data,items)
            folded=query.casefold()
            return dict(self.status(),entries=[dict(id=v['id'],text=v['text'][:4096],timestamp=v['timestamp'],truncated=len(v['text'])>4096) for v in items if folded in v['text'].casefold()])

    def clear(self):
        with self.locked():atomic(self.data,[])
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
        executable=os.environ.get('LUMEN_CLIPBOARD_COMMAND')
        command=[executable] if executable and os.path.isabs(executable) else [sys.executable,str(Path(__file__).resolve())]
        child=subprocess.Popen(['wl-paste','--type','text','--watch',*command,'ingest'])
        def stop(*_):raise SystemExit(0)
        signal.signal(signal.SIGTERM,stop)
        try:
            while child.poll() is None and store.enabled():
                store.prune()
                time.sleep(1)
            if child.poll() not in (None,0):raise ValueError('Clipboard watcher stopped')
        finally:
            if not store.enabled():store.prune()
            if child.poll() is None:
                child.terminate()
                try:child.wait(timeout=3)
                except subprocess.TimeoutExpired:child.kill();child.wait()
    elif args.action=='enable':
        store.set_enabled(True)
        try:subprocess.run(['systemctl','--user','start','lumen-clipboard-history.service'],check=True,capture_output=True,timeout=8)
        except (OSError,subprocess.SubprocessError):
            store.set_enabled(False);raise ValueError('Clipboard integration is unavailable; history remains disabled')
        result=store.status()
    elif args.action=='disable':
        result=store.set_enabled(False)
        subprocess.run(['systemctl','--user','stop','lumen-clipboard-history.service'],capture_output=True,timeout=8)
    elif args.action=='clear':result=store.clear()
    elif args.action=='copy':
        result=store.copy(args.id,lambda text:subprocess.run(['wl-copy','--type','text/plain;charset=utf-8'],input=text.encode('utf-8'),check=True,capture_output=True,timeout=5))
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
        print(json.dumps(dict(error='Clipboard operation unavailable; retry or disable history',enabled=False,entries=[])))
        sys.exit(1)
