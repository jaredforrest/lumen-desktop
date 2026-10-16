"""Real private profile files; no user data or authentication touched."""
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

ROOT=Path(__file__).resolve().parents[2]
spec=importlib.util.spec_from_file_location('bridge',ROOT/'res/lumen-desktop/profiles/profile_bridge.py')
module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)

class ProfileTests(unittest.TestCase):
    def setUp(self):
        self.tmp=tempfile.TemporaryDirectory();self.root=Path(self.tmp.name)
        self.policy={'ui.json':{'theme':'light','language':'en'}}
        self.fields={'ui.json':{'recent':[],'window':None}}
        self.bridge=module.Bridge('fixture',self.policy,self.fields,self.root/'profile',self.root/'state',self.root/'cache')
    def tearDown(self):self.tmp.cleanup()
    def test_original_profile_separates_state_and_config(self):
        module.atomic(self.bridge.profile/'ui.json',{'theme':'light','language':'en','recent':['my.docx'],'window':{'x':20}})
        self.bridge.prepare()
        self.assertEqual(module.load(self.bridge.state/'ui.json'),{'recent':['my.docx'],'window':{'x':20}})
        self.assertTrue((self.bridge.profile/'ui.json').is_symlink())
        self.assertEqual(module.load(self.bridge.cache/'ui.json')['theme'],'light')
    def test_clearing_cache_retains_state_and_recreates_policy(self):
        self.bridge.prepare()
        module.atomic(self.bridge.state/'ui.json',{'recent':['saved.docx'],'window':{'width':1200}})
        (self.bridge.cache/'ui.json').unlink()
        self.bridge.prepare()
        rebuilt=module.load(self.bridge.cache/'ui.json')
        self.assertEqual(rebuilt,{'theme':'light','language':'en','recent':['saved.docx'],'window':{'width':1200}})
    def test_native_config_change_is_proposal_not_state(self):
        self.bridge.prepare()
        module.atomic(self.bridge.cache/'ui.json',{'theme':'dark','language':'en','recent':['new.docx'],'window':None})
        proposed=self.bridge.collect()
        self.assertEqual(proposed['ui.json']['theme'],'dark')
        self.assertNotIn('theme',module.load(self.bridge.state/'ui.json'))
        self.bridge.project(self.policy)
        self.assertEqual(module.load(self.bridge.cache/'ui.json')['theme'],'light')
        self.assertEqual(module.load(self.bridge.cache/'ui.json')['recent'],['new.docx'])
    def test_unknown_fields_are_preserved_without_public_export(self):
        original={'theme':'dark','token':'private-fixture'}
        module.atomic(self.bridge.profile/'ui.json',original)
        with self.assertRaisesRegex(ValueError,'Unclassified'):self.bridge.prepare()
        self.assertEqual(module.load(self.bridge.profile/'ui.json'),original)
        self.assertFalse((self.bridge.profile/'ui.json').is_symlink())
    def test_personal_dictionary_is_state(self):
        policy={'autocomplete':True};fields={'userDictionary':[]}
        proposal,state=module.partition({'autocomplete':False,'userDictionary':['Jalio']},policy,fields)
        self.assertEqual(proposal,{'autocomplete':False});self.assertEqual(state,{'userDictionary':['Jalio']})

class NestedFieldTests(unittest.TestCase):
    def test_view_filters_are_state_and_tracking_policy_is_config(self):
        policy={'editing':{'markup':{'trackFormatting':True}}}
        fields={'editing':{'markup':{'comments':True,'ink':True}}}
        value={'editing':{'markup':{'trackFormatting':True,'comments':False,'ink':True}}}
        proposal,state=module.partition(value,policy,fields)
        self.assertEqual(proposal,policy)
        self.assertEqual(state,{'editing':{'markup':{'comments':False,'ink':True}}})
        self.assertEqual(module.merge(state,policy),value)
        with self.assertRaises(ValueError):module.partition(proposal,{},fields)

class PrismTests(unittest.TestCase):
    def test_credentials_are_state_and_detection_is_cache(self):
        with tempfile.TemporaryDirectory() as folder:
            root=Path(folder)
            policy={'prismlauncher.cfg':{'ApplicationTheme':'system','MaxMemAlloc':'8192'}}
            state={'prismlauncher.cfg':{'ProxyPass':'','SelectedInstance':''}}
            cached={'prismlauncher.cfg':{'JavaSignature':''}}
            bridge=module.Bridge('prism',policy,state,root/'profile',root/'state',root/'cache',cached)
            bridge.write_native(bridge.profile/'prismlauncher.cfg',{'ApplicationTheme':'system','MaxMemAlloc':'8192',
                                'ProxyPass':'private-fixture','SelectedInstance':'My game','JavaSignature':'probe-result'})
            bridge.prepare()
            saved=module.load(bridge.state/'prismlauncher.cfg')
            self.assertEqual(saved,{'ProxyPass':'private-fixture','SelectedInstance':'My game'})
            self.assertNotIn('JavaSignature',saved)
            self.assertNotIn('ProxyPass',policy['prismlauncher.cfg'])
            (bridge.cache/'prismlauncher.cfg').unlink()
            (bridge.cache/'prismlauncher.cfg.derived').unlink()
            bridge.prepare()
            value=bridge.read_native(bridge.cache/'prismlauncher.cfg')
            self.assertEqual(value['ProxyPass'],'private-fixture')
            self.assertEqual(value['SelectedInstance'],'My game')
            self.assertEqual(value['JavaSignature'],'')
            self.assertEqual(value['MaxMemAlloc'],'8192')
    def test_config_changes_are_proposals_and_unknown_data_survives(self):
        with tempfile.TemporaryDirectory() as folder:
            root=Path(folder);policy={'prismlauncher.cfg':{'ApplicationTheme':'system'}}
            bridge=module.Bridge('prism',policy,{'prismlauncher.cfg':{'ProxyPass':''}},root/'profile',root/'state',root/'cache')
            bridge.prepare()
            bridge.write_native(bridge.cache/'prismlauncher.cfg',{'ApplicationTheme':'dark','ProxyPass':'secret-fixture'})
            self.assertEqual(bridge.collect(),{'prismlauncher.cfg':{'ApplicationTheme':'dark'}})
            bridge.project(policy)
            self.assertEqual(bridge.read_native(bridge.cache/'prismlauncher.cfg')['ProxyPass'],'secret-fixture')
            bridge.write_native(bridge.cache/'prismlauncher.cfg',{'ApplicationTheme':'dark','ProxyPass':'updated-private','NewField':'unclassified'})
            with self.assertRaises(ValueError):bridge.collect()
            self.assertEqual(module.load(bridge.state/'prismlauncher.cfg')['ProxyPass'],'updated-private')
            self.assertEqual(module.load(bridge.state/'unclassified-profile-draft.json')['native']['NewField'],'unclassified')

if __name__=='__main__':unittest.main()
