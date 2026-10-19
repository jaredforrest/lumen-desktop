"""Opening and removing a Dock file reference must not mutate its resource."""
import importlib.util
from pathlib import Path
import tempfile
import unittest
from unittest.mock import Mock

SOURCE=Path(__file__).resolve().parents[2]/'res/lumen-desktop/runtime/file_alias.py'
spec=importlib.util.spec_from_file_location('file_alias',SOURCE);alias=importlib.util.module_from_spec(spec);spec.loader.exec_module(alias)

class FileAliasTests(unittest.TestCase):
    def setUp(self):
        self.temp=tempfile.TemporaryDirectory();self.addCleanup(self.temp.cleanup)
        self.path=Path(self.temp.name)/'literal # ü $(not-executed).txt';self.path.write_text('untouched')
        self.gio=Mock();self.gio.AppInfo.launch_default_for_uri.return_value=True
    def test_open_uses_literal_file_uri_and_preserves_target(self):
        self.assertEqual(alias.open_alias(str(self.path),self.gio),{'opened':True})
        self.assertEqual(self.gio.AppInfo.launch_default_for_uri.call_args.args[0],self.path.as_uri())
        self.assertEqual(self.path.read_text(),'untouched')
    def test_missing_directory_or_relative_target_never_launches(self):
        for value in ['relative.txt','file:///something',str(self.path.parent),str(self.path.parent/'gone'),'a\0b']:
            with self.subTest(value=value),self.assertRaises(ValueError):alias.open_alias(value,self.gio)
        self.gio.AppInfo.launch_default_for_uri.assert_not_called()
    def test_launch_failure_preserves_original(self):
        self.gio.AppInfo.launch_default_for_uri.return_value=False
        with self.assertRaises(ValueError):alias.open_alias(str(self.path),self.gio)
        self.assertEqual(self.path.read_text(),'untouched')

if __name__=='__main__':unittest.main()
