"""Google calendars through GNOME Online Accounts and Evolution Data Server.

Credentials, provider authentication and remote calendar protocol handling belong
exclusively to GOA/EDS. This module never requests or reads OAuth tokens.
"""
import argparse
from contextlib import contextmanager
from datetime import date, datetime, time, timedelta, timezone
import fcntl
import hashlib
import json
import os
from pathlib import Path
import re
import sys
import threading
import time as clock
from zoneinfo import ZoneInfo
from icloud_setup import private_write

ZONE = ZoneInfo(os.environ.get('LUMEN_CALENDAR_TIMEZONE', 'Europe/London'))
MAX_RESPONSE = 8 * 1024 * 1024
MAX_EVENTS = 10000


class CalendarError(ValueError):
    """Only fixed, credential-free user-facing messages."""


def paths():
    root = Path(os.environ.get('XDG_DATA_HOME', str(Path.home() / '.local/share'))) / 'calendar'
    return root, root / 'google-settings'


def read_json(path, default):
    try:
        if path.stat().st_size > MAX_RESPONSE:
            return default
        return json.loads(path.read_text())
    except (OSError, ValueError):
        return default


def fingerprint(raw):
    return hashlib.sha256(raw.encode()).hexdigest()


class Backend:
    """Bounded native EDS calls. No HTTP, client IDs or token access."""
    def __init__(self):
        try:
            import gi
            gi.require_version('EDataServer', '1.2')
            gi.require_version('ECal', '2.0')
            gi.require_version('ICalGLib', '3.0')
            gi.require_version('Goa', '1.0')
            from gi.repository import EDataServer, ECal, ICalGLib, Gio, Goa
            self.EDS, self.ECal, self.ICal, self.Gio = EDataServer, ECal, ICalGLib, Gio
            self.deadline = clock.monotonic() + 80
            self.registry = self.call(EDataServer.SourceRegistry.new_sync)
            self.goa = self.call(Goa.Client.new_sync)
            self.sources = {}
            self.clients = {}
        except Exception:
            raise CalendarError('Online Accounts or the calendar service is unavailable. Cached events were kept.') from None

    def call(self, method, *args):
        remaining = min(12, self.deadline-clock.monotonic())
        if remaining <= 0:
            raise CalendarError('The calendar service reached its time limit. Sync again shortly.')
        cancellable = self.Gio.Cancellable()
        timer = threading.Timer(remaining, cancellable.cancel)
        timer.daemon = True
        timer.start()
        try:
            return method(*args, cancellable)
        except CalendarError:
            raise
        except Exception:
            raise CalendarError('The calendar service could not complete the request. Sync before retrying a change.') from None
        finally:
            timer.cancel()

    def discover(self):
        accounts = {}
        for obj in self.goa.get_accounts():
            account = obj.get_account()
            if account.props.provider_type == 'google' and not account.props.calendar_disabled:
                accounts[account.props.id] = dict(id=account.props.id, attentionNeeded=bool(account.props.attention_needed))
        calendars = []
        self.sources = {}
        seen_accounts = set()
        for source in self.registry.list_sources(self.EDS.SOURCE_EXTENSION_CALENDAR):
            ancestor = self.registry.find_extension(source, self.EDS.SOURCE_EXTENSION_GOA)
            if ancestor is None:
                continue
            account_id = ancestor.get_extension(self.EDS.SOURCE_EXTENSION_GOA).get_account_id()
            if account_id not in accounts:
                continue
            seen_accounts.add(account_id)
            if not self.registry.check_enabled(source):
                continue
            uid = source.get_uid()
            color = source.get_extension(self.EDS.SOURCE_EXTENSION_CALENDAR).get_color() or ''
            color = color[:7] if re.fullmatch(r'#[0-9a-fA-F]{6}(?:[0-9a-fA-F]{2})?', color) else ''
            calendars.append(dict(id=uid, name=source.get_display_name(), accountId=account_id, accessRole='unknown', color=color))
            self.sources[uid] = source
            if len(calendars) > 250:
                raise CalendarError('Too many calendars are connected for this desktop view.')
        return dict(accounts=list(accounts.values()), calendars=calendars, pendingDiscovery=bool(set(accounts)-seen_accounts))

    def client(self, uid):
        if uid not in self.sources:
            self.discover()
        if uid not in self.sources:
            raise CalendarError('This Google calendar was disconnected. Reopen the calendar.')
        if uid not in self.clients:
            self.clients[uid] = self.call(self.ECal.Client.connect_sync, self.sources[uid], self.ECal.ClientSourceType.EVENTS, 5)
        return self.clients[uid]

    def load(self, uid, month, refresh_remote=True):
        client = self.client(uid)
        try:
            if refresh_remote and client.is_online():
                self.call(client.refresh_sync)
        except CalendarError:
            pass
        hi = (month.replace(day=28)+timedelta(days=4)).replace(day=1)
        lo_text = datetime.combine(month,time(),ZONE).astimezone(timezone.utc).strftime('%Y%m%dT%H%M%SZ')
        hi_text = datetime.combine(hi,time(),ZONE).astimezone(timezone.utc).strftime('%Y%m%dT%H%M%SZ')
        # Include recurring groups outside the original time span: a detached
        # occurrence may have been moved into this month. The same size/count
        # bounds apply to this extra recurrence metadata.
        query = f'(or (occur-in-time-range? (make-time "{lo_text}") (make-time "{hi_text}")) (has-recurrences?))'
        success, components = self.call(client.get_object_list_sync, query)
        if not success or len(components) > MAX_EVENTS:
            raise CalendarError('The calendar service returned too many or unreadable events.')
        # EDS ranged queries can return only a series master. Fetch its full
        # UID group so detached/moved/cancelled instances override the master.
        grouped = []
        fetched = set()
        for component in components:
            parsed = parse_event(component.as_ical_string())
            uid = str(parsed.get('UID',''))
            if any(parsed.get(key) for key in ('RRULE','RDATE','RECURRENCE-ID')):
                if uid in fetched:
                    continue
                fetched.add(uid)
                success, series = self.call(client.get_objects_for_uid_sync, uid)
                if not success:
                    raise CalendarError('A recurring event could not be read completely. The previous download was kept.')
                grouped.extend(item.get_icalcomponent() for item in series)
            else:
                grouped.append(component)
            if len(grouped) > MAX_EVENTS:
                raise CalendarError('This month contains too many event instances. The previous download was kept.')
        events = []
        size = 0
        seen = set()
        for component in grouped:
            parsed = parse_event(component.as_ical_string())
            rid = parsed.get('RECURRENCE-ID')
            key = (str(parsed.get('UID','')), rid.to_ical().decode() if rid else '')
            if key in seen:
                continue
            seen.add(key)
            raw = component.as_ical_string()
            ical = client.get_component_as_string(component)
            size += len(raw.encode()) + len(ical.encode())
            if size > MAX_RESPONSE:
                raise CalendarError('This calendar exceeds the supported cache size. Previous events were kept.')
            events.append(dict(raw=raw, ical=ical))
        return dict(events=events, accessRole='reader' if client.is_readonly() else 'writer', online=bool(client.is_online()))

    def current(self, uid, event_uid, recurrence_id):
        client = self.client(uid)
        if not client.is_online():
            raise CalendarError('Connect to the network before editing Google events.')
        self.call(client.refresh_sync)
        if client.is_readonly():
            raise CalendarError('This Google calendar is read-only.')
        success, component = self.call(client.get_object_sync, event_uid, recurrence_id or None)
        if not success:
            raise CalendarError('This event no longer exists. Sync the calendar.')
        return select_event(component.as_ical_string(), event_uid, recurrence_id).to_ical().decode()

    def create(self, uid, ical):
        client = self.client(uid)
        if not client.is_online() or client.is_readonly():
            raise CalendarError('Connect to the network and choose a writable Google calendar.')
        component = self.ICal.Component.new_from_string(ical)
        flags = self.ECal.OperationFlags.CONFLICT_FAIL | self.ECal.OperationFlags.DISABLE_ITIP_MESSAGE
        success, event_uid = self.call(client.create_object_sync, component, flags)
        if not success:
            raise CalendarError('The event could not be created. Sync before retrying.')
        return event_uid

    def update(self, uid, ical, recurring):
        client = self.client(uid)
        if not client.is_online() or client.is_readonly():
            raise CalendarError('Connect to the network and choose a writable Google calendar.')
        component = self.ICal.Component.new_from_string(ical)
        flags = self.ECal.OperationFlags.CONFLICT_FAIL | self.ECal.OperationFlags.DISABLE_ITIP_MESSAGE
        mode = self.ECal.ObjModType.THIS if recurring else self.ECal.ObjModType.ALL
        if not self.call(client.modify_object_sync, component, mode, flags):
            raise CalendarError('The event changed elsewhere or could not be saved. Sync before retrying.')

    def delete(self, uid, event_uid, recurrence_id):
        client = self.client(uid)
        if not client.is_online() or client.is_readonly():
            raise CalendarError('Connect to the network and choose a writable Google calendar.')
        flags = self.ECal.OperationFlags.CONFLICT_FAIL | self.ECal.OperationFlags.DISABLE_ITIP_MESSAGE
        mode = self.ECal.ObjModType.THIS if recurrence_id else self.ECal.ObjModType.ALL
        if not self.call(client.remove_object_sync, event_uid, recurrence_id or None, mode, flags):
            raise CalendarError('The event changed elsewhere or could not be deleted. Sync before retrying.')


