import json
import os
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

sys.path.insert(0,str(Path(__file__).resolve().parents[2]/'res/lumen-desktop/shell'))
from context_tools import detectors, learn, subject_source
from file_jobs import Jobs
from files import handle


class ContextTests(unittest.TestCase):
    def test_detected_actions_are_explicit_and_dates_valid(self):
        rows=detectors('https://example.org. x@example.org 2026-10-08 2026-02-31 +1 (415) 555-1234')['actions']
        self.assertEqual([r['uri'] for r in rows],['https://example.org','mailto:x@example.org','tel:+14155551234',''])
        self.assertIn('October 08',rows[-1]['label'])

    def test_personal_dictionary_rejects_protocol_commands(self):
        for word in ('test\n#','*learn','two words','/tmp/file',''):
            with self.assertRaises(ValueError):learn(word)

    def test_interrupted_receipt_never_replays_mutations(self):
        with tempfile.TemporaryDirectory() as tmp:
            source=Path(tmp)/'original';source.write_text('retained')
            journal=Path(tmp)/'history.json'
            journal.write_text(json.dumps([dict(id='old',state='running',operation='move',errors=[],started=1,sources=[str(source)],destination=tmp+'/missing',total=1,completed=0,bytes=0)]))
            jobs=Jobs(journal)
            self.assertEqual(jobs.state()['jobs'][0]['state'],'interrupted')
            self.assertFalse(jobs.state()['canUndo'])
            self.assertEqual(source.read_text(),'retained')
            self.assertFalse((Path(tmp)/'missing').exists())

    def test_corrupt_history_does_not_prevent_browsing(self):
        with tempfile.TemporaryDirectory() as tmp:
            file=Path(tmp)/'history';file.write_text('{')
            state=Jobs(file).state();self.assertTrue(state['historyWarning']);self.assertFalse(state['jobs'])

    def test_view_preferences_survive_restart_and_reject_bad_modes(self):
        with tempfile.TemporaryDirectory() as tmp,patch.dict(os.environ,{'XDG_STATE_HOME':tmp}):
            handle(dict(action='save-preferences',preferences=dict(view='columns',sort='modified',descending=True)))
            with self.assertRaises(ValueError):
                handle(dict(action='save-preferences',preferences=dict(view='malformed',sort='bad')))
            self.assertEqual(handle(dict(action='preferences'))['preferences'],dict(view='columns',sort='modified',descending=True))

    def test_get_info_reports_link_without_following_it(self):
        with tempfile.TemporaryDirectory() as tmp:
            file=Path(tmp)/'example';file.write_text('hello');file.chmod(0o640)
            link=Path(tmp)/'shortcut';link.symlink_to(file)
            self.assertEqual(handle(dict(action='info',path=str(file)))['info']['permissions'],'-rw-r-----')
            result=handle(dict(action='info',path=str(link)))['info']
            self.assertEqual(result['kind'],'Symbolic link');self.assertEqual(result['target'],str(file))

    def test_subject_export_does_not_accept_arbitrary_files(self):
        with tempfile.TemporaryDirectory() as tmp,patch.dict(os.environ,{'XDG_CACHE_HOME':tmp}):
            file=Path(tmp)/('a'*32+'.png');file.write_bytes(b'test')
            with self.assertRaises(ValueError):subject_source(str(file))
            folder=Path(tmp)/'lumen-desktop/subjects';folder.mkdir(parents=True)
            link=folder/file.name;link.symlink_to(file)
            with self.assertRaises(ValueError):subject_source(str(link))


if __name__=='__main__':unittest.main()
