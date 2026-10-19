"""Backwards-compatible whole-stack arrangement persistence."""
import os,sys,tempfile,unittest
from pathlib import Path
from unittest.mock import patch
sys.path.insert(0,str(Path(__file__).resolve().parents[2]/'res/lumen-desktop/shell'))
import desktop_layout
class StackPositions(unittest.TestCase):
    def test_stack_position_survives_group_switch_and_directory_rename(self):
        with tempfile.TemporaryDirectory() as temporary,patch.dict(os.environ,{'XDG_STATE_HOME':temporary}):
            folder=Path(temporary)/'Desktop';folder.mkdir();request={'directory':str(folder),'items':['1:2']}
            value={'grouping':'kind','positions':{'1:2':[.2,.8]},'stacks':{'stack:kind:Documents':[.5,.4]}}
            desktop_layout.handle(dict(request,action='save',layout=value));restored=desktop_layout.handle(dict(request,action='load'))['layout'];self.assertEqual(restored,value)
            restored['grouping']='none';desktop_layout.handle(dict(request,action='save',layout=restored));folder.rename(Path(temporary)/'Moved');request['directory']=str(Path(temporary)/'Moved')
            self.assertEqual(desktop_layout.handle(dict(request,action='load'))['layout']['stacks'],value['stacks'])
    def test_stack_positions_validate_scope_and_points(self):
        for key in ['stack:kind:../outside','stack:unknown:Documents','stack:kind:'+'a'*200,'1:2']:
            with self.assertRaises(ValueError):desktop_layout.validate({'grouping':'kind','positions':{},'stacks':{key:[0,0]}})
        with self.assertRaises(ValueError):desktop_layout.validate({'grouping':'kind','positions':{},'stacks':{'stack:kind:Documents':[2,0]}})
    def test_existing_two_field_layout_remains_valid(self):
        value=desktop_layout.validate({'grouping':'none','positions':{}});self.assertEqual(value.get('stacks',{}),{})
if __name__=='__main__':unittest.main()
