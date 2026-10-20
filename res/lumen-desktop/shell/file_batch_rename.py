"""Captured, expiring, no-overwrite local batch rename plans.

Plans never infer changed selections or retry a partially committed operation.
All paths are siblings. Existing destinations, including selected names and
symlinks, are conflicts; cycles are intentionally not staged through temp names.
"""
import ctypes
from collections import Counter
import os
from pathlib import Path
import secrets
import threading
import time
import file_identity


def clock():
    return time.clock_gettime(time.CLOCK_BOOTTIME)


def signature(info):
    return (info.st_dev, info.st_ino, info.st_mode, info.st_size, info.st_mtime_ns, info.st_ctime_ns)


def validate_name(value, maximum=255):
    if not isinstance(value,str) or value in ('','.','..') or '/' in value or any(ord(c)<32 or ord(c)==127 for c in value):
        raise ValueError('Names must not be empty or contain slashes or control characters')
    if len(os.fsencode(value))>maximum:raise ValueError('The proposed name is too long for this folder')
    return value


def transform(name, rule):
    if not isinstance(rule,dict) or set(rule)!={'mode','first','second'}:raise ValueError('Invalid rename transformation')
    mode,first,second=(rule[key] for key in ('mode','first','second'))
    if mode not in ('replace','prefix','suffix') or any(not isinstance(value,str) or len(value)>1024 for value in (first,second)):
        raise ValueError('Invalid rename transformation')
    if mode=='replace':
        if not first:raise ValueError('Enter the text to replace')
        return name.replace(first,second)
    if second:raise ValueError('Unexpected rename transformation text')
    return first+name if mode=='prefix' else name+first


def rename_noreplace(directory, source, destination):
    libc=ctypes.CDLL(None,use_errno=True)
    if libc.renameat2(directory,os.fsencode(source),directory,os.fsencode(destination),1):
        code=ctypes.get_errno();raise OSError(code,os.strerror(code),destination)


