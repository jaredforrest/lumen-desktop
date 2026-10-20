"""Bounded raster annotations exported as a new PNG, never an in-place edit."""
import math
import contextlib
import os
from pathlib import Path
import re
import secrets
import stat
import warnings
from urllib.parse import unquote,urlparse
from captured_resource_actions import ResourceUnavailable

MAX_STROKES=512
MAX_POINTS=8192
MAX_PIXELS=32_000_000
MAX_RENDER_WORK=64_000_000


def recipe(value):
    if not isinstance(value,list) or not 1<=len(value)<=MAX_STROKES:raise ResourceUnavailable('invalid_request','Add at least one annotation before saving')
    result=[];count=0
    for stroke in value:
        if not isinstance(stroke,dict) or set(stroke)!={'tool','color','width','points'} or stroke['tool'] not in ('pen','rectangle') or not isinstance(stroke['color'],str) or not re.fullmatch(r'#[0-9a-fA-F]{6}',stroke['color']):raise ResourceUnavailable('invalid_request','Invalid image annotation')
        width=stroke['width'];points=stroke['points']
        if type(width) not in (int,float) or not math.isfinite(width) or not .001<=width<=.05 or not isinstance(points,list) or not 2<=len(points)<=MAX_POINTS or (stroke['tool']=='rectangle' and len(points)!=2):raise ResourceUnavailable('invalid_request','Invalid annotation geometry')
        for point in points:
            if not isinstance(point,list) or len(point)!=2 or any(type(number) not in (int,float) or not math.isfinite(number) or not 0<=number<=1 for number in point):raise ResourceUnavailable('invalid_request','Annotation points must lie inside the image')
        count+=len(points)
        if count>MAX_POINTS:raise ResourceUnavailable('policy_limit','This annotation has too many points')
        result.append(dict(stroke,points=[point[:] for point in points]))
    return result


def save_copy(entry,parameters,validate,cancel=None):
    if not isinstance(parameters,dict) or set(parameters)!={'strokes','destination'}:raise ResourceUnavailable('invalid_request','Invalid markup export request')
    strokes=recipe(parameters.get('strokes'));value=parameters.get('destination')
    if not isinstance(value,str) or not value or len(value)>8192 or '\0' in value:raise ResourceUnavailable('invalid_request','Choose a new PNG destination')
    parsed=urlparse(value)
    if parsed.scheme:
        if parsed.scheme!='file' or parsed.netloc or parsed.query or parsed.fragment:raise ResourceUnavailable('invalid_request','Choose a local PNG destination')
        value=unquote(parsed.path)
    destination=Path(value)
    if not destination.is_absolute() or destination.suffix.lower()!='.png' or destination.name in ('.','..'):raise ResourceUnavailable('invalid_request','Save the annotated copy as a new PNG')
    def check_cancel():
        if cancel is not None and cancel.is_set():raise ResourceUnavailable('canceled','Image export canceled')
    def current():
        check_cancel();validate()
    with raster(entry,current) as original:
        image=original.convert('RGBA')
    try:return publish_copy(image,strokes,entry,destination,current,check_cancel)
    finally:image.close()



@contextlib.contextmanager
def raster(entry,validate):
    from PIL import Image,UnidentifiedImageError
    resource=entry.get('resource')
    if resource is None:raise ResourceUnavailable('unsupported','This preview cannot be annotated')
    validate();metadata=os.open(resource,os.O_PATH|os.O_NOFOLLOW);descriptor=None
    try:
        if not stat.S_ISREG(os.fstat(metadata).st_mode):raise ResourceUnavailable('source_changed','The retained image is unavailable')
        descriptor=os.open('/proc/self/fd/'+str(metadata),os.O_RDONLY|os.O_NONBLOCK)
        with os.fdopen(descriptor,'rb') as stream:
            descriptor=None
            try:
                with warnings.catch_warnings(action="error",category=Image.DecompressionBombWarning),Image.open(stream) as original:
                    if original.width*original.height>MAX_PIXELS:raise ResourceUnavailable('policy_limit','Markup supports images up to 32 million pixels')
                    if original.format not in ('PNG','JPEG','WEBP') or getattr(original,'is_animated',False):raise ResourceUnavailable('unsupported','Markup supports still PNG, JPEG and WebP images')
                    validate();yield original
            except (Image.DecompressionBombWarning,Image.DecompressionBombError) as exc:raise ResourceUnavailable('policy_limit','Markup supports images up to 32 million pixels') from exc
            except UnidentifiedImageError as exc:raise ResourceUnavailable('unsupported','This image cannot be decoded for markup') from exc
    finally:
        if descriptor is not None:os.close(descriptor)
        os.close(metadata)