def discover_sources():
    return Backend().discover()


def refresh_manifest(settings, discovery=None):
    previous = read_json(settings/'sources.json', {'accounts':[], 'calendars':[]})
    discovery = discovery if discovery is not None else discover_sources()
    old = {calendar['id']:calendar for calendar in previous['calendars']}
    for calendar in discovery['calendars']:
        prior = old.get(calendar['id'], {})
        if calendar['accountId'] == prior.get('accountId'):
            calendar['accessRole'] = prior.get('accessRole', calendar['accessRole'])
    # Disabled or removed sources must disappear immediately, even when their
    # parent account still exists. Discovery state is reported separately.
    if previous != discovery:
        private_write(settings/'sources.json', json.dumps(discovery))
    return discovery


def generation(settings):
    sources = read_json(settings/'sources.json', {'accounts':[], 'calendars':[]})
    return fingerprint(json.dumps(sorted((c['accountId'],c['id']) for c in sources['calendars']))) if sources['accounts'] else ''


def metadata_dir(settings):
    return settings/'eds'/ (generation(settings) or 'unconfigured')


def status(settings, refresh=True):
    error = ''
    try:
        sources = refresh_manifest(settings) if refresh else read_json(settings/'sources.json', {'accounts':[],'calendars':[]})
    except (CalendarError, OSError):
        sources = read_json(settings/'sources.json', {'accounts':[],'calendars':[]})
        error = 'Online Accounts could not be checked. Previously downloaded events remain available.'
    saved = read_json(metadata_dir(settings)/'status.json', {})
    if sources['accounts'] and not sources['calendars']:
        error = ('Google is connected. Waiting for calendar discovery; try Sync Now shortly.' if sources.get('pendingDiscovery',True)
                 else 'Google is connected, but no calendars are enabled in the calendar service.')
    if any(account.get('attentionNeeded') for account in sources['accounts']):
        error = 'Google needs you to sign in again. Open Google Setup.'
    return dict(configured=bool(sources['accounts']), calendars=sources['calendars'], generation=generation(settings),
                lastSuccess=saved.get('lastSuccess',''), error=error or saved.get('error',''), mode='eds')


