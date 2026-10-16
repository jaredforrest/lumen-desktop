"""Local Files operations. No shell evaluation, overwrite or permanent-delete API."""
import codecs, json, mimetypes, os, stat, subprocess, sys
from pathlib import Path
from gi.repository import Gio
sys.path.insert(0, str(Path(__file__).resolve().parent))
from file_jobs import remote as job_request, trash_items
import file_clipboard

LIMIT = 5000

def path(value):
    if not isinstance(value, str) or not value or '\0' in value:
        raise ValueError("A local path is required")
    p = Path(os.path.abspath(os.path.expanduser(value)))
    return p

def name(value):
    if not isinstance(value, str) or value in ('', '.', '..') or '/' in value or '\0' in value:
        raise ValueError("Enter a name without a slash")
    return value

def listing(data):
    if data.get('path') == 'trash:':
        return dict(path='trash:', parent=str(Path.home()), rows=trash_items(), breadcrumbs=[],
                    writable=False, limited=False, unreadable=0)
    folder = path(data.get('path', str(Path.home())))
    rows, unreadable, scanned, limited = [], 0, 0, False
    query = str(data.get('query', '')).casefold()
    with os.scandir(folder) as entries:
        for entry in entries:
            scanned += 1
            if scanned > 100000:
                limited = True
                break
            if not data.get('hidden') and entry.name.startswith('.'):
                continue
            if query and query not in entry.name.casefold():
                continue
            try:
                info = entry.stat(follow_symlinks=False)
                directory = entry.is_dir()
                rows.append(dict(name=entry.name, path=entry.path, uri=Path(entry.path).as_uri(), directory=directory,
                                 link=entry.is_symlink(), size=info.st_size, mtime=info.st_mtime,
                                 kind="Folder" if directory else mimetypes.guess_type(entry.name)[0] or "Document"))
            except OSError:
                unreadable += 1
            if len(rows) >= LIMIT:
                limited = True
                break
    sort = data.get('sort', 'name')
    key = {'name': lambda r:r['name'].casefold(), 'modified':lambda r:r['mtime'],
           'size':lambda r:r['size'], 'kind':lambda r:(r['kind'],r['name'].casefold())}.get(sort)
    if key is None:
        raise ValueError("Unknown sort order")
    rows.sort(key=lambda r:r['name'].casefold())
    rows.sort(key=key, reverse=bool(data.get('descending')))
    rows.sort(key=lambda r:not r['directory'])
    return dict(path=str(folder), parent=str(folder.parent), rows=rows,
                limited=limited, unreadable=unreadable, writable=os.access(folder,os.W_OK),
                breadcrumbs=[dict(name=p.name or '/', path=str(p)) for p in reversed([folder,*folder.parents])])

from file_preview import preview

def file(p):return Gio.File.new_for_path(str(p))

def copy_item(source,destination):
    # mkdir and GIO's no-overwrite copy fail if any target already exists.
    if source.is_dir() and not source.is_symlink():
        info = source.stat(follow_symlinks=False)
        # Keep partially copied trees private and writable, even when the source
        # is read-only. Restore metadata only after children have been copied.
        destination.mkdir(mode=0o700)
        destination.chmod(0o700)
        with os.scandir(source) as entries:
            for entry in entries:copy_item(Path(entry.path),destination/entry.name)
        destination.chmod(stat.S_IMODE(info.st_mode))
        os.utime(destination, ns=(info.st_atime_ns, info.st_mtime_ns), follow_symlinks=False)
    else:
        file(source).copy(file(destination),Gio.FileCopyFlags.NOFOLLOW_SYMLINKS,None,None,None)

