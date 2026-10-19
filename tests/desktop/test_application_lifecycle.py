"""Exact native process lifetime, safe cooperative Quit and ambiguous ownership."""
import importlib.util,os,subprocess,sys,time,unittest
from pathlib import Path
from unittest.mock import patch
ROOT=Path(__file__).resolve().parents[2];sys.path.insert(0,str(ROOT/'res/lumen-desktop/runtime'))
import application_lifecycle as lifecycle

def node(pid=22,app='org.example.Editor',ident=7,**extra):return dict(id=ident,pid=pid,app_id=app,shell='xdg_shell',**extra)
class FakeBackend:
 def __init__(self):self.identities={22:dict(pid=22,start=100,device=2,inode=3)};self.handles={};self.closed=[];self.counter=0
 def identity(self,pid):
  if pid not in self.identities:raise ValueError('unowned process')
  return self.identities[pid].copy()
 def open(self,pid):self.counter+=1;self.handles[self.counter]=pid;return self.counter
 def exited(self,handle):return self.handles[handle] not in self.identities
 def close(self,handle):self.closed.append(handle)
class Proxy:
 def __init__(self,pid=22):self.pid=pid;self.actions=['quit'];self.description=(True,'',[]);self.calls=[];self.before_activate=None
 def GetConnectionUnixProcessID(self,owner,**kwargs):return self.pid
 def List(self,**kwargs):return self.actions
 def Describe(self,*args,**kwargs):
  if self.before_activate:self.before_activate()
  return self.description
 def Activate(self,*args,**kwargs):self.calls.append((args,kwargs))
class Bus:
 def __init__(self):self.owner=':1.20';self.proxy=Proxy()
 def get_name_owner(self,app):return self.owner
 def get_object(self,*args,**kwargs):return self.proxy
class ApplicationLifecycleTests(unittest.TestCase):
 def setUp(self):self.backend=FakeBackend();self.tracker=lifecycle.Lifecycle(self.backend);self.addCleanup(self.tracker.close)
 def test_last_window_close_retains_live_process_and_reopen(self):
  row=self.tracker.refresh('session',[node()])[0];instance=row['instanceIds'];self.assertFalse(row['zeroWindows'])
  row=self.tracker.refresh('session',[])[0];self.assertTrue(row['zeroWindows']);self.assertEqual(row['instanceIds'],instance)
  row=self.tracker.refresh('session',[node(ident=9)])[0];self.assertEqual(row['windowCount'],1);self.assertEqual(row['instanceIds'],instance)
 def test_exit_and_pid_reuse_remove_old_identity(self):
  original=self.tracker.refresh('session',[node()])[0]['instanceIds'];self.backend.identities[22]['start']=200
  self.assertEqual(self.tracker.refresh('session',[]),[])
  replacement=self.tracker.refresh('session',[node()])[0]['instanceIds'];self.assertNotEqual(original,replacement);self.assertTrue(self.backend.closed)
 def test_shared_browser_process_never_fabricates_zero_window_app(self):
  rows=self.tracker.refresh('session',[node(),node(app='org.example.WebApp',ident=8)]);self.assertTrue(all(r['sharedProcess'] for r in rows))
  rows=self.tracker.refresh('session',[node()]);self.assertEqual([r['appId'] for r in rows],['org.example.Editor'])
  self.assertEqual(self.tracker.refresh('session',[]),[])
  with self.assertRaises(ValueError):self.tracker.exact_process('org.example.Editor')
 def test_independent_instances_do_not_become_one_quit_target(self):
  self.backend.identities[23]=dict(pid=23,start=300,device=2,inode=3)
  rows=self.tracker.refresh('session',[node(),node(pid=23,ident=8)])
  self.assertEqual(rows[0]['processCount'],2)
  with self.assertRaises(ValueError):self.tracker.exact_process('org.example.Editor')
 def test_session_epoch_and_unsupported_windows_fail_closed(self):
  self.tracker.refresh('session',[node()]);self.assertEqual(self.tracker.refresh('new-session',[]),[])
  for value in [node(app='org.quickshell'),node(pid=True),node(pid=999),node(window=44),dict(id=5,pid=22,window=44,shell='xwayland',app_id=None)]:
   self.assertEqual(self.tracker.refresh('new-session',[value]),[])
 def test_pid_changes_during_handle_acquisition_are_rejected(self):
  original=self.backend.open
  def race(pid):
   handle=original(pid);self.backend.identities[pid]['start']+=1;return handle
  self.backend.open=race;self.assertEqual(self.tracker.refresh('session',[node()]),[]);self.assertEqual(self.backend.closed,[1])
 def test_cooperative_quit_invokes_only_advertised_action(self):
  self.tracker.refresh('session',[node()]);bus=Bus();result=lifecycle.CooperativeApplication(self.tracker,bus).quit('org.example.Editor')
  self.assertTrue(result['requested']);self.assertEqual(bus.proxy.calls[0][0][0],'quit');self.assertEqual(result['status'],'requested-not-confirmed-exited')
  self.assertEqual(len(self.tracker.rows()),1)
 def test_unsupported_disabled_parameterized_quit_is_never_close_or_signal(self):
  self.tracker.refresh('session',[node()])
  for actions,description in [([], (True,'',[])),(['quit'],(False,'',[])),(['quit'],(True,'s',[]))]:
   bus=Bus();bus.proxy.actions=actions;bus.proxy.description=description
   with self.assertRaises(ValueError):lifecycle.CooperativeApplication(self.tracker,bus).quit('org.example.Editor')
   self.assertEqual(bus.proxy.calls,[])
 def test_unmatched_owner_and_changed_owner_never_receive_quit(self):
  self.tracker.refresh('session',[node()]);bus=Bus();bus.proxy.pid=999
  with self.assertRaises(ValueError):lifecycle.CooperativeApplication(self.tracker,bus).quit('org.example.Editor')
  self.assertEqual(bus.proxy.calls,[])
  bus.proxy.pid=22;bus.proxy.before_activate=lambda:setattr(bus,'owner',':1.99')
  with self.assertRaises(ValueError):lifecycle.CooperativeApplication(self.tracker,bus).quit('org.example.Editor')
  self.assertEqual(bus.proxy.calls,[])
 def test_real_disposable_child_exit_is_observed_with_pidfd(self):
  child=subprocess.Popen([sys.executable,'-c','import time;print("ready",flush=True);time.sleep(60)'],stdout=subprocess.PIPE,text=True)
  tracker=lifecycle.Lifecycle();self.addCleanup(tracker.close)
  try:
   self.assertEqual(child.stdout.readline().strip(),'ready')
   rows=tracker.refresh('private-fixture',[node(pid=child.pid)]);self.assertEqual(len(rows),1)
   self.assertTrue(tracker.refresh('private-fixture',[])[0]['zeroWindows'])
   child.terminate();child.wait(timeout=5);tracker.prune();self.assertEqual(tracker.rows(),[])
  finally:
   if child.poll() is None:child.kill();child.wait()
   child.stdout.close()
if __name__=='__main__':unittest.main()
