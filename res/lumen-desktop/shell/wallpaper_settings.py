"""Local image references and exact-display background selection.

Personal files stay in place. The installed consumer asset remains the fallback.
No image URL is fetched and display mode or host security policy is changed.
"""
import fcntl
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time
import warnings

from PIL import Image
from nix_settings import read_settings, set_section, runtime


def sway(*args):
    return json.loads(subprocess.check_output(['swaymsg', '-r', *args], timeout=5))


def identity(output):
    # Match the authoritative desktop catalog's display identity contract.
    values = [output.get(key, '') for key in ('make', 'model', 'serial')]
    if all(values) and values[2] not in ('Unknown', '0'):
        return 'display:' + json.dumps(values, separators=(',', ':'))
    return 'connector:' + output['name']


def key_for(value):
    return hashlib.sha256(value.encode()).hexdigest()


def image_info(path):
    if not isinstance(path, str) or not path.startswith('/') or any(c in path for c in '\n\r\x00'):
        raise ValueError('Choose an absolute local PNG or JPEG file.')
    stat = Path(path).stat()
    signature = [stat.st_dev, stat.st_ino, stat.st_size, stat.st_mtime_ns, stat.st_ctime_ns]
    directory = runtime() / 'wallpaper-metadata'
    directory.mkdir(mode=0o700, exist_ok=True)
    cache = directory / (key_for(path) + '.json')
    try:
        saved = json.loads(cache.read_text())
        if saved.get('signature') == signature:
            return saved['metadata']
    except (OSError, ValueError, KeyError):
        pass
    with warnings.catch_warnings():
        warnings.simplefilter('error', Image.DecompressionBombWarning)
        with Image.open(path) as image:
            if image.format not in ('PNG', 'JPEG') or getattr(image, 'n_frames', 1) != 1:
                raise ValueError('Only still PNG and JPEG images are supported.')
            if image.width * image.height > 64_000_000:
                raise ValueError('Image exceeds the 64 million pixel limit.')
            result = dict(width=image.width, height=image.height, format=image.format)
            image.verify()
        with Image.open(path) as decoded:
            decoded.load()
        with tempfile.NamedTemporaryFile(mode='w', dir=directory, delete=False) as pending:
            temporary = Path(pending.name)
            json.dump(dict(signature=signature, metadata=result), pending)
        temporary.replace(cache)
        return result


def fallback():
    return os.environ.get('LUMEN_WALLPAPER_FALLBACK', str(Path(__file__).with_name('wallpaper.png')))


def selection(settings, output):
    display = settings.get('displays', {}).get(key_for(identity(output)))
    return display['image'] if display and display['identity'] == identity(output) else settings.get('defaultImage')


def status():
    settings = read_settings().get('wallpaper', {})
    outputs = [o for o in sway('-t', 'get_outputs') if o.get('active')]
    connected = {identity(o) for o in outputs}
    rows = []
    for output in outputs:
        selected = selection(settings, output)
        try:
            info = image_info(selected) if selected else None
            error = ''
        except (OSError, ValueError, Image.DecompressionBombError, Image.DecompressionBombWarning) as failure:
            info, error = None, str(failure)
        rows.append(dict(identity=identity(output), key=key_for(identity(output)), name=output['name'],
                         scale=output.get('scale', 1), image=selected, metadata=info,
                         fallback=not bool(info), error=error))
    try:
        default_metadata = image_info(settings['defaultImage']) if settings.get('defaultImage') else None
        default_error = ''
    except (OSError, ValueError, Image.DecompressionBombError, Image.DecompressionBombWarning) as failure:
        default_metadata, default_error = None, str(failure)
    return dict(available=True, outputs=rows, settings=settings,
                defaultMetadata=default_metadata, defaultError=default_error,
                disconnected=[dict(key=k, **v) for k,v in settings.get('displays', {}).items() if v['identity'] not in connected],
                defaultImage=settings.get('defaultImage'), error='',
                capabilities={'perDisplay':True, 'perDesktop':False, 'formats':['PNG','JPEG'], 'maxPixels':64_000_000})


