"""Shortcut collisions are physical and scope-aware, never last-definition-wins."""
import importlib.util,json,subprocess,sys,tempfile,unittest
from pathlib import Path
ROOT=Path(__file__).resolve().parents[2]
spec=importlib.util.spec_from_file_location('shortcuts',ROOT/'res/lumen-desktop/runtime/shortcuts.py');module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)
def entry(name,chord,**extra):return {"id":name,"action":name,"chord":chord,**extra}
def validate(*rows):return module.validate(dict(version=1,bindings=list(rows)))
class Shortcuts(unittest.TestCase):
 def test_modifier_aliases_and_order_conflict(self):
  result=validate(entry('first','Mod4+Control+Shift+f'),entry('other','Shift+Ctrl+Super+F'))
  self.assertFalse(result['valid']);self.assertEqual(result['diagnostics'][0]['bindings'],['first','other'])
 def test_fullscreen_and_fill_are_distinct(self):
  self.assertTrue(validate(entry('fill','Super+Ctrl+Shift+f'),entry('fullscreen','Ctrl+Super+f'))['valid'])
 def test_press_release_and_disjoint_modes(self):
  self.assertTrue(validate(entry('press','Super_L'),entry('release','--release Super_L'),entry('other-mode','Super_L',mode='resize'))['valid'])
 def test_application_scope_is_explicit_not_inferred(self):
  self.assertTrue(validate(entry('editor','Ctrl+s',scope='application',appId='test.editor'),entry('writer','Control+s',scope='application',appId='test.writer'))['valid'])
  self.assertFalse(validate(entry('global','Ctrl+s'),entry('editor','Control+s',scope='application',appId='test.editor'))['valid'])
  self.assertFalse(validate(entry('current','Ctrl+s',scope='current-application'),entry('editor','Ctrl+s',scope='application',appId='test.editor'))['valid'])
 def test_devices_are_disjoint_only_when_both_explicit(self):
  self.assertTrue(validate(entry('a','Ctrl+s',device='keyboard-a'),entry('b','Ctrl+s',device='keyboard-b'))['valid'])
  self.assertFalse(validate(entry('a','Ctrl+s'),entry('b','--input-device=keyboard-b Ctrl+s'))['valid'])
 def test_locked_flag_extends_scope(self):
  self.assertFalse(validate(entry('lock','--locked Super+q'),entry('other','Mod4+q'))['valid'])
 def test_duplicate_action_still_rejects_overlapping_binding(self):
  self.assertFalse(validate(entry('a','Super+Tab',action='switch'),entry('b','Mod4+Tab',action='switch'))['valid'])
 def test_disabled_binding_does_not_capture(self):
  self.assertTrue(validate(entry('a','Ctrl+x'),entry('b','Control+x',enabled=False))['valid'])
 def test_invalid_registry_or_ambiguous_binding_rejected(self):
  for row in [entry('a','Ctrl+Control+x'),entry('a','AltGr+x'),entry('a','--made-up Ctrl+x'),entry('a','Ctrl+x',scope='application'),entry('a','Ctrl+x',scope='application',appId='test.*'),entry('a','Ctrl+x',scope='global',appId='test.editor'),entry('a','Ctrl+x',enabled=1),entry('a','--release Ctrl+x',trigger='press'),entry('a','Ctrl+x',unknown=True)]:
   with self.subTest(row=row),self.assertRaises(module.RegistryError):validate(row)
  with self.assertRaises(module.RegistryError):module.validate(dict(version=True,bindings=[]))
  with self.assertRaises(module.RegistryError):validate(entry('same','Ctrl+x'),entry('same','Ctrl+y'))
 def test_named_key_case_and_aliases_conflict(self):
  self.assertFalse(validate(entry('a','Control+Esc'),entry('b','Ctrl+Escape'))['valid'])
  self.assertFalse(validate(entry('a','Ctrl+F1'),entry('b','Control+f1'))['valid'])
 def test_default_registry_is_valid(self):
  result=module.validate(json.loads((ROOT/'res/lumen-desktop/runtime/shortcut-defaults.json').read_text()));self.assertTrue(result['valid'],result['diagnostics'])
  self.assertIn('window.fill',{r['action'] for r in result['bindings']})
 def test_cli_fails_with_structured_collision_diagnostics(self):
  with tempfile.TemporaryDirectory() as directory:
   source=Path(directory)/'input.json';output=Path(directory)/'output.json';source.write_text(json.dumps(dict(version=1,bindings=[entry('a','Super+x'),entry('b','Mod4+x')])))
   result=subprocess.run([sys.executable,module.__file__,str(source),'--output',str(output)],text=True,capture_output=True)
   self.assertEqual(result.returncode,1);self.assertEqual(json.loads(output.read_text())['diagnostics'][0]['code'],'shortcut_collision');self.assertIn('a, b',result.stderr)
if __name__=='__main__':unittest.main()
