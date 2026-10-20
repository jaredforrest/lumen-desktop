"""Captured policy packets, durable logical identities and no real commands."""
import copy,json,os,sys,tempfile,unittest
from pathlib import Path
from unittest.mock import patch
sys.path.insert(0,str(Path(__file__).resolve().parents[2]/'res/lumen-desktop/runtime'))
from spaces import SpacePolicy
from display_space_runtime import configured,synchronize,activation

class Runtime(unittest.TestCase):
 def setUp(self):
  self.tmp=tempfile.TemporaryDirectory();self.addCleanup(self.tmp.cleanup);self.path=Path(self.tmp.name)/'spaces.json'
  self.outputs=[dict(id=2**60+index,name=name,make='',model='',serial='',active=True,focused=index==1,rect=dict(x=index*800,y=0,width=800,height=600)) for index,name in enumerate(['A','B'])]
  self.workspaces=[dict(id=10,name='first',output='A',visible=True,focused=False),dict(id=11,name='second',output='A',visible=False,focused=False),dict(id=12,name='other',output='B',visible=True,focused=True)]
  self.model=SpacePolicy(self.path);self.model.reconcile(self.workspaces,self.outputs)
  self.native=dict(version=1,session='synthetic',generation='0',available=False,separateDisplays=True,coupled=False,primary=None);self.commands=[];self.reject=False
 def query(self,*args):
  if args==('-t','get_outputs'):return copy.deepcopy(self.outputs)
  if args==('-t','get_tree'):return dict(display_spaces=copy.deepcopy(self.native))
  self.assertEqual(len(args),1);prefix='lumen_window display-spaces configure ';self.assertTrue(args[0].startswith(prefix));packet=json.loads(bytes.fromhex(args[0][len(prefix):]));self.commands.append(packet)
  self.assertEqual(packet['expectedGeneration'],self.native['generation'])
  # All advertised logical contexts must already be durable before dispatch.
  state=json.loads(self.path.read_text());self.assertIn('automatic_primary',state)
  if not packet['separateDisplays']:self.assertTrue(state['display_cohorts'])
  if self.reject:return [dict(success=False)]
  self.native.update(generation=str(int(self.native['generation'])+1),available=True,separateDisplays=packet['separateDisplays'],coupled=not packet['separateDisplays'],primary=packet['primary'])
  return [dict(success=True)]
 def test_captured_packet_and_durable_contexts(self):
  state=synchronize(self.model,self.query,dict(separateDisplays=False));self.assertTrue(state['applied']);self.assertEqual(state['primary'],'A')
  self.assertEqual(len(self.commands),1);packet=self.commands[0];self.assertEqual(packet['outputs'][0]['id'],str(2**60));self.assertEqual(len(packet['cohorts']),2)
  before=json.loads(self.path.read_text());synchronize(self.model,self.query,dict(separateDisplays=False));self.assertEqual(len(self.commands),1);self.assertEqual(before,json.loads(self.path.read_text()))
 def test_rejected_registration_does_not_recreate_ids(self):
  self.reject=True
  with self.assertRaises(ValueError):synchronize(self.model,self.query,dict(separateDisplays=False))
  keys=set(self.model.state['desktops']);self.assertFalse(self.native['available']);self.reject=False
  self.assertTrue(synchronize(self.model,self.query,dict(separateDisplays=False))['applied']);self.assertEqual(keys,set(self.model.state['desktops']))
 def test_external_generation_requires_new_captured_registration(self):
  synchronize(self.model,self.query,{})
  self.native.update(generation='99',primary='B')
  synchronize(self.model,self.query,{})
  self.assertEqual(len(self.commands),2);self.assertEqual(self.commands[-1]['expectedGeneration'],'99');self.assertEqual(self.native['primary'],'A')
 def test_explicit_primary_and_preferred_reconnect(self):
  desired=dict(primaryDisplay=dict(kind='connector',name='B'))
  state=synchronize(self.model,self.query,desired);self.assertEqual(state['primary'],'B');self.assertFalse(state['fallback'])
 def test_missing_capability_is_explicit_and_has_no_command(self):
  def query(*args):return self.outputs if args==('-t','get_outputs') else {}
  state=synchronize(self.model,query,dict(separateDisplays=False));self.assertFalse(state['applied']);self.assertFalse(state['available']);self.assertTrue(state['error'])
 def test_activation_uses_exact_captured_cohort_and_no_text_command_injection(self):
  synchronize(self.model,self.query,dict(separateDisplays=False))
  desktop=self.model.desktop(self.model.order('A')[0])
  command=activation(self.model,desktop,self.query)
  prefix='lumen_window display-spaces activate '
  self.assertTrue(command.startswith(prefix));packet=json.loads(bytes.fromhex(command[len(prefix):]))
  self.assertEqual(packet['expectedGeneration'],self.native['generation']);self.assertEqual(packet['session'],'synthetic');self.assertEqual(packet['output'],'A');self.assertEqual(packet['workspace'],desktop['name']);self.assertEqual(packet['workspaceId'],str(desktop['id']))
  self.native['available']=False
  with self.assertRaises(ValueError):activation(self.model,desktop,self.query)
 def test_independent_activation_has_no_coupled_action(self):
  self.assertIsNone(activation(self.model,self.model.desktop(self.model.order('A')[0]),self.query))
 def test_private_preference_file(self):
  path=Path(self.tmp.name)/'settings.json';path.write_text(json.dumps(dict(spaces=dict(separateDisplays=False,primaryDisplay=dict(kind='connector',name='A')))))
  with patch.dict(os.environ,{'LUMEN_NIX_SETTINGS_ACTIVE':str(path)}):self.assertEqual(configured(),dict(separateDisplays=False,primaryDisplay='connector:A'))

if __name__=='__main__':unittest.main()
