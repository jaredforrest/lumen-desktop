import importlib.util,os,sys,tempfile,unittest
from pathlib import Path
from unittest.mock import patch
source=Path(__file__).resolve().parents[2]/'res/lumen-desktop/shell/system-controls.py'
sys.path.insert(0,str(source.parent));spec=importlib.util.spec_from_file_location('system_controls',source);controls=importlib.util.module_from_spec(spec);spec.loader.exec_module(controls)
class DdcCacheTests(unittest.TestCase):
 def call(self,*args):
  with patch.object(sys,'argv',['controls',*args]):return controls.main()
 def test_discovery_read_cache_and_write_invalidation(self):
  with tempfile.TemporaryDirectory() as t,patch.dict(os.environ,{'XDG_RUNTIME_DIR':t,'SWAYSOCK':''}),patch.object(controls.time,'clock_gettime',return_value=500):
   with patch.object(controls,'command',side_effect=['Display 1\n I2C bus: /dev/i2c-9\n Monitor: Test\n','VCP 10 C 30 80']) as cmd:
    self.assertEqual(self.call('displays')['displays'][0]['level'],.375);self.assertEqual(cmd.call_count,2)
    self.assertEqual(self.call('displays')['displays'][0]['level'],.375);self.assertEqual(cmd.call_count,2)
   with patch.object(controls,'command',return_value='') as cmd:self.call('brightness','9','40');cmd.assert_called_once_with(['ddcutil','--bus','9','setvcp','10','40'])
   with patch.object(controls,'command',return_value='VCP 10 C 40 80') as cmd:
    self.assertEqual(self.call('displays')['displays'][0]['level'],.5);cmd.assert_called_once_with(['ddcutil','--bus','9','getvcp','10','--brief'])
 def test_bad_cache_and_unavailable_control(self):
  with tempfile.TemporaryDirectory() as t,patch.dict(os.environ,{'XDG_RUNTIME_DIR':t,'SWAYSOCK':''}):
   (Path(t)/('lumen-ddc-'+str(os.getuid())+'.json')).write_text('[]')
   with patch.object(controls,'command',side_effect=['Display 1\n I2C bus: /dev/i2c-9\n','VCP 10 ERR']):self.assertEqual(self.call('displays')['displays'],[])
if __name__=='__main__':unittest.main()
