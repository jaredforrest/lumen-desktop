"""Real metadata tags, indexed filtering and copy preservation on disposable files."""
import errno,os,sys,tempfile,unittest
from pathlib import Path
from unittest.mock import patch
sys.path.insert(0,str(Path(__file__).resolve().parents[2]/'res/lumen-desktop/shell'))
import file_tags,files
from search import Index
from file_jobs import Jobs
class FileTags(unittest.TestCase):
    def real_tags(self,path,tags):
        try:return file_tags.write(path,tags)
        except ValueError as error:
            cause=error.__cause__
            if isinstance(cause,OSError) and cause.errno in (errno.ENOTSUP,errno.EOPNOTSUPP):self.skipTest('This build filesystem does not support user xattrs; real metadata cases require a supporting filesystem')
            raise
    def test_multiple_tags_survive_rename_and_are_filtered_in_files_and_index(self):
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory);folder=root/'files';folder.mkdir();item=folder/'report.txt';item.write_text('source');other=folder/'other.txt';other.write_text('other')
            self.real_tags(item,['red','blue']);renamed=folder/'renamed.txt';item.rename(renamed)
            self.assertEqual({tag['id'] for tag in file_tags.read(renamed)['tags']},{'red','blue'})
            self.assertEqual([row['path'] for row in files.listing({'path':str(folder),'query':'tag:red tag:blue'})['rows']],[str(renamed)])
            index=Index([folder],root/'index.sqlite')
            try:
                index.scan();self.assertEqual([r['path'] for r in index.search('tag:red tag:blue')['results']],[str(renamed)])
                file_tags.write(renamed,[]);index.scan();self.assertFalse(index.search('tag:red')['results'])
                with self.assertRaises(ValueError):index.search('tag:unknown')
            finally:index.closed.set();index.watcher.close()
    def test_tagged_copy_preserves_metadata_and_failed_destination_keeps_source(self):
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory);item=root/'source';item.write_text('source');target=root/'copy';self.real_tags(item,['green'])
            jobs=Jobs();jobs.jobs['fixture']={'bytes':0};jobs.tokens['fixture']=__import__('threading').Event()
            jobs.copy('fixture',item,target);self.assertEqual(file_tags.read(target)['tags'][0]['id'],'green')
            with patch.object(file_tags.os,'setxattr',side_effect=OSError(errno.ENOTSUP,'fixture unsupported')):
                with self.assertRaisesRegex(ValueError,'source data was kept'):file_tags.copy_metadata(item,root/'another')
            self.assertEqual(item.read_text(),'source')
    def test_copy_metadata_unsupported_destination_is_an_explicit_failure(self):
        with tempfile.TemporaryDirectory() as directory:
            source=Path(directory)/'source';source.write_text('source survives')
            with patch.object(file_tags.os,'getxattr',return_value=b'{"version":1,"tags":["green"]}'),patch.object(file_tags.os,'setxattr',side_effect=OSError(errno.ENOTSUP,'fixture unsupported')):
                with self.assertRaisesRegex(ValueError,'source data was kept'):file_tags.copy_metadata(source,Path(directory)/'target')
            self.assertEqual(source.read_text(),'source survives')
    def test_malformed_metadata_is_disclosed_without_returning_tags(self):
        for raw in (b'{}',b'{"version":true,"tags":["red"]}',b'{"version":1,"tags":["red","red"]}',b'not json'):
            with self.subTest(raw=raw),patch.object(file_tags.os,'getxattr',return_value=raw):
                result=file_tags.read('/fixture');self.assertEqual(result['tags'],[]);self.assertTrue(result['error'])
    def test_unsupported_storage_and_invalid_tags_are_disclosed(self):
        with patch.object(file_tags.os,'getxattr',side_effect=OSError(errno.ENOTSUP,'fixture')):
            result=file_tags.read('/fixture');self.assertFalse(result['supported']);self.assertIn('cannot store',result['error'])
        for tags in ('red',['red','red'],['unknown'],[False]):
            with self.subTest(tags=tags),self.assertRaises(ValueError):file_tags.validate(tags)
    def test_tag_write_binds_to_opened_inode_during_path_substitution(self):
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory);item=root/'item';item.write_text('original')
            self.real_tags(item,['red']);info=item.stat();expected={'device':info.st_dev,'inode':info.st_ino}
            original_open=file_tags.os.open
            def substitute(path,flags,*args,**kwargs):
                descriptor=original_open(path,flags,*args,**kwargs)
                if Path(path)==item:
                    item.rename(root/'old');item.write_text('replacement')
                return descriptor
            with patch.object(file_tags.os,'open',side_effect=substitute):file_tags.write(item,['blue'],expected)
            self.assertEqual(file_tags.read(item)['tags'],[])
            self.assertEqual(file_tags.read(root/'old')['tags'][0]['id'],'blue')
    def test_symbolic_links_are_not_followed_and_replaced_identity_is_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory);item=root/'file';item.write_text('original');link=root/'link';link.symlink_to(item)
            with self.assertRaises(ValueError):file_tags.write(link,['red'])
            info=item.lstat();identity={'device':info.st_dev,'inode':info.st_ino}
            item.rename(root/'old');item.write_text('replacement')
            with self.assertRaisesRegex(ValueError,'changed'):files.handle({'action':'set-tags','path':str(item),'identity':identity,'tags':['red']})
            self.assertEqual(file_tags.read(item)['tags'],[])
if __name__=='__main__':unittest.main()
