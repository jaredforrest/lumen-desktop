"""Google EDS fixtures: no account, token or personal network access."""
from datetime import date
import json
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch,Mock
sys.path.insert(0,str(Path(__file__).resolve().parents[2]/'res/lumen-desktop/shell'))
import google_calendar_eds as google
import calendar_store

CAL={'id':'source-fixture','name':'Personal','accountId':'fixture-account','accessRole':'writer','color':'#ac44cf'}
DISCOVERY={'accounts':[{'id':'fixture-account','attentionNeeded':False}],'calendars':[CAL]}
RAW='BEGIN:VEVENT\nUID:fixture\nDTSTART:20261008T080000Z\nDTEND:20261008T090000Z\nSUMMARY:Appointment\nX-EVOLUTION-CALDAV-ETAG:version-one\nEND:VEVENT\n'
def entry(raw=RAW):return {'raw':raw,'ical':'BEGIN:VCALENDAR\nVERSION:2.0\n'+raw+'END:VCALENDAR\n'}
FIELDS={'title':'Updated','location':'Room','notes':'Note','allDay':False,'start':'2026-10-08T11:00','end':'2026-10-08T12:00'}

class FakeBackend:
    def __init__(self,raw=RAW):
        self.raw=raw;self.created=[];self.updated=[];self.deleted=[]
    def discover(self):return json.loads(json.dumps(DISCOVERY))
    def load(self,uid,month,refresh_remote=True):return {'events':[entry(self.raw)],'accessRole':'writer','online':True}
    def current(self,*args):return self.raw
    def create(self,*args):self.created.append(args)
    def update(self,*args):self.updated.append(args)
    def delete(self,*args):self.deleted.append(args)

