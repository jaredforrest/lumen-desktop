import json,os,sys,tempfile,unittest
from pathlib import Path
from unittest.mock import patch
sys.path.insert(0,str(Path(__file__).resolve().parents[2]/'res/character-viewer'))
from replacements import replacement_for
class ReplacementTests(unittest.TestCase):
 def test_boundaries_unicode_and_disable(self):
  with tempfile.TemporaryDirectory() as tmp,patch.dict(os.environ,{'XDG_STATE_HOME':tmp}):
   prefs=Path(tmp)/'lumen-desktop/preferences';prefs.mkdir(parents=True)
   (prefs/'text-tools.json').write_text(json.dumps([{'name':'omw','text':'On my way!'},{'name':'café','text':'coffee'}]))
   self.assertEqual(replacement_for('Hello, omw '),(4,'On my way! '))
   self.assertEqual(replacement_for('café '),(6,'coffee '))
   for text in ('someomw ','_omw ','omw','omw.','omw next '):self.assertIsNone(replacement_for(text))
   (prefs/'text-tools-options.json').write_text('{"automaticReplacements":false}')
   self.assertIsNone(replacement_for('omw '))
 def test_invalid_rows_are_not_expanded(self):
  with tempfile.TemporaryDirectory() as tmp,patch.dict(os.environ,{'XDG_STATE_HOME':tmp}):
   prefs=Path(tmp)/'lumen-desktop/preferences';prefs.mkdir(parents=True)
   (prefs/'text-tools.json').write_text(json.dumps([None,{'name':5,'text':'wrong'},{'name':'omw','text':''},{'name':'two words','text':'skip'}]))
   self.assertIsNone(replacement_for('two words '));self.assertIsNone(replacement_for('omw '))
if __name__=='__main__':unittest.main()
