"""Selected search rows are scoped to an observed file and provider lifetime."""
import ctypes,json,os,sys,tempfile,unittest
from unittest.mock import patch
from pathlib import Path
sys.path.insert(0,str(Path(__file__).resolve().parents[2]/'res/lumen-desktop/shell'))
from search import Index
from preview_snapshots import parse_process_start,process_start
class SelectionTests(unittest.TestCase):
 def test_process_identity_parser_allows_parenthesis_in_comm(self):
  tail=['S']+['0']*18+['123456']+['0']*8
  self.assertEqual(parse_process_start('42 (valid ) command) '+' '.join(tail)),'123456')
 def test_replacement_and_provider_restart_reject_retired_selection(self):
  with tempfile.TemporaryDirectory() as tmp:
   root=Path(tmp)/'Documents';root.mkdir();file=root/'selected.txt';file.write_text('original')
   index=Index([root],Path(tmp)/'index.sqlite');other=None
   try:
    index.scan();row=index.search('selected')['results'][0]
    self.assertEqual(index.selected(row['path'],row['token']),file)
    alias=root/'hardlink.txt';os.link(file,alias)
    with self.assertRaises(ValueError):index.selected(str(alias),index.token(file))
    original=root/'retired';file.rename(original);file.write_text('replacement')
    with self.assertRaises(ValueError):index.selected(row['path'],row['token'])
    index.scan();replacement=index.search('selected')['results'][0]
    self.assertNotEqual(row['token'],replacement['token'])
    other=Index([root],Path(tmp)/'other.sqlite')
    with self.assertRaises(ValueError):other.selected(replacement['path'],replacement['token'])
    with self.assertRaises(ValueError):index.selected(replacement['path'],None)
   finally:
    index.closed.set();index.watcher.close()
    if other:other.closed.set();other.watcher.close()
 def test_preview_revalidates_after_reader_and_limits_snapshots(self):
  with tempfile.TemporaryDirectory() as tmp,patch.dict(os.environ,{'XDG_RUNTIME_DIR':tmp}):
   root=Path(tmp)/'Documents';root.mkdir();file=root/'selected.txt';file.write_text('original')
   index=Index([root],Path(tmp)/'index.sqlite')
   try:
    token=index.token(file)
    def replacement(path):
     file.write_text('replacement')
     return {'type':'text','text':'replacement'}
    with patch('search.shared_preview',replacement),self.assertRaises(ValueError):index.preview(str(file),token)
    source=root/'resource.png';source.write_bytes(b'original resource')
    token=index.token(source);snapshot=index.snapshots.capture(source,token)
    source.write_bytes(b'replaced resource');self.assertEqual(snapshot.read_bytes(),b'original resource')
    self.assertEqual(snapshot.stat().st_mode&0o777,0o400)
    large=root/'large.png'
    with large.open('wb') as stream:stream.truncate(64*1024*1024+1)
    with self.assertRaises(ValueError):index.snapshots.capture(large,index.token(large))
    index.snapshots.preview_bytes=128*1024*1024
    with self.assertRaises(ValueError):index.snapshots.capture(source,index.token(source))
    with self.assertRaises(ValueError):index.selected(str(source),'non-ascii token \u00e9')
   finally:index.closed.set();index.watcher.close()
 def test_snapshot_cleanup_preserves_live_provider_and_rejects_reused_pid(self):
  with tempfile.TemporaryDirectory() as tmp,patch.dict(os.environ,{'XDG_RUNTIME_DIR':tmp}):
   root=Path(tmp)/'Documents';root.mkdir();file=root/'picture.png';file.write_bytes(b'resource')
   base=Path(tmp)/('lumen-preview-snapshots-'+str(os.getuid()));base.mkdir(mode=0o700)
   start=process_start(os.getpid());live=base/(str(os.getpid())+'-'+start+'-live0001');live.mkdir();(live/'leased').write_text('leased')
   retired=base/(str(os.getpid())+'-'+str(int(start)+1)+'-dead0001');retired.mkdir()
   unrelated=base/'unrelated';unrelated.mkdir()
   index=Index([root],Path(tmp)/'index.sqlite')
   try:
    snapshot=index.snapshots.capture(file,index.token(file))
    self.assertTrue(live.exists());self.assertFalse(retired.exists());self.assertTrue(unrelated.exists())
    index.snapshots.close();self.assertFalse(snapshot.exists());self.assertEqual((live/'leased').read_text(),'leased')
   finally:index.closed.set();index.watcher.close();index.snapshots.close()
 def test_passive_nonregular_rejection_does_not_read_open_fifo(self):
  with tempfile.TemporaryDirectory() as tmp,patch.dict(os.environ,{'XDG_RUNTIME_DIR':tmp}):
   root=Path(tmp)/'Documents';root.mkdir();fifo=root/'unsupported';os.mkfifo(fifo)
   libc=ctypes.CDLL(None,use_errno=True);libc.inotify_init1.argtypes=[ctypes.c_int];libc.inotify_init1.restype=ctypes.c_int
   libc.inotify_add_watch.argtypes=[ctypes.c_int,ctypes.c_char_p,ctypes.c_uint32];libc.inotify_add_watch.restype=ctypes.c_int
   events=libc.inotify_init1(os.O_NONBLOCK|os.O_CLOEXEC);self.assertGreaterEqual(events,0)
   self.assertGreaterEqual(libc.inotify_add_watch(events,os.fsencode(fifo),0x20),0)
   index=Index([root],Path(tmp)/'index.sqlite')
   try:
    probe=os.open(fifo,os.O_RDONLY|os.O_NONBLOCK|os.O_NOFOLLOW);os.close(probe)
    baseline=os.read(events,4096);self.assertTrue(baseline,'IN_OPEN baseline did not observe read-open')
    with self.assertRaises(ValueError):index.snapshots.capture(fifo,index.token(fifo))
    with self.assertRaises(BlockingIOError):os.read(events,4096)
    print(json.dumps(dict(passiveFifoBaselineReadOpenObserved=True,unsupportedPreviewReadOpenObserved=False)))
   finally:index.closed.set();index.watcher.close();index.snapshots.close();os.close(events)
if __name__=='__main__':unittest.main()