def month_file(root, month):
    return metadata_dir(root/'google-settings')/'months'/(month.strftime('%Y-%m')+'.json')


def sync_month(root, month, backend=None, refresh_remote=True):
    settings = root/'google-settings'
    backend = backend or Backend()
    sources = refresh_manifest(settings, backend.discover())
    if not sources['accounts']:
        return
    if sources.get('pendingDiscovery') and not sources['calendars']:
        raise CalendarError('Google is connected. Waiting for calendar discovery; try Sync Now shortly.')
    directory = metadata_dir(settings)
    cache_file = month_file(root, month)
    settings.mkdir(parents=True,exist_ok=True,mode=0o700)
    with os.fdopen(os.open(settings/'sync.lock',os.O_CREAT|os.O_WRONLY,0o600),'w') as lock:
        try:
            fcntl.flock(lock,fcntl.LOCK_EX|fcntl.LOCK_NB)
        except BlockingIOError:
            raise CalendarError('A calendar sync is already running.') from None
        previous = read_json(cache_file,{'calendars':{}})
        old_status = status(settings,refresh=False)
        result = {'calendars':{},'month':month.isoformat()}
        errors=[]
        for calendar in sources['calendars']:
            uid=calendar['id']
            try:
                loaded=backend.load(uid,month,refresh_remote=refresh_remote)
                calendar['accessRole']=loaded['accessRole']
                result['calendars'][uid]=dict(calendar,events=loaded['events'])
                if not loaded['online']:
                    errors.append('Google is offline or still syncing; displaying calendar service cache.')
            except CalendarError:
                errors.append('A Google calendar could not sync. Its previous download was retained.')
                if uid in previous['calendars']:
                    result['calendars'][uid]=previous['calendars'][uid]
        encoded=json.dumps(result)
        if len(encoded.encode())>MAX_RESPONSE:
            raise CalendarError('This month exceeds the supported cache size. Previous events were kept.')
        private_write(cache_file,encoded)
        # Do not let a retired sync republish a removed account's source list.
        if generation(settings)==directory.name:
            private_write(settings/'sources.json',json.dumps(sources))
        private_write(directory/'status.json',json.dumps(dict(lastSuccess=datetime.now(timezone.utc).isoformat() if not errors else old_status['lastSuccess'],error='; '.join(set(errors)))))
        if errors:
            raise CalendarError('Some Google calendars are offline or still syncing. Cached events are available.')


