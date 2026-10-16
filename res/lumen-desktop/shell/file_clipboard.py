"""Interoperable URI clipboard; reject remote URLs and shell-like input."""
import os
from pathlib import Path
import select
import subprocess
from urllib.parse import unquote, urlsplit


def paths_from_uris(value):
    result = []
    for line in value.splitlines():
        if not line or line.startswith('#'):
            continue
        uri = urlsplit(line)
        if uri.scheme != 'file' or uri.netloc not in ('', 'localhost') or uri.query or uri.fragment:
            raise ValueError('Only local files can be pasted')
        path = unquote(uri.path)
        if not path.startswith('/') or '\0' in path:
            raise ValueError('Invalid file URI')
        result.append(path)
    if not result or len(result) > 5000:
        raise ValueError('The clipboard does not contain a supported file selection')
    return list(dict.fromkeys(result))


def read():
    def paste(mime):
        return subprocess.run(['wl-paste', '--no-newline', '--type', mime], capture_output=True, timeout=3, check=True).stdout.decode('utf-8')
    types = subprocess.run(['wl-paste', '--list-types'], capture_output=True, text=True, timeout=3, check=True).stdout.splitlines()
    action = 'copy'
    if 'x-special/gnome-copied-files' in types:
        header, _, body = paste('x-special/gnome-copied-files').partition('\n')
        if header not in ('cut', 'copy'):
            raise ValueError('Invalid file clipboard operation')
        action = 'move' if header == 'cut' else 'copy'
    elif 'text/uri-list' in types:
        body = paste('text/uri-list')
        if 'application/x-kde-cutselection' in types and paste('application/x-kde-cutselection').strip() == '1':
            action = 'move'
    else:
        raise ValueError('Copy files from a file manager before pasting')
    return dict(paths=paths_from_uris(body), operation=action)


def write(paths, operation):
    if operation not in ('copy', 'move'):
        raise ValueError('Unknown clipboard operation')
    if not paths or len(paths) > 5000:
        raise ValueError('Select files to copy')
    uris = [Path(p).absolute().as_uri() for p in paths]
    command = (['lumen-launch-app', '--'] if os.environ.get('LUMEN_LAUNCH_SCOPES') == '1' else [])
    child = subprocess.Popen(command + ['lumen-file-clipboard', operation, *uris], stdout=subprocess.PIPE,
                             stderr=subprocess.DEVNULL, start_new_session=True)
    if not select.select([child.stdout], [], [], 5)[0] or child.stdout.readline().strip() != b'ready':
        child.terminate()
        raise RuntimeError('The file clipboard could not be claimed')
    child.stdout.close()
    return dict(ok=True)
