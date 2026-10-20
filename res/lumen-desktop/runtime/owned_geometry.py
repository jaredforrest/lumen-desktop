"""Private placement records for explicitly registered singleton main windows.

Records are preferences, never native window authority. Every live dispatch must
still capture and revalidate the current process, session and native container.
"""
import contextlib
import fcntl
import json
import os
from pathlib import Path
import stat
import tempfile
import time

OWNERS = frozenset(('settings.main', 'calendar.main'))
MAX_BYTES = 16384


def owner_key(value):
    if not isinstance(value,str) or value not in OWNERS:
        raise ValueError('Unsupported owned geometry registration')
    return value


def integer(value, low, high):
    if type(value) is not int or not low <= value <= high:
        raise ValueError('Invalid owned geometry dimension')
    return value


def record(value):
    if not isinstance(value, dict) or set(value) != {'output', 'x', 'y', 'width', 'height'}:
        raise ValueError('Invalid owned geometry record')
    output = value['output']
    if not isinstance(output, str) or not output or len(output.encode()) > 256 or any(ord(c) < 32 for c in output):
        raise ValueError('Invalid owned geometry output')
    return dict(output=output, x=integer(value['x'], -32768, 32768),
                y=integer(value['y'], -32768, 32768),
                width=integer(value['width'], 1, 32768),
                height=integer(value['height'], 1, 32768))


def document(value):
    if not isinstance(value, dict) or set(value) != {'version', 'owners'} or value['version'] != 1 or type(value['version']) is not int:
        raise ValueError('Invalid owned geometry state')
    if not isinstance(value['owners'], dict) or not set(value['owners']).issubset(OWNERS):
        raise ValueError('Invalid owned geometry owners')
    return {'version': 1, 'owners': {owner_key(k): record(v) for k, v in value['owners'].items()}}


def state_directory():
    return Path(os.environ.get('XDG_STATE_HOME', str(Path.home() / '.local/state'))) / 'lumen-desktop' / 'owned-geometry'


class Store:
    def __init__(self, directory=None):
        self.directory = Path(directory) if directory is not None else state_directory()
        self.path = self.directory / 'placements.json'

    @contextlib.contextmanager
    def locked(self):
        self.directory.mkdir(parents=True, exist_ok=True, mode=0o700)
        info = self.directory.lstat()
        if not stat.S_ISDIR(info.st_mode) or info.st_uid != os.getuid():
            raise ValueError('Owned geometry state directory is unsafe')
        if info.st_mode & 0o077:
            raise ValueError('Owned geometry state directory is not private')
        fd = os.open(self.directory / '.placements.lock', os.O_CREAT | os.O_RDWR | os.O_NOFOLLOW | os.O_CLOEXEC, 0o600)
        try:
            info = os.fstat(fd)
            if not stat.S_ISREG(info.st_mode) or info.st_uid != os.getuid() or info.st_nlink != 1 or info.st_mode & 0o077:
                raise ValueError('Owned geometry state lock is unsafe')
            deadline=time.clock_gettime(time.CLOCK_BOOTTIME)+.5
            while True:
                try:
                    fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
                    break
                except BlockingIOError:
                    if time.clock_gettime(time.CLOCK_BOOTTIME)>=deadline:
                        raise ValueError('Owned geometry state is busy')
                    time.sleep(.01)
            yield
        finally:
            os.close(fd)

    def _read(self):
        try:
            fd = os.open(self.path, os.O_RDONLY | os.O_NONBLOCK | os.O_NOFOLLOW | os.O_CLOEXEC)
        except FileNotFoundError:
            return {'version': 1, 'owners': {}}
        with os.fdopen(fd, 'rb') as stream:
            info = os.fstat(stream.fileno())
            if not stat.S_ISREG(info.st_mode) or info.st_uid != os.getuid() or info.st_nlink != 1 or info.st_mode & 0o077 or info.st_size > MAX_BYTES:
                raise ValueError('Owned geometry state file is unsafe')
            raw = stream.read(MAX_BYTES + 1)
        if len(raw) > MAX_BYTES:
            raise ValueError('Owned geometry state exceeds supported size')
        return document(json.loads(raw))

    def get(self, owner):
        owner_key(owner)
        with self.locked():
            return self._read()['owners'].get(owner)

    def put(self, owner, placement):
        owner_key(owner)
        placement = record(placement)
        with self.locked():
            current = self._read()
            if current['owners'].get(owner) == placement:
                return False
            current['owners'][owner] = placement
            name = None
            try:
                with tempfile.NamedTemporaryFile(mode='w', dir=self.directory, prefix='.owned-geometry-', delete=False) as stream:
                    name = stream.name
                    json.dump(current, stream, sort_keys=True)
                    stream.write('\n')
                    stream.flush()
                    os.fsync(stream.fileno())
                os.replace(name, self.path)
                name = None
                fd = os.open(self.directory, os.O_RDONLY | os.O_DIRECTORY | os.O_CLOEXEC)
                try:
                    os.fsync(fd)
                finally:
                    os.close(fd)
            finally:
                if name is not None:
                    os.unlink(name)
            return True


