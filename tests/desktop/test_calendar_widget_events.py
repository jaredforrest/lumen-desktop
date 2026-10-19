"""Mixed calendar widget data uses only a disposable local mirror."""
from datetime import date
import importlib.util
from pathlib import Path
import sys,tempfile,unittest
from unittest.mock import patch
ROOT=Path(__file__).resolve().parents[2];sys.path.insert(0,str(ROOT/'res/lumen-desktop/shell'))
import calendar_store

def populate(root):
    calendars={'Personal':[
      ('all-day','DTSTART;VALUE=DATE:20261031\nDTEND;VALUE=DATE:20261102\nSUMMARY:Two-day all-day'),
      ('old-day','DTSTART:20261031T220000Z\nDTEND:20261031T230000Z\nSUMMARY:Previous day only')],
      'Travel':[
      ('west','DTSTART;TZID=America/Los_Angeles:20261031T183000\nDTEND;TZID=America/Los_Angeles:20261031T200000\nSUMMARY:West overlap'),
      ('east','DTSTART;TZID=America/New_York:20261031T220000\nDTEND;TZID=America/New_York:20261031T230000\nSUMMARY:East overlap'),
      ('utc','DTSTART:20261101T030000Z\nDTEND:20261101T040000Z\nSUMMARY:Later UTC')]}
    for name,events in calendars.items():
        folder=root/'icloud'/name;folder.mkdir(parents=True);(folder/'displayname').write_text(name)
        for uid,fields in events:(folder/(uid+'.ics')).write_text('BEGIN:VCALENDAR\nVERSION:2.0\nPRODID:-//Fixture//EN\nBEGIN:VEVENT\nUID:'+uid+'\n'+fields+'\nEND:VEVENT\nEND:VCALENDAR\n')
    settings=root/'icloud-settings';settings.mkdir();(settings/'vdirsyncer.conf').write_text('fixture only');(settings/'status.json').write_text('{"lastSuccess":"2026-10-31T12:00:00Z"}')

class WidgetEvents(unittest.TestCase):
    def test_mixed_month_boundary_source_and_overlap(self):
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory);populate(root);before={p:p.read_bytes() for p in root.rglob('*') if p.is_file()}
            with patch('google_calendar_eds.discover_sources',side_effect=AssertionError('No account discovery')):
                rows=calendar_store.handle(dict(action='list',selectedDate='2026-11-01',month='2026-11-01'),root)['events']
                self.assertEqual([(r['title'],r['startTime'],r['endTime']) for r in rows],[('Two-day all-day','00:00','00:00'),('West overlap','01:30','03:00'),('East overlap','02:00','03:00'),('Later UTC','03:00','04:00')])
                self.assertTrue(rows[0]['allDay']);self.assertEqual(rows[0]['endDate'],'2026-11-01');self.assertEqual([r['calendar'] for r in rows],['Personal','Travel','Travel','Travel']);self.assertEqual(len(set(r['id'] for r in rows)),4)
                self.assertEqual(calendar_store.handle(dict(action='list',selectedDate='2026-11-02',month='2026-11-01'),root)['events'],[])
            self.assertEqual(before,{p:p.read_bytes() for p in before})
if __name__=='__main__':unittest.main()
