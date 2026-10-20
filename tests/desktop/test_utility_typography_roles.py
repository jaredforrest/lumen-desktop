"""No essential literal-size overrides in the three migrated utility sources."""
import re,unittest
from pathlib import Path
SHELL=Path(__file__).resolve().parents[2]/'res/lumen-desktop/shell'
class UtilityTypographyRoles(unittest.TestCase):
 def test_migrated_explicit_fonts_use_shared_roles(self):
  for name in ('FileInfoPanel','TextToolsWindow','RemoteDesktopWindow'):
   with self.subTest(consumer=name):
    source=(SHELL/(name+'.qml')).read_text();self.assertFalse(re.search(r'font\.pixelSize\s*:\s*\d',source))
if __name__=='__main__':unittest.main()
