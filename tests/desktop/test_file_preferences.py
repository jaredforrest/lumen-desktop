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
    def test_sidebar_layout_validates_persists_and_preserves_unrelated_preferences(self):
        with tempfile.TemporaryDirectory() as directory,patch.dict(os.environ,{'XDG_STATE_HOME':directory}):
            files.handle({'action':'save-preferences','preferences':{'view':'list','sidebarWidth':280,'collapsedSidebarGroups':['locations','favourites']}})
            result=files.handle({'action':'preferences'})['preferences']
            self.assertEqual(result['sidebarWidth'],280)
            self.assertEqual(result['collapsedSidebarGroups'],['locations','favourites'])
            source=Path(directory)/'lumen-desktop/session/files.json';original=source.read_bytes()
            for invalid in ({'sidebarWidth':True},{'sidebarWidth':119},{'sidebarWidth':361},{'sidebarWidth':190.5},{'collapsedSidebarGroups':'locations'},{'collapsedSidebarGroups':['other']},{'collapsedSidebarGroups':['locations','locations']},{'collapsedSidebarGroups':[{}]}):
                with self.subTest(invalid=invalid),self.assertRaises(ValueError):files.handle({'action':'save-preferences','preferences':invalid})
                self.assertEqual(source.read_bytes(),original)
            files.handle({'action':'save-preferences','preferences':{'sidebarWidth':120,'collapsedSidebarGroups':[]}})
            self.assertEqual(files.handle({'action':'preferences'})['preferences']['view'],'list')

    def test_validated_merge_persists_and_invalid_edits_leave_bytes_unchanged(self):
        with tempfile.TemporaryDirectory() as directory,patch.dict(os.environ,{'XDG_STATE_HOME':directory}):
            files.handle({'action':'save-preferences','preferences':{'view':'list','descending':True,'hidden':True}})
            files.handle({'action':'save-preferences','preferences':{'sort':'size'}})
            self.assertEqual(files.handle({'action':'preferences'})['preferences'],{'view':'list','descending':True,'hidden':True,'sort':'size'})
            favorites=[{'name':'First','path':'/first'},{'name':'Second','path':'/second'}]
            files.handle({'action':'save-preferences','preferences':{'favorites':favorites,'groupBy':'kind','foldersFirst':False}})
            self.assertEqual(files.handle({'action':'preferences'})['preferences']['favorites'],favorites)
            files.handle({'action':'save-preferences','preferences':{'favorites':list(reversed(favorites))}})
            self.assertEqual(files.handle({'action':'preferences'})['preferences']['favorites'][0]['path'],'/second')
            source=Path(directory)/'lumen-desktop/session/files.json';original=source.read_bytes()
            for invalid in ({'hidden':1},{'hidden':'false'},{'descending':1},{'descending':'false'},{'view':'unknown'},{'favorites':[{'name':'relative','path':'relative'}]},{'favorites':[{'name':'one','path':'/same'},{'name':'two','path':'/same'}]},{'favorites':[{'name':'','path':'/path'}]},{'favorites':[{'name':'name','path':'/path','extra':True}]},{'groupBy':'unknown'},{'foldersFirst':1},{'unexpected':True},[]):
                with self.subTest(invalid=invalid),self.assertRaises(ValueError):
                    files.handle({'action':'save-preferences','preferences':invalid})
                self.assertEqual(source.read_bytes(),original)

if __name__=='__main__':unittest.main()
