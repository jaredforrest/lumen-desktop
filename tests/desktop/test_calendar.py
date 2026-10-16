"""Calendar mirror tests; disposable data only, no account, keyring or network."""
from datetime import datetime
import json
import os
from pathlib import Path
import sys
import subprocess
import tempfile
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / 'res/lumen-desktop/shell'))
import calendar_store
import icloud_setup


class CalendarTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name) / 'calendar'
        self.folder = self.root / 'icloud/example'
        self.folder.mkdir(parents=True)
        (self.folder / 'displayname').write_text('Personal')

    def event(self, fields, name='fixture'):
        file = self.folder / (name + '.ics')
        file.write_text('BEGIN:VCALENDAR\nVERSION:2.0\nPRODID:-//Fixture//EN\nBEGIN:VEVENT\nUID:' + name + '\n' + fields + '\nEND:VEVENT\nEND:VCALENDAR\n')
        return file

    def read(self, day='2026-10-08'):
        return calendar_store.handle(dict(selectedDate=day), self.root)

    def test_calendar_backends_follow_declared_timezone(self):
        source=str(Path(calendar_store.__file__).parent)
        code='import sys;sys.path.insert(0,'+repr(source)+');import calendar_store,google_calendar_eds;print(calendar_store.ZONE.key,google_calendar_eds.ZONE.key)'
        env=dict(os.environ,LUMEN_CALENDAR_TIMEZONE='America/New_York')
        self.assertEqual(subprocess.check_output([sys.executable,'-c',code],env=env,text=True).strip(),'America/New_York America/New_York')

    def test_icloud_calendar_colour_is_preserved(self):
        file=self.event('DTSTART:20261008T080000Z\nDTEND:20261008T090000Z\nSUMMARY:Coloured event')
        file.write_text(file.read_text().replace('VERSION:2.0', 'VERSION:2.0\nX-LUMEN-CALENDAR-COLOR:#aa44ccff'))
        self.assertEqual(self.read()['events'][0]['color'], '#aa44cc')

    def test_saved_view_never_waits_for_account_discovery(self):
        self.event('DTSTART:20261008T080000Z\nDTEND:20261008T090000Z\nSUMMARY:Saved event')
        with patch('google_calendar_eds.discover_sources', side_effect=AssertionError('Interactive view must stay local')):
            self.assertEqual(self.read()['events'][0]['title'], 'Saved event')

    def test_month_snapshot_contains_other_days_without_leaking_into_selected_day(self):
        self.event('DTSTART;VALUE=DATE:20261012\nDTEND;VALUE=DATE:20261013\nSUMMARY:Another day')
        result=self.read()
        self.assertEqual(result['events'], [])
        self.assertEqual(result['monthEvents'][0]['date'], '2026-10-12')

    def test_unconfigured_has_truthful_empty_state_and_does_not_create_database(self):
        result = self.read()
        self.assertFalse(result['configured'])
        self.assertEqual(result['lastSuccess'], '')
        self.assertEqual(result['events'], [])
        self.assertFalse((self.root / 'lumen-desktop.sqlite3').exists())

    def test_weekly_recurrence_and_exception_date(self):
        self.event('DTSTART;TZID=Europe/London:20261001T090000\nDTEND;TZID=Europe/London:20261001T100000\nSUMMARY:Weekly\nRRULE:FREQ=WEEKLY;COUNT=5\nEXDATE;TZID=Europe/London:20261015T090000')
        self.assertEqual(self.read()['events'][0]['startTime'], '09:00')
        self.assertEqual(self.read('2026-10-15')['events'], [])
        self.assertEqual(self.read()['days'], ['2026-10-01','2026-10-08','2026-10-22','2026-10-29'])

    def test_all_day_end_is_exclusive(self):
        self.event('DTSTART;VALUE=DATE:20261008\nDTEND;VALUE=DATE:20261010\nSUMMARY:Two days')
        self.assertTrue(self.read()['events'][0]['allDay'])
        self.assertEqual(self.read()['days'], ['2026-10-08','2026-10-09'])
        self.assertFalse(self.read('2026-10-10')['events'])

    def test_utc_time_converts_to_london_and_spans_midnight(self):
        self.event('DTSTART:20261007T230000Z\nDTEND:20261008T010000Z\nSUMMARY:UTC event')
        event = self.read()['events'][0]
        self.assertEqual(event['startTime'], '00:00')
        self.assertEqual(event['endTime'], '02:00')
        self.assertFalse(self.read('2026-10-07')['events'])

    def test_clock_change_keeps_london_recurring_wall_time(self):
        self.event('DTSTART;TZID=Europe/London:20261024T090000\nDTEND;TZID=Europe/London:20261024T100000\nRRULE:FREQ=DAILY;COUNT=3\nSUMMARY:Daily')
        before = self.read('2026-10-24')['events'][0]
        after = self.read('2026-10-25')['events'][0]
        self.assertEqual(before['startTime'], '09:00')
        self.assertEqual(after['startTime'], '09:00')
        self.assertEqual(after['start'] - before['start'], 25 * 3600)

    def test_overridden_recurrence_and_cancelled_event(self):
        file = self.event('DTSTART;TZID=Europe/London:20261001T090000\nDTEND;TZID=Europe/London:20261001T100000\nRRULE:FREQ=WEEKLY;COUNT=3\nSUMMARY:Original')
        file.write_text(file.read_text().replace('END:VCALENDAR', 'BEGIN:VEVENT\nUID:fixture\nRECURRENCE-ID;TZID=Europe/London:20261008T090000\nDTSTART;TZID=Europe/London:20261008T120000\nDTEND;TZID=Europe/London:20261008T130000\nSUMMARY:Moved\nEND:VEVENT\nEND:VCALENDAR'))
        self.event('DTSTART;VALUE=DATE:20261008\nSTATUS:CANCELLED\nSUMMARY:Cancelled', 'cancelled')
        self.assertEqual([(e['title'], e['startTime']) for e in self.read()['events']], [('Moved','12:00')])

    def test_source_bytes_untouched_and_unrelated_calendar_not_read(self):
        file = self.event('DTSTART;VALUE=DATE:20261008\nSUMMARY:Keep original')
        original = file.read_bytes()
        (self.root / 'unrelated.ics').write_text('unrelated')
        self.read()
        self.assertEqual(file.read_bytes(), original)
        self.assertEqual((self.root / 'unrelated.ics').read_text(), 'unrelated')

    def test_bad_dates_and_writes_rejected(self):
        for request in ({'selectedDate':'2026-02-31'}, {'selectedDate':'2026-10-08','month':'2026-11-01'}, {'selectedDate':'2026-10-08','action':'delete'}):
            with self.assertRaises(ValueError):
                calendar_store.handle(request, self.root)

    def test_symlinks_malformed_and_subhourly_skip_with_warning(self):
        bad = self.folder / 'bad.ics';bad.write_text('not a calendar')
        self.event('DTSTART:20261008T090000Z\nRRULE:FREQ=SECONDLY\nSUMMARY:Too frequent', 'frequent')
        (self.folder / 'link.ics').symlink_to(bad)
        result = self.read()
        self.assertEqual(result['events'], [])
        self.assertIn('skipped', result['warning'])
        self.assertIn('Sub-hourly', result['warning'])

    def test_config_read_only_both_remote_providers_and_secret_reference(self):
        conf = icloud_setup.configuration(self.root, self.root/'icloud-settings', 'example@example.org', '/test/secret-tool')
        self.assertEqual(conf.count('read_only = true'), 2)
        self.assertEqual(conf.count('collections = ["from b"]'), 2)
        self.assertNotIn('password =', conf)
        self.assertIn('password.fetch', conf)
        self.assertIn('https://contacts.icloud.com/', conf)
        self.assertIn('https://caldav.icloud.com/', conf)

    def test_private_config_and_status_writes(self):
        path = self.root/'icloud-settings/status.json'
        icloud_setup.private_write(path, '{"lastSuccess":"2026-10-08T12:00:00+00:00","error":""}')
        self.assertEqual(path.stat().st_mode & 0o777, 0o600)
        self.assertEqual(calendar_store.icloud_status(self.root)['lastSuccess'], '2026-10-08T12:00:00+00:00')

    def test_failed_sync_retains_last_success(self):
        settings = self.root/'icloud-settings'
        icloud_setup.private_write(settings/'vdirsyncer.conf', 'fixture')
        icloud_setup.private_write(settings/'status.json', json.dumps({'lastSuccess':'yesterday','error':''}))
        with patch('icloud_setup.subprocess.run', side_effect=OSError('offline')):
            with self.assertRaises(ValueError):
                icloud_setup.run_sync(settings)
        self.assertEqual(icloud_setup.status(settings)['lastSuccess'], 'yesterday')
        self.assertTrue(icloud_setup.status(settings)['error'])

    def test_authentication_failure_is_specific_without_leaking_diagnostics(self):
        settings = self.root/'icloud-settings'
        icloud_setup.private_write(settings/'vdirsyncer.conf', 'fixture')
        icloud_setup.private_write(settings/'status.json', json.dumps({'lastSuccess':'yesterday'}))
        error = subprocess.CalledProcessError(1, ['fixture'], stderr='401 Client Error: Unauthorized for https://private-account.invalid/secret')
        with patch('icloud_setup.discover_collections', side_effect=error):
            with self.assertRaisesRegex(ValueError, 'app-specific password'):
                icloud_setup.run_sync(settings, discover=True)
        result = icloud_setup.status(settings)
        self.assertEqual(result['lastSuccess'], 'yesterday')
        self.assertIn('401 Unauthorized', result['error'])
        self.assertNotIn('private-account', result['error'])
        self.assertNotIn('/secret', result['error'])

    def test_discovery_only_auto_creates_original_local_mirrors(self):
        settings = self.root/'icloud-settings'
        config = settings/'vdirsyncer.conf'
        original = icloud_setup.configuration(self.root, settings, 'fixture@example.org', '/test/secret-tool')
        icloud_setup.private_write(config, original)
        icloud_setup.validate_local_discovery(config)
        with patch('icloud_setup.subprocess.run') as run:
            icloud_setup.discover_collections(['vdirsyncer', '--config', str(config)])
            self.assertEqual(run.call_args.kwargs['input'], 'y\n' * 4096)
        for changed in (
            original.replace('read_only = true', 'read_only = false'),
            original.replace('["from b"]', '["from a"]'),
            original.replace('type = "filesystem"', 'type = "caldav"'),
            original.replace(str(self.root/'icloud'), '/different/location'),
            original + '\n[pair extra]\na="unsafe"\n',
        ):
            icloud_setup.private_write(config, changed)
            with patch('icloud_setup.subprocess.run') as run:
                with self.assertRaisesRegex(ValueError, 'read-only'):
                    icloud_setup.discover_collections(['vdirsyncer', '--config', str(config)])
                run.assert_not_called()


if __name__ == '__main__':
    unittest.main()
