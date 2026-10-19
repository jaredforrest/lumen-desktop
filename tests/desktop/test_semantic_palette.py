"""Semantic text/selection/focus contrast, independent of any reference colours."""
import importlib.util
import json
from pathlib import Path
import unittest

ROOT=Path(__file__).resolve().parents[2]
spec=importlib.util.spec_from_file_location('semantic_appearance',ROOT/'res/lumen-desktop/appearance/appearance.py')
appearance=importlib.util.module_from_spec(spec);spec.loader.exec_module(appearance)

def luminance(color):
    rgb=[int(color[i:i+2],16)/255 for i in (1,3,5)]
    linear=[value/12.92 if value<=.04045 else ((value+.055)/1.055)**2.4 for value in rgb]
    return sum(value*weight for value,weight in zip(linear,(.2126,.7152,.0722)))

def ratio(first,second):
    low,high=sorted((luminance(first),luminance(second)))
    return (high+.05)/(low+.05)

class SemanticPalette(unittest.TestCase):
    def test_all_accents_and_modes_resolve_readable_text_selection_and_focus(self):
        palettes=json.loads((ROOT/'res/lumen-desktop/appearance/palette.json').read_text())
        original=json.loads(json.dumps(palettes))
        for mode in ('light','dark'):
            for accent in appearance.ACCENTS:
                for increased in (False,True):
                    with self.subTest(mode=mode,accent=accent,increased=increased):
                        colors=appearance.resolve_palette(palettes,mode,{**appearance.DEFAULTS,'accent':accent,'increaseContrast':increased})
                        for foreground in ('text','muted'):
                            for surface in ('background','surface','field','hover'):
                                self.assertGreaterEqual(ratio(colors[foreground],colors[surface]),4.5,(foreground,surface,colors))
                        self.assertGreaterEqual(ratio(colors['selectedText'],colors['accent']),4.5)
                        for surface in ('background','surface','field'):
                            self.assertGreaterEqual(ratio(colors['accent'],colors[surface]),3,('focus indicator',surface,colors))
                            if increased:self.assertGreaterEqual(ratio(colors['border'],colors[surface]),3)
        self.assertEqual(palettes,original,'Palette resolution must not mutate source input')
    def test_selected_foreground_preserves_legible_choice_and_repairs_bad_tint_pair(self):
        palettes=json.loads((ROOT/'res/lumen-desktop/appearance/palette.json').read_text())
        palettes['light']['selectedText']='#ffffff'
        good=appearance.resolve_palette(palettes,'light',{**appearance.DEFAULTS,'accent':'red'})
        bad=appearance.resolve_palette(palettes,'light',{**appearance.DEFAULTS,'accent':'blue'})
        self.assertEqual(good['selectedText'],'#ffffff')
        self.assertEqual(bad['selectedText'],'#000000')
    def test_status_meaning_has_textual_semantics_not_colour_only(self):
        text=(ROOT/'res/lumen-desktop/shell/StatusMessage.qml').read_text()
        self.assertIn('Accessible.description: severity',text)
        self.assertIn('"Error: "',text)
        self.assertIn('"Success: "',text)

if __name__=='__main__':unittest.main()
