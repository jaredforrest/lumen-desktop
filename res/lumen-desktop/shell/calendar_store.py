"""Calendar view: read-only iCloud mirror and explicit Google event operations."""
from datetime import date, datetime, time, timedelta
import hashlib
import json
import os
from pathlib import Path
import re
import signal
import subprocess
import sys
import time as clock
from zoneinfo import ZoneInfo

ZONE = ZoneInfo(os.environ.get('LUMEN_CALENDAR_TIMEZONE', 'Europe/London'))
MAX_FILES = 10000
MAX_BYTES = 2 * 1024 * 1024
MAX_OCCURRENCES = 10000


def data_root():
    return Path(os.environ.get('XDG_DATA_HOME', str(Path.home() / '.local/share'))) / 'calendar'


def day(value):
    if not isinstance(value, str) or not re.fullmatch(r'\d{4}-\d{2}-\d{2}', value):
        raise ValueError('Use a date in YYYY-MM-DD format')
    return date.fromisoformat(value)


def local(value):
    if isinstance(value, datetime):
        return value.replace(tzinfo=ZONE) if value.tzinfo is None else value.astimezone(ZONE)
    return datetime.combine(value, time(), ZONE)


def icloud_status(root):
    settings = root / 'icloud-settings'
    result = {'configured': (settings / 'vdirsyncer.conf').is_file(), 'lastSuccess': '', 'syncError': ''}
    try:
        status = json.loads((settings / 'status.json').read_text())
        result['lastSuccess'] = status.get('lastSuccess', '')
        result['syncError'] = status.get('error', '')
    except (OSError, ValueError, AttributeError):
        pass
    return result


def read_status(root, refresh=False):
    from google_calendar_eds import status as google_status
    icloud = dict(icloud_status(root), id='icloud', name='iCloud')
    google = google_status(root / 'google-settings', refresh=refresh)
    google = dict(google, id='google', name='Google', syncError=google.get('error', ''))
    providers = [icloud, google]
    active = [provider for provider in providers if provider['configured']]
    successes = [provider['lastSuccess'] for provider in active if provider['lastSuccess']]
    return dict(configured=bool(active), lastSuccess=min(successes) if successes else '',
                syncError='; '.join(provider['name'] + ': ' + provider['syncError'] for provider in active if provider['syncError']),
                providers=providers)


def read_events(root, selected, month):
    from icalendar import Calendar
    import recurring_ical_events
    next_month = (month.replace(day=28) + timedelta(days=4)).replace(day=1)
    lo = datetime.combine(month, time(), ZONE)
    hi = datetime.combine(next_month, time(), ZONE)
    selected_lo = datetime.combine(selected, time(), ZONE)
    selected_hi = datetime.combine(selected + timedelta(days=1), time(), ZONE)
    rows, marks, warnings = [], set(), []
    files = 0
    occurrences = 0
    # Vdirsyncer stores each collection one level below this dedicated mirror.
    # Never follow symlinks or read a user's unrelated calendars.
    for provider, mirror in [('iCloud', root / 'icloud')]:
        if mirror.is_symlink():
            continue
        for folder in sorted(mirror.iterdir()) if mirror.is_dir() else []:
            if folder.is_symlink() or not folder.is_dir():
                continue
            name = provider + ' Calendar'
            metadata = folder / 'displayname'
            if metadata.is_file() and not metadata.is_symlink() and metadata.stat().st_size < 4096:
                name = metadata.read_text(errors='replace').strip()[:200] or name
            for file in sorted(folder.glob('*.ics')):
                files += 1
                if files > MAX_FILES:
                    warnings.append('Only the first 10,000 event files were read')
                    break
                if file.is_symlink() or file.stat().st_size > MAX_BYTES:
                    warnings.append('An oversized or linked calendar file was skipped')
                    continue
                try:
                    calendar = Calendar.from_ical(file.read_bytes())
                    color = str(calendar.get('COLOR', calendar.get('X-LUMEN-CALENDAR-COLOR', '')))
                    color = color[:7] if re.fullmatch(r'#[0-9a-fA-F]{6}(?:[0-9a-fA-F]{2})?', color) else ''
                    if any(str(component.get('RRULE', {}).get('FREQ', [''])[0]).upper() in ('SECONDLY', 'MINUTELY') for component in calendar.walk('VEVENT')):
                        warnings.append('Sub-hourly recurring events are not shown in this calendar view')
                        continue
                    expanded = recurring_ical_events.of(calendar).between(lo, hi)
                    occurrences += len(expanded)
                    if occurrences > MAX_OCCURRENCES:
                        warnings.append('Too many events in this month; some events were omitted')
                        break
                    for component in expanded:
                        if str(component.get('STATUS', '')).upper() == 'CANCELLED':
                            continue
                        start_value = component.decoded('DTSTART')
                        all_day = not isinstance(start_value, datetime)
                        start = local(start_value)
                        if component.get('DTEND') is not None:
                            end = local(component.decoded('DTEND'))
                        elif component.get('DURATION') is not None:
                            end = start + component.decoded('DURATION')
                        else:
                            end = start + (timedelta(days=1) if all_day else timedelta(seconds=1))
                        if end <= start:
                            end = start + timedelta(seconds=1)
                        first = max(month, start.date())
                        last = min(next_month - timedelta(days=1), (end - timedelta(microseconds=1)).date())
                        while first <= last:
                            marks.add(first.isoformat())
                            first += timedelta(days=1)
                        if start < hi and end > lo:
                            identity = provider + ':' + str(file.relative_to(mirror)) + str(component.get('UID', '')) + start.isoformat()
                            rows.append(dict(id=hashlib.sha256(identity.encode()).hexdigest(), title=str(component.get('SUMMARY', 'Untitled event'))[:1000], notes=str(component.get('DESCRIPTION', ''))[:8000], location=str(component.get('LOCATION', ''))[:1000], calendar=name, provider=provider, color=color, allDay=all_day, date=start.date().isoformat(), endDate=(end - timedelta(microseconds=1)).date().isoformat(), startTime=start.strftime('%H:%M'), endTime=end.strftime('%H:%M'), start=start.timestamp()))
                except TimeoutError:
                    raise
                except (ValueError, TypeError, KeyError, OverflowError, OSError):
                    warnings.append('An unreadable calendar event was skipped')
            if files > MAX_FILES or occurrences > MAX_OCCURRENCES:
                break
    from google_calendar_eds import read_events as google_events
    google_rows, google_marks = google_events(root, selected, month, whole_month=True)
    rows.extend(google_rows)
    marks.update(google_marks)
    rows.sort(key=lambda e: (not e['allDay'], e['start'], e['title']))
    selected_rows = [row for row in rows if row['date'] <= selected.isoformat() <= row['endDate']]
    return dict(events=selected_rows[:100], total=len(selected_rows), monthEvents=rows[:MAX_OCCURRENCES], days=sorted(marks), warning='; '.join(dict.fromkeys(warnings)))