class GoogleTests(unittest.TestCase):
    def setUp(self):
        self.tmp=tempfile.TemporaryDirectory();self.addCleanup(self.tmp.cleanup)
        self.root=Path(self.tmp.name)/'calendar';self.settings=self.root/'google-settings'
        self.discovery=patch('google_calendar_eds.discover_sources',side_effect=lambda:json.loads(json.dumps(DISCOVERY)))
        self.discovery.start();self.addCleanup(self.discovery.stop)
        self.backend=FakeBackend();google.sync_month(self.root,date(2026,10,1),self.backend)

    def rows(self,selected='2026-10-08'):
        return google.read_events(self.root,date.fromisoformat(selected),date(2026,10,1))[0]
    def test_calendar_color_survives_cache_and_expansion(self):
        self.assertEqual(self.rows()[0]['color'], '#ac44cf')

    def request(self,action='update',selected='2026-10-08'):
        row=self.rows(selected)[0]
        return dict(row,action=action,selectedDate=selected,month='2026-10-01',fields=FIELDS,operationId='a'*32)

    def test_mixed_providers_and_london_time(self):
        google.private_write(self.root/'icloud'/'personal'/'event.ics',entry()['ical'])
        result=calendar_store.handle({'selectedDate':'2026-10-08'},self.root)
        self.assertEqual({e['provider'] for e in result['events']},{'Google','iCloud'})
        self.assertTrue(all(e['startTime']=='09:00' for e in result['events']))

    def test_oneoff_event_does_not_get_synthetic_recurrence_identity(self):
        row=self.rows()[0]
        self.assertFalse(row['recurring']);self.assertEqual(row['recurrenceId'],'');self.assertTrue(row['editable'])

    def test_recurring_events_expand_with_exceptions(self):
        raw=RAW.replace('20261008','20261001').replace('SUMMARY:Appointment','RRULE:FREQ=WEEKLY;COUNT=4\nEXDATE:20261015T080000Z\nSUMMARY:Recurring')
        google.sync_month(self.root,date(2026,10,1),FakeBackend(raw))
        rows,days=google.read_events(self.root,date(2026,10,8),date(2026,10,1))
        self.assertTrue(rows[0]['recurring']);self.assertEqual(rows[0]['recurrenceId'],'20261008T080000Z')
        self.assertNotIn('2026-10-15',days)

    def test_generated_occurrence_edit_preserves_etag_and_scopes_this_instance(self):
        raw=RAW.replace('20261008','20261001').replace('SUMMARY:Appointment','RRULE:FREQ=WEEKLY;COUNT=4\nSUMMARY:Recurring')
        backend=FakeBackend(raw);google.sync_month(self.root,date(2026,10,1),backend)
        with patch('google_calendar_eds.sync_month'):
            google.mutate(self.root,self.request(),backend)
        saved=google.parse_event(backend.updated[0][1])
        self.assertTrue(backend.updated[0][2]);self.assertIn('RECURRENCE-ID',saved)
        self.assertNotIn('RRULE',saved);self.assertEqual(str(saved['X-EVOLUTION-CALDAV-ETAG']),'version-one')

    def test_all_day_end_exclusive(self):
        raw='BEGIN:VEVENT\nUID:day\nDTSTART;VALUE=DATE:20261008\nDTEND;VALUE=DATE:20261010\nSUMMARY:All day\nEND:VEVENT\n'
        google.sync_month(self.root,date(2026,10,1),FakeBackend(raw))
        row=self.rows()[0];self.assertTrue(row['allDay']);self.assertEqual(row['editEnd'],'2026-10-09');self.assertEqual(self.rows('2026-10-10'),[])

    def test_failed_calendar_retains_cache(self):
        backend=FakeBackend();backend.load=Mock(side_effect=google.CalendarError('offline'))
        old=google.month_file(self.root,date(2026,10,1)).read_bytes()
        with self.assertRaises(google.CalendarError):google.sync_month(self.root,date(2026,10,1),backend)
        self.assertEqual(google.month_file(self.root,date(2026,10,1)).read_bytes(),old)

    def test_discovery_pending_account_is_connected(self):
        with patch('google_calendar_eds.discover_sources',return_value={'accounts':[{'id':'new'}],'calendars':[]}):
            result=google.status(self.settings)
        self.assertTrue(result['configured']);self.assertIn('discovery',result['error']);self.assertEqual(result['calendars'],[])

    def test_disabled_or_removed_account_hides_its_cache(self):
        with patch('google_calendar_eds.discover_sources',return_value={'accounts':[],'calendars':[]}):
            result=google.status(self.settings)
        self.assertFalse(result['configured']);self.assertEqual(self.rows(),[])

    def test_disabling_all_sources_does_not_resurrect_cached_calendars(self):
        discovery={'accounts':DISCOVERY['accounts'],'calendars':[],'pendingDiscovery':False}
        with patch('google_calendar_eds.discover_sources',return_value=discovery):
            result=google.status(self.settings)
        self.assertTrue(result['configured']);self.assertEqual(result['calendars'],[])
        self.assertIn('no calendars are enabled',result['error']);self.assertEqual(self.rows(),[])

    def test_native_service_failure_retains_known_cache_with_warning(self):
        with patch('google_calendar_eds.discover_sources',side_effect=google.CalendarError('unavailable')):
            result=google.status(self.settings)
        self.assertTrue(result['configured']);self.assertTrue(result['error']);self.assertEqual(len(self.rows()),1)

    def test_reconnected_account_cannot_reuse_old_editor(self):
        request=self.request();backend=FakeBackend()
        backend.discover=lambda:{'accounts':[{'id':'new'}],'calendars':[dict(CAL,accountId='new')]}
        with self.assertRaisesRegex(google.CalendarError,'changed'):google.mutate(self.root,request,backend)
        self.assertFalse(backend.updated)

    def test_remote_revision_change_refuses_update(self):
        backend=FakeBackend(RAW.replace('version-one','version-two'))
        with self.assertRaisesRegex(google.CalendarError,'changed elsewhere'):google.mutate(self.root,self.request(),backend)
        self.assertFalse(backend.updated)

    def test_patch_preserves_alarm_unknown_properties_and_revision(self):
        raw=RAW.replace('END:VEVENT','X-FIXTURE:keep\nBEGIN:VALARM\nACTION:DISPLAY\nTRIGGER:-PT15M\nDESCRIPTION:Reminder\nEND:VALARM\nEND:VEVENT')
        backend=FakeBackend(raw);google.sync_month(self.root,date(2026,10,1),backend)
        with patch('google_calendar_eds.sync_month'):google.mutate(self.root,self.request(),backend)
        saved=google.parse_event(backend.updated[0][1]);self.assertEqual(str(saved['X-FIXTURE']),'keep')
        self.assertEqual(len(saved.walk('VALARM')),1);self.assertEqual(str(saved['X-EVOLUTION-CALDAV-ETAG']),'version-one')

    def test_delete_requires_confirmation(self):
        with self.assertRaisesRegex(google.CalendarError,'Confirm'):google.mutate(self.root,self.request('delete'),self.backend)
        request=self.request('delete');request['confirmDelete']=True
        with patch('google_calendar_eds.sync_month'):google.mutate(self.root,request,self.backend)
        self.assertEqual(self.backend.deleted,[(CAL['id'],'fixture','')])

    def test_create_stable_uid_and_failure_not_retried(self):
        backend=FakeBackend();backend.create=Mock(side_effect=google.CalendarError('unknown outcome'))
        with self.assertRaises(google.CalendarError):google.mutate(self.root,self.request('create'),backend)
        self.assertEqual(backend.create.call_count,1)
        self.assertEqual(str(google.parse_event(backend.create.call_args.args[1])['UID']),'a'*32+'@lumen-desktop.local')

    def test_unsupported_long_and_subminute_and_attendee_events_protected(self):
        for raw in (RAW.replace('SUMMARY:Appointment','DESCRIPTION:'+'x'*8001),RAW.replace('080000Z','080030Z'),RAW.replace('SUMMARY:Appointment','ATTENDEE:mailto:fixture@example.org')):
            backend=FakeBackend(raw);google.sync_month(self.root,date(2026,10,1),backend)
            self.assertFalse(self.rows()[0]['editable'])
            with self.assertRaises(google.CalendarError):google.mutate(self.root,self.request(),backend)
            self.assertFalse(backend.updated)

    def test_readonly_calendar_protected(self):
        backend=FakeBackend();backend.load=lambda uid,month,refresh_remote=True:dict(events=[entry()],accessRole='reader',online=True)
        google.sync_month(self.root,date(2026,10,1),backend)
        with self.assertRaises(google.CalendarError):google.mutate(self.root,self.request(),backend)
        self.assertFalse(backend.updated)

    def test_aggregate_bound_keeps_previous_cache(self):
        backend=FakeBackend(RAW.replace('SUMMARY:Appointment','DESCRIPTION:'+'x'*1000))
        path=google.month_file(self.root,date(2026,10,1));before=path.read_bytes()
        with patch('google_calendar_eds.MAX_RESPONSE',1500):
            with self.assertRaisesRegex(google.CalendarError,'cache size'):google.sync_month(self.root,date(2026,10,1),backend)
        self.assertEqual(path.read_bytes(),before)

    def test_successful_edit_invalidates_other_month_cache(self):
        path=google.month_file(self.root,date(2026,11,1));google.private_write(path,'{}')
        with patch('google_calendar_eds.sync_month'):google.mutate(self.root,self.request(),self.backend)
        self.assertFalse(path.exists())

    def test_accepted_write_remains_success_when_cache_fails(self):
        with patch('google_calendar_eds.sync_month',side_effect=OSError('disk full')):
            result=google.mutate(self.root,self.request(),self.backend)
        self.assertTrue(result['mutationSuccess']);self.assertIn('pending',result['mutationWarning'])

    def test_final_calendar_read_failure_keeps_accepted_save(self):
        with patch('google_calendar_eds.mutate',return_value={'mutationSuccess':True}),patch('calendar_store.read_events',side_effect=OSError('disk full')):
            result=calendar_store.handle(self.request(),self.root)
        self.assertTrue(result['mutationSuccess'])

    def test_invalid_dates_dst_and_all_day_fields(self):
        from icalendar import Event
        for changes in ({'title':''},{'end':'2026-10-08T10:00'},{'start':'2026-10-25T01:30','end':'2026-10-25T03:00'}):
            with self.assertRaises(google.CalendarError):google.apply_fields(Event(),dict(FIELDS,**changes))
        component=google.apply_fields(Event(),dict(FIELDS,allDay=True,start='2026-10-08',end='2026-10-08'))
        self.assertEqual(component.decoded('DTEND'),date(2026,10,9))

    def test_first_empty_cache_refreshes_when_eds_events_arrive_without_remote_refresh(self):
        backend=FakeBackend()
        backend.load=Mock(side_effect=[dict(events=[],accessRole='writer',online=True),dict(events=[entry()],accessRole='writer',online=True)])
        with patch('google_calendar_eds.Backend',return_value=backend):
            first=calendar_store.handle({'selectedDate':'2026-10-08','action':'load'},self.root)
            second=calendar_store.handle({'selectedDate':'2026-10-08','action':'load'},self.root)
        self.assertEqual(first['events'],[]);self.assertEqual(len(second['events']),1)
        self.assertEqual(backend.load.call_count,2)
        self.assertTrue(all(call.kwargs['refresh_remote'] is False for call in backend.load.call_args_list))

    def test_native_master_vcalendar_revision_matches_standalone_event(self):
        grouped=entry()['ical'].replace('END:VCALENDAR','BEGIN:VEVENT\nUID:fixture\nRECURRENCE-ID:20261009T080000Z\nDTSTART:20261009T090000Z\nDTEND:20261009T100000Z\nEND:VEVENT\nEND:VCALENDAR')
        selected=google.select_event(grouped,'fixture','')
        self.assertEqual(google.event_revision(selected.to_ical().decode()),google.event_revision(RAW))

    def test_native_range_master_fetches_detached_exceptions_as_full_series(self):
        import gi
        gi.require_version('ICalGLib','3.0')
        from gi.repository import ICalGLib
        master=ICalGLib.Component.new_from_string(RAW.replace('SUMMARY:Appointment','RRULE:FREQ=DAILY;COUNT=2\nSUMMARY:Master'))
        override=ICalGLib.Component.new_from_string(RAW.replace('SUMMARY:Appointment','RECURRENCE-ID:20261009T080000Z\nSUMMARY:Override').replace('DTSTART:20261008','DTSTART:20261009').replace('DTEND:20261008','DTEND:20261009'))
        client=Mock()
        client.is_online.return_value=True;client.is_readonly.return_value=False
        client.get_object_list_sync.return_value=(True,[master])
        client.get_objects_for_uid_sync.return_value=(True,[Mock(get_icalcomponent=lambda:master),Mock(get_icalcomponent=lambda:override)])
        client.get_component_as_string.side_effect=lambda component:'BEGIN:VCALENDAR\nVERSION:2.0\n'+component.as_ical_string()+'END:VCALENDAR\n'
        backend=google.Backend.__new__(google.Backend);backend.client=lambda uid:client
        backend.call=lambda method,*args:method(*args,None)
        loaded=backend.load('fixture',date(2026,10,1),refresh_remote=False)
        self.assertEqual(len(loaded['events']),2)
        self.assertTrue(any('SUMMARY:Override' in item['raw'] for item in loaded['events']))
        client.get_objects_for_uid_sync.assert_called_once();client.refresh_sync.assert_not_called()

    def test_timed_writes_use_utc_without_unregistered_timezone_id(self):
        from icalendar import Event
        component=google.apply_fields(Event(),FIELDS)
        self.assertEqual(component.decoded('DTSTART').hour,10)
        self.assertNotIn('TZID',component['DTSTART'].params)

    def test_background_does_not_print_private_events(self):
        import contextlib,io
        output=io.StringIO()
        with patch('calendar_store.sys.argv',['lumen-calendar','--sync-background']),patch('calendar_store.handle',return_value={'events':[{'title':'private fixture'}],'syncError':''}),contextlib.redirect_stdout(output):
            self.assertEqual(calendar_store.main(),0)
        self.assertNotIn('private fixture',output.getvalue())

if __name__=='__main__':unittest.main()
