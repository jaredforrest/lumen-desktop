"""Local filename index and bounded previews. JSON-lines transport; no shell evaluation."""
import json
import datetime
import codecs
from contextlib import contextmanager
import mimetypes
import os
from pathlib import Path
import sqlite3
import stat
import subprocess
import sys
import threading
import time
import unicodedata
sys.path.insert(0, str(Path(__file__).resolve().parent))
from file_preview import preview as shared_preview
from preview_snapshots import PreviewSnapshots,PreviewRequests
from urllib.parse import unquote,urlparse
from file_watch import Watcher
import file_tags
import shlex
import secrets
import hashlib

SKIP = {'node_modules', 'vendor', 'target', 'build', 'dist', '__pycache__', 'venv', 'Cache', 'Code Cache', 'GPUCache', 'component_crx_cache', 'blob_storage'}
LIMIT = 150000

def normalize(value):
    return ''.join(c for c in unicodedata.normalize('NFKD', value).casefold() if not unicodedata.combining(c))

def configured_roots():
    override = os.environ.get('LUMEN_SEARCH_ROOTS')
    names = json.loads(override) if override else [str(Path.home()/n) for n in ('Desktop', 'Documents', 'Downloads', 'Pictures', 'Music', 'Videos', 'data')]
    roots = []
    for name in names:
        path = Path(name).expanduser().resolve()
        if not any(path == r or r in path.parents for r in roots):
            roots = [r for r in roots if path not in r.parents]
            roots.append(path)
    return roots