def parse_event(text):
    from icalendar import Calendar
    component=Calendar.from_ical(text)
    if component.name=='VEVENT':return component
    return next(iter(component.walk('VEVENT')))


def select_event(text, uid, rid=''):
    from icalendar import Calendar
    whole=Calendar.from_ical(text)
    for component in whole.walk('VEVENT'):
        identity=component.get('RECURRENCE-ID')
        if str(component.get('UID',''))==uid and (identity.to_ical().decode() if identity else '')==(rid or ''):
            return component
    raise CalendarError('This event no longer exists. Sync and reopen it.')


def event_revision(text):
    return hashlib.sha256(parse_event(text).to_ical()).hexdigest()


def local(value):
    if isinstance(value,datetime):return value.replace(tzinfo=ZONE) if value.tzinfo is None else value.astimezone(ZONE)
    return datetime.combine(value,time(),ZONE)


def component_editable(calendar, component):
    if calendar.get('accessRole') not in ('writer','owner') or component.get('ATTENDEE') or component.get('X-GOOGLE-EVENT-TYPE'):
        return False
    if any(len(str(component.get(key,'')))>maximum for key,maximum in [('SUMMARY',1000),('DESCRIPTION',8000),('LOCATION',1000)]):
        return False
    for key in ('DTSTART','DTEND'):
        value=component.decoded(key) if component.get(key) is not None else None
        if isinstance(value,datetime) and (value.second or value.microsecond):return False
    return True


