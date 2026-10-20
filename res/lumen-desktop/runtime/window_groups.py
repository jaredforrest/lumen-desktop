"""Validate session-local native group facts and captured operations.

The compositor owns visibility. This module never emulates it with placement
commands and never persists compositor IDs across a session boundary.
"""
from dataclasses import dataclass


def integer(value, *, zero=False):
    if type(value) is not int or value < (0 if zero else 1) or value > 2**63-1:
        raise ValueError('Invalid native group identity')
    return value


@dataclass(frozen=True)
class Member:
    id: int
    pid: int
    group_hidden: bool


@dataclass(frozen=True)
class Group:
    id: int
    workspace: int
    selected: bool
    recent: int
    suspended: bool
    members: tuple[Member, ...]


@dataclass(frozen=True)
class Groups:
    revision: int
    enabled: bool
    locked: bool
    groups: tuple[Group, ...]
    separate_displays: bool = True

    @classmethod
    def read(cls, data):
        if not isinstance(data, dict) or type(data.get('version')) is not int or data['version'] != 1:
            raise ValueError('This compositor does not support native window groups')
        for key in ('enabled', 'locked', 'separate_displays'):
            if type(data.get(key)) is not bool:
                raise ValueError('Invalid native group capability')
        rows=data.get('groups')
        if not isinstance(rows,list) or len(rows)>256:
            raise ValueError('Invalid native group list')
        groups=[];seen=set();members=set();selected=set()
        for row in rows:
            if not isinstance(row,dict):raise ValueError('Invalid native group entry')
            gid=integer(row.get('id'));workspace=integer(row.get('workspace'))
            if gid in seen:raise ValueError('Duplicate native group')
            seen.add(gid)
            for key in ('selected','suspended'):
                if type(row.get(key)) is not bool:raise ValueError('Invalid native group state')
            if row['selected']:
                if workspace in selected:raise ValueError('Conflicting selected groups')
                selected.add(workspace)
            items=[]
            rows=row.get('members')
            if not isinstance(rows,list) or len(rows)>4096:raise ValueError('Invalid native member list')
            for item in rows:
                if not isinstance(item,dict):raise ValueError('Invalid native member')
                cid=integer(item.get('id'));pid=integer(item.get('pid'))
                if cid in members or type(item.get('group_hidden')) is not bool:raise ValueError('Invalid group membership')
                members.add(cid);items.append(Member(cid,pid,item['group_hidden']))
            groups.append(Group(gid,workspace,row['selected'],integer(row.get('recent'),zero=True),row['suspended'],tuple(items)))
        return cls(integer(data.get('revision'),zero=True),data['enabled'],data['locked'],tuple(groups),data['separate_displays'])

    def command(self, data):
        if type(data.get('groupRevision')) is not int or data['groupRevision'] != self.revision:
            raise ValueError('Window groups changed; refresh before acting')
        if self.locked:raise ValueError('Window groups are unavailable while locked')
        action=data.get('groupAction')
        prefix=f'lumen_window groups {self.revision} '
        if not self.separate_displays and action != 'off':raise ValueError('Window groups require separate display spaces')
        if action in ('on','off'):return prefix+action
        if not self.enabled:raise ValueError('Enable window groups first')
        gid=integer(data.get('groupId'));group=next((item for item in self.groups if item.id==gid),None)
        if group is None or group.suspended:raise ValueError('Group is unavailable in the current presentation')
        if action=='select':return prefix+f'select {gid}'
        if action not in ('add','remove'):raise ValueError('Unknown window group action')
        cid=integer(data.get('id'));pid=integer(data.get('pid'))
        member=next((m for item in self.groups for m in item.members if m.id==cid and m.pid==pid),None)
        if member is None:raise ValueError('The captured window is no longer available')
        if action=='remove' and member not in group.members:raise ValueError('The window is no longer in that group')
        return prefix+f'{action} {gid} {cid} {pid}'
