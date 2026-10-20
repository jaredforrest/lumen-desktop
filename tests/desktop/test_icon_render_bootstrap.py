"""The actual outer entry must launch a hermetic child before any UI work."""
import json,os,subprocess,sys,tempfile,unittest
from pathlib import Path
ROOT=Path(__file__).resolve().parents[2]
class IconBootstrap(unittest.TestCase):
 def test_standalone_outer_entry_sets_private_environment_and_bus(self):
  with tempfile.TemporaryDirectory(prefix='lumen-icon-bootstrap-') as private:
   root=Path(private);binpath=root/'bin';binpath.mkdir();capture=root/'capture.json';bus=binpath/'dbus-run-session'
   bus.write_text('#!'+sys.executable+'\n'+'''import json,os,sys
from pathlib import Path
configuration=Path(next(value.split("=",1)[1] for value in sys.argv if value.startswith("--config-file=")))
keys=["HOME","XDG_RUNTIME_DIR","XDG_CONFIG_HOME","XDG_STATE_HOME","XDG_CACHE_HOME","XDG_DATA_HOME","DBUS_SYSTEM_BUS_ADDRESS","DISPLAY","WAYLAND_DISPLAY","SWAYSOCK","DBUS_SESSION_BUS_ADDRESS","AT_SPI_BUS_ADDRESS"]
Path(os.environ["ICON_BOOTSTRAP_CAPTURE"]).write_text(json.dumps(dict(args=sys.argv[1:],configuration=configuration.read_text(),environment={key:os.environ.get(key) for key in keys})))
''');bus.chmod(0o700)
   env=dict(os.environ,PATH=str(binpath)+':'+os.environ['PATH'],ICON_BOOTSTRAP_CAPTURE=str(capture),HOME=str(root/'host-home'),DISPLAY=':host',WAYLAND_DISPLAY='host-wayland',SWAYSOCK='host-sway',DBUS_SESSION_BUS_ADDRESS='unix:path=/host-session',AT_SPI_BUS_ADDRESS='unix:path=/host-accessibility')
   result=subprocess.run([sys.executable,str(ROOT/'tests/desktop/icon_render.py'),'--assets',str(ROOT/'res/lumen-desktop/shell'),'--output',str(root/'output')],env=env,capture_output=True,text=True,timeout=10)
   self.assertEqual(result.returncode,0,result.stderr);data=json.loads(capture.read_text());values=data['environment'];runtime=Path(values['XDG_RUNTIME_DIR'])
   self.assertIn('--inside',data['args']);self.assertNotEqual(values['HOME'],env['HOME'])
   for key in ('HOME','XDG_CONFIG_HOME','XDG_STATE_HOME','XDG_CACHE_HOME','XDG_DATA_HOME'):self.assertTrue(Path(values[key]).is_relative_to(runtime))
   for key in ('DISPLAY','WAYLAND_DISPLAY','SWAYSOCK','DBUS_SESSION_BUS_ADDRESS','AT_SPI_BUS_ADDRESS'):self.assertIsNone(values[key])
   self.assertEqual(values['DBUS_SYSTEM_BUS_ADDRESS'],'unix:path='+str(runtime)+'/no-system-bus');self.assertIn('<auth>EXTERNAL</auth>',data['configuration'])
   self.assertNotIn('<include',data['configuration']);self.assertNotIn('<servicedir',data['configuration']);self.assertNotIn('<standard_session_servicedirs',data['configuration']);self.assertFalse(runtime.exists())
if __name__=='__main__':unittest.main()
