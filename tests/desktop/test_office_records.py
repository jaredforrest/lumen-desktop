import json,tempfile,unittest
from pathlib import Path
from office_fixture.records import generate
class Records(unittest.TestCase):
 def test_partial_observations_never_close_capability(self):
  with tempfile.TemporaryDirectory() as directory:
   path=Path(directory)/'result.json';original=dict(harnessPassed=True,installedTools={f'/nix/store/fake-{app}-0.5.0/bin/{app}-cli':'0'*64 for app in ['wordcraft','gridcraft','deckcraft']},capabilities=[dict(id='APP-OFFICE-01',action='create docx',status='fixture_verified'),dict(id='APP-OFFICE-02',action='ODS export',status='unsupported',reason='Data import only'),dict(id='APP-OFFICE-03',action='ODP export',status='provider_defect',actualFormat='deck-native')]);path.write_text(json.dumps(original));before=path.read_bytes();result=generate(path)
   self.assertEqual(path.read_bytes(),before);self.assertEqual(len(result['records']),16)
   for record in result['records']:
    self.assertTrue({'capability_id','provider_app_id','provider_version','supported_actions','unsupported_actions','backend','permission_set','offline_policy','scenario_results','reference_evidence','compatibility_differences','status'}<=record.keys());self.assertFalse(record['scope_complete']);self.assertNotIn(record['status'],['verified_compatible','verified_reference','dependency_blocked','explicitly_excluded'])
   sheet=next(r for r in result['records'] if r['capability_id']=='APP-OFFICE-02');self.assertEqual(sheet['unsupported_actions'][0]['reason'],'Data import only')
   slides=next(r for r in result['records'] if r['capability_id']=='APP-OFFICE-03');self.assertEqual(slides['compatibility_differences'][0]['actualFormat'],'deck-native')
 def test_unsuccessful_evidence_rejected(self):
  with tempfile.TemporaryDirectory() as directory:
   path=Path(directory)/'result.json';path.write_text('{"harnessPassed":false}')
   with self.assertRaises(ValueError):generate(path)
if __name__=='__main__':unittest.main()
