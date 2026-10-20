"""Describe a symbolic-link substitute without hiding an unavailable target."""
import errno
import os
import stat


def target_info(path):
    return os.stat(path)


def describe(path, info):
    if not stat.S_ISLNK(info.st_mode):
        return dict(link=False,directory=stat.S_ISDIR(info.st_mode),linkStatus='',linkError='',linkDetail='')
    try:
        target=target_info(path)
        return dict(link=True,directory=stat.S_ISDIR(target.st_mode),linkStatus='resolved',linkError='',linkDetail='Resolved symbolic link; opening follows its target.')
    except OSError as error:
        if error.errno in (errno.ENOENT,errno.ENOTDIR):
            state,message='broken','The symbolic-link target is missing.'
        elif error.errno==errno.ELOOP:
            state,message='chain-limit','The symbolic-link chain is cyclic or exceeds the operating-system limit.'
        elif error.errno in (errno.EACCES,errno.EPERM):
            state,message='permission-denied','Permission to inspect the symbolic-link target was denied.'
        else:
            state,message='unavailable','The symbolic-link target is unavailable.'
        message+=' Use File Info to inspect the target or remove the link; this does not remove the target.'
        return dict(link=True,directory=False,linkStatus=state,linkError=message,linkDetail=message)
