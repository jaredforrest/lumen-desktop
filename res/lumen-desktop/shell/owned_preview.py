"""Owned preview session: immutable resources, bounded requests and explicit retirement."""
import hashlib
import json
import mimetypes
import os
from pathlib import Path
import secrets
import signal
import stat
import sys
from urllib.parse import unquote, urlparse

from file_preview import preview
from preview_snapshots import PreviewSnapshots,PreviewRequests


class PreviewSession:
    def __init__(self):
        self.generation=secrets.token_hex(16)
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
            raise ValueError('The selected preview changed')
        return path

    def request(self,value,page=1,expected=None,token=None,request_id=None,lease=None):
        if not isinstance(value,str) or not value or '\0' in value or not Path(value).is_absolute():
            raise ValueError('A local absolute preview path is required')
        if type(page) is not int or not 1<=page<=100000:raise ValueError('Invalid preview page')
        path=Path(value);info=path.stat(follow_symlinks=False)
        if expected is not None:
            if not isinstance(expected,dict) or set(expected)!={'device','inode'} or any(type(expected[k]) is not int or expected[k]<0 for k in expected) or (info.st_dev,info.st_ino)!=(expected['device'],expected['inode']):
                raise ValueError('The selected preview changed')
        observed=self.identity(path,info)
        if token is not None and (not isinstance(token,str) or token!=observed):raise ValueError('The selected preview changed')
        if stat.S_ISREG(info.st_mode):
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
            # Renderers and media decoders only receive the retained resource.
            result.update(path=str(path),name=path.name,title=path.name,size=info.st_size,mtime=info.st_mtime,mime=mimetypes.guess_type(path.name)[0] or result.get("mime",""),sourceFacts=dict(device=info.st_dev,inode=info.st_ino,size=info.st_size,mode=info.st_mode,mtimeNs=info.st_mtime_ns,ctimeNs=info.st_ctime_ns),sourceToken=observed,sourceIdentity=dict(device=info.st_dev,inode=info.st_ino),resourcePath=str(resource),requestedPage=page)
            if request_id is not None:
                published=result.get('uri')
                self.requests.finish(request_id,[Path(unquote(urlparse(published).path))] if published else [])
            return result
        if stat.S_ISDIR(info.st_mode):
            result=preview(path,page);self.validate(str(path),observed)
            result.update(sourceToken=observed,sourceIdentity=dict(device=info.st_dev,inode=info.st_ino),requestedPage=page)
            return result
        raise ValueError('This file type cannot be previewed safely')

    def close(self):self.requests.close()


def main():
    os.umask(0o077);session=PreviewSession()
    def retire(*_):raise SystemExit(0)
    signal.signal(signal.SIGTERM,retire)
    try:
        print(json.dumps(dict(event='ready',generation=session.requests.generation)),flush=True)
        for line in sys.stdin:
            request={};registered=False
            try:
                if len(line)>32768:raise ValueError('Preview request is too large')
                request=json.loads(line)
                if not isinstance(request,dict) or type(request.get('id')) is not int or request['id']<1 or request.get('generation')!=session.requests.generation:raise ValueError('Invalid preview request')
                if request.get('action')=='retire-preview':
                    ids=request.get('requests')
                    if not isinstance(ids,list) or len(ids)>64:raise ValueError('Invalid preview release')
                    session.requests.retire(ids,request.get('keepSourceToken'))
                    result=dict(id=request['id'],action='retire-preview',retired=True,generation=session.requests.generation)
                elif request.get('action')=='preview':
                    session.requests.register(request['id'],request.get('token'));registered=True
                    with session.requests.work(request['id']) as lease:
                        result=session.request(request.get('path'),request.get('page',1),request.get('identity'),request.get('token'),request_id=request['id'],lease=lease)
                    result.update(id=request['id'],action='preview',sourcePath=request['path'],generation=session.requests.generation)
                else:raise ValueError('Invalid preview action')
            except (OSError,ValueError,KeyError,TypeError,OverflowError):
                if registered:session.requests.retire([request['id']])
                result=dict(id=request.get('id') if isinstance(request,dict) else None,action=request.get('action') if isinstance(request,dict) else 'preview',sourcePath=request.get('path') if isinstance(request,dict) else '',generation=session.requests.generation,error='Preview unavailable: the selected file changed, is unsupported, or exceeds the private preview budget')
            print(json.dumps(result,allow_nan=False),flush=True)
    finally:session.close()


if __name__=='__main__':main()
