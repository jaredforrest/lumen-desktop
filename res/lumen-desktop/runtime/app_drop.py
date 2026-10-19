"""Open readable local files with a selected app after validating the whole drop."""
import argparse
import json
import os
from pathlib import Path
import re
from urllib.parse import unquote, urlsplit


def local_file(uri):
    if not isinstance(uri,str) or len(uri)>32768 or '\0' in uri:
        raise ValueError('Invalid file URL')
    parts=urlsplit(uri)
    if parts.scheme!='file' or parts.netloc not in ('','localhost') or parts.query or parts.fragment:
        raise ValueError('Only local file URLs are supported')
    if re.search(r'%(?![0-9a-fA-F]{2})',parts.path):
        raise ValueError('Invalid file URL encoding')
    path=unquote(parts.path,errors='strict')
    if '\0' in path or not os.path.isabs(path):raise ValueError('Invalid local file path')
    file=Path(path)
    if not file.is_file() or not os.access(file,os.R_OK):
        raise ValueError('A dropped file is unavailable or unreadable')
    return file


def open_drop(desktop_id,uris,gio,desktop_info):
    if not isinstance(desktop_id,str) or not re.fullmatch(r'[A-Za-z0-9_.-]{1,256}',desktop_id):
        raise ValueError('Invalid application identity')
    if not isinstance(uris,list) or not uris or len(uris)>100:
        raise ValueError('Drop between one and 100 local files')
    files=[local_file(uri) for uri in uris]
    app=desktop_info.new(desktop_id if desktop_id.endswith('.desktop') else desktop_id+'.desktop')
    if not app or not (app.supports_files() or app.supports_uris()):
        raise ValueError('This application does not accept file drops')
    types=app.get_supported_types() or []
    for file in files:
        info=gio.File.new_for_path(str(file)).query_info('standard::content-type',gio.FileQueryInfoFlags.NONE,None)
        content=info.get_content_type()
        if not content or not any(gio.content_type_is_a(content,kind) for kind in types):
            raise ValueError('This application does not declare support for every dropped file type')
    if not app.launch_uris([file.as_uri() for file in files],gio.AppLaunchContext()):
        raise ValueError('The application could not open this drop')
    return {'ok':True,'count':len(files)}


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('desktop_id');parser.add_argument('uris')
    args=parser.parse_args()
    try:
        from gi.repository import Gio,GioUnix
        result=open_drop(args.desktop_id,json.loads(args.uris),Gio,GioUnix.DesktopAppInfo)
    except Exception as error:
        print(json.dumps({'error':str(error)}));return 1
    print(json.dumps(result));return 0


if __name__=='__main__':raise SystemExit(main())
