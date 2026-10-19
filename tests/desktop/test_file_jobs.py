"""Exercise real jobs on disposable trees, including cancellation and cross-device moves."""
import os, sys, tempfile, threading, time, unittest
from pathlib import Path
sys.path.insert(0,str(Path(__file__).resolve().parents[2]/'res/lumen-desktop/shell'))
from file_jobs import Jobs, trash_items
from file_clipboard import paths_from_uris

class FileJobs(unittest.TestCase):
    def setUp(self):
        self.temp=tempfile.TemporaryDirectory();self.root=Path(self.temp.name)
        self.src=self.root/'source';self.dst=self.root/'target';self.src.mkdir();self.dst.mkdir()
        self.old=os.environ.get('XDG_DATA_HOME');os.environ['XDG_DATA_HOME']=str(self.root/'data')
        self.jobs=Jobs()
    def tearDown(self):
        if self.old is None:os.environ.pop('XDG_DATA_HOME',None)
        else:os.environ['XDG_DATA_HOME']=self.old
        for folder,dirs,files in os.walk(self.root):Path(folder).chmod(0o700)
        self.temp.cleanup()
    def wait(self,ident):
        end=time.monotonic()+10
        while time.monotonic()<end:
            job=next(j for j in self.jobs.state()['jobs'] if j['id']==ident)
            if job['state'] not in ('running','cancelling'):return job
            time.sleep(.01)
        self.fail('Job did not complete')
    def runjob(self,op,paths=(),destination=None,**extra):
        return self.wait(self.jobs.start(dict(operation=op,paths=[str(p) for p in paths],destination=str(destination or self.dst),**extra))['job'])
    def test_copy_metadata_links_conflicts(self):
        (self.src/'private').mkdir(mode=0o700);(self.src/'private/a').write_text('payload')
        (self.src/'broken').symlink_to('/nonexistent-job-fixture')
        stamp=1612345678987654321;os.utime(self.src/'private',ns=(stamp,stamp))
        j=self.runjob('copy',[self.src]);self.assertEqual(j['state'],'complete')
        self.assertEqual((self.dst/'source/private').stat().st_mode&0o777,0o700)
        self.assertEqual((self.dst/'source/private').stat().st_mtime_ns,stamp)
        self.assertTrue((self.dst/'source/broken').is_symlink())
        self.assertEqual(self.runjob('copy',[self.src])['state'],'partial')
        self.assertEqual(self.runjob('copy',[self.src],conflict='keep-both')['state'],'complete')
        self.assertTrue((self.dst/'source copy 2').is_dir())
    def test_duplicate_in_place_preserves_sources_and_existing_copies(self):
        item=self.src/'report.txt';item.write_text('original')
        occupied=self.src/'report copy 2.txt';occupied.write_text('keep existing copy')
        folder=self.src/'folder';folder.mkdir();(folder/'nested').write_text('nested source')
        job=self.runjob('copy',[item,folder],destination=self.src,conflict='keep-both')
        self.assertEqual(job['state'],'complete')
        self.assertEqual(item.read_text(),'original');self.assertEqual(occupied.read_text(),'keep existing copy')
        self.assertEqual((self.src/'report copy 3.txt').read_text(),'original')
        self.assertEqual((self.src/'folder copy 2/nested').read_text(),'nested source')
        self.assertEqual((folder/'nested').read_text(),'nested source')

    def test_cancel_cleans_partial_and_preserves_source(self):
        (self.src/'a').write_bytes(b'x'*1024)
        entered=threading.Event();release=threading.Event();original=self.jobs.copy
        def delayed(ident,src,dst):
            entered.set();release.wait(3);return original(ident,src,dst)
        self.jobs.copy=delayed
        ident=self.jobs.start(dict(operation='copy',paths=[str(self.src)],destination=str(self.dst)))['job']
        self.assertTrue(entered.wait(2));self.jobs.cancel(ident);release.set()
        self.assertEqual(self.wait(ident)['state'],'cancelled')
        self.assertTrue((self.src/'a').exists());self.assertEqual(list(self.dst.iterdir()),[])
    def test_batch_partial_and_safe_undo(self):
        a=self.src/'a';b=self.src/'b';a.write_text('a');b.write_text('b');(self.dst/'b').write_text('keep')
        j=self.runjob('move',[a,b]);self.assertEqual(j['completed'],1);self.assertEqual(j['state'],'partial')
        self.assertEqual((self.dst/'b').read_text(),'keep');self.assertTrue(b.exists())
        self.assertEqual(self.runjob('undo')['state'],'complete');self.assertTrue(a.exists())
    def test_undo_refuses_modified_destination(self):
        a=self.src/'a';a.write_text('first');self.runjob('move',[a]);(self.dst/'a').write_text('new work')
        self.assertEqual(self.runjob('undo')['state'],'failed');self.assertEqual((self.dst/'a').read_text(),'new work')
    def test_reject_descendant_and_special_files(self):
        j=self.runjob('copy',[self.src],destination=self.src);self.assertEqual(j['state'],'partial')
        pipe=self.src/'pipe';os.mkfifo(pipe);j=self.runjob('copy',[pipe]);self.assertEqual(j['state'],'partial')
        self.assertFalse(list(self.dst.glob('.lumen-copy-*')))
    def test_cross_device_move(self):
        with tempfile.TemporaryDirectory(dir='/dev/shm',prefix='files-move-') as external:
            (self.src/'a').write_text('cross device');(self.src/'nested').mkdir();(self.src/'nested/b').write_text('b')
            j=self.runjob('move',[self.src],destination=Path(external));self.assertEqual(j['state'],'complete',j)
            self.assertFalse(self.src.exists());self.assertEqual((Path(external)/'source/a').read_text(),'cross device')
            self.assertEqual(self.runjob('undo')['state'],'complete');self.assertTrue((self.src/'nested/b').exists())
    def test_trash_restore_collision(self):
        a=self.src/'a';a.write_text('recover');self.assertEqual(self.runjob('trash',[a])['state'],'complete')
        rows=trash_items();self.assertEqual(len(rows),1);trashed=Path(rows[0]['path'])
        a.write_text('new');self.assertEqual(self.runjob('restore',[trashed])['state'],'partial');self.assertEqual(a.read_text(),'new')
        self.assertTrue(trashed.exists());a.unlink();self.assertEqual(self.runjob('restore',[trashed])['state'],'complete');self.assertEqual(a.read_text(),'recover')
    def test_long_filename_trash_restore(self):
        original=self.src/('x'*255);original.write_text('long name')
        self.assertEqual(self.runjob('trash',[original])['state'],'complete')
        self.assertEqual(self.runjob('restore',[Path(trash_items()[0]['path'])])['state'],'complete')
        self.assertEqual(original.read_text(),'long name')
    def test_clipboard_uris(self):
        self.assertEqual(paths_from_uris('file:///tmp/a%20b\r\nfile:///tmp/%23c'),['/tmp/a b','/tmp/#c'])
        for bad in ['https://x/a','file://remote/a','file:///tmp/a%00x','/tmp/a']:
            with self.assertRaises(ValueError):paths_from_uris(bad)

