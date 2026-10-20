"""Appearance validation and semantic contrast on real shared palettes."""
import importlib.util,json,unittest
from pathlib import Path
ROOT=Path(__file__).resolve().parents[2]
spec=importlib.util.spec_from_file_location('appearance_preferences',ROOT/'res/lumen-desktop/appearance/appearance.py')
module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)
def luminance(color):
    values=[int(color[i:i+2],16)/255 for i in (1,3,5)]
    values=[v/12.92 if v<=.04045 else ((v+.055)/1.055)**2.4 for v in values]
    return sum(v*w for v,w in zip(values,(.2126,.7152,.0722)))
def ratio(first,second):
    a,b=sorted((luminance(first),luminance(second)))
    return (b+.05)/(a+.05)
class AppearancePreferences(unittest.TestCase):
    def test_previous_compositor_never_receives_unsupported_zoom_command(self):
        preferences=dict(tileMargins=True,titlebarDoubleClick='zoom')
        self.assertEqual(module.placement_commands(preferences,False), ['lumen_window margins enable'])
        self.assertEqual(module.placement_commands(preferences,True), ['lumen_window margins enable','lumen_window titlebar-action zoom'])
        preferences['titlebarDoubleClick']='minimize'
        self.assertEqual(module.placement_commands(preferences,False)[1], 'lumen_window titlebar-action minimize')

    def test_fullscreen_menu_choices(self):
        self.assertEqual(module.MENU_DEFAULTS['visibility'],'desktop')
        for value in ('always','desktop','fullscreen','auto_hide'):self.assertTrue(module.valid_menu('visibility',value))
        for value in ('automatic','always'):self.assertTrue(module.valid_menu('fullscreenVisibility',value))
        for value in ('hidden','never',True,None,1):self.assertFalse(module.valid_menu('fullscreenVisibility',value))
    def test_text_scale_is_closed_and_rejects_booleans(self):
        for value in (1,1.15,1.3,1.5):self.assertTrue(module.valid('textScale',value))
        for value in (True,False,0,2,'1.5',1.25,None):self.assertFalse(module.valid('textScale',value))
    def test_minimized_destination_is_typed_and_default_preserving(self):
        self.assertEqual(module.DEFAULTS['dockMinimizeDestination'],'application')
        for value in ('application','windows'):self.assertTrue(module.valid('dockMinimizeDestination',value))
        for value in (True,None,'window','',[],{}):self.assertFalse(module.valid('dockMinimizeDestination',value))

    def test_closed_preferences(self):
        for key,value in module.DEFAULTS.items():self.assertTrue(module.valid(key,value),(key,value))
        for key,value in [('increaseContrast',1),('scrollbars','never'),('sidebarIconSize','huge'),('dockPins',['a','a']),('dockPins',['../a']),('dockPins',[False]),('dockPins','a'),('dockPins',['x'*257])]:
            self.assertFalse(module.valid(key,value),(key,value))
        self.assertTrue(module.valid('dockPins',['brave-browser','org.gnome.Evince.desktop']))
        self.assertTrue(module.valid('scrollbars','when_scrolling'))
    def test_desktop_and_file_alias_preferences_are_closed(self):
        self.assertTrue(module.valid_desktop('showFiles',True));self.assertFalse(module.valid_desktop('showFiles',1))
        self.assertTrue(module.valid_desktop('fileSort','name'));self.assertFalse(module.valid_desktop('fileSort','unknown'))
        self.assertTrue(module.valid('dockFiles',['/fixture/report.pdf']))
        for value in (['relative'],['/same','/same'],['/control\tpath'],[False]):self.assertFalse(module.valid('dockFiles',value))
    def test_windows_and_folder_stack_preferences_are_closed(self):
        self.assertTrue(module.valid_windows('tileMargins',False));self.assertTrue(module.valid_windows('titlebarDoubleClick','minimize'))
        self.assertFalse(module.valid_windows('tileMargins',1));self.assertTrue(module.valid_windows('titlebarDoubleClick','zoom'));self.assertFalse(module.valid_windows('titlebarDoubleClick','invented'))
        for key in ('edgeTiling','optionTiling','topEdgeFill','topEdgeOverview'):
            self.assertTrue(module.valid_windows(key,False));self.assertFalse(module.valid_windows(key,1))
        for value in (0,8,32):self.assertTrue(module.valid_windows('tileGap',value))
        for value in (-1,33,True,8.5,'8'):self.assertFalse(module.valid_windows('tileGap',value))
        self.assertTrue(module.valid('dockFolders',[{'path':'/folder','sort':'name','descending':False,'presentation':'grid'}]))
        for value in ([{'path':'relative'}],[{'path':'/same'},{'path':'/same'}],[{'path':'/tab\tpath'}],[{'path':'/folder','unknown':True}]):self.assertFalse(module.valid('dockFolders',value))
    def test_control_layout_requires_complete_order_and_unique_known_ids(self):
        self.assertTrue(module.valid_control('order',list(reversed(module.CONTROL_IDS))))
        self.assertTrue(module.valid_control('hidden',['wifi','media']))
        for key,value in [('order',['wifi']),('hidden',['unknown']),('hidden',['wifi','wifi']),('order',list(module.CONTROL_IDS)+['wifi']),('hidden',[False]),('other',[])]:
            self.assertFalse(module.valid_control(key,value),(key,value))
    def test_search_provider_ids_are_closed_and_unique(self):
        self.assertTrue(module.valid_search('disabledProviders',['files','clipboard']))
        for value in ('files',['files','files'],['remote'],[False],{}):self.assertFalse(module.valid_search('disabledProviders',value))
        self.assertFalse(module.valid_search('unknown',[]))
    def test_increased_contrast_strengthens_text_and_boundaries(self):
        palettes=json.loads((ROOT/'res/lumen-desktop/appearance/palette.json').read_text())
        original=json.loads(json.dumps(palettes))
        for mode in ('light','dark'):
            normal=module.resolve_palette(palettes,mode,module.DEFAULTS)
            high=module.resolve_palette(palettes,mode,{**module.DEFAULTS,'increaseContrast':True})
            for foreground in ('text','muted','border'):
                for background in ('background','surface','field'):
                    self.assertGreaterEqual(ratio(high[foreground],high[background]),ratio(normal[foreground],normal[background]))
                    self.assertGreaterEqual(ratio(high[foreground],high[background]),3 if foreground=='border' else 4.5)
            self.assertEqual(high['accent'],normal['accent'])
        self.assertEqual(palettes,original)
if __name__=='__main__':unittest.main()
