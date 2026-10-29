"""Default-size compatibility and explicit owned typography role boundaries."""
import re,unittest
from pathlib import Path
ROOT=Path(__file__).resolve().parents[2]/'res/lumen-desktop/shell'
class TypographyRoles(unittest.TestCase):
 def test_role_bases_preserve_ordinary_sizes_and_all_supported_scales(self):
  theme=(ROOT/'Theme.qml').read_text();match=re.search(r'typography: \(\{([^}]+)\}\)',theme);self.assertIsNotNone(match)
  values=dict((key,int(value))for key,value in re.findall(r'(\w+):(\d+)',match[1]));self.assertEqual(values,dict(caption=10,metadata=11,label=12,body=13,subheading=15,title=16,dialogHeading=17,heading=18,display=29))
  self.assertIn('return textSize(typography[role]',theme)
  self.assertIn('font.pixelSize: Theme.textRole(textRole)',(ROOT/'PanelLabel.qml').read_text())
 def test_owned_labels_do_not_bypass_roles_except_fixed_symbols(self):
  for name in ('FilesWindow.qml','CalendarWindow.qml','ColumnView.qml'):
   with self.subTest(window=name):
    source=(ROOT/name).read_text();source=source.replace('font.pixelSize:10/* fixed mark symbol */','').replace('text:FileLinks.badge(modelData);font.pixelSize:16','text:FileLinks.badge(modelData);')
    self.assertFalse(re.search(r'font.pixelSize:\s*\d',source),'Ordinary text must use a named typography role')
 def test_calendar_content_scrolls_instead_of_shrinking_text_to_fit(self):
  source=(ROOT/'CalendarWindow.qml').read_text();self.assertIn('contentWidth:main.width;contentHeight:main.height',source);self.assertIn('Math.max(calendarViewport.availableWidth,(root.view==="Day"?160:420)*Theme.textScale)',source);self.assertIn('onActiveFocusItemChanged()',source)
if __name__=='__main__':unittest.main()
