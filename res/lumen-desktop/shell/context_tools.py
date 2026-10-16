"""Explicit local text/image actions. Never monitor typing or dispatch detected links."""
import datetime
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile
from urllib.parse import quote, urlparse, unquote


def language(value=None):
    if value is None:value="en_GB"
    if value not in ("en_GB","en_US"):raise ValueError("Choose English (United Kingdom) or English (United States)")
    return value


def dictionary(locale="en_GB"):
    locale=language(locale)
    folder=Path(os.environ.get('XDG_STATE_HOME',str(Path.home()/'.local/state')))/'lumen-desktop/content'
    folder.mkdir(parents=True,exist_ok=True,mode=0o700)
    file=folder/('dictionary.'+locale+'.pws')
    if not file.exists():
        try:
            with file.open('x') as stream:stream.write('personal_ws-1.1 en 0 utf-8\n')
            file.chmod(0o600)
        except FileExistsError:pass
    return file


def check_spelling(text,locale='en_GB'):
    locale=language(locale)
    result=subprocess.run(['aspell','--lang='+locale,'--encoding=utf-8','--personal='+str(dictionary(locale)),'-a'],
        input='\n'.join('^'+line for line in text.splitlines())+'\n',text=True,capture_output=True,check=True,timeout=8)
    rows=[];seen=set()
    for line in result.stdout.splitlines():
        if line.startswith(('& ','# ')):
            word=line.split()[1]
            if word in seen:continue
            seen.add(word)
            rows.append(dict(word=word,suggestions=line.split(': ',1)[1].split(', ')[:8] if ': ' in line else []))
    return dict(spelling=rows)


def learn(word,locale='en_GB'):
    locale=language(locale)
    if not re.fullmatch(r"[A-Za-z][A-Za-z'-]{0,79}",word):raise ValueError('Enter one word to learn')
    subprocess.run(['aspell','--lang='+locale,'--encoding=utf-8','--personal='+str(dictionary(locale)),'-a'],
        input='*'+word+'\n#\n',text=True,capture_output=True,check=True,timeout=5)
    return dict(message='Added '+word+' to your '+('UK' if locale=='en_GB' else 'US')+' English dictionary')


def detectors(text):
    actions=[];seen=set()
    def add(label,uri):
        if uri not in seen and len(actions)<40:actions.append(dict(label=label,uri=uri));seen.add(uri)
    for match in re.finditer(r'https?://[^\s<>\"]+',text):
        uri=match.group().rstrip('.,;)');add(uri,uri)
    for address in re.findall(r'\b[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}\b',text):add('Email '+address,'mailto:'+quote(address,safe='@'))
    for number in re.findall(r'(?<!\w)\+?[0-9][0-9 ()-]{7,}[0-9](?!\w)',text):
        digits=re.sub(r'[^0-9+]','',number)
        if 10<=len(digits.lstrip('+'))<=15 and not re.fullmatch(r'\d{4}-\d{2}-\d{2}',number):add('Call '+number,'tel:'+digits)
    for value in re.findall(r'\b\d{4}-\d{2}-\d{2}\b',text):
        try:date=datetime.date.fromisoformat(value)
        except ValueError:continue
        # A date is informational until a calendar provider is selected.
        add(date.strftime('%A, %B %d, %Y'),'')
    return dict(actions=actions)


def ocr(value, page=1, seconds=None):
    from file_preview import preview
    p=Path(value).absolute()
    if not p.is_file() or p.stat().st_size>64*1024*1024:raise ValueError('Choose a local image, PDF or video up to 64 MB')
    with tempfile.TemporaryDirectory(prefix='lumen-ocr-') as directory:
        source=p
        if seconds is not None:
            position=float(seconds)
            if not 0<=position<=86400:raise ValueError('Invalid video position')
            source=Path(directory)/'frame.png'
            subprocess.run(['ffmpeg','-v','error','-ss',str(position),'-i',str(p),'-frames:v','1','-vf','scale=1600:1600:force_original_aspect_ratio=decrease',str(source)],capture_output=True,check=True,timeout=15)
        elif p.suffix.lower()=='.pdf':
            data=preview(p,page=page,resolution=2000)
            if data.get('type')!='image':raise ValueError(data.get('text','Unable to preview PDF'))
            source=Path(unquote(urlparse(data['uri']).path))
        result=subprocess.run(['prlimit','--as=1073741824','--cpu=20','--','tesseract',str(source),'stdout','-l','eng'],capture_output=True,text=True,check=True,timeout=25)
        return dict(text=result.stdout[:32768].strip(),message='Text recognized locally' if result.stdout.strip() else 'No text recognized')