class RenamePlans:
    def __init__(self, now=clock, mover=rename_noreplace):
        self.now,self.mover=now,mover
        self.lock=threading.Lock();self.plans={}

    def prune(self):
        self.plans={key:value for key,value in self.plans.items() if self.now()-value['created']<300}

    def preview(self, items, rule, policy='stop'):
        if policy not in ('stop','skip'):raise ValueError('Choose stop or skip for conflicts')
        if not isinstance(items,list) or not 2<=len(items)<=5000:raise ValueError('Select between2 and5,000 items to rename')
        paths=[];expected=[]
        for item in items:
            if not isinstance(item,dict) or set(item)!={'path','identity'} or not isinstance(item['path'],str) or not item['path'].startswith('/') or '\0' in item['path']:
                raise ValueError('A captured local selection is required')
            value=Path(os.path.abspath(item['path']))
            if value==value.parent:raise ValueError('The filesystem root cannot be renamed')
            paths.append(value);expected.append(file_identity.parse(item['identity']))
        parent=paths[0].parent
        if any(value.parent!=parent for value in paths) or len(set(paths))!=len(paths):raise ValueError('Select distinct items in one folder')
        # Validate a rule even if every selected row later proves unavailable.
        transform('probe',rule)
        descriptor=os.open(parent,os.O_RDONLY|os.O_DIRECTORY|os.O_NOFOLLOW)
        try:
            parent_info=os.fstat(descriptor);maximum=os.fpathconf(descriptor,'PC_NAME_MAX');rows=[];targets=[]
            for source,identity in zip(paths,expected):
                target=transform(source.name,rule)
                row=dict(path=str(source),before=source.name,after=target,status='ready',error='')
                try:
                    validate_name(target,maximum)
                    info=os.stat(source.name,dir_fd=descriptor,follow_symlinks=False)
                    if identity!={'device':info.st_dev,'inode':info.st_ino}:raise ValueError('The selected item changed. Select it again')
                    row['facts']=signature(info)
                    if target==source.name:row['status']='unchanged'
                    else:
                        try:os.stat(target,dir_fd=descriptor,follow_symlinks=False)
                        except FileNotFoundError:pass
                        else:row.update(status='conflict',error='An item with this name already exists')
                except (OSError,ValueError) as exc:row.update(status='invalid',error=str(exc))
                rows.append(row);targets.append(target)
            duplicates={value for value,count in Counter(targets).items() if count>1}
            for row in rows:
                if row['after'] in duplicates and row['status'] not in ('invalid','unchanged'):
                    row.update(status='conflict',error='Several selected items would have the same name')
            ready=sum(row['status']=='ready' for row in rows)
            executable=ready>0 and not any(row['status']=='invalid' for row in rows) and (policy=='skip' or not any(row['status']=='conflict' for row in rows))
            plan=dict(created=self.now(),parent=str(parent),parentIdentity=(parent_info.st_dev,parent_info.st_ino),rows=rows,policy=policy,executable=executable)
            with self.lock:
                self.prune()
                if len(self.plans)>=64:raise ValueError('Too many rename previews are open. Close one and try again')
                token=secrets.token_hex(24);self.plans[token]=plan
            return dict(token=token,rows=self.public(rows),executable=executable,count=ready,policy=policy)
        finally:os.close(descriptor)

    @staticmethod
    def public(rows):
        return [{key:row[key] for key in ('path','before','after','status','error')} for row in rows]

    def cancel(self, token):
        with self.lock:self.plans.pop(token,None)
        return dict(canceled=True)

    def execute(self, token, confirmed=False):
        if confirmed is not True:raise ValueError('Confirm the preview before renaming')
        with self.lock:
            self.prune();plan=self.plans.pop(token,None)
            if plan is None:raise ValueError('This rename preview expired or was already used. Preview again')
            if not plan['executable']:raise ValueError('Resolve the preview conflicts before renaming')
            rows=[dict(row) for row in plan['rows']]
            descriptor=os.open(plan['parent'],os.O_RDONLY|os.O_DIRECTORY|os.O_NOFOLLOW)
            try:
                info=os.fstat(descriptor)
                if (info.st_dev,info.st_ino)!=plan['parentIdentity']:raise ValueError('The selected folder changed. Preview again')
                # Reject stale previews as a whole, including rows the user chose to skip.
                for row in rows:
                    if signature(os.stat(row['before'],dir_fd=descriptor,follow_symlinks=False))!=row['facts']:
                        raise ValueError('A selected item changed. Nothing was renamed. Preview again')
                    if row['status']=='ready':
                        try:os.stat(row['after'],dir_fd=descriptor,follow_symlinks=False)
                        except FileNotFoundError:pass
                        else:raise ValueError('A destination appeared. Nothing was renamed. Preview again')
                stopped=False
                for row in rows:
                    if row['status']=='conflict':row['status']='skipped';continue
                    if row['status']=='unchanged':continue
                    if stopped:row.update(status='not_run',error='Stopped after the preceding error');continue
                    try:
                        if signature(os.stat(row['before'],dir_fd=descriptor,follow_symlinks=False))!=row['facts']:
                            raise ValueError('The selected item changed before its rename')
                        self.mover(descriptor,row['before'],row['after'])
                        row['status']='renamed'
                        # Renaming one hard link changes its shared inode ctime.
                        # Accept only that observed self-induced metadata change;
                        # content/mode/mtime differences still stop the plan.
                        observed=signature(os.stat(row['after'],dir_fd=descriptor,follow_symlinks=False))
                        if observed[:5]!=row['facts'][:5]:
                            stopped=True;row['error']='The renamed item changed concurrently; remaining changes stopped'
                        else:
                            for pending in rows:
                                if pending['status']=='ready' and pending['facts']==row['facts']:
                                    pending['facts']=observed
                    except (OSError,ValueError) as exc:
                        row.update(status='renamed' if row['status']=='renamed' else 'failed',error=str(exc));stopped=True
                try:os.fsync(descriptor)
                except OSError as exc:
                    return dict(ok=False,rows=self.public(rows),error='Names changed but the folder durability acknowledgement failed: '+str(exc))
                return dict(ok=not stopped,rows=self.public(rows))
            finally:os.close(descriptor)


plans=RenamePlans()
