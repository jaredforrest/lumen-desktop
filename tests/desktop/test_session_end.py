"""Captured cooperative preflight, explicit authorization and cancellation."""
import sys,unittest
from pathlib import Path
sys.path.insert(0,str(Path(__file__).resolve().parents[2]/'res/lumen-desktop/runtime'))
from application_lifecycle import Lifecycle,CooperativeApplication
from session_end import SessionEnd
from test_application_lifecycle import FakeBackend,Bus,node
class SessionEndTests(unittest.TestCase):
 def setUp(self):
  self.backend=FakeBackend();self.lifecycle=Lifecycle(self.backend);self.addCleanup(self.lifecycle.close);self.nodes=[node()];self.lifecycle.refresh('epoch',self.nodes);self.bus=Bus();self.session=SessionEnd(self.lifecycle,CooperativeApplication(self.lifecycle,self.bus))
 def exit(self):self.backend.identities.clear();self.nodes=[];self.lifecycle.refresh('epoch',[]);self.session.tick([])
 def test_wait_cancel_never_authorizes_later_exit(self):
  state=self.session.begin('reboot',self.nodes);token=state['token'];self.assertEqual(state['phase'],'waiting');self.assertEqual(len(self.bus.proxy.calls),1)
  self.session.cancel();self.exit();self.assertEqual(self.session.state()['phase'],'idle')
  with self.assertRaises(ValueError):self.session.authorize(token,[])
 def test_explicit_confirmation_consumes_exact_ready_token(self):
  state=self.session.begin('logout',self.nodes);token=state['token']
  with self.assertRaises(ValueError):self.session.authorize(token,self.nodes)
  self.exit();self.assertEqual(self.session.phase,'ready');self.assertEqual(self.session.authorize(token,[]),dict(action='logout',token=token))
  with self.assertRaises(ValueError):self.session.authorize(token,[])
 def test_unsupported_protocol_does_not_quit_any_app(self):
  self.bus.proxy.actions=[];self.assertEqual(self.session.begin('poweroff',self.nodes)['phase'],'blocked');self.assertEqual(self.bus.proxy.calls,[])
 def test_new_primary_window_blocks_but_owned_prompt_is_allowed(self):
  self.session.begin('reboot',self.nodes);dialog=node(ident=8,lumen_relationships={'parent':7});self.session.tick(self.nodes+[dialog]);self.assertEqual(self.session.phase,'waiting')
  self.session.tick(self.nodes+[node(ident=9)]);self.assertEqual(self.session.phase,'blocked')
 def test_replacement_and_new_session_invalidate_confirmation(self):
  self.session.begin('logout',self.nodes);self.backend.identities[22]['start']+=1;self.lifecycle.refresh('epoch',self.nodes);self.session.tick(self.nodes);self.assertEqual(self.session.phase,'blocked')
  self.session.begin('logout',self.nodes);self.lifecycle.refresh('replacement',[]);self.session.tick([]);self.assertEqual(self.session.phase,'blocked')
 def test_zero_window_observation_still_waits_for_actual_exit(self):
  self.lifecycle.refresh('epoch',[]);self.assertEqual(self.session.begin('logout',[])['phase'],'waiting');self.assertEqual(len(self.bus.proxy.calls),1)
 def test_shell_id_advertisement_does_not_hide_an_external_window(self):
  external=node(ident=9,app='org.quickshell');self.assertEqual(self.session.begin('logout',self.nodes+[external])['phase'],'blocked');self.assertEqual(self.bus.proxy.calls,[])
 def test_cancel_during_capability_checks_clears_undispatched_queue(self):
  self.backend.identities[23]=dict(pid=23,start=200,device=2,inode=4);self.nodes.append(node(pid=23,ident=8,app='org.example.Second'));self.lifecycle.refresh('epoch',self.nodes)
  self.assertEqual(self.session.begin('logout',self.nodes)['phase'],'checking');self.session.cancel();self.session.tick(self.nodes);self.assertEqual(self.bus.proxy.calls,[])
 def test_all_capabilities_checked_before_any_dispatch(self):
  self.backend.identities[23]=dict(pid=23,start=200,device=2,inode=4);self.nodes.append(node(pid=23,ident=8,app='org.example.Unsupported'));self.lifecycle.refresh('epoch',self.nodes)
  self.session.begin('logout',self.nodes);self.session.tick(self.nodes);self.assertEqual(self.session.phase,'blocked');self.assertEqual(self.bus.proxy.calls,[])
if __name__=='__main__':unittest.main()
