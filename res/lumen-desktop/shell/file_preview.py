"""Shared, bounded Files/Search previews. All helpers are local and time limited."""
import codecs
import hashlib
import json
import mimetypes
import os
import re
from pathlib import Path
import stat
import subprocess
import tempfile
import xml.etree.ElementTree as ET
import zipfile


def limited_command(args):
    # The independent timeout remains alive if its caller is retired mid-preview.
    # It owns only this command's process group; no ambient group is signalled.
    return subprocess.run(['timeout', '--signal=KILL', '8s', 'prlimit', '--as=805306368', '--cpu=6', '--fsize=16777216', '--', *args],
                          capture_output=True, check=True, timeout=9).stdout


def preview(value, page=1, resolution=1400):
    page = max(1, min(100000, int(page)))
    resolution = max(600, min(2400, int(resolution)))
    p = Path(value).absolute()
    info = p.stat()
    result = dict(path=str(p), name=p.name, title=p.name, size=info.st_size,
                  mtime=info.st_mtime, type='generic', text='This format has no supported preview',reason='unsupported')
    if stat.S_ISDIR(info.st_mode):
        with os.scandir(p) as entries:
            children = []
            for entry in entries:
                if not entry.name.startswith('.'):
                    children.append(entry.name + ('/' if entry.is_dir(follow_symlinks=False) else ''))
                if len(children) >= 21:
                    break
        return dict(result, reason=None, type='folder', text='\n'.join(sorted(children[:20], key=str.casefold)) or 'Empty folder', truncated=len(children)>20)
    if not stat.S_ISREG(info.st_mode):
        return result
    mime = mimetypes.guess_type(p.name)[0] or ''
    result['mime'] = mime
    if mime.startswith('image/') and info.st_size>32*1024*1024:return dict(result,reason='policy_limit',text='Inline image previews support files up to32 MiB')
    if mime in ('image/png', 'image/jpeg', 'image/gif', 'image/webp', 'image/bmp') and info.st_size <= 32*1024*1024:
        return dict(result, reason=None, type='image', uri=p.as_uri(), text='')
    if mime == 'application/pdf' and info.st_size <= 64*1024*1024:
        cache = Path(os.environ.get('XDG_CACHE_HOME', str(Path.home()/'.cache')))/'lumen-desktop/previews'
        cache.mkdir(parents=True, exist_ok=True, mode=0o700)
        try:
            details = limited_command(['pdfinfo', str(p)]).decode('utf-8', errors='replace')
            match = re.search(r'^Pages:\s+(\d+)', details, re.MULTILINE)
            pages = int(match.group(1)) if match else 1
            page = min(page, pages)
            key = hashlib.sha256(f'{p}:{info.st_dev}:{info.st_ino}:{info.st_size}:{info.st_mtime_ns}:{page}:{resolution}'.encode()).hexdigest()
            target = cache/(key+'.png')
            if not target.exists():
                with tempfile.TemporaryDirectory(dir=cache) as temp:
                    output = Path(temp)/'page'
                    limited_command(['pdftoppm', '-f', str(page), '-singlefile', '-scale-to', str(resolution), '-png', str(p), str(output)])
                    output.with_suffix('.png').chmod(0o600)
                    os.replace(output.with_suffix('.png'), target)
                # Concurrent preview requests can prune the same cache entry.
                candidates = []
                for old in cache.glob('*.png'):
                    try:candidates.append((old.stat().st_mtime, old))
                    except FileNotFoundError:pass
                for _, old in sorted(candidates, reverse=True)[100:]:
                    old.unlink(missing_ok=True)
            return dict(result, reason=None, type='image', uri=target.as_uri(), text=f'Page {page} of {pages}', page=page, pages=pages, truncated=False)
        except subprocess.TimeoutExpired:
            return dict(result,reason='provider_timeout',text='PDF preview timed out')
        except (OSError, ValueError, subprocess.SubprocessError):
            return dict(result, reason='provider_failure', text='PDF preview unavailable (encrypted, damaged, too complex, or renderer unavailable)')
    if mime.startswith(('audio/', 'video/')):
        try:
            data = json.loads(limited_command(['ffprobe', '-v', 'error', '-show_entries',
                'format=duration:stream=codec_type,codec_name,width,height,sample_rate,channels', '-of', 'json', str(p)]))
            lines = []
            seconds = float(data.get('format', {}).get('duration', 0))
            lines.append(f'Duration: {int(seconds)//60}:{int(seconds)%60:02d}')
            for stream in data.get('streams', [])[:8]:
                label = stream.get('codec_type', '').title() + ': ' + stream.get('codec_name', '')
                if stream.get('width'):label += f" · {stream['width']} × {stream['height']}"
                if stream.get('channels'):label += f" · {stream['channels']} channels"
                lines.append(label)
            return dict(result, reason=None, type='media', uri=p.as_uri(), duration=seconds, text='\n'.join(lines))
        except subprocess.TimeoutExpired:
            return dict(result,reason='provider_timeout',text='Media preview timed out')
        except (ValueError, OSError, subprocess.SubprocessError):
            return dict(result, reason='provider_failure', text='Media information unavailable')
    if p.suffix.lower() in ('.docx', '.odt') and info.st_size <= 64*1024*1024:
        try:
            with zipfile.ZipFile(p) as archive:
                entry = archive.getinfo('word/document.xml' if p.suffix.lower()=='.docx' else 'content.xml')
                if entry.file_size > 4*1024*1024:
                    return dict(result, reason='policy_limit', text='Document is too large for a quick preview')
                xml = ET.fromstring(archive.read(entry))
                paragraphs = [''.join(node.itertext()) for node in xml.iter() if node.tag.rsplit('}',1)[-1] in ('p','h')]
                text = '\n'.join(paragraphs)
                return dict(result, reason=None, type='text', text=text[:8192] or 'No document text', truncated=len(text)>8192)
        except (OSError, KeyError, zipfile.BadZipFile, ET.ParseError, RuntimeError):
            return dict(result, reason='provider_failure', text='Document preview unavailable')
    fd = os.open(p, os.O_RDONLY | os.O_NONBLOCK)
    with os.fdopen(fd, 'rb') as stream:
        if not stat.S_ISREG(os.fstat(stream.fileno()).st_mode):
            return result
        raw = stream.read(8193)
    if b'\0' in raw:
        return result
    try:
        text = codecs.getincrementaldecoder('utf-8')().decode(raw[:8192], final=len(raw)<=8192)
    except UnicodeDecodeError:
        return result
    if any(ord(c)<32 and c not in '\n\r\t' for c in text):
        return result
    lines = text.splitlines()
    return dict(result, reason=None, type='text', text='\n'.join(lines[:80]), truncated=len(raw)>8192 or len(lines)>80)


if __name__=='__main__':
    import sys
    try:print(json.dumps(preview(sys.argv[1], int(sys.argv[2]) if len(sys.argv)>2 else 1)))
    except Exception as exc:print(json.dumps(dict(error=str(exc))))
