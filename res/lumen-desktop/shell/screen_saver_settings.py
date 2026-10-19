"""Decorative presentation settings, independent of host authentication policy."""
import fcntl
import json
from pathlib import Path
import sys
from nix_settings import read_settings, set_section, runtime
sys.path.insert(0,str(Path(__file__).resolve().parent.parent/'notifications'))
from session_lock import SessionLockAuthority

DEFAULTS=dict(enabled=False,idleSeconds=300,motion=True)


def status():
    authority=SessionLockAuthority()
    try:lock=authority.snapshot()
    finally:authority.close()
    return dict(preferences=dict(DEFAULTS,**read_settings().get('screenSaver',{})),lock=lock,error='')


def handle(request):
    if not isinstance(request,dict):raise ValueError('A screen saver request object is required.')
    if request.get('action')=='set':
        key,value=request.get('key'),request.get('value')
        if key in ('enabled','motion'):
            valid=type(value) is bool
        elif key=='idleSeconds':
            valid=type(value) is int and 15<=value<=7200
        else:valid=False
        if not valid:raise ValueError('Unsupported screen saver preference or value.')
        with (runtime()/'screen-saver.lock').open('a') as lock:
            fcntl.flock(lock,fcntl.LOCK_EX)
            current=dict(DEFAULTS,**read_settings().get('screenSaver',{}));current[key]=value
            set_section('screenSaver',current)
    elif request.get('action')!='status':raise ValueError('Unsupported screen saver action.')
    return status()


def main():
    try:result=handle(json.loads(sys.stdin.readline()))
    except (OSError,ValueError,RuntimeError) as error:result=dict(error=str(error))
    print(json.dumps(result))

if __name__=='__main__':main()
