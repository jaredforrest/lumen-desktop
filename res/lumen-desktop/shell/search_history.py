"""Opt-in recent query history. Clipboard searches are excluded by the caller."""
import contextlib
import fcntl
import json
import os
import secrets
from pathlib import Path
import stat
import sys
import tempfile
from nix_settings import read_settings

LIMIT=20
MAX_QUERY=512

def private(path,directory=False):
    info=path.lstat()
    if info.st_uid!=os.getuid() or (not stat.S_ISDIR(info.st_mode) if directory else not stat.S_ISREG(info.st_mode)) or info.st_mode & 0o077:
        raise ValueError('Search history requires an owned private '+('directory' if directory else 'file'))

def enabled():
    return read_settings().get('search',{}).get('queryHistory') is True

@contextlib.contextmanager
def store():
    root=Path(os.environ.get('XDG_STATE_HOME',str(Path.home()/'.local/state')))/'lumen-desktop/search'
    root.mkdir(mode=0o700,parents=True,exist_ok=True);private(root,True)
    lock=root/'history.lock'
    descriptor=os.open(lock,os.O_CREAT|os.O_RDWR|os.O_NOFOLLOW,0o600)
    with os.fdopen(descriptor,'r+') as stream:
        private(lock);fcntl.flock(stream,fcntl.LOCK_EX)
        yield root/'queries.json'

def read(path):
    if not path.exists():return []
    private(path)
    if path.stat().st_size>65536:raise ValueError('Search history exceeds its size limit')
    values=json.loads(path.read_text())
    if not isinstance(values,list) or len(values)>LIMIT or any(not isinstance(v,str) or not v or len(v)>MAX_QUERY or any(ord(c)<32 for c in v) for v in values):
        raise ValueError('Search history is malformed; clear it to reset')
    return values

def write(path,values):
    descriptor,temporary=tempfile.mkstemp(prefix='.queries-',dir=path.parent)
    try:
        with os.fdopen(descriptor,'w') as stream:
            json.dump(values,stream,ensure_ascii=False);stream.flush();os.fsync(stream.fileno())
        os.replace(temporary,path)
    finally:
        Path(temporary).unlink(missing_ok=True)

def handle(data):
    if not isinstance(data,dict):raise ValueError('A search-history request is required')
    action=data.get('action')
    if action not in ('list','record','clear'):raise ValueError('Unsupported search-history action')
    with store() as path:
        allowed=enabled()
        epoch_path=path.with_name('generation.json')
        if epoch_path.exists():
            private(epoch_path)
            if epoch_path.stat().st_size>128:raise ValueError('Invalid history generation')
            generation=json.loads(epoch_path.read_text())
            if not isinstance(generation,str) or len(generation)!=64 or any(c not in '0123456789abcdef' for c in generation):raise ValueError('Invalid history generation')
        else:
            generation=secrets.token_hex(32);write(epoch_path,generation)
        if action=='clear' or not allowed:
            # Disabling the preference also removes the previous history on the
            # next history request; never return old text while disabled.
            generation=secrets.token_hex(32);write(epoch_path,generation)
            path.unlink(missing_ok=True)
            return {'enabled':allowed,'queries':[],'generation':generation}
        values=read(path)
        if action=='record':
            if data.get('generation')!=generation:
                return {'enabled':allowed,'queries':values[:LIMIT],'generation':generation,'retired':True}
            query=data.get('query')
            if not isinstance(query,str):raise ValueError('A query string is required')
            query=query.strip()
            if len(query)>MAX_QUERY or any(ord(c)<32 for c in query):raise ValueError('Query cannot be saved to history')
            if query:values=[query]+[old for old in values if old!=query];write(path,values[:LIMIT])
        return {'enabled':allowed,'queries':values[:LIMIT],'generation':generation}

def main():
    line=sys.stdin.readline(65537)
    if len(line)>65536:raise ValueError('Search-history request is too large')
    print(json.dumps(handle(json.loads(line)),ensure_ascii=True))
if __name__=='__main__':
    try:main()
    except (OSError,ValueError,TypeError) as error:
        print(json.dumps({'error':str(error)}));sys.exit(1)
