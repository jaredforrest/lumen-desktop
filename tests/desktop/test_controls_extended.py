import copy,importlib.util,json,os,sys,tempfile,unittest
from pathlib import Path
from unittest.mock import patch
sys.path.insert(0,str(Path(__file__).resolve().parents[2]/'res/lumen-desktop/shell'))
import display_settings as displays
import text_tools
import nix_settings
class ExtendedControls(unittest.TestCase):
 def test_display_validation(self):
  output=dict(name='DP-1',active=True,make='Fixture',model='Monitor',serial='1',modes=[dict(width=1440,height=1000,refresh=60000)])
  valid=dict(name='DP-1',mode='1440x1000@60.000Hz',scale=1.5,x=0,y=0)
  self.assertEqual(displays.validate(valid,[output])['identity'],['DP-1','Fixture','Monitor','1'])
  for changes in [dict(name='missing'),dict(mode='9999x1'),dict(scale=0),dict(x=99999)]:
   with self.assertRaises(ValueError):displays.validate({**valid,**changes},[output])
 def test_expired_display_confirmation(self):
  with tempfile.TemporaryDirectory() as t,patch.dict(os.environ,{'XDG_RUNTIME_DIR':t,'XDG_STATE_HOME':t}):
   pending,saved=displays.paths();displays.atomic(pending,dict(token='expired',deadline=0))
   with self.assertRaises(ValueError):displays.handle(dict(action='confirm',token='expired'))
   self.assertIsNone(saved)
 def test_snippet_storage_and_shortcuts(self):
  values={'snippets':[],'menuShortcuts':{}}
  def set_section(section,value):
   values[section]=copy.deepcopy(value);return copy.deepcopy(values)
  with tempfile.TemporaryDirectory() as t,patch.dict(os.environ,{'XDG_STATE_HOME':t}),patch.object(nix_settings,'read_settings',side_effect=lambda:copy.deepcopy(values)),patch.object(nix_settings,'set_section',side_effect=set_section):
   result=text_tools.handle(dict(action='save',name=';test',text='Literal $HOME `command`\nSecond line'))
   self.assertEqual(result['snippets'][0]['text'],'Literal $HOME `command`\nSecond line')
   text_tools.handle(dict(action='save-shortcut',slot=1,app='Fixture',path='File/Save'))
   self.assertEqual(values['menuShortcuts']['1']['app'],'Fixture')
   self.assertEqual(text_tools.handle(dict(action='delete',name=';test'))['snippets'],[])
if __name__=='__main__':unittest.main()