def eligible(row):
    """Only ordinary independent main placements are remembered or restored."""
    node, workspace, floating = row
    state = node.get('lumen_state') or {}
    relation = node.get('lumen_relationships') or {}
    pair = node.get('lumen_pair') or {}
    return (floating and node.get('type') == 'floating_con' and workspace is not None
            and workspace.get('name') != '__i3_scratch' and not node.get('fullscreen_mode')
            and node.get('scratchpad_state') in (None, 'none')
            and not pair.get('id') and not pair.get('survivor')
            and not any(state.get(k) for k in ('filled', 'zoomed', 'minimised', 'hidden', 'group_hidden', 'desktop_hidden'))
            and relation.get('parent_supported') is True and relation.get('parent') is None
            and relation.get('modal') is not True)


def observed(row, output):
    if not eligible(row):
        return None
    node, workspace, _ = row
    area, outer, client = workspace['rect'], node['rect'], node['window_rect']
    decoration = (node.get('deco_rect') or {}).get('height', 0)
    return record(dict(output=output, x=outer['x'] - area['x'],
                       y=outer['y'] - decoration - area['y'],
                       width=client['width'], height=client['height']))


def restore_plan(row, saved, min_width, min_height, margin_width=0, margin_height=0):
    """Restore into the current workspace only; a missing output cannot strand UI.

    The caller may select the saved connected output first through its exact
    captured native target. This function never invents or resurrects a desktop.
    """
    if not eligible(row):
        return None
    saved = record(saved)
    node, workspace, _ = row
    area, box, client = workspace['rect'], node['rect'], node['window_rect']
    decoration = (node.get('deco_rect') or {}).get('height', 0)
    pad_width = box['width'] - client['width']
    pad_height = box['height'] + decoration - client['height']
    if pad_width < 0 or pad_height < 0:
        raise ValueError('Invalid native geometry padding')
    minimum_width = integer(min_width, 1, 32768)
    minimum_height = integer(min_height, 1, 32768)
    if minimum_width + pad_width > area['width'] or minimum_height + pad_height > area['height']:
        return None
    available_width = max(minimum_width, area['width'] - max(0, margin_width) - pad_width)
    available_height = max(minimum_height, area['height'] - max(0, margin_height) - pad_height)
    width = max(minimum_width, min(saved['width'], available_width))
    height = max(minimum_height, min(saved['height'], available_height))
    outer_width, outer_height = width + pad_width, height + pad_height
    return dict(width=width, height=height, outerWidth=outer_width, outerHeight=outer_height,
                x=area['x'] + max(0, min(saved['x'], area['width'] - outer_width)),
                y=area['y'] + max(0, min(saved['y'], area['height'] - outer_height)))


def enabled():
    """Read the published preference without activating settings compilation."""
    path = Path(os.environ.get('LUMEN_APPEARANCE_STATE', str(state_directory().parent / 'appearance.json')))
    try:
        if path.stat().st_size > 1024 * 1024:
            return False
        data = json.loads(path.read_text())
        return data.get('windowsPreferences', {}).get('rememberOwnedGeometry') is True
    except (OSError, ValueError, AttributeError):
        return False


def output_for(tree, workspace_id):
    def visit(node, output=None):
        if node.get('type') == 'output':
            output = node.get('name')
        if node.get('type') == 'workspace' and node.get('id') == workspace_id:
            return output
        for child in node.get('nodes', []) + node.get('floating_nodes', []):
            found = visit(child, output)
            if found is not None:
                return found
        return None
    return visit(tree)
