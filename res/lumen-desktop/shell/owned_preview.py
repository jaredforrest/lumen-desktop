"""Owned preview session: immutable resources, bounded requests and explicit retirement."""
import hashlib
import json
import mimetypes
import os
from pathlib import Path
import secrets
import signal
import stat
import subprocess
import threading
import sys
from urllib.parse import unquote, urlparse

from file_preview import preview
import file_identity
from preview_snapshots import PreviewSnapshots,PreviewRequests
from captured_resource_actions import CapturedResourceActions,ResourceUnavailable,unavailable


class PreviewSession:
    def __init__(self):
        self.generation=secrets.token_hex(16)
        self.actions=CapturedResourceActions();self.action_keys={}
        self.resources=PreviewSnapshots(self.identity,self.validate)
        self.requests=PreviewRequests(self.resources)

    def identity(self,path,info=None):
        path=Path(path)
        info=info or path.stat(follow_symlinks=False)
        facts=(str(path),info.st_dev,info.st_ino,info.st_ctime_ns,info.st_mtime_ns,info.st_size)
        return self.generation+':'+hashlib.sha256(json.dumps(facts).encode()).hexdigest()

    def validate(self,value,token):
        path=Path(value)
        if not path.is_absolute() or path.is_symlink() or self.identity(path)!=token:
            raise ResourceUnavailable('source_changed','The selected preview changed')
        return path

    def request(self,value,page=1,expected=None,token=None,request_id=None,lease=None):
        if not isinstance(value,str) or not value or '\0' in value or not Path(value).is_absolute():
            raise ValueError('A local absolute preview path is required')
        if type(page) is not int or not 1<=page<=100000:raise ValueError('Invalid preview page')
        path=Path(value);info=path.stat(follow_symlinks=False)
        if expected is not None:
            try:captured_identity=file_identity.parse(expected)
            except ValueError as error:raise ResourceUnavailable('source_changed','The selected preview changed') from error
            if captured_identity!=dict(device=info.st_dev,inode=info.st_ino):
                raise ResourceUnavailable('source_changed','The selected preview changed')
        observed=self.identity(path,info)
        if token is not None and (not isinstance(token,str) or token!=observed):raise ResourceUnavailable('source_changed','The selected preview changed')
        capability=self.actions.capture(path,expected=(info.st_dev,info.st_ino,info.st_mode,info.st_size,info.st_mtime_ns,info.st_ctime_ns))
        if request_id is not None:self.action_keys[request_id]=capability
        if stat.S_ISREG(info.st_mode):
            if info.st_size>64*1024*1024:raise ResourceUnavailable('policy_limit','This preview supports files up to64 MiB')
            resource=self.resources.capture(path,observed,lease)
            if request_id is not None:self.requests.retain_source(request_id,observed)
            result=preview(resource,page)
            # Shared renderers may publish a prunable intermediate raster.
            # Retain that resource under this same provider lifetime as well.
            uri=result.get('uri')
            if uri and uri!=resource.as_uri():
                parsed=urlparse(uri)
                if parsed.scheme!='file' or parsed.netloc:raise ValueError('Unsupported preview resource')
                rendered=Path(unquote(parsed.path))
                retained=self.resources.capture(rendered,self.identity(rendered),lease)
                result['uri']=retained.as_uri()
            self.validate(str(path),observed)
            self.actions.bind_resource(capability,resource,observed)
            result["actionCapability"]=capability
            # Renderers and media decoders only receive the retained resource.
            result.update(path=str(path),name=path.name,title=path.name,size=info.st_size,mtime=info.st_mtime,mime=mimetypes.guess_type(path.name)[0] or result.get("mime",""),sourceFacts=dict(device=str(info.st_dev),inode=str(info.st_ino),size=info.st_size,mode=info.st_mode,mtimeNs=info.st_mtime_ns,ctimeNs=info.st_ctime_ns),sourceToken=observed,sourceIdentity=file_identity.wire(info),resourcePath=str(resource),requestedPage=page)
            if request_id is not None:
                published=result.get('uri')
                self.requests.finish(request_id,[Path(unquote(urlparse(published).path))] if published else [])
            return result
        if stat.S_ISDIR(info.st_mode):
            result=preview(path,page);self.validate(str(path),observed)
            result.update(actionCapability=capability,sourceToken=observed,sourceIdentity=file_identity.wire(info),requestedPage=page)
            return result
        raise ValueError('This file type cannot be previewed safely')

    def close(self):self.requests.close()


