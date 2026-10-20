"""Anchor one captured directory listing to its owned descriptor."""
import contextlib
import os
from pathlib import Path
import stat
from captured_resource_actions import ResourceUnavailable,facts,parse_facts


def fd_facts(fd):
    value=os.fstat(fd)
    return (value.st_dev,value.st_ino,value.st_mode,value.st_size,value.st_mtime_ns,value.st_ctime_ns)


@contextlib.contextmanager
def scan(folder,expected=None):
    folder=Path(folder)
    if expected is None:
        with os.scandir(folder) as entries:yield entries,lambda entry:(entry.path,entry.path)
        return
    if not folder.is_absolute() or '\0' in str(folder):raise ResourceUnavailable('invalid_request','An absolute local directory is required')
    expected=parse_facts(expected)
    metadata=data=None
    try:
        metadata=os.open(folder,os.O_PATH|os.O_NOFOLLOW|os.O_CLOEXEC)
        observed=fd_facts(metadata)
        if observed!=expected:raise ResourceUnavailable('source_changed','The selected folder changed; refresh the stack')
        if not stat.S_ISDIR(observed[2]):raise ResourceUnavailable('unsupported','Captured navigation requires a local directory, not a symbolic link')
        # Only our already-validated directory descriptor is reopened for reading.
        data=os.open('/proc/self/fd/'+str(metadata),os.O_RDONLY|os.O_DIRECTORY|os.O_CLOEXEC)
        if fd_facts(data)!=expected:raise ResourceUnavailable('source_changed','The selected folder changed; refresh the stack')
        anchored=Path('/proc/self/fd')/str(data)
        with os.scandir(data) as entries:yield entries,lambda entry:(str(folder/entry.name),str(anchored/entry.name))
        if fd_facts(data)!=expected or facts(folder)!=expected:raise ResourceUnavailable('source_changed','The selected folder changed during listing; refresh the stack')
    except (FileNotFoundError,NotADirectoryError) as exc:
        raise ResourceUnavailable('source_changed','The selected folder is no longer available') from exc
    finally:
        if data is not None:os.close(data)
        if metadata is not None:os.close(metadata)