def handle(data):
    action=data.get('action')
    if action in ('preferences', 'save-preferences'):
        import fcntl, tempfile
        file=Path(os.environ.get('XDG_STATE_HOME',str(Path.home()/'.local/state')))/'lumen-desktop/session/files.json'
        file.parent.mkdir(parents=True,exist_ok=True,mode=0o700)
        with file.with_suffix('.lock').open('a') as lock:
            fcntl.flock(lock,fcntl.LOCK_EX)
            try:prefs=json.loads(file.read_text())
            except (OSError,ValueError):prefs={}
            if not isinstance(prefs,dict):prefs={}
            if action=='save-preferences':
                incoming=data.get('preferences',{})
                allowed={'view':['grid','list','columns','gallery'],'sort':['name','modified','size','kind'],'descending':[True,False]}
                if not isinstance(incoming,dict) or any(key not in allowed for key in incoming):
                    raise ValueError('Unknown file preference')
                for key,value in incoming.items():
                    if value not in allowed[key] or (key=='descending' and type(value) is not bool):
                        raise ValueError('Invalid file preference: '+key)
                prefs.update(incoming)
                fd,temp=tempfile.mkstemp(dir=file.parent)
                with os.fdopen(fd,'w') as stream:json.dump(prefs,stream);stream.flush();os.fsync(stream.fileno())
                os.replace(temp,file)
            return dict(preferences=prefs)
    if action=='column-list':return listing(data)
    if action in ('job-start', 'job-status', 'job-cancel'):return job_request(data)
    if action=='clipboard-paste':return job_request(dict(file_clipboard.read(), action='job-start', destination=data['destination'], conflict=data.get('conflict','skip'),requestId=data.get('requestId')))
    if action=='drop':return job_request(dict(action='job-start',operation='copy',paths=file_clipboard.paths_from_uris(data['uris']),destination=data['destination'],requestId=data.get('requestId')))
    if action=='clipboard-write':return file_clipboard.write(data['paths'], data['operation'])
    if action=='clipboard-read':return file_clipboard.read()
    if action=='volumes':
        rows=[]
        for mount in Gio.VolumeMonitor.get().get_mounts():
            volume=mount.get_volume();device=volume.get_identifier('unix-device') if volume else None
            local=mount.get_root().get_path()
            if local and device:rows.append(dict(name=mount.get_name(),path=local,device=device,eject=mount.can_unmount()))
        return dict(volumes=rows)
    if action=='eject':
        mount=next((m for m in Gio.VolumeMonitor.get().get_mounts() if m.get_root().get_path()==data['path']),None)
        if not mount or not mount.can_unmount() or not mount.get_volume():raise ValueError('This volume cannot be unmounted')
        device=mount.get_volume().get_identifier('unix-device')
        subprocess.run(['udisksctl','unmount','--block-device',device,'--no-user-interaction'],check=True,capture_output=True,timeout=20)
        return dict(ok=True)
    if action in ('applications','open-with'):
        p=path(data['path']);mime=mimetypes.guess_type(p.name)[0] or 'application/octet-stream'
        applications=[a for a in Gio.AppInfo.get_all_for_type(mime) if a.should_show() and a.get_id()]
        if action=='applications':return dict(applications=[dict(id=a.get_id(),name=a.get_display_name()) for a in applications])
        app=next((a for a in applications if a.get_id()==data.get('application')),None)
        if app is None:raise ValueError('This application is no longer available')
        filename=app.get_filename()
        subprocess.run((['lumen-launch-app','--'] if os.environ.get('LUMEN_LAUNCH_SCOPES')=='1' else []) + ['gio','launch',filename,p.as_uri()],check=True,timeout=12,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
        return dict(ok=True)
    if action=='list':return listing(data)
    if action=='preview':return preview(data['path'], data.get('page',1))
    p=path(data['path'])
    if action=='info':
        import pwd,grp,datetime
        info=p.lstat()
        def identity(lookup,key):
            try:return lookup(key)[0]
            except KeyError:return str(key)
        def date(stamp):return datetime.datetime.fromtimestamp(stamp).astimezone().strftime('%B %d, %Y at %H:%M:%S %Z')
        return dict(info=dict(name=p.name,location=str(p.parent),kind='Symbolic link' if p.is_symlink() else 'Folder' if p.is_dir() else mimetypes.guess_type(p.name)[0] or 'Document',size=str(info.st_size)+' bytes',modified=date(info.st_mtime),permissions=stat.filemode(info.st_mode),owner=identity(pwd.getpwuid,info.st_uid),group=identity(grp.getgrgid,info.st_gid),target=os.readlink(p) if p.is_symlink() else ''))
    if action=='open':
        if not p.exists():raise FileNotFoundError("This item is no longer available")
        subprocess.run((['lumen-launch-app','--'] if os.environ.get('LUMEN_LAUNCH_SCOPES') == '1' else []) + ['xdg-open',p.as_uri()],check=True,timeout=12,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
        return dict(ok=True)
    if action=='mkdir':
        target=p/name(data['name'])
        target.mkdir()
        return dict(ok=True,selected=str(target))
    if action=='rename':
        target=p.parent/name(data['name'])
        if target==p:return dict(ok=True,selected=str(p))
        file(p).move(file(target),Gio.FileCopyFlags.NOFOLLOW_SYMLINKS,None,None,None)
        return dict(ok=True,selected=str(target))
    if action in ('copy','move'):
        folder=path(data['destination'])
        if not folder.is_dir():raise ValueError("The destination is not a folder")
        destination=folder/p.name
        if destination==p or destination.resolve()==p.resolve():
            raise ValueError("Choose a different destination folder")
        if p.is_dir() and not p.is_symlink() and p.resolve() in destination.resolve().parents:
            raise ValueError("A folder cannot be placed inside itself")
        if os.path.lexists(destination):raise FileExistsError("An item with this name already exists; nothing was replaced")
        if action=='copy':
            try:copy_item(p,destination)
            except Exception as exc:
                raise RuntimeError("Copy stopped. Any files already copied remain at "+str(destination)+". "+str(exc)) from exc
        else:file(p).move(file(destination),Gio.FileCopyFlags.NOFOLLOW_SYMLINKS,None,None,None)
        return dict(ok=True,selected=str(destination))
    if action=='trash':
        if p==p.parent or p==Path.home():raise ValueError("This location cannot be moved to Trash")
        trash=Path(os.environ.get('XDG_DATA_HOME',str(Path.home()/'.local/share')))/'Trash'
        result=subprocess.run(['trash-put','--trash-dir',str(trash),'--',str(p)],capture_output=True,text=True)
        if result.returncode:raise RuntimeError(result.stderr.strip() or "Unable to move this item to Trash")
        return dict(ok=True)
    raise ValueError("Unsupported Files action")

def main():
    import threading
    from concurrent.futures import ThreadPoolExecutor
    from file_watch import Watcher
    output_lock=threading.Lock()
    watch_lock=threading.Lock()
    latest_list=[None]
    def emit(message):
        with output_lock:
            print(json.dumps(message,ensure_ascii=True),flush=True)
    watcher=Watcher(lambda directory:emit(dict(event='changed',path=directory)))
    def dispatch(request):
        try:
            result=handle(request)
            if request.get('action')=='list':
                from file_jobs import trash_root
                with watch_lock:
                    if request.get('id')==latest_list[0]:
                        watcher.set_paths([trash_root()/'files'] if result['path']=='trash:' else [result['path']])
        except Exception as exc:
            result={'error':str(exc)}
        emit(dict(result,id=request.get('id'),action=request.get('action')))
    with ThreadPoolExecutor(max_workers=4) as pool:
        try:
            for line in sys.stdin:
                try:
                    request=json.loads(line)
                    if not isinstance(request,dict):raise ValueError('Invalid request')
                    if request.get('action')=='list':
                        with watch_lock:latest_list[0]=request.get('id')
                    pool.submit(dispatch,request)
                except Exception as exc:
                    emit(dict(error=str(exc)))
        finally:
            watcher.close()
if __name__=='__main__':main()