def cutout(value):
    import uuid
    p=Path(value).absolute()
    if p.suffix.lower() not in ('.png','.jpg','.jpeg','.webp') or not p.is_file() or p.stat().st_size>32*1024*1024:
        raise ValueError('Choose a PNG, JPEG or WebP image up to 32 MB')
    cache=Path(os.environ.get('XDG_CACHE_HOME',str(Path.home()/'.cache')))/'lumen-desktop/subjects'
    cache.mkdir(parents=True,exist_ok=True,mode=0o700)
    target=cache/(uuid.uuid4().hex+'.png')
    try:
        subprocess.run(['prlimit','--as=4294967296','--cpu=60','--','rembg','i','-m','u2netp',str(p),str(target)],
            env={**os.environ,'OMP_NUM_THREADS':'4'},capture_output=True,check=True,timeout=65)
        target.chmod(0o600)
        return dict(subject=str(target),uri=target.as_uri(),message='Subject extracted locally · inspect the edges before saving')
    except BaseException:
        target.unlink(missing_ok=True);raise


def subject_source(value):
    cache=Path(os.environ.get('XDG_CACHE_HOME',str(Path.home()/'.cache')))/'lumen-desktop/subjects'
    p=Path(value)
    if not p.is_file() or p.is_symlink() or p.parent.resolve()!=cache.resolve() or not re.fullmatch(r'[0-9a-f]{32}\.png',p.name):
        raise ValueError('The subject preview is no longer available')
    return p


def save_subject(source,destination,overwrite=False):
    from gi.repository import Gio
    src=subject_source(source);dst=Path(destination).expanduser().absolute()
    if dst.suffix.lower()!='.png':raise ValueError('Save the transparent image as PNG')
    Gio.File.new_for_path(str(src)).copy(Gio.File.new_for_path(str(dst)),Gio.FileCopyFlags.OVERWRITE if overwrite is True else Gio.FileCopyFlags.NONE,None,None,None)
    return dict(message='Saved '+str(dst))


def handle(data):
    action=data.get('action')
    if action=='cutout':return cutout(data['path'])
    if action=='save-subject':return save_subject(data['source'],data['destination'],data.get('overwrite') is True)
    if action=='copy-subject':
        with subject_source(data['source']).open('rb') as stream:subprocess.run(['wl-copy','--type','image/png'],stdin=stream,check=True,timeout=5)
        return dict(message='Transparent image copied')
    if action=='ocr':return ocr(data['path'],data.get('page',1),data.get('seconds'))
    text=str(data.get('text',''))
    if not text or len(text)>32768:raise ValueError('Enter between 1 and 32,768 characters')
    if action=='spelling':return check_spelling(text,language(data.get('language')))
    if action=='learn':return learn(text,language(data.get('language')))
    if action=='detect':return detectors(text)
    if action=='copy':
        subprocess.run(['wl-copy','--type','text/plain;charset=utf-8'],input=text.encode(),check=True,timeout=5)
        return dict(message='Copied')
    raise ValueError('Unknown contextual action')


if __name__=='__main__':
    import sys, signal
    def cancel(*_):raise SystemExit(0)
    signal.signal(signal.SIGTERM,cancel)
    os.umask(0o077)
    try:print(json.dumps(handle(json.loads(sys.stdin.readline()))))
    except Exception as exc:print(json.dumps(dict(error=str(exc))))