def handle(request, root=None):
    if not isinstance(request, dict):
        raise ValueError('Calendar request must be an object')
    selected = day(request.get('selectedDate'))
    month = day(request.get('month', selected.replace(day=1).isoformat())).replace(day=1)
    if selected.year != month.year or selected.month != month.month:
        raise ValueError('Selected date must be in the displayed month')
    root = Path(root) if root else data_root()
    action = request.get('action', 'list')
    mutation = {}
    if action == 'sync':
        from concurrent.futures import ThreadPoolExecutor
        result = read_status(root, refresh=True)
        active = [provider for provider in result['providers'] if provider['configured']]
        def sync_provider(provider):
            command = 'lumen-icloud-setup' if provider['id'] == 'icloud' else 'lumen-google-calendar-sync'
            try:
                subprocess.run([command, 'sync'] + (['--month', month.isoformat()] if provider['id'] == 'google' else []), check=True, timeout=100, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
                return ''
            except (subprocess.SubprocessError, OSError):
                return provider['name'] + ' sync failed. Previously downloaded events remain available.'
        with ThreadPoolExecutor(max_workers=2) as workers:
            errors = list(workers.map(sync_provider, active))
        result = read_status(root)
        result['syncError'] = result['syncError'] or '; '.join(error for error in errors if error)
    elif action == 'load':
        from google_calendar_eds import month_file, sync_month, CalendarError
        result = read_status(root, refresh=True)
        if result['providers'][1]['configured']:
            try:
                sync_month(root, month, refresh_remote=False)
            except CalendarError as error:
                result['syncError'] = str(error)
        result = dict(read_status(root), syncError=result['syncError'])
    elif action in ('create', 'update', 'delete'):
        from google_calendar_eds import mutate
        mutation = mutate(root, request)
        try:
            result = read_status(root)
        except Exception:
            result = dict(configured=True,providers=[],lastSuccess='',syncError='')
    elif action == 'list':
        result = read_status(root)
    else:
        raise ValueError('Unknown calendar action')
    try:
        result.update(read_events(root, selected, month))
    except Exception:
        if not mutation.get('mutationSuccess'):
            raise
        result.update(events=[],monthEvents=[],total=0,days=[],warning='')
        mutation['mutationWarning'] = 'The calendar service saved the change, but the local view could not refresh. Sync again.'
    result.update(mutation)
    result.update(timezone=ZONE.key, date=selected.isoformat(), month=month.isoformat())
    return result


def timed_out(signum, frame):
    raise TimeoutError('Calendar processing took too long. Cached events could not be displayed.')


def main():
    signal.signal(signal.SIGALRM, timed_out)
    try:
        if sys.argv[1:] == ['--sync-background']:
            today = datetime.now(ZONE).date()
            signal.alarm(115)
            result = handle({'action':'sync','selectedDate':today.isoformat(),'month':today.replace(day=1).isoformat()})
            if result.get('syncError'):
                print('Calendar sync needs attention; cached events were retained.')
                return 1
            return 0
        data = sys.stdin.readline(65537)
        if len(data) > 65536:
            raise ValueError('Calendar request is too large')
        request = json.loads(data)
        if not isinstance(request, dict):
            raise ValueError('Calendar request must be an object')
        signal.alarm(115 if request.get('action') != 'list' else 10)
        print(json.dumps(handle(request), ensure_ascii=False))
    except (ValueError, TypeError, OverflowError, TimeoutError, OSError, ImportError) as exc:
        print(json.dumps({'error': str(exc)}))
        return 1
    finally:
        signal.alarm(0)
    return 0


if __name__ == '__main__':
    sys.exit(main())