class FileJobRequests(unittest.TestCase):
    setUp=FileJobs.setUp
    tearDown=FileJobs.tearDown
    wait=FileJobs.wait
    runjob=FileJobs.runjob
    def test_duplicate_request_returns_original_without_second_copy(self):
        item=self.src/'item';item.write_text('content')
        request=dict(operation='copy',paths=[str(item)],destination=str(self.dst),conflict='keep-both',requestId='fixture-copy-1')
        first=self.jobs.start(request);self.wait(first['job'])
        second=self.jobs.start(request)
        self.assertEqual(first['job'],second['job'])
        self.assertEqual([p.name for p in self.dst.iterdir()],['item'])
        with self.assertRaisesRegex(ValueError,'different operation'):
            self.jobs.start({**request,'conflict':'skip'})

    def test_request_receipt_survives_service_restart(self):
        journal=self.root/'journal.json';self.jobs=Jobs(journal)
        item=self.src/'item';item.write_text('content')
        request=dict(operation='move',paths=[str(item)],destination=str(self.dst),requestId='fixture-move-1')
        ident=self.jobs.start(request)['job'];self.wait(ident)
        restarted=Jobs(journal)
        self.assertEqual(restarted.start(request)['job'],ident)
        self.assertEqual((self.dst/'item').read_text(),'content')

    def test_unknown_outcome_is_not_automatically_retried(self):
        import json
        journal=self.root/'journal.json';self.jobs=Jobs(journal)
        item=self.src/'item';item.write_text('content')
        request=dict(operation='copy',paths=[str(item)],destination=str(self.dst),requestId='fixture-interrupted-1')
        ident=self.jobs.start(request)['job'];self.wait(ident)
        rows=json.loads(journal.read_text());rows[0]['state']='running';journal.write_text(json.dumps(rows))
        restarted=Jobs(journal);reply=restarted.start(request)
        self.assertEqual(reply['job'],ident)
        self.assertEqual(reply['jobs'][0]['state'],'interrupted')
        self.assertEqual(len(list(self.dst.iterdir())),1)

    def test_cross_device_flush_failure_keeps_source_and_complete_destination(self):
        from unittest.mock import patch
        import file_jobs
        with tempfile.TemporaryDirectory(dir='/dev/shm',prefix='files-flush-') as external:
            target=Path(external);item=self.src/'item';item.write_text('source content')
            original=file_jobs.sync_directory
            def failed(directory):
                if Path(directory)==target:raise OSError('simulated destination durability failure')
                return original(directory)
            with patch.object(file_jobs,'sync_directory',side_effect=failed):
                result=self.runjob('move',[item],destination=target)
            self.assertEqual(result['state'],'partial')
            self.assertEqual(item.read_text(),'source content')
            self.assertEqual((target/'item').read_text(),'source content')
            self.assertFalse(list(target.glob('.lumen-copy-*')))

    def test_cross_device_source_mutation_discards_staging_and_keeps_source(self):
        with tempfile.TemporaryDirectory(dir='/dev/shm',prefix='files-race-') as external:
            item=self.src/'item';item.write_text('before')
            original=self.jobs.copy
            def changed(ident,src,dst):
                original(ident,src,dst);src.write_text('newer source')
            self.jobs.copy=changed
            result=self.runjob('move',[item],destination=Path(external))
            self.assertEqual(result['state'],'partial')
            self.assertEqual(item.read_text(),'newer source')
            self.assertEqual(list(Path(external).iterdir()),[])

    def test_snapshot_cannot_mutate_operation_history(self):
        item=self.src/'item';item.write_text('content');self.runjob('copy',[item])
        self.jobs.state()['jobs'][0]['errors'].append('injected')
        self.assertEqual(self.jobs.state()['jobs'][0]['errors'],[])

    def test_reject_invalid_request_envelopes_before_mutation(self):
        for request in ([],dict(operation='copy',paths='not-a-list'),dict(operation='copy',requestId='bad id'),dict(operation='copy',requestId=7)):
            with self.assertRaises(ValueError):self.jobs.start(request)
        self.assertEqual(self.jobs.state()['jobs'],[])

if __name__=='__main__':unittest.main()
