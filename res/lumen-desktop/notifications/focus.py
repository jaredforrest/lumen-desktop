"""Validated, local Focus policy. Schedules use the current local civil timezone."""
import datetime
import json
import math
import os
import time

DEFAULTS = [dict(id='quiet', name='Do Not Disturb', allowedApps=[], allowCritical=False, schedules=[]),
            dict(id='work', name='Work', allowedApps=[], allowCritical=False, schedules=[]),
            dict(id='personal', name='Personal', allowedApps=[], allowCritical=False, schedules=[])]


def profile(value):
    if not isinstance(value, dict) or set(value) - {'id','name','allowedApps','allowCritical','schedules'}:
        raise ValueError('Invalid Focus profile')
    ident, name = value.get('id'), value.get('name')
    if not isinstance(ident,str) or not ident or len(ident)>64 or not all(c.isascii() and (c.isalnum() or c in '-_') for c in ident):
        raise ValueError('Invalid Focus identity')
    if not isinstance(name,str) or not name.strip() or len(name)>64:
        raise ValueError('Invalid Focus name')
    apps = value.get('allowedApps', [])
    if not isinstance(apps,list) or len(apps)>100 or any(not isinstance(a,str) or not a or len(a)>256 for a in apps):
        raise ValueError('Invalid Focus application exceptions')
    critical = value.get('allowCritical',False)
    if type(critical) is not bool: raise ValueError('Invalid critical notification permission')
    schedules=value.get('schedules',[])
    if not isinstance(schedules,list) or len(schedules)>16: raise ValueError('Invalid Focus schedules')
    for item in schedules:
        if not isinstance(item,dict) or set(item)!={'days','start','end'}: raise ValueError('Invalid Focus schedule')
        days=item['days']
        if not isinstance(days,list) or not days or len(days)>7 or any(type(d) is not int or not 0<=d<=6 for d in days) or len(set(days))!=len(days): raise ValueError('Invalid schedule weekdays')
        if any(type(item[k]) is not int or not 0<=item[k]<1440 for k in ('start','end')) or item['start']==item['end']: raise ValueError('Invalid schedule time')
    return dict(id=ident,name=name.strip(),allowedApps=list(dict.fromkeys(apps)),allowCritical=critical,schedules=schedules)


def scheduled(item, now):
    minute=now.hour*60+now.minute
    for rule in item['schedules']:
        start,end=rule['start'],rule['end']
        if start<end and now.weekday() in rule['days'] and start<=minute<end: return True
        if start>end and ((now.weekday() in rule['days'] and minute>=start) or ((now.weekday()-1)%7 in rule['days'] and minute<end)): return True
    return False


class Focus:
    def __init__(self,path):
        self.path=path
        self.profiles=json.loads(json.dumps(DEFAULTS))
        self.manual=None
        self.until=0
        self.boot_deadline=0
        try:
            value=json.loads(path.read_text())
            if not isinstance(value,dict) or value.get('version')!=1 or not isinstance(value.get('profiles'),list):raise ValueError('Invalid Focus state')
            profiles=[profile(v) for v in value['profiles']]
            if not 1<=len(profiles)<=20 or len({v['id'] for v in profiles})!=len(profiles): raise ValueError('Invalid Focus profiles')
            manual=value.get('manual')
            until=value.get('until',0)
            if manual is not None and (not isinstance(manual,str) or manual!='' and manual not in {v['id'] for v in profiles}): raise ValueError('Invalid active Focus')
            if type(until) not in (int,float) or not math.isfinite(until) or until<0: raise ValueError('Invalid Focus expiry')
            self.profiles,self.manual,self.until=profiles,manual,until
            if until:
                remaining=min(86400,until-time.time())
                if remaining<=0:self.manual=None;self.until=0
                else:self.boot_deadline=self.boot_time()+remaining
        except FileNotFoundError: pass
        except (ValueError,KeyError,TypeError):
            # Preserve corrupt data; do not silently overwrite it at startup.
            path.replace(path.with_name('focus-invalid-%s.json'%time.time_ns()))

    @staticmethod
    def boot_time(): return time.clock_gettime(time.CLOCK_BOOTTIME)

    def save(self):
        tmp=self.path.with_suffix('.tmp')
        with tmp.open('w') as stream:
            os.chmod(tmp,0o600)
            json.dump(dict(version=1,profiles=self.profiles,manual=self.manual,until=self.until),stream)
            stream.flush();os.fsync(stream.fileno())
        tmp.replace(self.path)

    def selected(self):
        if self.until and (time.time()>=self.until or self.boot_time()>=self.boot_deadline):
            self.manual=None;self.until=0;self.boot_deadline=0;self.save()
        if self.manual is not None:
            return next((p for p in self.profiles if p['id']==self.manual),None)
        now=datetime.datetime.now().astimezone()
        return next((p for p in self.profiles if scheduled(p,now)),None)

    def state(self):
        active=self.selected()
        return dict(profiles=self.profiles,active=active['id'] if active else '',name=active['name'] if active else '',until=self.until,scheduled=self.manual is None)

    def allows(self,app,urgency):
        active=self.selected()
        return not active or app in active['allowedApps'] or urgency==2 and active['allowCritical']

    def command(self,data):
        operation=data.get('operation','activate')
        if operation=='profile':
            item=profile(data.get('profile'))
            old=next((p for p in self.profiles if p['id']==item['id']),None)
            if old:self.profiles[self.profiles.index(old)]=item
            elif len(self.profiles)<20:self.profiles.append(item)
            else:raise ValueError('Too many Focus profiles')
        elif operation=='activate':
            ident=data.get('profile','')
            if ident is not None and (not isinstance(ident,str) or ident!='' and ident not in {p['id'] for p in self.profiles}): raise ValueError('Unknown Focus')
            duration=data.get('seconds',0)
            if type(duration) is not int or not 0<=duration<=86400:raise ValueError('Invalid Focus duration')
            self.manual=ident;self.until=time.time()+duration if duration else 0
            self.boot_deadline=self.boot_time()+duration if duration else 0
        else:raise ValueError('Unknown Focus operation')
        self.save()
