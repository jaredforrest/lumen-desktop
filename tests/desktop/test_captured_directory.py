"""Actual directory descriptors prevent restored-path retarget during listing."""
import contextlib,os,sys,tempfile,unittest
from pathlib import Path
from unittest.mock import patch
sys.path.insert(0,str(Path(__file__).resolve().parents[2]/'res/lumen-desktop/shell'))
import captured_directory as directory
from captured_resource_actions import facts,wire_facts,ResourceUnavailable

class CapturedDirectory(unittest.TestCase):
 def setUp(self):
  self.temp=tempfile.TemporaryDirectory();self.addCleanup(self.temp.cleanup);self.root=Path(self.temp.name);self.folder=self.root/'selected';self.folder.mkdir();(self.folder/'captured.txt').write_text('captured');self.expected=wire_facts(facts(self.folder))
 def test_anchored_same_path_retarget_and_restore(self):
  import files
  left=self.root/'left';right=self.root/'right';left.mkdir();right.mkdir();self.folder.rename(left/'selected');self.folder=left/'selected';(right/'selected').mkdir();(right/'selected/replacement.txt').write_text('replacement');expected=wire_facts(facts(self.folder));real=os.scandir;seen=[]
  @contextlib.contextmanager
  def swapped(value):
   if isinstance(value,int):
    seen.append(value);left.rename(self.root/'retired');right.rename(left)
    try:
     with real(value) as rows:yield rows
    finally:left.rename(right);(self.root/'retired').rename(left)
   else:
    with real(value) as rows:yield rows
  with patch.object(directory.os,'scandir',swapped):result=files.listing(dict(path=str(self.folder),expectedFacts=expected))
  self.assertTrue(seen);self.assertEqual([row['name'] for row in result['rows']],['captured.txt']);self.assertEqual(result['rows'][0]['path'],str(self.folder/'captured.txt'));self.assertEqual(wire_facts(facts(self.folder)),expected)
 def test_replacement_before_capture_refuses(self):
  self.folder.rename(self.root/'retired');self.folder.mkdir()
  with self.assertRaises(ResourceUnavailable):
   with directory.scan(self.folder,self.expected):self.fail('Replacement opened')
 def test_mutation_during_scan_refuses_commit_and_reaps_descriptors(self):
  before=set(os.listdir('/proc/self/fd'))
  with self.assertRaises(ResourceUnavailable):
   with directory.scan(self.folder,self.expected) as (entries,paths):list(entries);(self.folder/'new.txt').write_text('new')
  self.assertEqual(set(os.listdir('/proc/self/fd')),before)
 def test_directory_symbolic_link_refuses_without_read_open(self):
  link=self.root/'link';link.symlink_to(self.folder,target_is_directory=True);calls=[];real=os.open
  def observe(path,flags,*args,**kwargs):calls.append(flags);return real(path,flags,*args,**kwargs)
  with patch.object(directory.os,'open',observe),self.assertRaises(ResourceUnavailable):
   with directory.scan(link,wire_facts(facts(link))):self.fail('Symbolic link accepted')
  self.assertEqual(len(calls),1);self.assertTrue(calls[0]&os.O_PATH)
 def test_malformed_expected_never_opens_and_plain_listing_unchanged(self):
  with patch.object(directory.os,'open',side_effect=AssertionError('Opened malformed selection')),self.assertRaises(ResourceUnavailable):
   with directory.scan(self.folder,['bad']):pass
  with directory.scan(self.folder) as (entries,paths):self.assertEqual([paths(row)[0] for row in entries],[str(self.folder/'captured.txt')])

if __name__=='__main__':unittest.main()
