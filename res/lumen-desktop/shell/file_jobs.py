"""Window-independent, no-overwrite file jobs and native personal Trash recovery."""
import configparser
import errno
import json
import os
from pathlib import Path
import shutil
import socket
import stat
import threading
import time
from urllib.parse import unquote, quote
import uuid
from gi.repository import Gio


def local_path(value):
    if not isinstance(value, str) or not value or '\0' in value:
        raise ValueError('A local path is required')
    return Path(os.path.abspath(os.path.expanduser(value)))


def signature(p):
    s = p.lstat()
    return (s.st_dev, s.st_ino, s.st_mode, s.st_size, s.st_mtime_ns)


def snapshot(p):
    result = {str(p): signature(p)}
    if p.is_dir() and not p.is_symlink():
        for directory, dirs, files in os.walk(p, followlinks=False):
            for name in dirs + files:
                child = Path(directory) / name
                result[str(child)] = signature(child)
    return result


def move_exact(src, dst):
    # Linux renameat2 is atomic and never replaces an existing destination, including
    # a dangling symlink. EXDEV is the only reason to fall back to copying.
    import ctypes
    libc = ctypes.CDLL(None, use_errno=True)
    if libc.renameat2(-100, os.fsencode(src), -100, os.fsencode(dst), 1):
        code = ctypes.get_errno()
        raise OSError(code, os.strerror(code), str(dst))


def trash_root():
    return Path(os.environ.get('XDG_DATA_HOME', str(Path.home()/'.local/share')))/'Trash'


def trash_items():
    rows = []
    root = trash_root()
    for info in sorted((root/'info').glob('*.trashinfo')):
        try:
            parser = configparser.ConfigParser(interpolation=None)
            parser.read(info)
            raw = unquote(parser['Trash Info']['Path'])
            if not raw.startswith('/'):
                # Legacy trash-cli --trash-dir records are relative to the volume
                # containing Trash, not to Files's working directory.
                top = root.resolve()
                while not os.path.ismount(top) and top != top.parent:
                    top = top.parent
                raw = str(top/raw)
            original = local_path(raw)
            item = root/'files'/info.name[:-10]
            s = item.lstat()
            rows.append(dict(name=original.name, path=str(item), uri=item.as_uri(), original=str(original),
                trashInfo=str(info), directory=item.is_dir(), link=item.is_symlink(), size=s.st_size,
                mtime=s.st_mtime, kind='Trashed item', deleted=parser['Trash Info'].get('DeletionDate', '')))
        except (OSError, ValueError, KeyError, configparser.Error):
            continue
    return rows


def restore_info(p):
    # Only restore records belonging to our own personal Trash; never trust a caller's
    # destination or arbitrary .trashinfo file as a deletion instruction.
    return next((r for r in trash_items() if r['path'] == str(p)), None)


class Cancelled(Exception):
    pass


