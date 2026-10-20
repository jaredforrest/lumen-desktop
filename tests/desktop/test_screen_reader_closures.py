"""Actual supported GIO closure registration: method callbacks and retirement."""
import os,subprocess as sp,sys,tempfile,time,unittest
from pathlib import Path
from private_native_bus import start_private_bus
if '--serve' in sys.argv:
 import gi
 gi.require_version('Gio','2.0')
 from gi.repository import Gio,GLib
 connection=Gio.bus_get_sync(Gio.BusType.SESSION,None)
 xml='<node><interface name="org.lumen.ReaderTest"><method name="Echo"><arg type="s" direction="in"/><arg type="s" direction="out"/></method><method name="Delayed"><arg type="s" direction="in"/><arg type="s" direction="out"/></method></interface></node>'
 def callback(connection,sender,path,interface,method,parameters,invocation):
  def reply():invocation.return_value(parameters);return False
  if method=='Delayed':GLib.idle_add(reply)
  else:reply()
 info=Gio.DBusNodeInfo.new_for_xml(xml).interfaces[0];registration=connection.register_object_with_closures2('/test',info,callback,None,None);print(connection.get_unique_name(),flush=True)
 GLib.MainLoop().run();raise SystemExit
class Closures(unittest.TestCase):
 def test_real_callbacks_and_retained_delayed_invocations(self):
  import dbus
  processes=[]
  with tempfile.TemporaryDirectory(prefix='lumen-reader-closures-') as directory:
   root=Path(directory);env=dict(os.environ,HOME=directory,XDG_RUNTIME_DIR=directory)
   try:
    start_private_bus(env,root,processes)
    with (root/'server.log').open('w') as log:server=sp.Popen([sys.executable,__file__,'--serve'],env=env,stdout=sp.PIPE,stderr=log,text=True)
    processes.append(server)
    import select
    self.assertTrue(select.select([server.stdout],[],[],5)[0]);owner=server.stdout.readline().strip();self.assertTrue(owner.startswith(':'))
    bus=dbus.bus.BusConnection(env['DBUS_SESSION_BUS_ADDRESS']);obj=bus.get_object(owner,'/test',introspect=False);interface=dbus.Interface(obj,'org.lumen.ReaderTest')
    for index in range(20):
     self.assertEqual(str(interface.Echo(str(index),timeout=2)),str(index));self.assertEqual(str(interface.Delayed(str(index),timeout=2)),str(index))
   finally:
    for process in reversed(processes):
     if process.poll() is None:
      process.terminate()
      try:process.wait(timeout=5)
      except sp.TimeoutExpired:process.kill();process.wait(timeout=5)
     for stream in (process.stdin,process.stdout,process.stderr):
      if stream and not stream.closed:stream.close()
if __name__=='__main__':unittest.main()
