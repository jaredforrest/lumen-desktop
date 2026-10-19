"""Decorative preferences cannot set host lock, power or inhibitor policy."""
import json,os,sys,tempfile,unittest
from pathlib import Path
from unittest.mock import patch
ROOT=Path(__file__).resolve().parents[2];sys.path.insert(0,str(ROOT/'res/lumen-desktop/shell'));sys.path.insert(0,str(Path(__file__).parent/'fixtures'))
import screen_saver_settings as saver
from input_ipc import settings_adapter

class ScreenSaverPreferences(unittest.TestCase):
    def test_closed_values_and_independent_persistence(self):
        original=dict(os.environ)
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory);os.environ.update(settings_adapter(root));os.environ['XDG_RUNTIME_DIR']=directory
            try:
                with patch.object(saver,'status',side_effect=lambda:json.loads((root/'settings.json').read_text())):
                    saver.handle(dict(action='set',key='enabled',value=True));saver.handle(dict(action='set',key='idleSeconds',value=15));saver.handle(dict(action='set',key='motion',value=False))
                    self.assertEqual(json.loads((root/'settings.json').read_text())['screenSaver'],dict(enabled=True,idleSeconds=15,motion=False))
                    for key,value in [('enabled',1),('motion','false'),('idleSeconds',True),('idleSeconds',14),('idleSeconds',7201),('locked',False),('ignoreInhibitors',True),('lockDelay',0)]:
                        with self.assertRaises(ValueError):saver.handle(dict(action='set',key=key,value=value))
                    saver.handle(dict(action='set',key='enabled',value=False));self.assertFalse(json.loads((root/'settings.json').read_text())['screenSaver']['enabled'])
                    self.assertEqual(json.loads((root/'settings.json').read_text())['inputs'],{})
            finally:os.environ.clear();os.environ.update(original)
    def test_unknown_lock_status_is_not_unlocked(self):
        with patch.dict(os.environ,{'SWAYSOCK':''}), patch.object(saver,'read_settings',return_value={}):
            state=saver.status();self.assertFalse(state['lock']['known']);self.assertTrue(state['lock']['locked']);self.assertFalse(state['preferences']['enabled'])

if __name__=='__main__':unittest.main()
