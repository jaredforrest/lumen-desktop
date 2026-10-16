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

if __name__=='__main__':unittest.main()
