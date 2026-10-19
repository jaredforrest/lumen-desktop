"""Open a declared file alias without modifying or deleting its target."""
import argparse
import json
import os
from pathlib import Path


def target(value):
    if not isinstance(value,str) or not value or len(value)>4096 or '\0' in value or not os.path.isabs(value):
        raise ValueError('A file alias requires an absolute local path')
    path=Path(value)
    if not path.is_file() or not os.access(path,os.R_OK):
        raise ValueError('This file alias target is unavailable or unreadable')
    return path


def open_alias(value,gio):
    path=target(value)
    if not gio.AppInfo.launch_default_for_uri(path.as_uri(),gio.AppLaunchContext()):
        raise ValueError('No application could open this file')
    return {'opened':True}


def main():
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('path');args=parser.parse_args()
    try:
        from gi.repository import Gio
        result=open_alias(args.path,Gio)
    except Exception as error:
        print(json.dumps({'error':str(error)}));return 1
    print(json.dumps(result));return 0


if __name__=='__main__':raise SystemExit(main())
