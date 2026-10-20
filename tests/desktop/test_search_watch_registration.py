"""Real GIO registration seams and captured subscription generations."""
import sys,tempfile,threading,time,unittest
from pathlib import Path
sys.path.insert(0,str(Path(__file__).resolve().parents[2]/'res/lumen-desktop/shell'))
from search import Index
from file_watch import Watcher
from gi.repository import GLib

def wait(predicate):
 end=time.monotonic()+5
 while time.monotonic()<end:
  if predicate():return
  time.sleep(.02)
 raise AssertionError('Watch registration did not reconcile within its bound')

class WatchRegistrationTests(unittest.TestCase):
 def test_create_before_initial_and_descendant_registration_is_reconciled(self):
  with tempfile.TemporaryDirectory() as tmp:
   root=Path(tmp)/'Documents';root.mkdir();(root/'existing.txt').write_text('existing')
   index=Index([root],Path(tmp)/'index.sqlite');original=index.watcher.set_paths;injected=set()
   def seam(paths,ready=None):
    paths=set(paths)
    if 'root' not in injected:
     injected.add('root');(root/'seam-file.txt').write_text('created before initial registration')
     (root/'new').mkdir();(root/'new/first.txt').write_text('first descendant')
    elif str(root/'new') in paths and 'child' not in injected:
     injected.add('child');(root/'new/late.txt').write_text('created after descendant enumeration')
     (root/'new/deeper').mkdir();(root/'new/deeper/final.txt').write_text('new descendant while arming')
    if ready is None:original(paths)
    else:original(paths,ready)
   index.watcher.set_paths=seam
   try:
    index.scan()
    wait(lambda:len(index.search('ext:txt')['results'])==5)
    self.assertEqual(injected,{'root','child'})
    wait(lambda:str(root/'new/deeper') in index.watcher.monitors)
    (root/'new/deeper/live.txt').write_text('ordinary subsequent live event')
    wait(lambda:len(index.search('live.txt')['results'])==1)
   finally:index.closed.set();index.watcher.close()
 def test_superseded_ready_retains_pending_seam_until_current_ack(self):
  with tempfile.TemporaryDirectory() as tmp:
   root=Path(tmp)/'Documents';root.mkdir();index=Index([root],Path(tmp)/'index.sqlite');index.watcher.close()
   callbacks=[]
   class Deferred:
    def set_paths(self,paths,ready):callbacks.append(ready)
    def close(self):pass
   index.watcher=Deferred()
   try:
    index.update_watches([str(root)]);index.update_watches([str(root)])
    callbacks[0]({str(root)},set());self.assertEqual(index.dirty,set())
    callbacks[1]({str(root)},set());self.assertEqual(index.dirty,{str(root)})
    index.dirty.clear();index.closed.set();callbacks[1]({str(root)},set());self.assertEqual(index.dirty,set())
   finally:
    index.closed.set()
    if index.dirty_timer:index.dirty_timer.cancel()
 def test_queued_watcher_updates_and_close_cannot_reactivate_old_subscription(self):
  with tempfile.TemporaryDirectory() as tmp:
   root=Path(tmp);first=root/'first';second=root/'second';first.mkdir();second.mkdir();watcher=Watcher(lambda path:None)
   entered=threading.Event();release=threading.Event();acks=[]
   def block():entered.set();release.wait(5);return False
   try:
    GLib.idle_add(block);self.assertTrue(entered.wait(3))
    watcher.set_paths([first],lambda registered,failed:acks.append(('first',registered)))
    watcher.set_paths([second],lambda registered,failed:acks.append(('second',registered)))
    release.set();wait(lambda:bool(acks));self.assertEqual(acks,[('second',{str(second)})])
    entered.clear();release.clear();GLib.idle_add(block);self.assertTrue(entered.wait(3))
    watcher.set_paths([first],lambda registered,failed:acks.append(('closed',registered)))
    watcher.close();release.set();wait(lambda:not watcher.thread.is_alive())
    self.assertEqual(acks,[('second',{str(second)})]);self.assertEqual(watcher.monitors,{})
   finally:release.set();watcher.close()
if __name__=='__main__':unittest.main()