class Jobs:
    def __init__(self, journal=None):
        self.lock = threading.RLock()
        self.jobs = {}
        self.tokens = {}
        self.undo = []
        self.journal = Path(journal) if journal else None
        self.journal_warning = ''
        if self.journal and self.journal.exists():
            try:
                rows=json.loads(self.journal.read_text())
                if not isinstance(rows,list):raise ValueError('Invalid history format')
                for job in rows[-40:]:
                    if not isinstance(job,dict) or not all(k in job for k in ('id','state','operation','errors','started')):continue
                    if job['state'] in ('running','cancelling'):
                        job['state']='interrupted'
                        job['errors']=list(job['errors'])+['Interrupted by a service or system exit. Check source and destination before retrying; no files were automatically changed during recovery.']
                    self.jobs[job['id']]=job
            except (OSError,ValueError,TypeError):
                self.journal_warning='File-operation history could not be read; existing files were not changed.'

    def persist(self):
        if not self.journal:return
        import tempfile
        self.journal.parent.mkdir(parents=True,exist_ok=True,mode=0o700)
        fd,temp=tempfile.mkstemp(dir=self.journal.parent)
        try:
            with os.fdopen(fd,'w') as stream:
                json.dump(list(self.jobs.values())[-40:],stream);stream.flush();os.fsync(stream.fileno())
            os.replace(temp,self.journal)
            directory=os.open(self.journal.parent,os.O_RDONLY | os.O_DIRECTORY)
            try:os.fsync(directory)
            finally:os.close(directory)
        finally:
            Path(temp).unlink(missing_ok=True)

    def state(self):
        with self.lock:
            return dict(jobs=[dict(j) for j in list(self.jobs.values())[-40:]], canUndo=bool(self.undo), historyWarning=self.journal_warning)

    def update(self, ident, **fields):
        with self.lock:
            self.jobs[ident].update(fields)
            try:self.persist()
            except OSError:self.journal_warning="File-operation history could not be saved; check disk space."

    def check(self, ident):
        if self.tokens[ident].is_set():
            raise Cancelled('Cancelled; uncopied source data was kept')

    def cancel(self, ident):
        with self.lock:
            if ident in self.tokens:
                self.tokens[ident].set()
        return self.state()

    def start(self, data):
        action = data.get('operation')
        if action not in ('copy', 'move', 'trash', 'restore', 'undo'):
            raise ValueError('Unknown file operation')
        paths = list(dict.fromkeys(str(local_path(p)) for p in data.get('paths', [])))
        if action != 'undo' and not paths:
            raise ValueError('Select at least one item')
        if len(paths) > 5000:
            raise ValueError('Select no more than 5,000 items per operation')
        # A selected parent subsumes its children; processing both can remove data twice.
        paths = [p for p in paths if not any(Path(q) in Path(p).parents and not Path(q).is_symlink() for q in paths if q != p)]
        if action in ('copy', 'move'):
            destination = local_path(data.get('destination'))
            if not destination.is_dir():
                raise ValueError('The destination is not a folder')
        if data.get('conflict', 'skip') not in ('skip', 'keep-both'):
            raise ValueError('Choose Skip or Keep Both; replacement is not supported')
        ident = uuid.uuid4().hex
        with self.lock:
            # Serialize mutations while allowing all browsing/preview requests to proceed.
            if any(j['state'] in ('running', 'cancelling') for j in self.jobs.values()):
                raise ValueError('Wait for the current file operation or cancel it first')
            self.tokens[ident] = threading.Event()
            self.jobs[ident] = dict(id=ident, operation=action, state='running', total=len(paths),
                completed=0, bytes=0, current='', errors=[], selected='', started=time.time(), sources=paths, destination=data.get('destination',''))
            try:self.persist()
            except OSError:
                self.jobs.pop(ident);self.tokens.pop(ident);raise ValueError('Unable to save operation history; no files were changed')
        threading.Thread(target=self.run, args=(ident, {**data, 'paths': paths}), daemon=False).start()
        return dict(job=ident, **self.state())

    def copy(self, ident, src, dst):
        self.check(ident)
        info = src.lstat()
        if stat.S_ISDIR(info.st_mode):
            dst.mkdir(mode=0o700)
            with os.scandir(src) as entries:
                for entry in entries:
                    self.copy(ident, Path(entry.path), dst/entry.name)
            dst.chmod(stat.S_IMODE(info.st_mode))
            os.utime(dst, ns=(info.st_atime_ns, info.st_mtime_ns), follow_symlinks=False)
        elif stat.S_ISLNK(info.st_mode):
            dst.symlink_to(os.readlink(src))
        elif stat.S_ISREG(info.st_mode):
            fd = os.open(src, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
            with os.fdopen(fd, 'rb') as inp:
                opened = os.fstat(inp.fileno())
                if not stat.S_ISREG(opened.st_mode) or (opened.st_dev, opened.st_ino) != (info.st_dev, info.st_ino):
                    raise ValueError('Source changed while copying')
                outfd = os.open(dst, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
                with os.fdopen(outfd, 'wb') as out:
                    while True:
                        self.check(ident)
                        chunk = inp.read(1024*1024)
                        if not chunk:
                            break
                        out.write(chunk)
                        with self.lock:
                            self.jobs[ident]['bytes'] += len(chunk)
                    out.flush()
                    os.fsync(out.fileno())
            dst.chmod(stat.S_IMODE(info.st_mode))
            os.utime(dst, ns=(info.st_atime_ns, info.st_mtime_ns), follow_symlinks=False)
        else:
            raise ValueError('Device files, sockets and pipes cannot be copied')

    def transfer(self, ident, src, dst, moving=False, conflict='skip'):
        if dst == src or dst.resolve() == src.resolve():
            if not moving and conflict == 'keep-both':
                pass
            else:
                raise ValueError('Choose a different destination')
        if src.is_dir() and not src.is_symlink() and src.resolve() in dst.resolve().parents:
            raise ValueError('A folder cannot be placed inside itself')
        if os.path.lexists(dst):
            if conflict != 'keep-both':
                raise FileExistsError('Skipped: an item with this name already exists')
            original = dst
            for i in range(2, 10002):
                dst = original.with_name(f'{original.stem} copy {i}{original.suffix}')
                if not os.path.lexists(dst):
                    break
            else:
                raise FileExistsError('No available copy name')
        before = snapshot(src)
        if moving:
            try:
                move_exact(src, dst)
                return dst
            except OSError as exc:
                if exc.errno != errno.EXDEV:
                    raise
        staging = dst.parent/('.lumen-copy-' + uuid.uuid4().hex)
        staging.mkdir(mode=0o700)
        try:
            partial = staging/'item'
            self.copy(ident, src, partial)
            self.check(ident)
            if before != snapshot(src):
                raise ValueError('Source changed during copying; source was kept and partial output discarded')
            move_exact(partial, dst)
            if moving:
                # Publishing the complete destination precedes any removal of source.
                if before != snapshot(src):
                    raise ValueError('Source changed; both source and completed destination were kept')
                # Remove only objects included in the verified copy. Newly created
                # children prevent rmdir instead of being swept up by recursive delete.
                for value, expected in sorted(before.items(),key=lambda item:len(Path(item[0]).parts),reverse=True):
                    original=Path(value)
                    current=signature(original)
                    if stat.S_ISDIR(expected[2]):
                        if current[:3]!=expected[:3]:raise ValueError('Source directory changed; remaining source data was kept')
                        original.rmdir()
                    else:
                        if current!=expected:raise ValueError('Source changed; remaining source data was kept')
                        original.unlink()
            return dst
        finally:
            if staging.exists():
                for directory, dirs, files in os.walk(staging, topdown=True, followlinks=False):
                    os.chmod(directory, 0o700)
                shutil.rmtree(staging)

    def run(self, ident, data):
        action = data['operation']
        errors, records = [], []
        try:
            if action == 'undo':
                with self.lock:
                    if not self.undo:
                        raise ValueError('No reversible move is available')
                    record = self.undo[-1]
                src, dst, expected = record
                if snapshot(dst) != expected:
                    raise ValueError('The moved item has changed; Undo cannot safely restore it')
                self.transfer(ident, dst, src, moving=True)
                with self.lock:
                    self.undo.pop()
                self.update(ident, completed=1, selected=str(src))
            else:
                for value in data['paths']:
                    self.check(ident)
                    p = local_path(value)
                    self.update(ident, current=p.name)
                    try:
                        if p == p.parent or p == Path.home():
                            raise ValueError('This location cannot be modified')
                        if action in ('copy', 'move'):
                            target = local_path(data['destination'])/p.name
                            target = self.transfer(ident, p, target, action == 'move', data.get('conflict', 'skip'))
                            if action == 'move':
                                records.append((p, target, snapshot(target)))
                        elif action == 'trash':
                            root = trash_root()
                            (root/'files').mkdir(parents=True, exist_ok=True, mode=0o700)
                            (root/'info').mkdir(parents=True, exist_ok=True, mode=0o700)
                            name = uuid.uuid4().hex
                            stored, info = root/'files'/name, root/'info'/(name+'.trashinfo')
                            # Store a recoverable absolute location before moving the
                            # data. Standard personal-Trash tools can read this record.
                            with info.open('x') as stream:
                                os.chmod(info, 0o600)
                                stream.write('[Trash Info]\nPath='+quote(str(p), safe='/')+'\nDeletionDate='+time.strftime('%Y-%m-%dT%H:%M:%S')+'\n')
                                stream.flush()
                                os.fsync(stream.fileno())
                            try:
                                self.transfer(ident, p, stored, True)
                            except BaseException:
                                if not os.path.lexists(stored):
                                    info.unlink(missing_ok=True)
                                raise
                            target = None
                        else:
                            info = restore_info(p)
                            if not info:
                                raise ValueError('Trash record is no longer available')
                            target = local_path(info['original'])
                            if not target.parent.is_dir():
                                raise ValueError('The original parent folder no longer exists')
                            target = self.transfer(ident, p, target, True, data.get('conflict', 'skip'))
                            Path(info['trashInfo']).unlink()
                        with self.lock:
                            self.jobs[ident]['completed'] += 1
                        self.update(ident, selected=str(target) if target else '')
                    except Cancelled:
                        raise
                    except Exception as exc:
                        errors.append(f'{p.name}: {exc}')
            self.update(ident, state='partial' if errors else 'complete', errors=errors)
        except Cancelled as exc:
            self.update(ident, state='cancelled', errors=errors + [str(exc)])
        except Exception as exc:
            self.update(ident, state='failed', errors=errors + [str(exc)])
        finally:
            with self.lock:
                self.undo = (self.undo + records)[-20:]
                # Bound session memory without discarding active work.
                while len(self.jobs) > 40:
                    old = next(iter(self.jobs))
                    self.jobs.pop(old)
                    self.tokens.pop(old, None)


_jobs = Jobs()


def request(data):
    if data['action'] == 'job-start':
        return _jobs.start(data)
    if data['action'] == 'job-cancel':
        return _jobs.cancel(data['job'])
    return _jobs.state()


def remote(data):
    if os.environ.get('LUMEN_FILE_SERVICE') != '1':
        return request(data)
    with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as conn:
        conn.settimeout(5)
        conn.connect(str(Path(os.environ['XDG_RUNTIME_DIR'])/'lumen-file-operations.sock'))
        conn.sendall(json.dumps(data).encode()+b'\n')
        result = json.loads(conn.makefile('rb').readline(4*1024*1024))
        if result.get('error'):
            raise RuntimeError(result['error'])
        return result


def serve():
    global _jobs
    _jobs=Jobs(Path(os.environ.get('XDG_STATE_HOME',str(Path.home()/'.local/state')))/'lumen-desktop/operations/file-jobs.json')
    import signal
    os.umask(0o077)
    endpoint = Path(os.environ['XDG_RUNTIME_DIR'])/'lumen-file-operations.sock'
    endpoint.unlink(missing_ok=True)
    def stop(*_):
        for ident in list(_jobs.tokens):
            _jobs.cancel(ident)
        raise SystemExit(0)
    signal.signal(signal.SIGTERM, stop)
    with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as listener:
        listener.bind(str(endpoint))
        listener.listen(16)
        while True:
            conn, _ = listener.accept()
            with conn:
                conn.settimeout(3)
                try:
                    data = json.loads(conn.makefile('rb').readline(1024*1024))
                    result = request(data)
                except Exception as exc:
                    result = {'error': str(exc)}
                conn.sendall(json.dumps(result).encode()+b'\n')


if __name__ == '__main__':
    serve()