def read_events(root, selected, month, private=False, whole_month=False):
    from icalendar import Calendar
    import recurring_ical_events
    directory=metadata_dir(root/'google-settings')
    cache=read_json(directory/'months'/(month.strftime('%Y-%m')+'.json'),{'calendars':{}})
    lo=datetime.combine(selected,time(),ZONE);hi=lo+timedelta(days=1)
    month_end=(month.replace(day=28)+timedelta(days=4)).replace(day=1)
    rows,marks=[],set()
    count=0
    for calendar in cache['calendars'].values():
        combined=Calendar();combined.add('VERSION','2.0');originals={};timezones=set()
        for entry in calendar.get('events',[]):
            try:
                base=parse_event(entry['raw'])
                rid=base.get('RECURRENCE-ID')
                key=(str(base.get('UID','')),rid.to_ical().decode() if rid else '')
                originals[key]=entry['raw']
                whole=Calendar.from_ical(entry['ical'])
                for component in whole.subcomponents if whole.name=='VCALENDAR' else [whole]:
                    if component.name=='VTIMEZONE':
                        tz=str(component.get('TZID',''))
                        if tz in timezones:continue
                        timezones.add(tz)
                    if component.name in ('VTIMEZONE','VEVENT'):combined.add_component(component)
            except (ValueError,TypeError,KeyError,StopIteration):continue
        if any(str(c.get('RRULE',{}).get('FREQ',[''])[0]).upper() in ('SECONDLY','MINUTELY') for c in combined.walk('VEVENT')):
            continue
        for event in recurring_ical_events.of(combined).between(datetime.combine(month,time(),ZONE),datetime.combine(month_end,time(),ZONE)):
            count+=1
            if count>MAX_EVENTS:raise CalendarError('Too many events in this month to display safely.')
            if str(event.get('STATUS','')).upper()=='CANCELLED':continue
            start_value=event.decoded('DTSTART');start=local(start_value);all_day=not isinstance(start_value,datetime)
            end=local(event.decoded('DTEND')) if event.get('DTEND') else start+(event.decoded('DURATION') if event.get('DURATION') else timedelta(days=1) if all_day else timedelta(seconds=1))
            uid=str(event.get('UID',''));rid=event.get('RECURRENCE-ID');rid_text=rid.to_ical().decode() if rid else ''
            master_raw=originals.get((uid,''))
            if master_raw:
                master=parse_event(master_raw)
                if not any(master.get(key) for key in ('RRULE','RDATE','EXDATE','EXRULE')):
                    rid=None;rid_text='';event.pop('RECURRENCE-ID',None)
            source_rid=rid_text if (uid,rid_text) in originals else ''
            original=originals.get((uid,source_rid),'')
            first,last=max(month,start.date()),min(month_end-timedelta(days=1),(end-timedelta(microseconds=1)).date())
            while first<=last:marks.add(first.isoformat());first+=timedelta(days=1)
            if (whole_month and start.date()<month_end and end.date()>=month) or (start<hi and end>lo):
                row=dict(id=fingerprint(directory.name+calendar['id']+uid+rid_text),provider='Google',calendar=calendar['name'],color=calendar.get('color',''),calendarId=calendar['id'],eventId=uid,
                         recurrenceId=rid_text,sourceRecurrenceId=source_rid,etag=event_revision(original) if original else "",accountGeneration=directory.name,recurring=bool(rid),
                         editable=bool(original) and component_editable(calendar,event),title=str(event.get('SUMMARY','Untitled event'))[:1000],notes=str(event.get('DESCRIPTION',''))[:8000],location=str(event.get('LOCATION',''))[:1000],
                         allDay=all_day,date=start.date().isoformat(),endDate=(end-timedelta(microseconds=1)).date().isoformat(),startTime=start.strftime('%H:%M'),endTime=end.strftime('%H:%M'),start=start.timestamp(),
                         editStart=start.date().isoformat() if all_day else start.strftime('%Y-%m-%dT%H:%M'),editEnd=(end-timedelta(days=1)).date().isoformat() if all_day else end.strftime('%Y-%m-%dT%H:%M'))
                if private:row['_ical']=event.to_ical().decode()
                rows.append(row)
    return rows,marks


def apply_fields(component, fields):
    if not isinstance(fields,dict) or not isinstance(fields.get('title'),str) or not fields['title'].strip() or len(fields['title'])>1000:
        raise CalendarError('Enter an event title of at most 1,000 characters.')
    if any(not isinstance(fields.get(key,''),str) or len(fields.get(key,''))>maximum for key,maximum in [('notes',8000),('location',1000)]):
        raise CalendarError('Use at most 8,000 characters for notes and 1,000 for location.')
    try:
        if fields.get('allDay') is True:
            start=date.fromisoformat(fields['start']);end=date.fromisoformat(fields['end'])+timedelta(days=1)
        else:
            start=datetime.fromisoformat(fields['start']);end=datetime.fromisoformat(fields['end'])
            if start.tzinfo or end.tzinfo:raise ValueError()
            start,end=start.replace(tzinfo=ZONE),end.replace(tzinfo=ZONE)
            if any(value.replace(fold=0).utcoffset()!=value.replace(fold=1).utcoffset() for value in (start,end)):
                raise CalendarError('This time falls within a daylight-saving clock change. Set it in Google Calendar instead.')
        if end<=start:raise ValueError()
        if isinstance(start,datetime):
            # Store the selected instant in UTC, avoiding unregistered TZID
            # overrides that EDS ranged queries can otherwise silently omit.
            start,end=start.astimezone(timezone.utc),end.astimezone(timezone.utc)
    except (ValueError,KeyError,TypeError) as error:
        if isinstance(error,CalendarError):raise
        raise CalendarError('Enter valid start and end dates/times, with the end after the start.') from None
    for key in ('SUMMARY','DESCRIPTION','LOCATION','DTSTART','DTEND','DURATION'):
        component.pop(key,None)
    for key,value in [('SUMMARY',fields['title'].strip()),('DESCRIPTION',fields.get('notes','')),('LOCATION',fields.get('location','')),('DTSTART',start),('DTEND',end)]:component.add(key,value)
    component.pop('DTSTAMP',None);component.add('DTSTAMP',datetime.now(timezone.utc))
    return component


