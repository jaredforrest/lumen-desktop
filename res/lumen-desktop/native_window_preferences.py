"""Apply declared appearance preferences before startup; preserve profile data."""
import json
import os
import socket
import re
from pathlib import Path
import sys
import tempfile


def update(path, key, value):
    data = json.loads(path.read_text()) if path.exists() else {}
    if not isinstance(data, dict):
        raise ValueError('Expected an application preferences object')
    target = data
    for part in key[:-1]:
        target = target.setdefault(part, {})
        if not isinstance(target, dict):
            raise ValueError('Expected a window preferences object')
    if target.get(key[-1]) == value:
        return
    target[key[-1]] = value
    write(path, json.dumps(data, ensure_ascii=False))


def write(path, text):
    path.parent.mkdir(parents=True, exist_ok=True)
    fd, temporary = tempfile.mkstemp(dir=path.parent)
    try:
        with os.fdopen(fd, 'w') as stream:
            stream.write(text)
            stream.flush()
            os.fsync(stream.fileno())
        os.replace(temporary, path)
    finally:
        Path(temporary).unlink(missing_ok=True)


def running(root):
    lock = root / 'SingletonLock'
    if not lock.is_symlink():
        return lock.exists()
    try:
        host, process = os.readlink(lock).rsplit('-', 1)
        if host != socket.gethostname():
            return True
        os.kill(int(process), 0)
        return True
    except ProcessLookupError:
        # A crash or reboot can leave a stale Chromium/Electron lock.
        return False
    except (PermissionError, ValueError, OSError):
        return True


def main():
    config = Path(os.environ.get('XDG_CONFIG_HOME', str(Path.home() / '.config')))
    app = sys.argv[1]
    if app == 'brave':
        root = config / 'BraveSoftware/Brave-Browser'
        args = sys.argv[2:]
        for i, arg in enumerate(args):
            if arg.startswith('--user-data-dir='):
                root = Path(arg.split('=', 1)[1]).expanduser()
            elif arg == '--user-data-dir' and i + 1 < len(args):
                root = Path(args[i + 1]).expanduser()
        # The running browser owns its profile. Apply when it starts after logout.
        if running(root):
            return
        profiles = list(root.glob('*/Preferences')) or [root / 'Default/Preferences']
        appearance = json.loads(Path(__file__).with_name('brave-appearance.json').read_text())
        for profile in profiles:
            for key, value in appearance['profile'].items():
                update(profile, key.split('.'), value)
        for key, value in appearance['local_state'].items():
            update(root / 'Local State', key.split('.'), value)
    elif app == 'gimp':
        if any(arg in ('--help','--version','-v','--dump-gimprc') for arg in sys.argv[2:]):
            return
        for process in Path('/proc').iterdir():
            if not process.name.isdigit():
                continue
            try:
                if process.stat().st_uid == os.geteuid() and (process / 'exe').resolve().name.lstrip('.').startswith('gimp'):
                    return
            except (OSError, PermissionError):
                pass
        path = Path(os.environ.get('GIMP3_DIRECTORY', str(config / 'GIMP/3.2'))) / 'gimprc'
        args=sys.argv[2:]
        for i,arg in enumerate(args):
            if arg.startswith('--gimprc='):path=Path(arg.split('=',1)[1])
            elif arg in ('--gimprc','-g') and i+1<len(args):path=Path(args[i+1])
        original=path.read_text() if path.exists() else ''
        text=re.sub(r'(?m)^[ \t]*\(theme-color-scheme\s+(?:light|dark|gray|system)\s*\)[ \t]*$', '(theme-color-scheme system)', original)
        if text==original and '(theme-color-scheme system)' not in text:
            text=text.rstrip()+'\n(theme-color-scheme system)\n'
        if text!=original:write(path,text)
    elif app == 'obsidian':
        root = config / 'obsidian'
        if running(root):
            return
        update(root / 'obsidian.json', ['frame'], 'native')
    else:
        raise ValueError('Unsupported window preferences adapter')


if __name__ == '__main__':
    main()