class Index:
    def __init__(self, roots, database, emit=lambda message: None):
        self.roots, self.database, self.emit = roots, Path(database), emit
        self.source_generation = secrets.token_hex(16)
        self.snapshots = PreviewSnapshots(self.token,self.validate_snapshot)
        self.preview_requests = PreviewRequests(self.snapshots)
        self.database.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
        self.database.parent.chmod(0o700)
        self.lock = threading.Lock()
        self.scanning = False
        self.last_scan = 0
        self.count = 0
        self.limited = False
        self.unreadable = 0
        self.closed = threading.Event()
        self.dirty = set()
        self.dirty_timer = None
        self.watched = set()
        self.watch_generation = 0
        self.watch_requested = set()
        self.watch_pending = set()
        self.watcher = Watcher(self.changed)

        with self.connect() as db:
            db.execute('PRAGMA journal_mode=WAL')
            db.execute('CREATE TABLE IF NOT EXISTS files(path TEXT PRIMARY KEY, name TEXT, norm TEXT, basename TEXT, folder INTEGER, size INTEGER, mtime REAL, generation INTEGER)')
            if 'tags' not in {row[1] for row in db.execute('PRAGMA table_info(files)')}:
                db.execute("ALTER TABLE files ADD COLUMN tags TEXT NOT NULL DEFAULT ''")
            # A changed scope must not expose stale rows from a previous scope.
            db.execute('CREATE TABLE IF NOT EXISTS settings(key TEXT PRIMARY KEY, value TEXT)')
            scope = json.dumps([str(r) for r in roots])
            old = db.execute("SELECT value FROM settings WHERE key='scope'").fetchone()
            if not old or old[0] != scope:
                db.execute('DELETE FROM files')
                db.execute("INSERT OR REPLACE INTO settings VALUES ('scope', ?)", (scope,))
            self.count = db.execute('SELECT count(*) FROM files').fetchone()[0]
        self.database.chmod(0o600)

    def changed(self, directory):
        with self.lock:
            if self.closed.is_set():return
            self.dirty.add(directory)
            if self.dirty_timer is None:
                self.dirty_timer = threading.Timer(.35, self.refresh_dirty)
                self.dirty_timer.daemon = True
                self.dirty_timer.start()

    def update_watches(self, paths):
        with self.lock:
            if self.closed.is_set():return
            self.watched=set(paths)
            self.watch_pending=(self.watch_pending | (self.watched-self.watch_requested)) & self.watched
            self.watch_requested=set(self.watched)
            self.watch_generation+=1
            generation=self.watch_generation
            def armed(registered,failed):
                with self.lock:
                    if self.closed.is_set() or generation!=self.watch_generation:return
                    # New watches did not observe the preceding enumeration.
                    # Reconcile after registration, including new descendants.
                    self.dirty.update(self.watch_pending & registered)
                    self.watch_pending-=registered
                    if self.dirty and self.dirty_timer is None:
                        self.dirty_timer=threading.Timer(.35,self.refresh_dirty)
                        self.dirty_timer.daemon=True;self.dirty_timer.start()
                    if failed:self.limited=True;self.unreadable+=len(failed)
                if failed:self.emit(self.status())
            self.watcher.set_paths(self.watched,armed)

    def refresh_dirty(self):
        with self.lock:
            self.dirty_timer = None
            if self.closed.is_set():return
            if self.scanning:
                self.dirty_timer = threading.Timer(.5,self.refresh_dirty)
                self.dirty_timer.daemon=True;self.dirty_timer.start();return
            self.scanning = True
            pending, self.dirty = self.dirty, set()
        try:
            with self.connect() as db:
                stack = [Path(p) for p in pending]
                visited = 0
                while stack and visited < 10000 and not self.closed.is_set():
                    directory = stack.pop()
                    prefix=str(directory)+'/'
                    existing = {r[0] for r in db.execute("SELECT path FROM files WHERE substr(path,1,?)=? AND instr(substr(path,?),'/')=0",(len(prefix),prefix,len(prefix)+1))}
                    seen=set()
                    try:
                        self.allowed(directory)
                        with os.scandir(directory) as entries:
                            for entry in entries:
                                if entry.name.startswith('.') or entry.is_symlink():continue
                                folder=entry.is_dir(follow_symlinks=False)
                                if folder and entry.name in SKIP:continue
                                if not folder and not entry.is_file(follow_symlinks=False):continue
                                info=entry.stat(follow_symlinks=False);p=str(Path(entry.path).absolute());seen.add(p)
                                db.execute('INSERT OR REPLACE INTO files VALUES (?,?,?,?,?,?,?,?,?)',(p,entry.name,normalize(p),normalize(entry.name),int(folder),info.st_size,info.st_mtime,time.time_ns(),file_tags.index_text(p)))
                                visited+=1
                                if folder and p not in existing:stack.append(Path(p))
                        if len(self.watched)<8192:self.watched.add(str(directory))
                    except (OSError,ValueError):pass
                    for removed in existing-seen:
                        nested=removed+'/'
                        db.execute('DELETE FROM files WHERE path=? OR substr(path,1,?)=?',(removed,len(nested),nested))
                        self.watched={p for p in self.watched if p!=removed and not p.startswith(nested)}
                self.count=db.execute('SELECT count(*) FROM files').fetchone()[0]
                if stack:self.limited=True
            self.update_watches(self.watched)
        except (OSError,sqlite3.Error):
            self.unreadable+=1
        finally:
            self.scanning=False;self.emit(self.status())

    @contextmanager
    def connect(self):
        connection=sqlite3.connect(self.database,timeout=3)
        try:
            with connection:yield connection
        finally:connection.close()

    def status(self):
        return {'event': 'index', 'scanning': self.scanning, 'count': self.count,
                'limited': self.limited, 'unreadable': self.unreadable,
                'roots': [str(r) for r in self.roots]}

    def refresh(self, force=False):
        with self.lock:
            if self.closed.is_set() or self.scanning or (not force and time.monotonic()-self.last_scan < 60):
                return
            self.scanning = True
        self.emit(self.status())
        threading.Thread(target=self.scan, daemon=True).start()

    def scan(self):
        generation = time.time_ns()
        count, unreadable, limited, batch = 0, 0, False, []
        watched=set()
        stack = list(reversed([r for r in self.roots if r.is_dir()]))
        try:
            with self.connect() as db:
                while stack and not self.closed.is_set():
                    directory = stack.pop()
                    if len(watched)<8192:watched.add(str(directory))
                    try:
                        with os.scandir(directory) as entries:
                            for entry in entries:
                                if entry.name.startswith('.') or entry.is_symlink():
                                    continue
                                folder = entry.is_dir(follow_symlinks=False)
                                if folder and entry.name in SKIP:
                                    continue
                                if not folder and not entry.is_file(follow_symlinks=False):
                                    continue
                                try:
                                    info = entry.stat(follow_symlinks=False)
                                except OSError:
                                    unreadable += 1
                                    continue
                                path = str(Path(entry.path).absolute())
                                batch.append((path, entry.name, normalize(path), normalize(entry.name), int(folder), info.st_size, info.st_mtime, generation, file_tags.index_text(path)))
                                count += 1
                                if folder:
                                    stack.append(Path(path))
                                if len(batch) >= 500:
                                    db.executemany('INSERT OR REPLACE INTO files VALUES (?,?,?,?,?,?,?,?,?)', batch)
                                    db.commit()
                                    batch.clear()
                                    self.count = count
                                    self.emit(self.status())
                                if count >= LIMIT:
                                    limited = True
                                    break
                    except OSError:
                        unreadable += 1
                    if limited:
                        break
                if self.closed.is_set():
                    return
                db.executemany('INSERT OR REPLACE INTO files VALUES (?,?,?,?,?,?,?,?,?)', batch)
                # Do not discard rows beyond an interrupted or capped traversal.
                if not limited:
                    db.execute('DELETE FROM files WHERE generation != ?', (generation,))
                db.commit()
        except sqlite3.Error:
            unreadable += 1
        finally:
            self.count, self.unreadable, self.limited = count, unreadable, limited
            self.update_watches(watched)
            with self.lock:
                self.scanning = False
                self.last_scan = time.monotonic()
            self.emit(self.status())

    def allowed(self, path):
        p = Path(path)
        if not p.is_absolute() or p.is_symlink():
            raise ValueError('This file is no longer available')
        resolved = p.resolve(strict=True)
        if not any(resolved == r or r in resolved.parents for r in self.roots):
            raise ValueError('This file is outside the search folders')
        if any(part.startswith('.') or part in SKIP for r in self.roots if resolved == r or r in resolved.parents for part in resolved.relative_to(r).parts):
            raise ValueError('Hidden files are excluded from search')
        return resolved

    def token(self, path, info=None):
        info = info or path.stat(follow_symlinks=False)
        facts = (str(path), info.st_dev, info.st_ino, info.st_ctime_ns, info.st_mtime_ns, info.st_size)
        return self.source_generation + ':' + hashlib.sha256(json.dumps(facts).encode()).hexdigest()

    # This is an observed-object dispatch guard, not a capability held by an
    # external URI handler. That handler may reopen the path after dispatch.
    def selected(self, path, token):
        selected = self.allowed(path)
        if not isinstance(token, str) or not token or self.token(selected) != token:
            raise ValueError('The selected file changed. Search again before opening it.')
        return selected

    def validate_snapshot(self,path,token):
        # Request authorization remains selected(); internal renderer outputs
        # share this budget but live outside the configured search roots.
        if self.token(Path(path))!=token:raise ValueError('The preview resource changed')

    def search(self, query):
        terms=shlex.split(query[:256])
        kind=next((t[5:].lower() for t in terms if t.startswith('kind:')), '')
        extension=next((t[4:].lower().lstrip('.') for t in terms if t.startswith('ext:')), '')
        prefixes=('kind:','ext:','in:','since:','before:','tag:')
        tags=[t[4:].casefold() for t in terms if t.startswith('tag:')]
        if any(tag not in file_tags.IDS for tag in tags):raise ValueError('Unknown colour tag')
        words=normalize(' '.join(t for t in terms if not t.startswith(prefixes))).split()[:8]
        folder_query=next((t[3:] for t in terms if t.startswith('in:')), '')
        dates={key:next((t[len(key)+1:] for t in terms if t.startswith(key+':')), '') for key in ('since','before')}
        boundaries={}
        for key,value in dates.items():
            if value:
                try:boundaries[key]=datetime.datetime.combine(datetime.date.fromisoformat(value),datetime.time()).timestamp()
                except ValueError:raise ValueError(key+' requires YYYY-MM-DD')
        if (kind or extension or folder_query or boundaries or tags) and not words:words=['']
        if not words:
            return {'results': [], 'more': False}
        patterns = ['%'+w.replace('\\', '\\\\').replace('%', '\\%').replace('_', '\\_')+'%' for w in words]
        sql = 'SELECT path,name,folder,size,mtime FROM files WHERE '+' AND '.join("norm LIKE ? ESCAPE '\\'" for _ in words)
        if kind=='folder':sql+=' AND folder=1'
        elif kind=='file':sql+=' AND folder=0'
        if extension:
            sql += " AND lower(name) LIKE ? ESCAPE '\\'"
            patterns.append('%.'+extension.replace('\\','\\\\').replace('%','\\%').replace('_','\\_'))
        if folder_query:
            sql += " AND substr(norm,1,length(norm)-length(basename)) LIKE ? ESCAPE '\\'"
            patterns.append('%'+normalize(folder_query).replace('\\','\\\\').replace('%','\\%').replace('_','\\_')+'%')
        for tag in tags:
            sql += ' AND instr(tags,?)>0'
            patterns.append('|'+tag+'|')
        for key,value in boundaries.items():
            sql += ' AND mtime'+('>=' if key=='since' else '<')+'?'
            patterns.append(value)
        sql += ' ORDER BY CASE WHEN basename=? THEN 0 WHEN substr(basename,1,?)=? THEN 1 ELSE 2 END, folder DESC, length(name), basename, path LIMIT 121'
        q = " ".join(words).strip()
        with self.connect() as db:
            rows = db.execute(sql, (*patterns, q, len(q), q)).fetchall()
        results = []
        for path, name, folder, size, mtime in rows:
            try: token = self.token(self.allowed(path))
            except (OSError,ValueError): continue
            results.append({'kind': 'file', 'id': path, 'path': path, 'token': token, 'title': name,
                            'subtitle': str(Path(path).parent), 'folder': bool(folder), 'size': size,
                            'mtime': mtime, 'mime': 'inode/directory' if folder else mimetypes.guess_type(name)[0] or 'application/octet-stream'})
        return {'results': results[:80], 'more': len(results)>80}

    def preview(self, path, token=None,request_id=None,lease=None):
        selected=self.allowed(path);token=token or self.token(selected)
        self.selected(str(selected),token)
        info=selected.stat(follow_symlinks=False)
        mime=mimetypes.guess_type(selected.name)[0] or ''
        if stat.S_ISDIR(info.st_mode):
            fd=os.open(selected,os.O_RDONLY|os.O_DIRECTORY|os.O_NOFOLLOW)
            try:
                if self.token(selected,os.fstat(fd))!=token:raise ValueError('The selected folder changed')
                children=[]
                with os.scandir(fd) as entries:
                    for entry in entries:
                        if not entry.name.startswith('.'):
                            children.append(entry.name+('/' if entry.is_dir(follow_symlinks=False) else ''))
                        if len(children)>=21:break
                result=dict(type='folder',text='\n'.join(sorted(children[:20],key=str.casefold)) or 'Empty folder',truncated=len(children)>20)
            finally:os.close(fd)
        else:
            resource=self.snapshots.capture(selected,token,lease)
            if request_id is not None:self.preview_requests.retain_source(request_id,token)
            result=shared_preview(resource)
            uri=result.get('uri')
            if uri and uri!=resource.as_uri():
                parsed=urlparse(uri)
                if parsed.scheme!='file' or parsed.netloc:raise ValueError('Unsupported preview resource')
                rendered=Path(unquote(parsed.path));retained=self.snapshots.capture(rendered,self.token(rendered),lease)
                result['uri']=retained.as_uri()
        if mime.startswith(('audio/','video/')):result.pop('uri',None)
        self.selected(str(selected),token)
        if request_id is not None:
            published=result.get('uri')
            self.preview_requests.finish(request_id,[Path(unquote(urlparse(published).path))] if published else [])
        return dict(result,path=str(selected),name=selected.name,title=selected.name,size=info.st_size,mtime=info.st_mtime)

    def open(self, path, parent=False):
        path = self.allowed(path)
        if parent:
            path = path.parent
        # URI encoding prevents shell/option interpretation, including unusual filenames.
        subprocess.run((['lumen-launch-app', '--'] if os.environ.get('LUMEN_LAUNCH_SCOPES') == '1' else []) + ['xdg-open', path.as_uri()], check=True, timeout=8, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        return {'opened': True}

def main():
    os.umask(0o077)
    output_lock = threading.Lock()
    def emit(message):
        with output_lock:
            print(json.dumps(message, ensure_ascii=False), flush=True)
    cache = Path(os.environ.get('XDG_CACHE_HOME', Path.home()/'.cache'))/'lumen-desktop/search/index.sqlite'
    index = Index(configured_roots(), cache, emit)
    emit({'event': 'ready', 'previewGeneration':index.preview_requests.generation, **{k:v for k,v in index.status().items() if k!='event'}})
    index.refresh(force=True)
    from concurrent.futures import ThreadPoolExecutor
    def dispatch(request):
        try:
            action=request.get('action')
            if action=='search':
                index.refresh();result=index.search(str(request.get('query','')))
            elif action in ('preview','open','parent','browse'):
                selected=index.selected(request['path'],request.get('token'))
                if action=='preview':
                    with index.preview_requests.work(request['id']) as lease:
                        result=index.preview(str(selected),request.get('token'),request_id=request['id'],lease=lease)
                elif action=='browse':
                    target=selected.parent if request.get('parent') else selected
                    if not target.is_dir():raise ValueError('The selected folder is unavailable')
                    result={'browse':str(target)}
                else:result=index.open(str(selected),action=='parent')
            elif action=='retire-preview':
                index.preview_requests.retire(request['requests'],request.get('keepSourceToken'))
                result=dict(retired=True,previewGeneration=index.preview_requests.generation)
            elif action=='refresh':index.refresh(force=True);result={}
            else:raise ValueError('Unknown search command')
            emit({'id':request.get('id'),'action':action,**result})
        except (ValueError,KeyError,OSError,sqlite3.Error,subprocess.SubprocessError):
            if request.get('action')=='preview':index.preview_requests.retire([request.get('id')])
            emit({'id':request.get('id'),'action':request.get('action'),'previewGeneration':index.preview_requests.generation,'error':'The file is unavailable, the query is incomplete, or its application could not be opened'})
    try:
        with ThreadPoolExecutor(max_workers=3) as pool,ThreadPoolExecutor(max_workers=1) as previews:
            for line in sys.stdin:
                try:
                    request=json.loads(line)
                    if not isinstance(request,dict):raise ValueError('Invalid request')
                    if request.get('action') in ('preview','retire-preview'):
                        if request.get('previewGeneration')!=index.preview_requests.generation:raise ValueError('Invalid preview provider identity')
                        if request['action']=='preview':index.preview_requests.register(request.get('id'),request.get('token'))
                        else:
                            ids=request.get('requests')
                            if not isinstance(ids,list) or len(ids)>64:raise ValueError('Invalid preview release')
                            index.preview_requests.retire(ids,request.get('keepSourceToken'),wait=False)
                        previews.submit(dispatch,request)
                    else:pool.submit(dispatch,request)
                except ValueError:emit({'error':'Invalid search request'})
    finally:
        index.closed.set()
        index.watcher.close()
        index.preview_requests.close()

if __name__ == '__main__':
    main()
