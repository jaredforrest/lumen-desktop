"""Explicit native-fixture buses need no host config or activation services."""
import os,signal,subprocess as sp,tempfile,unittest
from pathlib import Path
from private_native_bus import start_private_bus

class PrivateBusTests(unittest.TestCase):
 def test_sanitized_bus_without_host_defaults(self):
  with tempfile.TemporaryDirectory(prefix='lumen-private-bus-') as private:
   env=dict(os.environ,HOME=private,XDG_RUNTIME_DIR=private,DBUS_SESSION_BUS_ADDRESS='unix:path=/unavailable/session',DBUS_SYSTEM_BUS_ADDRESS='unix:path=/unavailable/system')
   processes=[]
   try:
    bus=start_private_bus(env,private,processes)
    reply=sp.check_output(['dbus-send','--session','--print-reply','--dest=org.freedesktop.DBus','/org/freedesktop/DBus','org.freedesktop.DBus.ListActivatableNames'],env=env,text=True,timeout=5)
    self.assertEqual(reply.count('string "'),1,reply)
    self.assertIn('string "org.freedesktop.DBus"',reply)
    self.assertEqual(env['DBUS_SYSTEM_BUS_ADDRESS'],'unix:path='+private+'/no-system-bus')
    self.assertNotIn('servicedir',(Path(private)/'bus.conf').read_text())
   finally:
    for bus in processes:
     if bus.poll() is None:os.killpg(bus.pid,signal.SIGTERM)
     bus.wait(timeout=5)
     if bus.stdout:bus.stdout.close()
 def test_native_call_sites_start_bus_before_compositor(self):
  for name in ('window_relationships_input.py','edge_layouts_input.py','paired_layouts_input.py'):
   source=(Path(__file__).parent/name).read_text()
   self.assertLess(source.index('  start_private_bus(env,a.output,processes)'),source.index("launch([a.sway"))
  source=(Path(__file__).parent/'native_hot_corners.py').read_text()
  self.assertLess(source.index("env['DBUS_SESSION_BUS_ADDRESS']=bus.stdout.readline()"),source.index('launch([a.sway'))
  self.assertIn("'--config-file='+str(bus_config)",source)

if __name__=='__main__':unittest.main()
