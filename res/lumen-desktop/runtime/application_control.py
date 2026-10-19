"""One-use, confirmed termination of exact native-window process identities."""
import argparse
import contextlib
import fcntl
import json
import math
import os
from pathlib import Path
import re
import signal
import stat
import subprocess
import sys
import tempfile
import time
import uuid

from protocol import loads
from state import runtime_epoch

MAX_WINDOWS=100
LIFETIME=60


def now():return time.clock_gettime(time.CLOCK_BOOTTIME)


def window_nodes(tree):
    result=[];pending=[tree];seen=set()
    while pending:
        if len(seen)>100000:raise ValueError('Compositor tree exceeds the supported limit')
        node=pending.pop()
        if not isinstance(node,dict):raise ValueError('Invalid compositor tree')
        ident=node.get('id')
        if ident is not None:
            if type(ident) is not int or ident in seen:raise ValueError('Invalid compositor identity')
            seen.add(ident)
        for key in ('nodes','floating_nodes'):
            children=node.get(key,[])
            if not isinstance(children,list):raise ValueError('Invalid compositor children')
            pending.extend(children)
        if node.get('app_id') is not None or node.get('window') is not None:result.append(node)
    return result


class Backend:
    def tree(self):
        socket=os.environ.get('SWAYSOCK')
        if not socket:raise ValueError('No supported compositor session')
        epoch=runtime_epoch(socket)
        tree=loads(subprocess.check_output(['swaymsg','-r','-t','get_tree'],timeout=5))
        if runtime_epoch(socket)!=epoch:raise ValueError('Compositor changed; select the application again')
        return epoch,window_nodes(tree)

    def identity(self,pid):
        if type(pid) is not int or pid<2:raise ValueError('Invalid application process')
        root=Path('/proc')/str(pid)
        if root.stat().st_uid!=os.getuid():raise ValueError('Application process has an unexpected owner')
        values=(root/'stat').read_text().rsplit(')',1)[1].split()
        if len(values)<20 or values[0]=='Z':raise ValueError('Application process has already exited')
        status=(root/'status').read_text()
        uids=next((line.split()[1:] for line in status.splitlines() if line.startswith('Uid:')),[])
        if len(uids)!=4 or any(int(uid)!=os.getuid() for uid in uids):raise ValueError('Privileged application processes cannot be terminated here')
        executable=root/'exe';info=executable.stat()
        if 'quickshell' in executable.resolve().name:raise ValueError('The desktop shell cannot be force quit')
        return {'pid':pid,'start':int(values[19]),'device':info.st_dev,'inode':info.st_ino}

    def open(self,pid):
        if not hasattr(os,'pidfd_open') or not hasattr(signal,'pidfd_send_signal'):
            raise ValueError('Safe process handles are unavailable on this system')
        return os.pidfd_open(pid,0)
    def close(self,fd):os.close(fd)
    def terminate(self,fd):signal.pidfd_send_signal(fd,signal.SIGKILL)