def browse(path=None):
    folder = Path(path or Path.home())
    if not folder.is_absolute():
        raise ValueError('Choose an absolute local folder.')
    selected = None
    if folder.is_file():
        selected = str(folder)
        folder = folder.parent
    entries = []
    for entry in folder.iterdir():
        try:
            directory = entry.is_dir()
            if directory or entry.suffix.lower() in ('.png', '.jpg', '.jpeg'):
                entries.append(dict(name=entry.name, path=str(entry), directory=directory))
        except OSError:
            continue
    entries.sort(key=lambda row:(not row['directory'], row['name'].casefold()))
    return dict(action='browse', folder=str(folder), parent=str(folder.parent), selected=selected,
                entries=entries[:1000], truncated=len(entries)>1000, error='')


def apply():
    with (runtime() / 'wallpaper-render.lock').open('a') as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        snapshot = status()
        directory = runtime() / 'wallpaper-images'
        directory.mkdir(mode=0o700, exist_ok=True)
        for output in snapshot['outputs']:
            image = output['image'] if not output['fallback'] else fallback()
            # Sway expands background paths with wordexp. A private safe alias
            # preserves arbitrary literal filenames without evaluating their text.
            reference = directory / (output['key'] + '.png')
            pending = reference.with_suffix('.new')
            pending.unlink(missing_ok=True)
            pending.symlink_to(image)
            pending.replace(reference)
            reply = sway('output ' + json.dumps(output['name']) + ' bg ' + json.dumps(str(reference)) + ' fill "#202024"')
            if not reply or not all(row.get('success') for row in reply):
                raise RuntimeError('The compositor rejected the background for ' + output['name'])
        return snapshot


def change(request):
    scope = request.get('scope')
    if scope not in ('default', 'display'):
        raise ValueError('Choose all displays or one connected display.')
    path = request.get('image')
    if path is not None:
        image_info(path)
    with (runtime() / 'wallpaper.lock').open('a') as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        current = read_settings().get('wallpaper', {})
        value = dict(defaultImage=current.get('defaultImage'), displays=dict(current.get('displays', {})))
        if scope == 'default':
            value['defaultImage'] = path
        else:
            target = request.get('identity')
            matches = [o for o in status()['outputs'] if o['identity'] == target]
            if len(matches) != 1:
                raise ValueError('The selected display is disconnected or ambiguous. Refresh and choose again.')
            key = key_for(target)
            if path is None:
                value['displays'].pop(key, None)
            else:
                value['displays'][key] = dict(identity=target, image=path)
        set_section('wallpaper', value)
    return apply()


def watch():
    previous = None
    while True:
        try:
            snapshot = status()
            signatures = []
            for row in snapshot['outputs']:
                try:
                    stat = Path(row['image'] or fallback()).stat()
                    stamp = (stat.st_mtime_ns, stat.st_size, stat.st_ino)
                except OSError:
                    stamp = None
                signatures.append((row, stamp))
            signature = json.dumps(signatures, sort_keys=True)
            if signature != previous:
                apply()
                previous = signature
        except (OSError, ValueError, RuntimeError, subprocess.SubprocessError) as error:
            print(json.dumps({'error':str(error)}), flush=True)
        time.sleep(2)


def main():
    if sys.argv[1:] == ['watch']:
        watch()
        return
    request = {}
    try:
        request = json.loads(sys.stdin.readline())
        if not isinstance(request, dict):
            raise ValueError('A wallpaper request object is required.')
        if request.get('action') == 'browse':
            result = browse(request.get('path'))
        elif request.get('action') == 'status':
            result = status()
        elif request.get('action') == 'set':
            result = change(request)
        else:
            raise ValueError('Unsupported wallpaper action.')
    except (OSError, ValueError, RuntimeError, subprocess.SubprocessError, Image.DecompressionBombError, Image.DecompressionBombWarning) as error:
        result = dict(action=request.get('action') if isinstance(request, dict) else '', available=False, error=str(error))
    print(json.dumps(result))


if __name__ == '__main__':
    main()