def main():
    os.umask(0o077);session=PreviewSession();output_lock=threading.Lock();action_lock=threading.Lock();workers={}
    def emit(value):
        with output_lock:print(json.dumps(value,allow_nan=False),flush=True)
    def action_worker(request,cancel):
        lease=session.resources.begin()
        try:
            capability=request.get('capability');operation=request.get('operation');entry=session.actions.selected(capability)
            if entry.get('resourceToken'):session.resources.retain(lease,entry['resourceToken'])
            with session.resources.pin(lease):
                if operation=='applications':result=session.actions.applications(capability)
                elif operation in ('open','open-with','set-default'):result=session.actions.dispatch(capability,operation,request.get('application'),choice=request.get('choice'))
                elif operation in ('ocr','cutout'):result=session.actions.content(capability,operation,request.get('parameters') or {},cancel=cancel)
                else:raise ResourceUnavailable('invalid_request','Unsupported preview action')
            result.update(id=request['id'],action='resource-action',operation=operation,capability=capability,generation=session.requests.generation)
        except (OSError,ValueError,KeyError,TypeError,OverflowError,subprocess.SubprocessError) as exc:
            result=dict(id=request['id'],action='resource-action',operation=request.get('operation'),capability=request.get('capability'),generation=session.requests.generation,**unavailable(exc))
        finally:session.resources.release(lease)
        with action_lock:workers.pop(request['id'],None)
        emit(result)

    def retire(*_):raise SystemExit(0)
    signal.signal(signal.SIGTERM,retire)
    try:
        emit(dict(event='ready',generation=session.requests.generation))
        seen_ids=set();highest_input=0
        for line in sys.stdin:
            request={};registered=False
            try:
                if len(line)>32768:raise ValueError('Preview request is too large')
                request=json.loads(line)
                if not isinstance(request,dict) or type(request.get('id')) is not int or not 1<=request['id']<=9007199254740991 or request.get('generation')!=session.requests.generation:raise ValueError('Invalid preview request')
                if request['id'] in seen_ids or request['id']<=highest_input-128:raise ResourceUnavailable('invalid_request','This request identity was retired')
                highest_input=max(highest_input,request['id']);seen_ids={value for value in seen_ids if value>highest_input-128};seen_ids.add(request['id'])
                if request.get('action')=='retire-preview':
                    ids=request.get('requests')
                    if not isinstance(ids,list) or len(ids)>64:raise ValueError('Invalid preview release')
                    session.actions.retire([session.action_keys.pop(key) for key in ids if key in session.action_keys])
                    session.requests.retire(ids,request.get('keepSourceToken'))
                    result=dict(id=request['id'],action='retire-preview',retired=True,generation=session.requests.generation)
                elif request.get('action')=='cancel-action':
                    if type(request.get('target')) is not int or not 1<=request['target']<=9007199254740991:raise ResourceUnavailable('invalid_request','Invalid cancellation identity')
                    with action_lock:
                        worker=workers.get(request.get('target'))
                        accepted=bool(worker and worker['capability']==request.get('capability') and worker['operation'] in ('ocr','cutout'))
                        if accepted:worker['cancel'].set()
                    result=dict(id=request['id'],action='cancel-action',generation=session.requests.generation,accepted=accepted)
                elif request.get('action')=='resource-action':
                    with action_lock:
                        if workers:raise ResourceUnavailable('busy','Another preview action is still completing')
                        cancel=threading.Event();thread=threading.Thread(target=action_worker,args=(request,cancel));workers[request['id']]=dict(thread=thread,cancel=cancel,capability=request.get('capability'),operation=request.get('operation'));thread.start()
                    continue
                elif request.get('action')=='preview':
                    session.requests.register(request['id'],request.get('token'));registered=True
                    with session.requests.work(request['id']) as lease:
                        result=session.request(request.get('path'),request.get('page',1),request.get('identity'),request.get('token'),request_id=request['id'],lease=lease)
                    result.update(id=request['id'],action='preview',sourcePath=request['path'],generation=session.requests.generation)
                else:raise ValueError('Invalid preview action')
            except (OSError,ValueError,KeyError,TypeError,OverflowError,subprocess.SubprocessError) as exc:
                if registered:session.requests.retire([request['id']])
                result=dict(id=request.get('id') if isinstance(request,dict) else None,action=request.get('action') if isinstance(request,dict) else 'preview',sourcePath=request.get('path') if isinstance(request,dict) else '',generation=session.requests.generation,capability=request.get('capability') if isinstance(request,dict) else None,actionCapability=session.action_keys.get(request.get('id')) if isinstance(request,dict) else None,**unavailable(exc))
            emit(result)
    finally:
        with action_lock:remaining=list(workers.values())
        for worker in remaining:worker['cancel'].set()
        for worker in remaining:worker['thread'].join()
        session.close()


if __name__=='__main__':main()
