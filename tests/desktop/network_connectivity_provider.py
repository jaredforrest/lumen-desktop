"""Actual activation-free private bus: per-interface checks, transitions and loss."""
import argparse,json,os,subprocess as sp,sys,tempfile,time
from contextlib import ExitStack
from pathlib import Path
from private_native_bus import start_private_bus
parser=argparse.ArgumentParser();parser.add_argument('--serve',action='store_true');parser.add_argument('--output',type=Path);args=parser.parse_args()
if args.serve:
 import dbus,dbus.service
 from dbus.mainloop.glib import DBusGMainLoop
 from gi.repository import GLib
 DBusGMainLoop(set_as_default=True);bus=dbus.bus.BusConnection(os.environ['DBUS_SYSTEM_BUS_ADDRESS']);name=dbus.service.BusName('org.freedesktop.NetworkManager',bus=bus,do_not_queue=True)
 class Provider(dbus.service.Object):
  case='full'
  @dbus.service.method('org.freedesktop.NetworkManager',in_signature='s',out_signature='o')
  def GetDeviceByIpIface(self,interface):assert interface=='wlan0';return '/device/wifi'
  @dbus.service.method('org.freedesktop.NetworkManager',out_signature='ao')
  def GetDevices(self):return ['/device/wifi','/device/ethernet']
  @dbus.service.method('org.freedesktop.DBus.Properties',in_signature='s',out_signature='a{sv}')
  def GetAll(self,interface):
   if interface=='org.freedesktop.NetworkManager':return dict(WirelessEnabled=dbus.Boolean(self.case!='disabled'),WirelessHardwareEnabled=dbus.Boolean(True),ConnectivityCheckAvailable=dbus.Boolean(True),ConnectivityCheckEnabled=dbus.Boolean(self.case!='unchecked'),Connectivity=dbus.UInt32(4))
   assert interface=='org.freedesktop.NetworkManager.Device'
   if self.__dbus_object_path__=='/device/ethernet':return dict(DeviceType=dbus.UInt32(1),Interface='eth0',Managed=dbus.Boolean(True),State=dbus.UInt32(100),Ip4Connectivity=dbus.UInt32(4),Ip6Connectivity=dbus.UInt32(4))
   state={'disconnected':30,'associating':50,'acquiring':70,'unavailable':20}.get(self.case,100)
   value={'none':1,'portal':2,'limited':3,'unknown':0}.get(self.case,4)
   if self.case=='owner-loss':bus.release_name('org.freedesktop.NetworkManager')
   return dict(DeviceType=dbus.UInt32(2),Interface='wlan0',Managed=dbus.Boolean(True),State=dbus.UInt32(state),Ip4Connectivity=dbus.UInt32(value),Ip6Connectivity=dbus.UInt32(1))
  @dbus.service.method('org.lumen.TestConnectivity',in_signature='s')
  def SetCase(self,value):Provider.case=str(value)
 root=Provider(bus,'/org/freedesktop/NetworkManager');wifi=Provider(bus,'/device/wifi');ethernet=Provider(bus,'/device/ethernet');print('READY',flush=True);GLib.MainLoop().run();raise SystemExit
assert args.output is not None;args.output.mkdir(parents=True,exist_ok=False);processes=[]
def reap():
 for process in reversed(processes):
  if process.poll() is None:
   process.terminate()
   try:process.wait(timeout=5)
   except sp.TimeoutExpired:process.kill();process.wait(timeout=5)
  for stream in (process.stdin,process.stdout,process.stderr):
   if stream and not stream.closed:stream.close()
with ExitStack() as stack:
 stack.callback(reap);private=stack.enter_context(tempfile.TemporaryDirectory(prefix='lumen-connectivity-'));env=dict(os.environ,HOME=private,XDG_RUNTIME_DIR=private)
 start_private_bus(env,args.output,processes);address=env.pop('DBUS_SESSION_BUS_ADDRESS');env['DBUS_SYSTEM_BUS_ADDRESS']=address
 import dbus
 bus=dbus.bus.BusConnection(address)
 assert set(map(str,bus.call_blocking('org.freedesktop.DBus','/org/freedesktop/DBus','org.freedesktop.DBus','ListActivatableNames','',())))=={'org.freedesktop.DBus'}
 def wait(test):
  deadline=time.monotonic()+5
  while time.monotonic()<deadline:
   if test():return
   time.sleep(.02)
  raise AssertionError('Private provider not ready')
 def launch(index):
  process=sp.Popen([sys.executable,__file__,'--serve'],env=env,stdout=stack.enter_context((args.output/f'provider-{index}.log').open('w')),stderr=sp.STDOUT);processes.append(process);wait(lambda:bus.name_has_owner('org.freedesktop.NetworkManager'));return process
 env['PYTHONPATH']=str(Path(__file__).resolve().parents[2]/'res/lumen-desktop/shell')
 def snapshot(interface='wlan0'):
  return json.loads(sp.check_output([sys.executable,'-c',f'import json,network_connectivity as n;print(json.dumps(n.status({interface!r})))'],env=env,text=True,timeout=8))
 first=launch(1);records=[]
 for case,phase,connectivity in [('full','connected','full'),('portal','connected','portal'),('limited','connected','limited'),('none','connected','none'),('unknown','connected','unknown'),('disabled','disabled','unknown'),('disconnected','disconnected','unknown'),('associating','associating','unknown'),('acquiring','acquiring','unknown'),('unchecked','connected','unknown'),('unavailable','unavailable','unknown')]:
  bus.call_blocking('org.freedesktop.NetworkManager','/org/freedesktop/NetworkManager','org.lumen.TestConnectivity','SetCase','s',(case,));value=snapshot();assert (value['phase'],value['connectivity'])==(phase,connectivity),(case,value);records.append(dict(case=case,status=value))
 assert snapshot(None)['phase']=='unavailable'
 bus.call_blocking('org.freedesktop.NetworkManager','/org/freedesktop/NetworkManager','org.lumen.TestConnectivity','SetCase','s',('owner-loss',));assert snapshot() is None
 first.terminate();first.wait(timeout=5);second=launch(2);assert snapshot()['connectivity']=='full';second.terminate();second.wait(timeout=5);assert snapshot() is None
 processes[0].terminate();processes[0].wait(timeout=5);started=time.monotonic();assert snapshot() is None;assert time.monotonic()-started<3,'Dead private bus must fail promptly'
 (args.output/'result.json').write_text(json.dumps(dict(passed=True,cases=records,checks=['actual per-device transitions','global Ethernet full never overrides Wi-Fi','unchecked service unknown','owner loss refuses snapshot','new owner fresh facts','service loss clears facts','activation-free private bus','actual bus daemon loss']),indent=2));print('PASS actual private connectivity transitions and owner loss/replacement')
