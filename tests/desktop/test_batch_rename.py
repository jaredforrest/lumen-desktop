"""Disposable filesystem proofs for captured no-overwrite rename transactions."""
import errno
import os
from pathlib import Path
import sys
import tempfile
import unittest
sys.path.insert(0,str(Path(__file__).resolve().parents[2]/'res/lumen-desktop/shell'))
from file_batch_rename import RenamePlans,rename_noreplace
import file_identity


class BatchRename(unittest.TestCase):
    def setUp(self):
        self.temp=tempfile.TemporaryDirectory(prefix='lumen-batch-rename-');self.addCleanup(self.temp.cleanup)
        self.root=Path(self.temp.name);self.manager=RenamePlans()
    def items(self,*names):
        result=[]
        for name in names:
            path=self.root/name
            if not path.exists() and not path.is_symlink():path.write_text('contents of '+name)
            result.append(dict(path=str(path),identity=file_identity.wire(path.lstat())))
        return result
    def preview(self,items,mode='prefix',first='new-',second='',policy='stop',manager=None):
        return (manager or self.manager).preview(items,dict(mode=mode,first=first,second=second),policy)
    def confirm(self,plan,manager=None):return (manager or self.manager).execute(plan['token'],True)
    def test_preview_is_readonly_confirm_preserves_unicode_and_results(self):
        items=self.items('café.txt','二.txt');plan=self.preview(items)
        self.assertTrue(plan['executable']);self.assertEqual(sorted(p.name for p in self.root.iterdir()),['café.txt','二.txt'])
        result=self.confirm(plan);self.assertTrue(result['ok']);self.assertEqual([r['status'] for r in result['rows']],['renamed','renamed'])
        for item in items:self.assertEqual((self.root/('new-'+Path(item['path']).name)).read_text(),'contents of '+Path(item['path']).name)
        with self.assertRaises(ValueError):self.confirm(plan)
    def test_case_only_and_noop_replacement(self):
        items=self.items('lower-one','lower-two');plan=self.preview(items,'replace','lower','LOWER')
        self.assertTrue(self.confirm(plan)['ok']);self.assertTrue((self.root/'LOWER-one').exists())
        plan=self.preview(self.items('LOWER-one','LOWER-two'),'replace','absent','same');self.assertFalse(plan['executable']);self.assertTrue(all(r['status']=='unchanged' for r in plan['rows']))
    def test_conflict_stop_then_explicit_skip_never_overwrites(self):
        items=self.items('one','two');(self.root/'new-one').write_text('bystander')
        plan=self.preview(items);self.assertFalse(plan['executable'])
        with self.assertRaises(ValueError):self.confirm(plan)
        plan=self.preview(items,policy='skip');result=self.confirm(plan)
        self.assertEqual([r['status'] for r in result['rows']],['skipped','renamed']);self.assertEqual((self.root/'new-one').read_text(),'bystander');self.assertTrue((self.root/'one').exists())
    def test_duplicate_targets_dangling_symlink_and_cycles_reject(self):
        items=self.items('a','aa');plan=self.preview(items,'replace','a','x');self.assertTrue(plan['executable'])
        plan=self.preview(items,'replace','a','');self.assertFalse(plan['executable'])
        (self.root/'new-a').symlink_to('missing');plan=self.preview(items);self.assertFalse(plan['executable']);self.assertTrue((self.root/'new-a').is_symlink())
        plan=self.preview(items,'suffix','a');self.assertFalse(plan['executable'])
        items=self.items('match-1','match-2');plan=self.preview(items,'replace','1','2');self.assertFalse(plan['executable'])
    def test_invalid_names_and_transformation_inputs_do_not_write(self):
        items=self.items('one','two')
        for value in ('../','/','\0','\n','x'*256):
            with self.subTest(value=value):
                plan=self.preview(items,first=value);self.assertFalse(plan['executable'])
        for rule in ({},dict(mode='unknown',first='a',second=''),dict(mode='replace',first='',second='b'),dict(mode='prefix',first='a',second='b')):
            with self.assertRaises(ValueError):self.manager.preview(items,rule)
        self.assertEqual(sorted(p.name for p in self.root.iterdir()),['one','two'])
    def test_replaced_source_and_changed_contents_reject_entire_stale_plan(self):
        for mutate in ('replace','write'):
            items=self.items('one','two');plan=self.preview(items)
            target=self.root/'two'
            if mutate=='replace':target.unlink();target.write_text('replacement')
            else:target.write_text('changed contents')
            with self.assertRaises(ValueError):self.confirm(plan)
            self.assertTrue((self.root/'one').exists());self.assertFalse((self.root/'new-one').exists())
    def test_destination_appears_after_preview_rejects_all(self):
        plan=self.preview(self.items('one','two'));(self.root/'new-two').write_text('bystander')
        with self.assertRaises(ValueError):self.confirm(plan)
        self.assertTrue((self.root/'one').exists());self.assertEqual((self.root/'new-two').read_text(),'bystander')
    def test_parent_replacement_rejects_new_folder_and_old_files_untouched(self):
        folder=self.root/'folder';folder.mkdir();paths=[]
        for name in ('one','two'):
            p=folder/name;p.write_text(name);paths.append(dict(path=str(p),identity=file_identity.wire(p.stat())))
        plan=self.preview(paths);folder.rename(self.root/'old');folder.mkdir()
        with self.assertRaises(ValueError):self.confirm(plan)
        self.assertEqual(sorted(p.name for p in (self.root/'old').iterdir()),['one','two']);self.assertEqual(list(folder.iterdir()),[])
    def test_plan_expiry_cancel_and_explicit_confirmation(self):
        tick=[1];manager=RenamePlans(now=lambda:tick[0]);items=self.items('one','two');plan=self.preview(items,manager=manager)
        for value in (False,1,'true',None):
            with self.assertRaises(ValueError):manager.execute(plan['token'],value)
        tick[0]=301
        with self.assertRaises(ValueError):self.confirm(plan,manager)
        plan=self.preview(items,manager=manager);manager.cancel(plan['token'])
        with self.assertRaises(ValueError):self.confirm(plan,manager)
    def test_mismatched_captured_identity_refuses_preview(self):
        items=self.items('one','two');items[1]['identity']['inode']=str(int(items[1]['identity']['inode'])+1)
        plan=self.preview(items);self.assertFalse(plan['executable']);self.assertEqual(plan['rows'][1]['status'],'invalid')
    def test_partial_permission_failure_reports_exact_commits_and_stops(self):
        calls=[]
        def mover(fd,source,target):
            calls.append(source)
            if source=='two':raise PermissionError(errno.EACCES,'Permission denied',source)
            rename_noreplace(fd,source,target)
        manager=RenamePlans(mover=mover);plan=self.preview(self.items('one','two','three'),manager=manager);result=self.confirm(plan,manager)
        self.assertFalse(result['ok']);self.assertEqual([r['status'] for r in result['rows']],['renamed','failed','not_run']);self.assertEqual(calls,['one','two']);self.assertTrue((self.root/'new-one').exists());self.assertTrue((self.root/'two').exists());self.assertTrue((self.root/'three').exists())
    def test_actual_permission_denial_preserves_sources(self):
        if os.getuid()==0:self.skipTest('Permission denial requires unprivileged fixture process')
        plan=self.preview(self.items('one','two'));self.root.chmod(0o500)
        try:
            result=self.confirm(plan);self.assertFalse(result['ok']);self.assertEqual(result['rows'][0]['status'],'failed');self.assertTrue((self.root/'one').exists());self.assertTrue((self.root/'two').exists())
        finally:self.root.chmod(0o700)
    def test_atomic_racing_destination_creation_does_not_replace(self):
        def mover(fd,source,target):
            descriptor=os.open(target,os.O_CREAT|os.O_EXCL|os.O_WRONLY,0o600,dir_fd=fd)
            with os.fdopen(descriptor,'w') as stream:stream.write('racing bystander')
            rename_noreplace(fd,source,target)
        manager=RenamePlans(mover=mover);plan=self.preview(self.items('one','two'),manager=manager);result=self.confirm(plan,manager)
        self.assertFalse(result['ok']);self.assertEqual((self.root/'new-one').read_text(),'racing bystander');self.assertTrue((self.root/'one').exists())
    def test_new_source_replacement_between_rows_stops_remaining(self):
        def mover(fd,source,target):
            rename_noreplace(fd,source,target)
            if source=='one':
                os.unlink('two',dir_fd=fd)
                descriptor=os.open('two',os.O_CREAT|os.O_EXCL|os.O_WRONLY,0o600,dir_fd=fd);os.close(descriptor)
        manager=RenamePlans(mover=mover);plan=self.preview(self.items('one','two'),manager=manager);result=self.confirm(plan,manager)
        self.assertEqual([r['status'] for r in result['rows']],['renamed','failed']);self.assertTrue((self.root/'two').exists())
    def test_malformed_selections_never_create_authority(self):
        items=self.items('one','two')
        for selected in (None,[],items[:1],items+items,dict(items=items),[dict(path=items[0]['path'],identity=False),items[1]],[dict(path='relative',identity=items[0]['identity']),items[1]]):
            with self.subTest(selected=selected),self.assertRaises(ValueError):self.preview(selected)
        self.assertEqual(self.manager.plans,{})
    def test_simultaneous_confirmations_cannot_repeat_a_plan(self):
        from concurrent.futures import ThreadPoolExecutor
        plan=self.preview(self.items('one','two'))
        def attempt():
            try:return self.confirm(plan)['ok']
            except ValueError:return False
        with ThreadPoolExecutor(max_workers=2) as executor:outcomes=list(executor.map(lambda _:attempt(),range(2)))
        self.assertEqual(sorted(outcomes),[False,True]);self.assertEqual(sorted(p.name for p in self.root.iterdir()),['new-one','new-two'])
    def test_hardlinks_keep_independent_names_and_shared_content(self):
        first=self.root/'one';first.write_text('shared');os.link(first,self.root/'two')
        items=[dict(path=str(p),identity=file_identity.wire(p.lstat())) for p in (first,self.root/'two')]
        result=self.confirm(self.preview(items));self.assertTrue(result['ok'],result)
        self.assertEqual((self.root/'new-one').stat().st_ino,(self.root/'new-two').stat().st_ino)
    def test_symlink_and_directory_entries_keep_their_objects(self):
        (self.root/'folder').mkdir();(self.root/'link').symlink_to('missing')
        items=[dict(path=str(p),identity=file_identity.wire(p.lstat())) for p in (self.root/'folder',self.root/'link')]
        self.assertTrue(self.confirm(self.preview(items))['ok']);self.assertTrue((self.root/'new-link').is_symlink());self.assertTrue((self.root/'new-folder').is_dir())

if __name__=='__main__':unittest.main()
