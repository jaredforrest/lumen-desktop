"""Stack actions validate captured versions and fixed dispatch before any handoff."""
import json,os,sys,tempfile,unittest
from pathlib import Path
from unittest.mock import Mock,patch
sys.path.insert(0,str(Path(__file__).resolve().parents[2]/'res/lumen-desktop/shell'))
import folder_stack_actions as actions
from captured_resource_actions import facts,wire_facts,ResourceUnavailable

class StackActions(unittest.TestCase):
 def setUp(self):
  self.temp=tempfile.TemporaryDirectory();self.addCleanup(self.temp.cleanup);self.root=Path(self.temp.name);self.path=self.root/'literal # ü $(unchanged).txt';self.path.write_text('captured');self.request=dict(path=str(self.path),facts=wire_facts(facts(self.path)));self.runner=Mock()
 def test_literal_default_open_uses_fixed_command(self):
  with patch.dict(os.environ,{'LUMEN_LAUNCH_SCOPES':'0'}):result=actions.handle(self.request,self.runner)
  self.assertTrue(result['dispatched']);self.assertEqual(self.runner.call_args.args[0],['xdg-open',self.path.as_uri()]);self.assertEqual(self.path.read_text(),'captured')
 def test_replacement_and_in_place_change_never_dispatch(self):
  self.path.rename(self.root/'retired');self.path.write_text('replacement')
  with self.assertRaises(ResourceUnavailable):actions.handle(self.request,self.runner)
  self.runner.assert_not_called();self.request['facts']=wire_facts(facts(self.path));self.path.write_text('changed again')
  with self.assertRaises(ResourceUnavailable):actions.handle(self.request,self.runner)
  self.runner.assert_not_called()
 def test_directory_handoff_is_lossless_and_does_not_spawn(self):
  request=dict(path=str(self.root),facts=wire_facts(facts(self.root)));result=actions.handle(request,self.runner)
  self.assertEqual(result,dict(folder=str(self.root),expectedFacts=request['facts']));self.runner.assert_not_called()
 def test_symlink_and_special_file_are_explicitly_unsupported(self):
  link=self.root/'link';link.symlink_to(self.path);fifo=self.root/'pipe';os.mkfifo(fifo)
  for path in (link,fifo):
   with self.subTest(path=path),self.assertRaises(ResourceUnavailable):actions.handle(dict(path=str(path),facts=wire_facts(facts(path))),self.runner)
  self.runner.assert_not_called()
 def test_closed_protocol_rejects_ui_commands_and_imprecise_facts(self):
  for request in (dict(self.request,command=['true']),dict(path=str(self.path),facts=[1]*6),dict(path='relative',facts=self.request['facts'])):
   with self.assertRaises(ResourceUnavailable):actions.handle(request,self.runner)
  self.runner.assert_not_called()

if __name__=='__main__':unittest.main()
