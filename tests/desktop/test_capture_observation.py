"""Graph facts never become a permission or hardware-use guarantee."""
import json,sys,unittest
from unittest.mock import patch
from pathlib import Path
sys.path.insert(0,str(Path(__file__).resolve().parents[2]/'res/lumen-desktop/shell'))
import capture_observation as c

def node(i,kind,**props):return dict(id=i,type='PipeWire:Interface:Node',info=dict(props={'media.class':kind,**props}))
def link(i,source,target,state='active'):return dict(id=i,type='PipeWire:Interface:Link',info={'output-node-id':source,'input-node-id':target,'state':state})
class CaptureObservation(unittest.TestCase):
 def graph(self):return [node(1,'Audio/Source'),node(2,'Stream/Input/Audio',**{'client.id':'3'}),dict(id=3,type='PipeWire:Interface:Client',info=dict(props={'application.name':'Synthetic recorder'})),link(4,1,2)]
 def test_active_capture_and_reported_identity(self):
  d=c.describe(self.graph());self.assertTrue(d['available']);self.assertEqual(d['rows'][0]['kind'],'audio-input');self.assertEqual(d['rows'][0]['reportedApplication'],'Synthetic recorder');self.assertIn('not authenticated',d['rows'][0]['attribution'])
 def test_paused_and_removed_links_clear_without_historical_use(self):
  g=self.graph();g[-1]['info']['state']='paused';self.assertEqual(c.describe(g)['rows'],[]);self.assertEqual(c.describe(g[:-1])['rows'],[])
 def test_video_does_not_guess_camera_or_screen(self):
  d=c.describe([node(1,'Video/Source'),node(2,'Stream/Input/Video'),link(3,1,2)]);self.assertEqual(d['rows'][0]['kind'],'video-input');self.assertNotIn('camera',json.dumps(d['rows']))
 def test_multiple_channels_deduplicate_capture(self):
  g=self.graph()+[link(5,1,2)];self.assertEqual(c.describe(g)['rows'][0]['sources'],['1']);self.assertEqual(len(c.describe(g)['rows']),1)
 def test_incomplete_duplicate_and_imprecise_graph_reject(self):
  for g in [self.graph()[1:],self.graph()+[node(1,'Audio/Source')],[node(True,'Audio/Source')],[node(2**40,'Audio/Source')]]:
   with self.assertRaises(ValueError):c.describe(g)
 def test_empty_is_observation_not_idle_guarantee(self):
  d=c.describe([]);self.assertTrue(d['available']);self.assertEqual(d['rows'],[]);self.assertIn('not assessed',d['coverage'])
 def test_actual_child_transport_success_failure_and_bytes(self):
  command=[sys.executable,'-c','print('+repr(json.dumps(self.graph()))+')'];self.assertEqual(len(c.describe(c.read_graph(command))['rows']),1)
  with self.assertRaises(ValueError):c.read_graph([sys.executable,'-c','print("x"*10000)'],max_bytes=32)
  self.assertFalse(c.snapshot([sys.executable,'-c','raise SystemExit(2)'])['available'])
 def test_late_eof_is_rejected_even_after_child_exits(self):
  now=[0.0];read=c.os.read;popen=c.subprocess.Popen;descriptor=[None]
  def child(*args,**kwargs):
   process=popen(*args,**kwargs);descriptor[0]=process.stdout.fileno();return process
  def receive(*args):
   value=read(*args)
   if args[0]==descriptor[0] and not value:now[0]=100
   return value
  with patch.object(c,'clock',lambda:now[0]),patch.object(c.subprocess,'Popen',child),patch.object(c.os,'read',receive):
   with self.assertRaises(TimeoutError):c.read_graph([sys.executable,'-c','print("[]")'],deadline=.05)
  self.assertEqual(now[0],100)
 def test_late_parser_completion_is_rejected(self):
  now=[0.0];loads=json.loads
  def parse(value):result=loads(value);now[0]=100;return result
  with patch.object(c,'clock',lambda:now[0]),patch.object(c.json,'loads',parse):
   with self.assertRaises(TimeoutError):c.read_graph([sys.executable,'-c','print("[]")'],deadline=.05)
 def test_actual_timeout_kills_and_reaps_child(self):
  with self.assertRaises(TimeoutError):c.read_graph([sys.executable,'-c','import time;time.sleep(10)'],deadline=.05)
if __name__=='__main__':unittest.main()
