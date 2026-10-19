"""Desktop directory resolution does not create or mutate the user's folder."""
import os,sys,tempfile,unittest
from pathlib import Path
from unittest.mock import patch
sys.path.insert(0,str(Path(__file__).resolve().parents[2]/'res/lumen-desktop/shell'))
import desktop_files
class DesktopFilesLocation(unittest.TestCase):
    def test_configured_directory_is_used_without_creating_missing_folder(self):
        with tempfile.TemporaryDirectory() as directory:
            missing=Path(directory)/'custom desktop'
            with patch.object(desktop_files.GLib,'get_user_special_dir',return_value=str(missing)):
                result=desktop_files.directory();self.assertEqual(result['path'],str(missing));self.assertFalse(result['exists']);self.assertFalse(missing.exists())
            missing.mkdir()
            with patch.object(desktop_files.GLib,'get_user_special_dir',return_value=str(missing)):
                self.assertTrue(desktop_files.directory()['exists'])
    def test_user_folder_disabled_to_home_does_not_expose_home_as_desktop(self):
        with patch.object(desktop_files.GLib,'get_user_special_dir',return_value='/fixture/home'),patch.object(desktop_files.Path,'home',return_value=Path('/fixture/home')):
            result=desktop_files.directory();self.assertTrue(result['disabled']);self.assertFalse(result['exists']);self.assertFalse(result['writable'])
    def test_fallback_is_home_desktop_and_relative_config_rejected(self):
        with patch.object(desktop_files.GLib,'get_user_special_dir',return_value=None),patch.object(desktop_files.Path,'home',return_value=Path('/fixture/home')):
            self.assertEqual(desktop_files.directory()['path'],'/fixture/home/Desktop')
        with patch.object(desktop_files.GLib,'get_user_special_dir',return_value='relative'):
            with self.assertRaises(ValueError):desktop_files.directory()
if __name__=='__main__':unittest.main()