class ApplicationControl:
    def __init__(self,directory,backend=None,clock=now):
        self.directory=Path(directory);self.directory.mkdir(parents=True,mode=0o700,exist_ok=True)
        info=self.directory.lstat()
        if not stat.S_ISDIR(info.st_mode) or info.st_uid!=os.getuid():raise ValueError('Invalid confirmation directory')
        self.directory.chmod(0o700);self.backend=backend or Backend();self.clock=clock

    @contextlib.contextmanager
    def locked(self):
        fd=os.open(self.directory/'lock',os.O_CREAT|os.O_RDWR|os.O_NOFOLLOW,0o600)
        try:
            info=os.fstat(fd)
            if not stat.S_ISREG(info.st_mode) or info.st_uid!=os.getuid() or info.st_mode&0o077:raise ValueError('Invalid confirmation lock')
            fcntl.flock(fd,fcntl.LOCK_EX);yield
        finally:os.close(fd)

    def receipt(self,token):
        if not isinstance(token,str) or not re.fullmatch('[0-9a-f]{32}',token):raise ValueError('Invalid confirmation identity')
        return self.directory/token

    def read(self,token):
        fd=os.open(self.receipt(token),os.O_RDONLY|os.O_NOFOLLOW)
        with os.fdopen(fd,'rb') as stream:
            info=os.fstat(stream.fileno())
            if not stat.S_ISREG(info.st_mode) or info.st_uid!=os.getuid() or info.st_mode&0o077 or info.st_size>131072:raise ValueError('Invalid confirmation receipt')
            value=loads(stream.read())
        if not isinstance(value,dict) or set(value)!={'version','created','epoch','windows','processes'} or type(value['version']) is not int or value['version']!=1:
            raise ValueError('Invalid confirmation receipt')
        if type(value['created']) not in (int,float) or not isinstance(value['epoch'],str) or not value['epoch'] or len(value['epoch'])>256:raise ValueError('Invalid confirmation time or session')
        age=self.clock()-value['created']
        if not math.isfinite(age) or not 0<=age<LIFETIME:raise ValueError('Confirmation expired; select the application again')
        if not isinstance(value['windows'],list) or not 1<=len(value['windows'])<=MAX_WINDOWS or not isinstance(value['processes'],list) or not 1<=len(value['processes'])<=MAX_WINDOWS:
            raise ValueError('Invalid confirmation processes')
        for window in value['windows']:
            if not isinstance(window,dict) or set(window)!={'id','pid','app_id'} or any(type(window.get(key)) is not int or window[key]<1 for key in ('id','pid')) or not isinstance(window.get('app_id'),str) or not 1<=len(window['app_id'])<=256 or window['app_id']=='org.quickshell':raise ValueError('Invalid confirmation window')
        for process in value['processes']:
            if not isinstance(process,dict) or set(process)!={'pid','start','device','inode'} or any(type(item) is not int or item<0 for item in process.values()) or process['pid']<2:raise ValueError('Invalid confirmation process')
        if len({window['id'] for window in value['windows']})!=len(value['windows']) or len({process['pid'] for process in value['processes']})!=len(value['processes']) or {window['pid'] for window in value['windows']}!={process['pid'] for process in value['processes']}:raise ValueError('Confirmation process membership is inconsistent')
        return value

    def prepare(self,descriptors):
        if not isinstance(descriptors,list) or not 1<=len(descriptors)<=MAX_WINDOWS:raise ValueError('Select a running application with native windows')
        epoch,nodes=self.backend.tree();selected=[]
        for descriptor in descriptors:
            if not isinstance(descriptor,dict) or set(descriptor)!={'appId','title'} or not isinstance(descriptor.get('appId'),str) or not 1<=len(descriptor['appId'])<=256 or not isinstance(descriptor.get('title'),str) or len(descriptor['title'])>8192:raise ValueError('Invalid selected window')
            matches=[node for node in nodes if node.get('app_id')==descriptor['appId'] and node.get('name')==descriptor['title']]
            if len(matches)!=1:raise ValueError('The selected window changed or is ambiguous; select it again')
            node=matches[0]
            if node.get('shell')!='xdg_shell' or node.get('app_id')=='org.quickshell':raise ValueError('Force Quit is available only for external native applications')
            if type(node.get('pid')) is not int or node['pid']<2:raise ValueError('Application process identity is unavailable')
            if any(prior['id']==node['id'] for prior in selected):raise ValueError('A window was selected more than once')
            selected.append({'id':node['id'],'pid':node.get('pid'),'app_id':node['app_id']})
        if len({window['app_id'] for window in selected})!=1:raise ValueError('Select windows from one application at a time')
        selected_ids={window['id'] for window in selected};selected_pids={window['pid'] for window in selected}
        if any(node.get('pid') in selected_pids and node.get('id') not in selected_ids for node in nodes):raise ValueError('This process also owns unselected windows; select the whole application or close those windows first')
        processes=[self.backend.identity(pid) for pid in sorted(selected_pids)]
        value={'version':1,'created':self.clock(),'epoch':epoch,'windows':selected,'processes':processes}
        with self.locked():
            # Receipts expire quickly and never survive logout. Remove abandoned
            # confirmations without inspecting unrelated runtime files.
            for path in self.directory.iterdir():
                if re.fullmatch('[0-9a-f]{32}',path.name) and path.lstat().st_mtime<time.time()-LIFETIME:path.unlink()
            token=uuid.uuid4().hex;fd=os.open(self.receipt(token),os.O_WRONLY|os.O_CREAT|os.O_EXCL|os.O_NOFOLLOW,0o600)
            with os.fdopen(fd,'w') as stream:json.dump(value,stream,allow_nan=False)
        return {'token':token,'processCount':len(processes),'windowCount':len(selected),'expiresIn':LIFETIME}

    def cancel(self,token):
        with self.locked():self.receipt(token).unlink(missing_ok=True)
        return {'cancelled':True}

    def confirm(self,token):
        with self.locked():
            value=self.read(token)
            # Consume before any side effect; failures require a fresh selection.
            self.receipt(token).unlink()
            epoch,nodes=self.backend.tree()
            if epoch!=value['epoch']:raise ValueError('The desktop session changed; select the application again')
            by_id={node['id']:node for node in nodes}
            for window in value['windows']:
                node=by_id.get(window['id'])
                if not node or node.get('pid')!=window['pid'] or node.get('app_id')!=window['app_id'] or node.get('shell')!='xdg_shell':raise ValueError('The selected application windows changed; select them again')
            selected_ids={window['id'] for window in value['windows']};selected_pids={process['pid'] for process in value['processes']}
            if any(node.get('pid') in selected_pids and node.get('id') not in selected_ids for node in nodes):raise ValueError('The application process now owns other windows; review the selection again')
            handles=[]
            try:
                for process in value['processes']:
                    fd=self.backend.open(process['pid']);handles.append(fd)
                    if self.backend.identity(process['pid'])!=process:raise ValueError('Application process identity changed; nothing was terminated')
                terminated=0;already_exited=0
                for fd in handles:
                    try:self.backend.terminate(fd);terminated+=1
                    except ProcessLookupError:already_exited+=1
                return {'terminationRequested':terminated,'alreadyExited':already_exited}
            finally:
                for fd in handles:self.backend.close(fd)


def main():
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('--request-id',default='');parser.add_argument('action',choices=['prepare','confirm','cancel']);parser.add_argument('value');args=parser.parse_args()
    try:
        runtime=os.environ.get('XDG_RUNTIME_DIR')
        if not runtime:raise ValueError('No private desktop runtime directory')
        control=ApplicationControl(Path(runtime)/'lumen-desktop/force-quit')
        result=control.prepare(loads(args.value.encode())) if args.action=='prepare' else getattr(control,args.action)(args.value)
        print(json.dumps(dict(result,requestId=args.request_id)));return 0
    except (ValueError,OSError,TypeError,KeyError,IndexError,subprocess.SubprocessError) as error:
        print(json.dumps({'error':str(error),'requestId':args.request_id}));return 1


if __name__=='__main__':raise SystemExit(main())
