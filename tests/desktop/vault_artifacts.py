"""Credential-free retained diagnostics for disposable vault fixtures."""
from pathlib import Path
import stat
import subprocess
import os,signal
from fixture_resources import close_reaped_pipes

def run_cli(command,environment,timeout=30):
    """Do not let TimeoutExpired render credential-bearing CLI arguments."""
    try:return subprocess.run(command,env=environment,text=True,capture_output=True,timeout=timeout)
    except subprocess.TimeoutExpired:raise RuntimeError('Disposable CLI command exceeded its deadline')from None

def cleanup(https,server,providers,guard,directory):
    """Every retained artifact is audited even when earlier cleanup fails."""
    failures=[]
    if https is not None:
        try:https.close()
        except Exception as error:failures.append('https:'+type(error).__name__)
    children=([server]if server is not None else [])+list(reversed(providers))
    for child in children:
        try:
            if child.poll()is None:os.killpg(child.pid,signal.SIGTERM)
            try:child.wait(timeout=5)
            except subprocess.TimeoutExpired:os.killpg(child.pid,signal.SIGKILL);child.wait(timeout=5)
        except Exception as error:failures.append('child:'+type(error).__name__)
    try:close_reaped_pipes(children)
    except Exception as error:failures.append('pipes:'+type(error).__name__)
    audited=None
    try:audited=guard.audit(directory)
    except Exception as error:failures.append('artifact-audit:'+type(error).__name__)
    if failures:raise RuntimeError('Disposable fixture cleanup failed: '+','.join(failures))from None
    return audited

class ArtifactSecrets:
    def __init__(self):self.values=set()
    def add(self,value):
        if not isinstance(value,str) or not value:raise ValueError('Nonempty disposable sentinel required')
        self.values.add(value)
        return value
    def redact(self,text):
        for value in sorted(self.values,key=len,reverse=True):text=text.replace(value,'[disposable credential redacted]')
        return text
    def audit(self,directory):
        leaks=0;files=0
        for path in Path(directory).rglob('*'):
            info=path.lstat()
            if stat.S_ISDIR(info.st_mode):continue
            if not stat.S_ISREG(info.st_mode) or info.st_size>8*1024*1024:raise ValueError('Unexpected retained vault artifact')
            data=path.read_bytes();files+=1
            if b'-----BEGIN 'in data and b'PRIVATE KEY-----'in data:
                path.write_text('[private key artifact removed]\n');leaks+=1;continue
            cleaned=data
            for value in sorted(self.values,key=len,reverse=True):cleaned=cleaned.replace(value.encode(),b'[disposable credential redacted]')
            if cleaned!=data:path.write_bytes(cleaned);leaks+=1
        if leaks:raise ValueError('Disposable credential appeared in retained artifacts; removed')
        return files
