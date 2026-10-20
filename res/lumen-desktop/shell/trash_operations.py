"""Captured personal-Trash deletion, with explicit confirmation and recovery journal."""
import configparser
import contextlib
import copy
import ctypes
import json
import os
from pathlib import Path
import secrets
import stat
import threading
import time
from urllib.parse import unquote
import file_identity

MAX_ITEMS=1000
MAX_NODES=4096
MAX_BYTES=8*1024**3
MAX_DEPTH=64
MAX_MANIFEST_BYTES=4*1024**2
MAX_JOURNAL_BYTES=32*1024**2
MAX_PUBLIC_BYTES=1024**2
MAX_ERROR_BYTES=1024
MAX_MESSAGE_CHARS=1024
PUBLIC_ENVELOPE_BYTES=128
PREPARE_SECONDS=2
DELETE_SECONDS=5
CONFIRM_SECONDS=120


def token_value(value):
    if not isinstance(value,str) or len(value)!=48 or any(char not in '0123456789abcdef' for char in value):raise ValueError('Invalid Trash confirmation')
    return value

def facts(info):return (info.st_dev,info.st_ino,info.st_mode,info.st_size,info.st_mtime_ns,info.st_ctime_ns)
def mount_id(fd):
    for line in Path('/proc/self/fdinfo/%d'%fd).read_text().splitlines():
        if line.startswith('mnt_id:'):return int(line.split(':',1)[1])
    raise ValueError('Native mount identity is unavailable')

def rename_exact(source,source_fd,target,target_fd):
    # This prevents destination replacement; it is not an inode-conditional move.
    library=ctypes.CDLL(None,use_errno=True)
    if library.renameat2(source_fd,os.fsencode(source),target_fd,os.fsencode(target),1):
        code=ctypes.get_errno();raise OSError(code,os.strerror(code),target)

@contextlib.contextmanager
def directory(path,dir_fd=None,secure=True):
    fd=os.open(path,os.O_RDONLY|os.O_DIRECTORY|os.O_NOFOLLOW|os.O_CLOEXEC,dir_fd=dir_fd)
    try:
        info=os.fstat(fd)
        if secure and (info.st_uid!=os.getuid() or info.st_mode&0o022):raise ValueError('Trash directories must be owned and writable only by this user')
        yield fd
    finally:os.close(fd)

def identity(fd):
    info=os.fstat(fd);return (info.st_dev,info.st_ino,info.st_mode,info.st_uid,mount_id(fd))

def observed(parent,name):
    fd=os.open(name,os.O_PATH|os.O_NOFOLLOW|os.O_CLOEXEC,dir_fd=parent)
    try:return facts(os.fstat(fd)),mount_id(fd)
    finally:os.close(fd)

def encoded_size(value):return len(json.dumps(value).encode())

def error_fields(value):
    value=str(value);low=0;high=min(len(value),MAX_ERROR_BYTES)
    while low<high:
        middle=(low+high+1)//2
        if encoded_size(value[:middle])<=MAX_ERROR_BYTES:low=middle
        else:high=middle-1
    return dict(error=value[:low],errorTruncated=low<len(value))

def bounded_names(fd,limit,deadline):
    names=[]
    with os.scandir(fd) as entries:
        for entry in entries:
            if len(names)>=limit or time.monotonic()>deadline:raise ValueError('Trash inspection exceeds its declared limits; nothing was changed')
            names.append(entry.name)
    return sorted(names)