def describe(entry,validate):
    with raster(entry,validate) as image:
        orientation=image.getexif().get(274,1);width,height=image.size
        result=dict(supported=True,width=width,height=height,format=image.format,orientation=orientation if type(orientation) is int and 1<=orientation<=8 else 1)
    validate();return result


def render_plan(size,strokes):
    """Conservative raster work units, including wide joins, before any drawing."""
    width,height=size;work=0;plan=[]
    for stroke in strokes:
        points=[(round(x*(width-1)),round(y*(height-1))) for x,y in stroke['points']];pen=max(1,round(stroke['width']*min(size)))
        if stroke['tool']=='rectangle':
            (x1,y1),(x2,y2)=points;distance=2*(abs(x2-x1)+abs(y2-y1));cost=distance*pen+4*pen*pen
        else:cost=sum((math.hypot(b[0]-a[0],b[1]-a[1])+pen)*pen for a,b in zip(points,points[1:]))+pen*pen
        work+=cost
        if work>MAX_RENDER_WORK:raise ResourceUnavailable('policy_limit','This drawing is too complex at this image size; undo or use fewer strokes')
        plan.append((stroke,points,pen))
    return plan

def publish_copy(image,strokes,entry,destination,current,check_cancel):
    from PIL import ImageDraw
    plan=render_plan(image.size,strokes)
    current()
    draw=ImageDraw.Draw(image)
    for stroke,points,width in plan:
        check_cancel()
        if stroke['tool']=='pen':
            radius=(width-1)/2
            for first,second in zip(points,points[1:]):
                check_cancel();draw.line((first,second),fill=stroke['color'],width=width)
                if width>1:draw.ellipse((first[0]-radius,first[1]-radius,first[0]+radius,first[1]+radius),fill=stroke['color'])
            if width>1:
                last=points[-1];draw.ellipse((last[0]-radius,last[1]-radius,last[0]+radius,last[1]+radius),fill=stroke['color'])
        else:
            (x1,y1),(x2,y2)=points;draw.rectangle((min(x1,x2),min(y1,y2),max(x1,x2),max(y1,y2)),outline=stroke['color'],width=width)
    current()
    try:parent=destination.parent.resolve(strict=True);directory=os.open(parent,os.O_RDONLY|os.O_DIRECTORY)
    except PermissionError as exc:raise ResourceUnavailable('permission_denied','Permission to save in that folder was denied') from exc
    except (FileNotFoundError,NotADirectoryError) as exc:raise ResourceUnavailable('destination_unavailable','The destination folder is unavailable; choose another folder') from exc
    temporary='.lumen-markup-'+secrets.token_hex(16)+'.png';created=False
    try:
        if (parent/destination.name)==entry['path'].resolve():raise ResourceUnavailable('target_exists','Choose a different filename; the original is never overwritten')
        fd=os.open(temporary,os.O_WRONLY|os.O_CREAT|os.O_EXCL|os.O_NOFOLLOW,0o600,dir_fd=directory)
        created=True
        with os.fdopen(fd,'wb') as stream:image.save(stream,format='PNG');stream.flush();os.fsync(stream.fileno())
        current()
        try:resolved_parent=destination.parent.resolve(strict=True);parent_now=resolved_parent.stat()
        except OSError as exc:raise ResourceUnavailable('destination_changed','The destination folder changed; choose it again') from exc
        opened=os.fstat(directory)
        if resolved_parent!=parent or (parent_now.st_dev,parent_now.st_ino)!=(opened.st_dev,opened.st_ino):raise ResourceUnavailable('destination_changed','The destination folder changed; choose it again')
        try:os.link(temporary,destination.name,src_dir_fd=directory,dst_dir_fd=directory,follow_symlinks=False)
        except FileExistsError as exc:raise ResourceUnavailable('target_exists','That filename already exists; choose a new name') from exc
        try:os.fsync(directory);warning=''
        except OSError:warning='The copy was saved, but the folder could not acknowledge durable storage'
        return dict(saved=True,path=str(parent/destination.name),message='Saved annotated copy',warning=warning)
    except PermissionError as exc:raise ResourceUnavailable('permission_denied','Permission to save in that folder was denied') from exc
    finally:
        if created:
            try:os.unlink(temporary,dir_fd=directory)
            except FileNotFoundError:pass
        os.close(directory)
