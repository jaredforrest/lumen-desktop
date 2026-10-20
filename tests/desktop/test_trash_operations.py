"""Confirmed deletion touches only disposable captured personal-Trash records."""
import json,os,stat,sys,tempfile,time,unittest
from pathlib import Path
from unittest.mock import patch
from urllib.parse import quote
sys.path.insert(0,str(Path(__file__).resolve().parents[2]/'res/lumen-desktop/shell'))
import trash_operations as trash
import file_identity

class TrashOperations(unittest.TestCase):
 def setUp(self):
  self.temporary=tempfile.TemporaryDirectory(prefix='lumen-trash-test-');self.home=Path(self.temporary.name);self.root=self.home/'Trash';self.root.mkdir(mode=0o700)
  for name in ('files','info'):(self.root/name).mkdir(mode=0o700)
  self.engine=trash.TrashOperations(self.root)
 def tearDown(self):self.engine.close();self.temporary.cleanup()
 def item(self,name='one',directory=False,symlink=None):
  path=self.root/'files'/name
  if symlink is not None:path.symlink_to(symlink)
  elif directory:path.mkdir(mode=0o700);(path/'child.txt').write_text('captured child')
  else:path.write_text('captured '+name)
  record=self.root/'info'/(name+'.trashinfo');record.write_text('[Trash Info]\nPath='+quote(str(self.home/'Original'/name),safe='/')+'\nDeletionDate=2026-10-20T12:00:00\n');record.chmod(0o600)
  return path
 def reference(self,path):return dict(path=str(path),identity=file_identity.wire(path.lstat()))
 def wait(self):
  deadline=time.monotonic()+5
  while self.engine.busy and time.monotonic()<deadline:time.sleep(.01)
  self.assertFalse(self.engine.busy);return self.engine.status()['operations'][-1]
 def test_journal_and_recovery_are_private_and_symlink_safe(self):
  self.item();plan=self.engine.prepare('empty');outside=self.home/'outside';outside.mkdir();(self.root/'.lumen-deletion').symlink_to(outside,target_is_directory=True)
  with self.assertRaises(OSError):self.engine.confirm(plan['token'])
  self.assertEqual(list(outside.iterdir()),[]);self.assertTrue((self.root/'files/one').exists())
 def test_journal_permissions_and_failed_durability_prevent_mutation(self):
  self.item();plan=self.engine.prepare('empty')
  with patch.object(trash.os,'fsync',side_effect=OSError('unsupported durability')):
   with self.assertRaises(OSError):self.engine.confirm(plan['token'])
  self.assertTrue((self.root/'files/one').exists());self.assertFalse(self.engine.busy)
  plan=self.engine.prepare('empty');self.engine.confirm(plan['token']);self.wait()
  self.assertEqual(stat.S_IMODE(self.engine.journal.stat().st_mode),0o600);self.assertEqual(stat.S_IMODE(self.engine.journal.parent.stat().st_mode),0o700)
 def test_stop_and_cooperative_budget_keep_remaining_contents(self):
  path=self.item(directory=True);plan=self.engine.prepare('empty')
  with patch.object(trash,'DELETE_SECONDS',-1):self.engine.confirm(plan['token']);result=self.wait()
  self.assertEqual(result['state'],'partial');self.assertTrue((path/'child.txt').exists());self.assertEqual(result['items'][0]['removedNodes'],0)
  plan=self.engine.prepare('empty');original=self.engine.check
  def stopped(deadline):self.engine.stop_event.set();return original(deadline)
  with patch.object(self.engine,'check',side_effect=stopped):self.engine.confirm(plan['token']);result=self.wait()
  self.assertEqual(result['state'],'partial');self.assertTrue((path/'child.txt').exists())
 def test_rollback_never_overwrites_later_arrival(self):
  path=self.item();plan=self.engine.prepare('empty');original=self.engine.remove
  def collide(fd,name,expected,directory_entry=False):
   if name=='data':path.write_text('later arrival');raise PermissionError('synthetic failure')
   return original(fd,name,expected,directory_entry)
  with patch.object(self.engine,'remove',side_effect=collide):self.engine.confirm(plan['token']);result=self.wait()
  item=result['items'][0];self.assertEqual(path.read_text(),'later arrival');self.assertEqual(item['state'],'kept');self.assertTrue(Path(item['payloadPath']).exists());self.assertTrue(Path(item['metadataPath']).exists());self.assertTrue(item['recoveryPath'])
 def test_recovery_root_replacement_rejects_next_dispatch(self):
  self.item();self.engine.persist();recovery=self.engine.journal.parent;recovery.rename(self.root/'retired-recovery');recovery.mkdir(mode=0o700);plan=self.engine.prepare('empty')
  with self.assertRaises(ValueError):self.engine.confirm(plan['token'])
  self.assertFalse(self.engine.busy);self.assertTrue((self.root/'files/one').exists());self.assertEqual(list(recovery.iterdir()),[])
 def test_actual_listing_emits_lossless_selected_identity(self):
  from file_jobs import trash_items
  from types import SimpleNamespace
  path=self.item();native=path.lstat()
  with patch.dict(os.environ,{'XDG_DATA_HOME':str(self.home)}):
   row=trash_items()[0];self.assertEqual(row['identity'],file_identity.wire(native))
   original=Path.lstat
   def high(value,*args,**kwargs):
    observed=original(value,*args,**kwargs)
    if value!=path:return observed
    return SimpleNamespace(st_dev=2**60+7,st_ino=2**63+9,st_mode=observed.st_mode,st_size=observed.st_size,st_mtime=observed.st_mtime)
   with patch.object(Path,'lstat',high):row=trash_items()[0]
   self.assertEqual(row['identity'],dict(device=str(2**60+7),inode=str(2**63+9)))
 def test_preparation_cancel_and_expiry_never_mutate(self):
  path=self.item();plan=self.engine.prepare('selected',[self.reference(path)]);self.assertEqual(plan['count'],1);self.assertIn('cannot be undone',plan['consequence']);self.assertTrue(path.exists())
  self.engine.cancel(plan['token'])
  with self.assertRaises(ValueError):self.engine.confirm(plan['token'])
  plan=self.engine.prepare('empty');self.engine.pending[plan['token']]['expires']=0
  with self.assertRaises(ValueError):self.engine.confirm(plan['token'])
  self.assertTrue(path.exists())
 def test_confirmed_selected_file_and_duplicate_confirmation(self):
  path=self.item();other=self.item('kept');plan=self.engine.prepare('selected',[self.reference(path)]);self.engine.confirm(plan['token']);result=self.wait();self.assertEqual(result['state'],'complete',result);self.assertFalse(path.exists());self.assertTrue(other.exists());self.assertFalse((self.root/'info/one.trashinfo').exists())
  self.engine.confirm(plan['token']);self.assertEqual(len(self.engine.history),1);self.assertTrue(other.exists())
 def test_empty_capture_excludes_later_arrivals(self):
  first=self.item();plan=self.engine.prepare('empty');late=self.item('later');self.engine.confirm(plan['token']);self.assertEqual(self.wait()['state'],'complete');self.assertFalse(first.exists());self.assertTrue(late.exists());self.assertTrue((self.root/'info/later.trashinfo').exists())
 def test_replaced_item_and_metadata_are_kept(self):
  for replacement in ('item','metadata'):
   with self.subTest(replacement=replacement):
    path=self.item(replacement);plan=self.engine.prepare('selected',[self.reference(path)])
    target=path if replacement=='item' else self.root/'info'/(replacement+'.trashinfo');target.rename(target.with_name(target.name+'.old'));target.write_text('replacement')
    self.engine.confirm(plan['token']);result=self.wait();self.assertEqual(result['state'],'partial',result);self.assertTrue(path.exists());self.assertEqual(target.read_text(),'replacement')
 def test_new_child_after_confirmation_is_preserved(self):
  path=self.item(directory=True);plan=self.engine.prepare('empty');(path/'later.txt').write_text('later');self.engine.confirm(plan['token']);result=self.wait();self.assertEqual(result['state'],'partial');self.assertEqual((path/'later.txt').read_text(),'later');self.assertTrue((path/'child.txt').exists())
 def test_directory_and_symlink_delete_do_not_follow_external_target(self):
  outside=self.home/'outside';outside.write_text('must remain');directory=self.item('folder',directory=True);link=self.item('link',symlink=outside);plan=self.engine.prepare('empty');self.engine.confirm(plan['token']);result=self.wait();self.assertEqual(result['state'],'complete',result);self.assertFalse(directory.exists());self.assertFalse(link.is_symlink());self.assertEqual(outside.read_text(),'must remain')
 def test_unsupported_special_or_orphan_rejects_entire_empty_preparation(self):
  normal=self.item();pipe=self.item('pipe');pipe.unlink();os.mkfifo(pipe)
  with self.assertRaises(ValueError):self.engine.prepare('empty')
  self.assertTrue(normal.exists());pipe.unlink();(self.root/'info/pipe.trashinfo').unlink();(self.root/'files/orphan').write_text('keep')
  with self.assertRaises(OSError):self.engine.prepare('empty')
  self.assertTrue(normal.exists());self.assertEqual((self.root/'files/orphan').read_text(),'keep')
 def test_arbitrary_paths_network_routes_and_imprecise_identities_reject(self):
  path=self.item()
  for reference in (dict(path=str(self.home/'outside'),identity=dict(device='1',inode='2')),dict(path='smb://server/share/item',identity=dict(device='1',inode='2')),dict(path=str(path),identity=dict(device=2**60,inode=2**60))):
   with self.assertRaises(ValueError):self.engine.prepare('selected',[reference])
  self.assertTrue(path.exists())
 def test_root_replacement_fails_without_touching_replacement(self):
  path=self.item();plan=self.engine.prepare('empty');original=self.root.with_name('OriginalTrash');self.root.rename(original);self.root.mkdir(mode=0o700)
  for name in ('files','info'):(self.root/name).mkdir(mode=0o700)
  new=self.root/'files/one';new.write_text('replacement')
  with self.assertRaises(ValueError):self.engine.confirm(plan['token'])
  self.assertFalse((self.root/'.lumen-deletion').exists());self.assertEqual(new.read_text(),'replacement');self.assertTrue((original/'files/one').exists())
 def test_post_stage_replacement_is_not_deleted(self):
  path=self.item();plan=self.engine.prepare('empty');original=trash.rename_exact;swapped=False
  def swap(source,source_fd,target,target_fd):
   nonlocal swapped
   if source=='one' and not swapped:
    swapped=True;path.rename(path.with_name('retired'));path.write_text('replacement')
   return original(source,source_fd,target,target_fd)
  with patch.object(trash,'rename_exact',side_effect=swap):self.engine.confirm(plan['token']);result=self.wait()
  self.assertEqual(result['state'],'partial',result);self.assertEqual(path.read_text(),'replacement');self.assertEqual(path.with_name('retired').read_text(),'captured one')
 def test_late_descendant_during_deletion_remains_in_recovery(self):
  path=self.item(directory=True);plan=self.engine.prepare('empty');original=self.engine.remove;injected=False
  def late(fd,name,expected,directory_entry=False):
   nonlocal injected
   if name=='child.txt' and not injected:
    injected=True;descriptor=os.open('later.txt',os.O_WRONLY|os.O_CREAT|os.O_EXCL,0o600,dir_fd=fd)
    with os.fdopen(descriptor,'w') as stream:stream.write('later must remain')
   return original(fd,name,expected,directory_entry)
  with patch.object(self.engine,'remove',side_effect=late):self.engine.confirm(plan['token']);result=self.wait()
  item=result['items'][0];self.assertEqual(item['state'],'partial',result);recovery=Path(item['recoveryPath']);self.assertEqual((recovery/'data/later.txt').read_text(),'later must remain');self.assertTrue((recovery/'record.trashinfo').exists());self.assertEqual(item['removedNodes'],1)
 def test_bounds_reject_before_mutation(self):
  path=self.item(directory=True)
  for setting,value in [('MAX_ITEMS',0),('MAX_NODES',1),('MAX_BYTES',1),('PREPARE_SECONDS',-1)]:
   with patch.object(trash,setting,value),self.assertRaises(ValueError):self.engine.prepare('empty')
   self.assertTrue((path/'child.txt').exists())
 def test_permission_failure_preserves_payload_and_record(self):
  path=self.item();plan=self.engine.prepare('empty')
  with patch.object(self.engine,'remove',side_effect=PermissionError('synthetic denied deletion')):self.engine.confirm(plan['token']);result=self.wait()
  self.assertEqual(result['items'][0]['state'],'kept',result);self.assertTrue(path.exists());self.assertTrue((self.root/'info/one.trashinfo').exists())
 def test_restart_marks_interrupted_and_does_not_retry(self):
  path=self.item();plan=self.engine.prepare('empty');token=plan['token'];self.engine.history[token]=dict(id=token,state='running',items=[dict(state='deleting',recoveryPath='/synthetic/recovery')]);self.engine.persist();restored=trash.TrashOperations(self.root,self.engine.journal)
  try:self.assertEqual(restored.status()['operations'][0]['state'],'interrupted');self.assertTrue(restored.status()['operations'][0]['items'][0]['uncertain']);self.assertTrue(path.exists())
  finally:restored.close()
 def test_unicode_names_and_retained_capability_limits(self):
  path=self.item('café "quote" Ω');plan=self.engine.prepare('selected',[self.reference(path)]);self.assertEqual(plan['names'],[path.name]);self.engine.confirm(plan['token']);self.assertEqual(self.wait()['state'],'complete')

if __name__=='__main__':unittest.main()
