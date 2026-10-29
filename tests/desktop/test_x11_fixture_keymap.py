"""Bounded fixture map retains the physical keyboard's full supported US vocabulary."""
import re
import unittest
from pathlib import Path
from application_fixture.keyboard import PLAIN, SHIFTED, SPECIAL

class KeymapContract(unittest.TestCase):
    def setUp(self):
        self.source=(Path(__file__).parent/'fixtures/x11-us.xkb').read_text()
        self.rows={int(code)-8:[part.strip()for part in values.split(',')] for code,values in re.findall(r'key <K(\d+)> \{ \[([^]]+)\]',self.source)}

    def test_all_supported_physical_keys_have_unique_bounded_codes(self):
        declared=re.findall(r'<K(\d+)>=(\d+);',self.source)
        self.assertEqual(len(declared),len({name for name,_ in declared}))
        self.assertTrue(all(int(name)==int(value) and 8<=int(value)<=255 for name,value in declared))
        self.assertTrue(set(PLAIN.values())|set(SPECIAL.values()) <= self.rows.keys())

    def test_letters_digits_and_shifted_number_row(self):
        for char,code in PLAIN.items():
            if char.isalpha():self.assertEqual(self.rows[code],[char,char.upper()])
            elif char.isdigit():self.assertEqual(self.rows[code][0],char)
        for char,name in zip('!@#$%^&*()','exclam at numbersign dollar percent asciicircum ampersand asterisk parenleft parenright'.split()):
            self.assertEqual(self.rows[PLAIN[SHIFTED[char]]][1],name)

    def test_shifted_punctuation_navigation_and_core_modifiers(self):
        names='underscore plus braceleft braceright colon quotedbl asciitilde bar less greater question'.split()
        for char,name in zip('_+{}:"~|<>?',names):self.assertEqual(self.rows[PLAIN[SHIFTED[char]]][1],name)
        for name in ('Left','Right','Up','Down','Home','End','Return','Escape','Tab'):
            self.assertEqual(self.rows[SPECIAL[name]],[name])
        for modifier,keys in {'Shift':(50,62),'Control':(37,105),'Mod1':(64,108),'Mod4':(133,134)}.items():
            self.assertIn('modifier_map '+modifier+' {'+','.join('<K%03d>'%key for key in keys)+'};',self.source)

if __name__=='__main__':unittest.main()
