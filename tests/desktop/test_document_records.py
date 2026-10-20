import json,tempfile,unittest
from pathlib import Path
from office_fixture.document_records import generate
from office_fixture.records import PROVIDERS
class Records(unittest.TestCase):
 def fixture(self,directory,passed=True):
  path=Path(directory)/'result.json';path.write_text(json.dumps(dict(passed=passed,checks=['capture','reopen','invalid','pdf','destination'],source='a'*40,installedTools={f'/nix/store/fixture-{name}-0.5.0/bin/{name}-cli':'b'*64 for name in ('wordcraft','gridcraft','deckcraft')})));return path
 def test_partial_workflow_stays_partial(self):
  with tempfile.TemporaryDirectory() as directory:
   records=generate(self.fixture(directory))['records'];self.assertEqual(len(records),6);self.assertTrue(all(not record['scope_complete'] for record in records));self.assertEqual(records[0]['status'],'implemented_unverified');self.assertTrue(records[0]['unsupported_actions']);self.assertEqual(records[2]['status'],'unassessed');self.assertEqual(records[3]['supported_actions'],['pdf']);self.assertTrue(all(record['status']!='verified_compatible' for record in records))
 def test_native_provider_evidence_does_not_leak_between_apps(self):
  with tempfile.TemporaryDirectory() as directory:
   path=Path(directory)/'sheet.json';path.write_text(json.dumps(dict(passed=True,app='sheet',source='c'*40,checks=['PDF chooser Cancel','PDF Save'],facts=dict(native=dict(executable='/nix/store/fixture/bin/.gridcraft-native')))))
   records=generate(self.fixture(directory),[path])['records'];sheet=next(record for record in records if record['provider_app_id']==PROVIDERS['sheet'][0] and record['capability_id']=='T26-DOC-010')
   self.assertIn('PDF Save',sheet['supported_actions']);self.assertTrue(sheet['unsupported_actions']);self.assertFalse(sheet['scope_complete'])
   self.assertTrue(all('PDF Save' not in record['supported_actions'] for record in records if record is not sheet))
 def test_failed_observation_cannot_generate_success(self):
  with tempfile.TemporaryDirectory() as directory:
   with self.assertRaises(ValueError):generate(self.fixture(directory,False))
if __name__=='__main__':unittest.main()
