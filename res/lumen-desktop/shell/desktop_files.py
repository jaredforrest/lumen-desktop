"""Resolve the user's desktop directory without evaluating configuration as code."""
import json,os
from pathlib import Path
from gi.repository import GLib

def directory():
    configured=GLib.get_user_special_dir(GLib.UserDirectory.DIRECTORY_DESKTOP)
    path=Path(configured) if configured else Path.home()/'Desktop'
    if not path.is_absolute():raise ValueError('The desktop directory must be an absolute local path')
    disabled=bool(configured) and path==Path.home()
    return {'path':str(path),'disabled':disabled,'exists':not disabled and path.is_dir(),'writable':not disabled and path.is_dir() and os.access(path,os.W_OK)}

if __name__=='__main__':
    try:print(json.dumps(directory(),ensure_ascii=True))
    except (OSError,ValueError) as error:print(json.dumps({'error':str(error)}));raise SystemExit(1)