class TrashOperations:
    def __init__(self,root,journal=None):
        self.root=Path(root);self.journal=self.root/'.lumen-deletion/journal.json'
        if not self.root.is_absolute():raise ValueError('Personal Trash must be an absolute location')
        if journal is not None and Path(journal)!=self.journal:raise ValueError('Trash recovery journal must remain within personal Trash')
        self.root_identity=None;self.recovery_identity=None;self.lock=threading.RLock();self.pending={};self.history={};self.active=None;self.stop_event=threading.Event();self.thread=None
        try:
            with directory(self.root) as root:
                self.root_identity=identity(root)
                with self.recovery(root) as recovery:
                    descriptor=os.open('journal.json',os.O_RDONLY|os.O_NOFOLLOW|os.O_CLOEXEC,dir_fd=recovery)
                    with os.fdopen(descriptor,'rb') as stream:
                        info=os.fstat(stream.fileno())
                        if not stat.S_ISREG(info.st_mode) or info.st_uid!=os.getuid() or info.st_mode&0o077 or info.st_size>MAX_JOURNAL_BYTES:raise ValueError('Unsupported Trash recovery journal')
                        raw=stream.read(MAX_JOURNAL_BYTES+1)
                        if len(raw)>MAX_JOURNAL_BYTES or facts(os.fstat(stream.fileno()))!=facts(info):raise ValueError('Trash recovery journal changed while it was read')
                        saved=json.loads(raw)
        except FileNotFoundError:saved=None
        if saved is not None:
            if not isinstance(saved,dict) or saved.get('version')!=1 or not isinstance(saved.get('operations'),dict):raise ValueError('Unsupported Trash journal')
            self.history=saved['operations']
            for operation in self.history.values():
                if operation.get('state')=='running':
                    operation['state']='interrupted';operation['message']='Deletion was interrupted; remaining contents were kept. No destructive action was retried.'
                    for item in operation.get('items',[]):
                        if item.get('state') in ('pending','staging','deleting'):item.update(state='interrupted',uncertain=True)

    @property
    def busy(self):
        with self.lock:return self.active is not None

    @contextlib.contextmanager
    def recovery(self,root,create=False):
        current=identity(root)
        if self.root_identity is not None and current!=self.root_identity:raise ValueError('The personal Trash root changed')
        self.root_identity=current
        if create:
            try:os.mkdir('.lumen-deletion',0o700,dir_fd=root);os.fsync(root)
            except FileExistsError:pass
        with directory('.lumen-deletion',root) as recovery:
            info=os.fstat(recovery);current=identity(recovery)
            if info.st_mode&0o077 or current[-1]!=self.root_identity[-1]:raise ValueError('Trash recovery directory must be private and on the same mount')
            if self.recovery_identity is not None and current!=self.recovery_identity:raise ValueError('The Trash recovery directory changed')
            self.recovery_identity=current
            yield recovery

    def persist(self):
        encoded=json.dumps(dict(version=1,operations=self.history)).encode()
        if len(encoded)>MAX_JOURNAL_BYTES:raise ValueError('Trash operation journal is full; no new deletion can be started')
        with directory(self.root) as root,self.recovery(root,create=True) as recovery:
            try:
                target=os.stat('journal.json',dir_fd=recovery,follow_symlinks=False)
                if not stat.S_ISREG(target.st_mode) or target.st_uid!=os.getuid() or target.st_mode&0o077:raise ValueError('Unsupported Trash recovery journal')
            except FileNotFoundError:pass
            temporary='.journal.'+secrets.token_hex(8)
            try:
                descriptor=os.open(temporary,os.O_WRONLY|os.O_CREAT|os.O_EXCL|os.O_NOFOLLOW|os.O_CLOEXEC,0o600,dir_fd=recovery)
                with os.fdopen(descriptor,'wb') as stream:stream.write(encoded);stream.flush();os.fsync(stream.fileno())
                os.replace(temporary,'journal.json',src_dir_fd=recovery,dst_dir_fd=recovery);os.fsync(recovery)
            finally:
                try:os.unlink(temporary,dir_fd=recovery)
                except FileNotFoundError:pass


    @contextlib.contextmanager
    def roots(self,expected=None):
        with directory(self.root) as root,directory('files',root) as files,directory('info',root) as info:
            values=[identity(fd) for fd in (root,files,info)]
            if any(value[-1]!=values[0][-1] for value in values):raise ValueError('Trash directories on separate mounts are unsupported')
            if expected is not None and values!=expected:raise ValueError('The personal Trash location changed')
            yield root,files,info,values

    def status_payload(self,history,busy,operations=None):
        return dict(available=True,busy=busy,operations=list(history.values())[-1:] if operations is None else operations,history=[dict(id=value['id'],state=value['state'],count=len(value.get('items',[])),message=value.get('message','')) for value in list(history.values())[-40:]],limits=dict(items=MAX_ITEMS,nodes=MAX_NODES,bytes=MAX_BYTES,prepareSeconds=PREPARE_SECONDS,deleteSeconds=DELETE_SECONDS,confirmationSeconds=CONFIRM_SECONDS,manifestBytes=MAX_MANIFEST_BYTES,resultBytes=MAX_PUBLIC_BYTES,errorBytes=MAX_ERROR_BYTES),scope='Recognized items already in personal Trash only')

    def bounded_public(self,value):
        if encoded_size(value)+PUBLIC_ENVELOPE_BYTES>MAX_PUBLIC_BYTES:raise ValueError('Trash result exceeds the supported transport budget; retained journal was not changed')
        return json.loads(json.dumps(value))

    def reserve(self,token,plan,history):
        items=[]
        for index,outcome in enumerate(plan['outcomes']):
            stage=self.root/'.lumen-deletion'/(token+'-'+str(index))
            item=dict(outcome,state='interrupted',removedNodes=MAX_NODES,recoveryPath=str(stage),uncertain=True,error='\\'*(MAX_ERROR_BYTES//2-1),errorTruncated=False)
            for key,name in (('payloadPath','data'),('metadataPath','record.trashinfo')):
                item[key]=max((outcome[key],str(stage/name)),key=encoded_size)
            items.append(item)
        operation=dict(id=token,mode=plan['mode'],state='interrupted',started=-1.7976931348623157e308,items=items,message='\U0010ffff'*MAX_MESSAGE_CHARS)
        candidate=dict(history);candidate[token]=operation
        while len(candidate)>40:
            completed=next((key for key,value in candidate.items() if key!=token and value.get('state')=='complete'),None)
            if completed is None:raise ValueError('Review retained incomplete Trash operations before starting another deletion')
            candidate.pop(completed)
        for value in candidate.values():self.bounded_public(self.status_payload(candidate,False,[value]))
        self.bounded_public(dict(operation=operation))
        if encoded_size(dict(version=1,operations=candidate))>MAX_JOURNAL_BYTES:raise ValueError('Trash journal cannot reserve all possible outcomes; nothing was changed')

    def status(self):
        with self.lock:return self.bounded_public(self.status_payload(self.history,self.active is not None))

    def inspect(self,ident):
        with self.lock:
            if token_value(ident) not in self.history:raise ValueError('Trash operation is no longer in the retained journal')
            return self.bounded_public(dict(operation=self.history[ident]))

    def capture_tree(self,parent,name,mount,budget,prefix=()):
        if time.monotonic()>budget['deadline']:raise ValueError('Trash preparation exceeded its time limit; nothing was changed')
        value,current_mount=observed(parent,name)
        if current_mount!=mount:raise ValueError('Mounted contents cannot be permanently deleted through Trash')
        if not (stat.S_ISDIR(value[2]) or stat.S_ISREG(value[2]) or stat.S_ISLNK(value[2])):raise ValueError('Special files cannot be permanently deleted through this action')
        budget['nodes']+=1;budget['bytes']+=value[3] if stat.S_ISREG(value[2]) else 0;budget['manifestBytes']=budget.get('manifestBytes',0)+sum(len(os.fsencode(part)) for part in prefix)+128
        if budget['nodes']>MAX_NODES or budget['bytes']>MAX_BYTES or len(prefix)>MAX_DEPTH or budget['manifestBytes']>MAX_MANIFEST_BYTES:raise ValueError('Trash selection exceeds the declared limits; nothing was changed')
        result=[dict(path=list(prefix),facts=value)]
        if stat.S_ISDIR(value[2]):
            with directory(name,parent,secure=False) as child:
                if facts(os.fstat(child))!=value:raise ValueError('A selected Trash folder changed')
                names=bounded_names(child,MAX_NODES-budget['nodes'],budget['deadline'])
                for item in names:result.extend(self.capture_tree(child,item,mount,budget,prefix+(item,)))
                if facts(os.fstat(child))!=value:raise ValueError('A selected Trash folder changed while it was inspected')
        return result

    def capture(self,files,info,name,mount,budget,expected=None):
        if not isinstance(name,str) or not name or name in ('.','..') or '/' in name or '\0' in name:raise ValueError('Invalid Trash entry')
        metadata=name+'.trashinfo';descriptor=os.open(metadata,os.O_RDONLY|os.O_NOFOLLOW|os.O_NONBLOCK|os.O_CLOEXEC,dir_fd=info)
        try:
            meta=os.fstat(descriptor)
            if not stat.S_ISREG(meta.st_mode) or meta.st_uid!=os.getuid() or meta.st_size>65536 or mount_id(descriptor)!=mount:raise ValueError('Unsupported Trash recovery record')
            raw=os.read(descriptor,65537);budget['manifestBytes']=budget.get('manifestBytes',0)+len(raw)
            if budget['manifestBytes']>MAX_MANIFEST_BYTES:raise ValueError('Trash recovery records exceed the manifest limit')
            if len(raw)>65536 or facts(os.fstat(descriptor))!=facts(meta):raise ValueError('Trash recovery record changed')
        finally:os.close(descriptor)
        parser=configparser.ConfigParser(interpolation=None);parser.read_string(raw.decode('utf-8'));original=unquote(parser['Trash Info']['Path'])
        if '\0' in original:raise ValueError('Invalid Trash recovery path')
        tree=self.capture_tree(files,name,mount,budget)
        if expected is not None and file_identity.parse(expected)!=dict(device=tree[0]['facts'][0],inode=tree[0]['facts'][1]):raise ValueError('The selected Trash item changed')
        return dict(name=name,label=Path(original).name or name,metadata=metadata,metadataFacts=facts(meta),tree=tree)

    def prepare(self,mode,references=None):
        if mode not in ('empty','selected'):raise ValueError('Unsupported Trash confirmation action')
        if mode=='selected' and (not isinstance(references,list) or not references):raise ValueError('Select at least one Trash item')
        with self.lock:
            if self.active is not None:raise ValueError('Wait for the current deletion or stop it first')
            now=time.monotonic();self.pending={key:value for key,value in self.pending.items() if value['expires']>now}
            if len(self.pending)>=32:raise ValueError('Too many pending Trash confirmations')
            budget=dict(nodes=0,bytes=0,deadline=now+PREPARE_SECONDS)
            with self.roots() as (_,files,info,root_identity):
                if mode=='empty':
                    if references is not None:raise ValueError('Empty Trash does not accept caller paths')
                    names=bounded_names(files,MAX_ITEMS,budget['deadline'])
                    selected=[(name,None) for name in names]
                else:
                    selected=[]
                    for row in references:
                        if not isinstance(row,dict) or set(row)!={'path','identity'} or not isinstance(row['path'],str):raise ValueError('Captured Trash identities are required')
                        path=Path(row['path'])
                        if not path.is_absolute() or path.parent!=self.root/'files':raise ValueError('Permanent deletion is limited to personal Trash items')
                        file_identity.parse(row['identity']);selected.append((path.name,row['identity']))
                    if len(set(name for name,_ in selected))!=len(selected):raise ValueError('Repeated Trash item')
                if not selected:raise ValueError('Trash is already empty')
                if len(selected)>MAX_ITEMS:raise ValueError('Trash selection exceeds the item limit; nothing was changed')
                captured=[self.capture(files,info,name,root_identity[0][-1],budget,expected) for name,expected in selected]
            outcomes=[dict(name=item['label'],storedName=item['name'],state='pending',removedNodes=0,recoveryPath='',payloadPath=str(self.root/'files'/item['name']),metadataPath=str(self.root/'info'/item['metadata'])) for item in captured]
            token=secrets.token_hex(24);plan=dict(items=captured,outcomes=outcomes,roots=root_identity,expires=now+CONFIRM_SECONDS,mode=mode)
            self.reserve(token,plan,self.history);self.pending[token]=plan
            return dict(token=token,mode=mode,count=len(captured),names=[item['label'] for item in captured],nodes=budget['nodes'],bytes=budget['bytes'],consequence='These items will be permanently deleted. This action cannot be undone.',expiresIn=CONFIRM_SECONDS,limits=self.status()['limits'])

    def cancel(self,token):
        token_value(token)
        with self.lock:self.pending.pop(token,None)
        return dict(cancelled=True)

    def confirm(self,token):
        token_value(token)
        with self.lock:
            if token in self.history:
                result=self.status();result['operations']=[json.loads(json.dumps(self.history[token]))];return self.bounded_public(result)
            if self.active is not None:raise ValueError('Wait for the current deletion or stop it first')
            plan=self.pending.pop(token,None)
            if plan is None or plan['expires']<=time.monotonic():raise ValueError('This confirmation expired; review the Trash again')
            with self.roots(plan['roots']):pass
            self.reserve(token,plan,self.history)
            previous=self.history;self.history=dict(previous)
            self.history[token]=dict(id=token,mode=plan['mode'],state='running',started=time.time(),items=copy.deepcopy(plan['outcomes']),message='Permanent deletion started')
            while len(self.history)>40:
                completed=next((key for key,value in self.history.items() if key!=token and value.get('state')=='complete'),None)
                if completed is None:self.history=previous;raise ValueError('Review retained incomplete Trash operations before starting another deletion')
                self.history.pop(completed)
            try:self.persist()
            except BaseException:self.history=previous;raise
            self.active=token;self.stop_event=threading.Event();self.thread=threading.Thread(target=self.run,args=(token,plan),daemon=False);self.thread.start()
            return self.status()

    def stop(self):self.stop_event.set();return self.status()

    def check(self,deadline):
        if self.stop_event.is_set():raise ValueError('Deletion stopped; remaining contents were kept')
        if time.monotonic()>deadline:raise ValueError('Deletion reached its time limit; remaining contents were kept')

    def remove(self,fd,name,expected,directory_entry=False):
        current,_=observed(fd,name)
        if directory_entry:
            if current[:3]!=tuple(expected[:3]):raise ValueError('A captured folder changed')
            os.rmdir(name,dir_fd=fd)
        else:
            if current!=tuple(expected):raise ValueError('A captured item changed')
            os.unlink(name,dir_fd=fd)

    def delete_tree(self,parent,name,manifest,deadline,outcome):
        by_path={tuple(row['path']):row['facts'] for row in manifest}
        def visit(fd,entry,path):
            self.check(deadline);expected=by_path[path]
            if stat.S_ISDIR(expected[2]):
                with directory(entry,fd,secure=False) as child:
                    if facts(os.fstat(child))[:3]!=tuple(expected[:3]):raise ValueError('A captured folder was replaced')
                    for child_path in sorted((key for key in by_path if key[:-1]==path and key!=path),reverse=True):visit(child,child_path[-1],child_path)
                self.remove(fd,entry,expected,True)
            else:self.remove(fd,entry,expected)
            outcome['removedNodes']+=1
        visit(parent,name,())

    def run(self,token,plan):
        operation=self.history[token];deadline=time.monotonic()+DELETE_SECONDS
        try:
            with self.roots(plan['roots']) as (root,files,info,_):
                with self.recovery(root) as quarantine:
                    for index,(captured,outcome) in enumerate(zip(plan['items'],operation['items'])):
                        stage_name=token+'-'+str(index);staged_data=False;staged_info=False
                        try:
                            self.check(deadline);budget=dict(nodes=0,bytes=0,deadline=deadline)
                            current=self.capture(files,info,captured['name'],plan['roots'][0][-1],budget)
                            if current!=captured:raise ValueError('Trash contents changed after confirmation; review them again')
                            os.mkdir(stage_name,0o700,dir_fd=quarantine)
                            with directory(stage_name,quarantine) as stage:
                                outcome.update(state='staging',recoveryPath=str(self.root/'.lumen-deletion'/stage_name));self.persist()
                                rename_exact(captured['name'],files,'data',stage);staged_data=True;outcome['payloadPath']=str(self.root/'.lumen-deletion'/stage_name/'data')
                                # Rename may update top-level ctime. Recheck object identity,
                                # size and mtime before recording that expected local change.
                                moved,_=observed(stage,'data');original=captured['tree'][0]['facts']
                                if moved[:5]!=tuple(original[:5]):raise ValueError('The staged Trash object changed')
                                meta,_=observed(info,captured['metadata'])
                                if meta!=tuple(captured['metadataFacts']):raise ValueError('The recovery record changed before staging')
                                rename_exact(captured['metadata'],info,'record.trashinfo',stage);staged_info=True;outcome['metadataPath']=str(self.root/'.lumen-deletion'/stage_name/'record.trashinfo')
                                moved_meta,_=observed(stage,'record.trashinfo')
                                if moved_meta[:5]!=tuple(captured['metadataFacts'][:5]):raise ValueError('The staged recovery record changed')
                                os.fsync(files);os.fsync(info);os.fsync(stage);manifest=copy.deepcopy(captured['tree']);manifest[0]['facts']=moved
                                outcome.update(state='deleting');self.persist();self.delete_tree(stage,'data',manifest,deadline,outcome)
                                outcome['payloadPath']='';self.remove(stage,'record.trashinfo',moved_meta);outcome['metadataPath']='';os.fsync(stage)
                            os.rmdir(stage_name,dir_fd=quarantine);os.fsync(quarantine);outcome.update(state='deleted',recoveryPath='')
                        except Exception as error:
                            outcome.update(state='partial' if outcome['removedNodes'] else 'kept',**error_fields(error))
                            # Never overwrite a later arrival while restoring an untouched item.
                            if staged_data and not outcome['removedNodes']:
                                try:
                                    with directory(stage_name,quarantine) as stage:
                                        rename_exact('data',stage,captured['name'],files);outcome['payloadPath']=str(self.root/'files'/captured['name'])
                                        if staged_info:rename_exact('record.trashinfo',stage,captured['metadata'],info);outcome['metadataPath']=str(self.root/'info'/captured['metadata'])
                                    os.rmdir(stage_name,dir_fd=quarantine);os.fsync(files);os.fsync(info);os.fsync(quarantine);outcome['recoveryPath']=''
                                except Exception as recovery_error:
                                    truncated=outcome.get('errorTruncated',False);outcome.update(error_fields(outcome['error']+'; remaining item kept at recovery location: '+str(recovery_error)));outcome['errorTruncated']=truncated or outcome['errorTruncated']
                        self.persist()
            operation['state']='complete' if all(item['state']=='deleted' for item in operation['items']) else 'partial'
            operation['message']='Confirmed items were permanently deleted' if operation['state']=='complete' else 'Some items were kept or only partly deleted; inspect each result and recovery location'
        except Exception as error:
            operation.update(state='failed',message=str(error)[:MAX_MESSAGE_CHARS])
            for item in operation['items']:
                if item['state']=='pending':item.update(state='kept',**error_fields(error))
        finally:
            with self.lock:
                self.active=None
                try:self.persist()
                except (OSError,ValueError):operation.update(state='interrupted',message='The operation result could not be saved; inspect Trash and recovery locations before any new confirmation')

    def close(self):
        self.stop_event.set()
        if self.thread is not None:self.thread.join(timeout=10)
