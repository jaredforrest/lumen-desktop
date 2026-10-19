"""Sorting/grouping, explicit associations and distinguishable symbolic links."""
import os,sys,tempfile,unittest
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch
sys.path.insert(0,str(Path(__file__).resolve().parents[2]/'res/lumen-desktop/shell'))
import files
class FileViewActions(unittest.TestCase):
    def test_folders_first_is_optional_and_grouping_preserves_sort(self):
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory);(root/'z-folder').mkdir();(root/'b.txt').write_text('b');(root/'a.png').write_text('a');(root/'c.jpg').write_text('c')
            request={'path':directory,'sort':'name'}
            self.assertEqual(files.listing(request)['rows'][0]['name'],'z-folder')
            self.assertEqual(files.listing({**request,'foldersFirst':False})['rows'][0]['name'],'a.png')
            rows=files.listing({**request,'foldersFirst':False,'groupBy':'kind'})['rows']
            self.assertEqual([row['group'] for row in rows],['Documents','Folders','Images','Images'])
            self.assertEqual([row['name'] for row in rows if row['group']=='Images'],['a.png','c.jpg'])
            with self.assertRaises(ValueError):files.listing({**request,'groupBy':'unknown'})
            with self.assertRaises(ValueError):files.listing({**request,'foldersFirst':1})
    def test_real_rename_and_move_do_not_confuse_preference_file_with_gio_factory(self):
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory);source=root/'source.txt';source.write_text('keep');destination=root/'folder';destination.mkdir()
            result=files.handle({'action':'rename','path':str(source),'name':'renamed.txt'})
            renamed=root/'renamed.txt';self.assertEqual(result['selected'],str(renamed));self.assertFalse(source.exists())
            moved=files.handle({'action':'move','path':str(renamed),'destination':str(destination)})
            self.assertEqual(Path(moved['selected']).read_text(),'keep');self.assertFalse(renamed.exists())

    def test_link_is_distinct_and_collision_safe_with_recoverable_broken_result(self):
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory);item=root/'original.txt';item.write_text('source')
            first=Path(files.handle({'action':'make-link','path':str(item)})['selected'])
            second=Path(files.handle({'action':'make-link','path':str(item)})['selected'])
            self.assertTrue(first.is_symlink());self.assertNotEqual(first,second);self.assertEqual(first.read_text(),'source')
            self.assertTrue(next(row for row in files.listing({'path':directory})['rows'] if row['path']==str(first))['link'])
            item.rename(root/'moved.txt')
            with self.assertRaisesRegex(FileNotFoundError,'File Info'):files.handle({'action':'open','path':str(first)})
            first.unlink();self.assertEqual((root/'moved.txt').read_text(),'source')
    def test_default_association_requires_explicit_registered_app(self):
        class App:
            def __init__(self,id):self.id=id;self.defaults=[]
            def get_id(self):return self.id
            def should_show(self):return True
            def get_display_name(self):return self.id
            def set_as_default_for_type(self,mime):self.defaults.append(mime);return True
        first,second=App('first.desktop'),App('second.desktop')
        api=SimpleNamespace(get_all_for_type=lambda mime:[first,second],get_default_for_type=lambda mime,uris:first)
        with patch.object(files,'Gio',SimpleNamespace(AppInfo=api)),patch.object(files.subprocess,'run') as run:
            result=files.handle({'action':'applications','path':'/fixture/file.pdf'})
            self.assertTrue(result['applications'][0]['default']);self.assertFalse(result['applications'][1]['default'])
            files.handle({'action':'set-default','path':'/fixture/file.pdf','application':'second.desktop'})
            self.assertEqual(second.defaults,['application/pdf']);self.assertEqual(first.defaults,[]);run.assert_not_called()
            with self.assertRaises(ValueError):files.handle({'action':'set-default','path':'/fixture/file.pdf','application':'unregistered.desktop'})
if __name__=='__main__':unittest.main()