def mutate(root, request, backend=None):
    from icalendar import Event
    settings=root/'google-settings'
    backend=backend or Backend()
    sources=refresh_manifest(settings,backend.discover())
    if request.get('accountGeneration')!=generation(settings):
        raise CalendarError('Connected calendars changed. Reopen this event before editing.')
    ident=request.get('calendarId');calendar=next((c for c in sources['calendars'] if c['id']==ident),None)
    if not calendar or calendar.get('accessRole') not in ('owner','writer'):
        raise CalendarError('Choose a writable Google calendar.')
    action=request.get('action')
    if action=='create':
        operation_id=request.get('operationId','')
        if not re.fullmatch('[a-f0-9]{32}',operation_id):raise CalendarError('The create request is missing its operation identifier.')
        component=Event();component.add('UID',operation_id+'@lumen-desktop.local')
        apply_fields(component,request.get('fields'))
        backend.create(ident,component.to_ical().decode())
    elif action in ('update','delete'):
        rows,_=read_events(root,date.fromisoformat(request['selectedDate']),date.fromisoformat(request['month']),private=True)
        row=next((r for r in rows if r['calendarId']==ident and r['eventId']==request.get('eventId') and r['recurrenceId']==request.get('recurrenceId','')),None)
        if not row or not row['editable'] or row['etag']!=request.get('etag'):
            raise CalendarError('Sync and reopen this event before editing; some event types require Google Calendar.')
        current=backend.current(ident,row['eventId'],row['sourceRecurrenceId'])
        if event_revision(current)!=row['etag']:
            raise CalendarError('This event changed elsewhere. Sync and reopen it before editing.')
        if action=='delete':
            if request.get('confirmDelete') is not True:raise CalendarError('Confirm deletion of the selected event first.')
            backend.delete(ident,row['eventId'],row['recurrenceId'])
        else:
            component=parse_event(row['_ical'])
            # Native CalDAV uses this revision property for conditional PUT/DELETE.
            original=parse_event(current)
            for key in [k for k in original.keys() if k.startswith('X-EVOLUTION-')]:
                component.pop(key,None);component[key]=original[key]
            apply_fields(component,request.get('fields'))
            backend.update(ident,component.to_ical().decode(),row['recurring'])
    else:raise CalendarError('Unknown calendar action.')
    try:
        displayed=month_file(root,date.fromisoformat(request['month']))
        for cache in displayed.parent.glob('*.json'):
            if cache!=displayed:cache.unlink()
        sync_month(root,date.fromisoformat(request['month']),backend,refresh_remote=False)
        return dict(mutationSuccess=True,mutationWarning='')
    except Exception:
        return dict(mutationSuccess=True,mutationWarning='The calendar service saved the change. Google sync may still be pending; try Sync Now.')


def main():
    os.umask(0o077)
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('action',choices=['sync','status'],nargs='?',default='sync')
    parser.add_argument('--month',default=datetime.now(ZONE).date().replace(day=1).isoformat())
    args=parser.parse_args();root,settings=paths()
    try:
        if args.action=='status':print(json.dumps(status(settings)))
        else:sync_month(root,date.fromisoformat(args.month).replace(day=1))
    except CalendarError as error:
        print(str(error),file=sys.stderr);return 1
    except Exception:
        print('The calendar service could not complete the operation. Cached events were kept.',file=sys.stderr);return 1
    return 0


if __name__=='__main__':sys.exit(main())
