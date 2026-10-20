"""Integration checks against the real Nix evaluator and isolated settings files."""
import importlib.util
import json
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location('nix_settings', ROOT/'res/lumen-desktop/shell/nix_settings.py')
settings = importlib.util.module_from_spec(spec)
spec.loader.exec_module(settings)


class NixSettingsTest(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.root = Path(self.directory.name)
        self.source = self.root/'desktop.nix'
        self.source.write_text('{}\n')
        self.environment = dict(os.environ)
        os.environ['XDG_RUNTIME_DIR'] = str(self.root/'runtime')
        os.environ['LUMEN_NIX_SETTINGS_SOURCE'] = str(self.source)

    def tearDown(self):
        os.environ.clear(); os.environ.update(self.environment)
        self.directory.cleanup()

    def test_display_space_separation_and_primary_identity(self):
        self.assertEqual(settings.transaction()['spaces'],dict(separateDisplays=True,primaryDisplay=None))
        value=settings.transaction('spaces',dict(separateDisplays=False,primaryDisplay=dict(kind='connector',name='SECONDARY')),merge=True)['spaces']
        self.assertFalse(value['separateDisplays']);self.assertEqual(value['primaryDisplay']['name'],'SECONDARY')
        self.assertIsNone(value['primaryDisplay']['serial'])
        before=self.source.read_bytes()
        for invalid in (dict(separateDisplays=1),dict(primaryDisplay='SECONDARY'),dict(primaryDisplay=dict(kind='connector',name='line\nbreak')),dict(primaryDisplay=dict(kind='display',make='A',model='B',serial='Unknown'))):
            with self.assertRaises(ValueError):settings.transaction('spaces',invalid,merge=True)
            self.assertEqual(self.source.read_bytes(),before)

    def test_menu_bar_typed_presentation(self):
        values=settings.transaction('menuBar', {'hourCycle':'24','locale':'fr_FR','showSeconds':True,'fullscreenVisibility':'always'},merge=True)
        self.assertEqual(values['menuBar']['hourCycle'],'24')
        self.assertEqual(values['menuBar']['visibility'],'always')
        self.assertEqual(values['menuBar']['order'],['battery','sound','focus','search','controls'])
        for invalid in ({'fullscreenVisibility':'hidden'},{'fullscreenVisibility':True},{'hourCycle':'25'},{'locale':'../bad'},{'showSeconds':1},{'order':['sound','sound']}):
            with self.assertRaises(ValueError):settings.transaction('menuBar',invalid,merge=True)
        self.assertEqual(settings.transaction('menuBar',{'showDate':False},merge=True)['menuBar']['locale'],'fr_FR')

    def test_menu_visibility_migration_has_one_authority(self):
        for legacy,canonical in [('automatic','desktop'),('always','always')]:
            self.source.write_text(settings.serialize({'menuBar':{'fullscreenVisibility':legacy,'locale':'fr_FR'}}))
            result=settings.transaction()['menuBar']
            self.assertEqual(result['visibility'],canonical);self.assertNotIn('fullscreenVisibility',result)
            result=settings.transaction('menuBar',{'showSeconds':True},merge=True)['menuBar']
            self.assertEqual(result['visibility'],canonical);self.assertEqual(result['locale'],'fr_FR')
            self.assertNotIn('fullscreenVisibility',self.source.read_text())
        for mode in ('always','desktop','fullscreen','auto_hide'):
            self.assertEqual(settings.transaction('menuBar',{'visibility':mode},merge=True)['menuBar']['visibility'],mode)
        original=self.source.read_bytes()
        for invalid in ({'visibility':'automatic'},{'visibility':True},{'visibility':'desktop','fullscreenVisibility':'automatic'}):
            with self.assertRaises(ValueError):settings.transaction('menuBar',invalid,merge=True)
            self.assertEqual(self.source.read_bytes(),original)
        self.source.write_text(settings.serialize({'menuBar':{'visibility':'desktop','fullscreenVisibility':'automatic'}}))
        with self.assertRaises(ValueError):settings.transaction()

    def test_text_scale_is_typed_and_persistent(self):
        values=settings.transaction('appearance',{'textScale':1.5},merge=True)
        self.assertEqual(values['appearance']['textScale'],1.5)
        for invalid in (True,2,1.25,'1.5'):
            with self.assertRaises(ValueError):settings.transaction('appearance',{'textScale':invalid},merge=True)
        self.assertEqual(settings.transaction('appearance',{'accent':'green'},merge=True)['appearance']['textScale'],1.5)

    def test_screen_saver_cannot_override_lock_policy(self):
        result=settings.transaction('screenSaver',{'enabled':True,'idleSeconds':15,'motion':False})
        self.assertEqual(result['screenSaver'],{'enabled':True,'idleSeconds':15,'motion':False})
        for invalid in ({'enabled':1},{'idleSeconds':True},{'idleSeconds':14},{'idleSeconds':7201},{'motion':'off'},{'lockDelay':0},{'ignoreInhibitors':True}):
            with self.assertRaises(ValueError):settings.transaction('screenSaver',invalid)
        self.assertFalse(settings.transaction('screenSaver',{})['screenSaver']['enabled'])

    def test_wallpaper_references_are_typed(self):
        import hashlib
        identity='connector:Fixture-1';key=hashlib.sha256(identity.encode()).hexdigest()
        value={'defaultImage':'/temporary image.png','displays':{key:{'identity':identity,'image':'/other.jpg'}}}
        self.assertEqual(settings.transaction('wallpaper',value)['wallpaper'],value)
        for invalid in ({'defaultImage':'https://example.invalid/image.png'}, {'defaultImage':'relative.png'}, {'defaultImage':'/bad\nname'}, {'displays':{'wrong':{'identity':identity,'image':'/x.png'}}}, {'displays':{key:{'identity':identity,'image':'/x.png','download':True}}}):
            with self.assertRaises(ValueError):settings.transaction('wallpaper',invalid)
        self.assertEqual(settings.transaction('wallpaper',{'defaultImage':None,'displays':{}})['wallpaper']['defaultImage'],None)

    def test_device_preferences_have_closed_typed_values(self):
        profile={'identity':{'identifier':'2:2:Fixture_Mouse','name':'Fixture Mouse','type':'pointer'},'values':{'natural_scroll':True,'pointer_accel':0.2},'baseline':{'natural_scroll':False,'pointer_accel':0.0}}
        import hashlib
        key=hashlib.sha256(json.dumps(profile['identity'],sort_keys=True,separators=(',',':')).encode()).hexdigest()
        result=settings.transaction('inputs',{key:profile})
        self.assertIs(result['inputs'][key]['values']['natural_scroll'],True)
        import copy
        for option,value in (('natural_scroll',1),('pointer_accel',2),('scroll_factor',0),('repeat_rate',1.5),('events','disabled')):
            invalid=copy.deepcopy(profile);invalid['values'][option]=value
            with self.assertRaises(ValueError):settings.transaction('inputs',{key:invalid})
        with self.assertRaises(ValueError):settings.transaction('inputs',{'unmatched-key':profile})
        invalid=copy.deepcopy(profile);invalid['baseline']={}
        with self.assertRaises(ValueError):settings.transaction('inputs',{key:invalid})
        invalid=copy.deepcopy(profile);invalid['values']['tap']=True;invalid['baseline']['tap']=False
        with self.assertRaises(ValueError):settings.transaction('inputs',{key:invalid})
        invalid=copy.deepcopy(profile);invalid['identity']['identifier']='type:pointer'
        with self.assertRaises(ValueError):settings.transaction('inputs',{key:invalid})
        self.assertIs(settings.transaction('appearance',{'accent':'green'},merge=True)['inputs'][key]['baseline']['natural_scroll'],False)

    def test_patch_preserves_sibling_preferences(self):
        settings.transaction('appearance', {'accent':'green','mode':'dark'})
        values=settings.transaction('appearance', {'reduceMotion':True,'switchToAppSpace':False},merge=True)
        self.assertEqual(values['appearance']['accent'],'green')
        self.assertEqual(values['appearance']['mode'],'dark')
        self.assertTrue(values['appearance']['reduceMotion'])
        self.assertFalse(values['appearance']['switchToAppSpace'])
        original=self.source.read_bytes()
        with self.assertRaises(ValueError):settings.transaction('appearance',{'unexpected':True},merge=True)
        self.assertEqual(self.source.read_bytes(),original)
        with self.assertRaises(ValueError):settings.transaction('appearance',[],merge=True)

    def test_accessibility_and_dock_preferences_have_closed_types(self):
        values=settings.transaction('appearance', {'increaseContrast':True,'scrollbars':'when_scrolling','sidebarIconSize':'large','dockPins':['brave-browser','org.gnome.Evince'],'dockRecentApps':False,'dockIndicators':False,'dockFolders':[{'path':'/example','sort':'name','descending':False,'presentation':'grid'}],'dockPosition':'left','dockAutoHide':True,'dockSize':32})
        self.assertTrue(values['appearance']['increaseContrast'])
        self.assertEqual(values['appearance']['sidebarIconSize'],'large')
        original=self.source.read_bytes()
        for patch in ({'increaseContrast':1},{'sidebarIconSize':'huge'},{'dockPins':['one','one']},{'dockPins':['../escape']},{'dockPins':['x'*257]},{'dockIndicators':'yes'},{'dockFolders':[{'path':'relative'}]},{'dockFolders':[{'path':'/newline\npath'}]},{'dockFolders':[{'path':'/tab\tpath'}]},{'dockFolders':[{'path':'/same'},{'path':'/same'}]},{'dockPosition':'top'},{'dockSize':31},{'dockAutoHide':1}):
            with self.subTest(patch=patch),self.assertRaises(ValueError):settings.transaction('appearance',patch,merge=True)
            self.assertEqual(self.source.read_bytes(),original)
        self.assertEqual(settings.transaction('appearance',{'dockPins':None},merge=True)['appearance']['dockPins'],None)

    def test_search_provider_preferences_reject_unknown_duplicate_ids(self):
        values=settings.transaction('search',{'disabledProviders':['files','clipboard']})
        self.assertEqual(values['search']['disabledProviders'],['files','clipboard'])
        original=self.source.read_bytes()
        for value in (['unknown'],['files','files'],'files',[False]):
            with self.subTest(value=value),self.assertRaises(ValueError):settings.transaction('search',{'disabledProviders':value},merge=True)
            self.assertEqual(self.source.read_bytes(),original)

    def test_control_layout_and_notification_privacy_are_typed(self):
        order=['wifi','bluetooth','sharing','media','displays','appearance','focus','brightness','sound']
        self.assertEqual(settings.transaction('controlCentre',{'order':list(reversed(order)),'hidden':['media']})['controlCentre']['hidden'],['media'])
        self.assertEqual(settings.transaction('notifications',{'preview':'never'},merge=True)['notifications']['preview'],'never')
        original=self.source.read_bytes()
        for section,value in [('controlCentre',{'order':['wifi']}),('controlCentre',{'hidden':['wifi','wifi']}),('controlCentre',{'hidden':['unknown']}),('notifications',{'preview':'when-unlocked'})]:
            with self.subTest(value=value),self.assertRaises(ValueError):settings.transaction(section,value,merge=True)
            self.assertEqual(self.source.read_bytes(),original)

    def test_desktop_opt_in_and_file_aliases_are_typed(self):
        values=settings.transaction('desktop',{'showFiles':True,'fileSort':'modified','fileDescending':True})
        self.assertTrue(values['desktop']['showFiles'])
        self.assertEqual(settings.transaction('appearance',{'dockFiles':['/fixture/report.pdf']},merge=True)['appearance']['dockFiles'],['/fixture/report.pdf'])
        original=self.source.read_bytes()
        for section,value in [('desktop',{'showFiles':1}),('desktop',{'fileSort':'unknown'}),('appearance',{'dockFiles':['relative']}),('appearance',{'dockFiles':['/same','/same']}),('appearance',{'dockFiles':['/control\tpath']})]:
            with self.subTest(value=value),self.assertRaises(ValueError):settings.transaction(section,value,merge=True)
            self.assertEqual(self.source.read_bytes(),original)

    def test_window_preferences_are_typed(self):
        self.assertEqual(settings.transaction('windows',{'titlebarDoubleClick':'zoom'},merge=True)['windows']['titlebarDoubleClick'],'zoom')
        values=settings.transaction('windows',{'tileMargins':False,'titlebarDoubleClick':'minimize'})
        self.assertEqual(values['windows'],{'tileMargins':False,'titlebarDoubleClick':'minimize','tileGap':8,'edgeTiling':True,'optionTiling':True,'topEdgeFill':True,'topEdgeOverview':True})
        original=self.source.read_bytes()
        for value in ({'tileMargins':1},{'titlebarDoubleClick':'invented'},{'tileGap':-1},{'tileGap':33},{'tileGap':True},{'edgeTiling':1},{'optionTiling':'true'}):
            with self.subTest(value=value),self.assertRaises(ValueError):settings.transaction('windows',value,merge=True)
            self.assertEqual(self.source.read_bytes(),original)

    def test_invalid_value_preserves_source_and_active_bundle(self):
        settings.transaction()
        original = self.source.read_bytes()
        active = (settings.runtime()/'current').resolve()
        with self.assertRaises(ValueError):
            settings.transaction('appearance', {'mode': 'unsupported'})
        self.assertEqual(self.source.read_bytes(), original)
        self.assertEqual((settings.runtime()/'current').resolve(), active)

    def test_success_commits_nix_and_survives_runtime_loss(self):
        settings.transaction('appearance', {'mode': 'dark'})
        self.assertIn('"appearance" =', self.source.read_text())
        (settings.runtime()/'current').unlink()
        values = settings.transaction()
        self.assertEqual(values['appearance']['mode'], 'dark')
        self.assertTrue(str((settings.runtime()/'current').resolve()).startswith('/nix/store/'))

    def test_text_is_literal_not_nix_interpolation(self):
        text = '${builtins.abort "must not evaluate"}\nQuotes: " and \\ and é'
        values = settings.transaction('menuShortcuts', {'1': {'app': text, 'path': 'File/Save'}})
        self.assertEqual(values['menuShortcuts']['1']['app'], text)
        self.assertEqual(settings.transaction()['menuShortcuts']['1']['app'], text)

    def test_application_update_keeps_other_application_config(self):
        values=settings.transaction('applicationPreferences',{'ui.json':{'theme':'dark'}},application='wordcraft')
        self.assertEqual(values['applicationPreferences']['wordcraft']['ui.json']['theme'],'dark')
        self.assertEqual(values['applicationPreferences']['gridcraft']['ui.json']['dark'],False)
        with self.assertRaises(ValueError):
            settings.transaction('applicationPreferences',{'ui.json':{'recent':['private.docx']}},application='wordcraft')

    def test_live_controls_are_not_configuration_sections(self):
        for section in ('audio','brightness','files','dictionaryWords','snippets'):
            with self.assertRaises(ValueError):settings.transaction(section,{})
        with self.assertRaises(ValueError):
            settings.transaction('notifications',{'dnd':True,'policies':{}})

    def test_failed_system_activation_preserves_source(self):
        original=self.source.read_bytes()
        os.environ['LUMEN_NIX_SETTINGS_BUILD_SOURCE']='/unused-test-input'
        with patch.object(settings,'rebuild_system',return_value=Path('/nix/store/test-system')) as rebuild:
            with patch.object(settings.subprocess,'run',wraps=settings.subprocess.run) as run:
                actual=run._mock_wraps
                def execute(command,**kwargs):
                    if command[0]=='pkexec':raise ValueError('Authentication cancelled')
                    return actual(command,**kwargs)
                run.side_effect=execute
                with self.assertRaises(ValueError):settings.transaction('appearance',{'mode':'dark'})
            rebuild.assert_called_once()
        self.assertEqual(self.source.read_bytes(),original)

    def test_display_identity_and_unknown_sections(self):
        display = dict(name='DP-1', identity=['DP-1', 'Maker', 'Model', None],
                       mode='1920x1080@60.000Hz', scale=1.25, x=0, y=0)
        self.assertEqual(settings.transaction('displays', {'DP-1': display})['displays']['DP-1'], display)
        with self.assertRaises(ValueError): settings.transaction('unknown', {})


if __name__ == '__main__': unittest.main()
