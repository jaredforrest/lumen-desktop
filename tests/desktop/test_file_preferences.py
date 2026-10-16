"""Validate file-view preference writes against disposable user state."""
import importlib.util
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

ROOT=Path(__file__).resolve().parents[2]
spec=importlib.util.spec_from_file_location('file_preferences',ROOT/'res/lumen-desktop/shell/files.py')
files=importlib.util.module_from_spec(spec);spec.loader.exec_module(files)

class FilePreferences(unittest.TestCase):
    def test_validated_merge_persists_and_invalid_edits_leave_bytes_unchanged(self):
        with tempfile.TemporaryDirectory() as directory,patch.dict(os.environ,{'XDG_STATE_HOME':directory}):
            files.handle({'action':'save-preferences','preferences':{'view':'list','descending':True}})
            files.handle({'action':'save-preferences','preferences':{'sort':'size'}})
            self.assertEqual(files.handle({'action':'preferences'})['preferences'],{'view':'list','descending':True,'sort':'size'})
            source=Path(directory)/'lumen-desktop/session/files.json';original=source.read_bytes()
            for invalid in ({'descending':1},{'descending':'false'},{'view':'unknown'},{'unexpected':True},[]):
                with self.subTest(invalid=invalid),self.assertRaises(ValueError):
                    files.handle({'action':'save-preferences','preferences':invalid})
                self.assertEqual(source.read_bytes(),original)

if __name__=='__main__':unittest.main()
