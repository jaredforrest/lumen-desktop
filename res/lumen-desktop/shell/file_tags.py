"""Portable coloured file tags in a versioned user extended attribute.

Tags belong to the inode independently of its filename. Symbolic links are not
followed; filesystems that cannot store user attributes report that limitation.
"""
import errno,json,os,stat
from pathlib import Path
ATTRIBUTE='user.lumen.tags.v1'
TAGS=(
    {'id':'red','label':'Red','color':'#e54b4b'},
    {'id':'orange','label':'Orange','color':'#e79735'},
    {'id':'yellow','label':'Yellow','color':'#d6b82f'},
    {'id':'green','label':'Green','color':'#50a86b'},
    {'id':'blue','label':'Blue','color':'#438ed6'},
    {'id':'purple','label':'Purple','color':'#9b6ac6'},
    {'id':'grey','label':'Grey','color':'#929298'},
)
IDS={tag['id'] for tag in TAGS}
MISSING={errno.ENODATA,getattr(errno,'ENOATTR',errno.ENODATA)}

def validate(ids):
    if not isinstance(ids,list) or any(not isinstance(value,str) or value not in IDS for value in ids) or len(ids)!=len(set(ids)):
        raise ValueError('Choose distinct supported colour tags')
    return ids

def read(path):
    try:
        raw=os.getxattr(path,ATTRIBUTE,**({} if type(path) is int else {'follow_symlinks':False}))
        if len(raw)>4096:raise ValueError('Tag metadata exceeds its size limit')
        data=json.loads(raw)
        if not isinstance(data,dict) or type(data.get('version')) is not int or data.get('version')!=1:raise ValueError('Unsupported tag metadata version')
        ids=validate(data.get('tags'))
        return {'tags':[dict(tag) for tag in TAGS if tag['id'] in ids],'supported':True,'error':''}
    except OSError as error:
        if error.errno in MISSING:return {'tags':[],'supported':True,'error':''}
        return {'tags':[],'supported':False,'error':'This item cannot store colour tags: '+str(error)}
    except (ValueError,TypeError,UnicodeError) as error:
        return {'tags':[],'supported':True,'error':'Colour tag metadata could not be read: '+str(error)}

def write(path,ids,expected=None):
    validate(ids)
    try:descriptor=os.open(path,os.O_RDONLY|os.O_NOFOLLOW|os.O_CLOEXEC|os.O_NONBLOCK)
    except OSError as error:raise ValueError('Colour tags require an accessible regular file or folder without following links: '+str(error)) from error
    try:
        info=os.fstat(descriptor)
        if not (stat.S_ISREG(info.st_mode) or stat.S_ISDIR(info.st_mode)):
            raise ValueError('Colour tags can be stored on regular files and folders, without following links')
        if expected is not None and expected!={'device':info.st_dev,'inode':info.st_ino}:
            raise ValueError('This item changed while its tags were being edited; reopen Colour Tags')
        try:
            if ids:os.setxattr(descriptor,ATTRIBUTE,json.dumps({'version':1,'tags':ids},separators=(',',':')).encode())
            else:
                try:os.removexattr(descriptor,ATTRIBUTE)
                except OSError as error:
                    if error.errno not in MISSING:raise
        except OSError as error:raise ValueError('This filesystem or item cannot save colour tags: '+str(error)) from error
        return read(descriptor)
    finally:os.close(descriptor)

def copy_metadata(source,destination):
    """A tagged cross-volume move must not silently discard its metadata."""
    try:raw=os.getxattr(source,ATTRIBUTE,follow_symlinks=False)
    except OSError as error:
        if error.errno in MISSING or error.errno in (errno.ENOTSUP,errno.EOPNOTSUPP):return
        raise
    if len(raw)>4096:raise ValueError('Tag metadata exceeds its size limit')
    try:os.setxattr(destination,ATTRIBUTE,raw,follow_symlinks=False)
    except OSError as error:raise ValueError('Destination cannot preserve colour tags; source data was kept: '+str(error)) from error

def index_text(path):
    return ''.join('|'+tag['id']+'|' for tag in read(path)['tags'])
