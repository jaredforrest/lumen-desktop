"""Connectivity is per-device and never inferred from association or another route."""
import sys,unittest
from pathlib import Path
from unittest.mock import patch
sys.path.insert(0,str(Path(__file__).resolve().parents[2]/'res/lumen-desktop/shell'))
import network_connectivity as n

def manager():return dict(WirelessEnabled=True,WirelessHardwareEnabled=True,ConnectivityCheckAvailable=True,ConnectivityCheckEnabled=True,Connectivity=4)
def device():return dict(DeviceType=2,Interface='wlan0',Managed=True,State=100,Ip4Connectivity=4,Ip6Connectivity=1)
class Bus:
 def __init__(self):self.owners=[':1.42',':1.42'];self.manager=manager();self.device=device();self.calls=[]
 def call_blocking(self,destination,path,interface,method,signature,args,timeout):
  assert (destination,path,interface,method,signature,args)==('org.freedesktop.DBus','/org/freedesktop/DBus','org.freedesktop.DBus','GetNameOwner','s',(n.SERVICE,));assert 0<timeout<=1;return self.owners.pop(0)
 def get_object(self,owner,path,introspect=False):self.calls.append((owner,path));return self
 def GetAll(self,interface,timeout):return self.manager if interface==n.SERVICE else self.device
 def GetDevices(self,timeout):return ['/device/1']
 def GetDeviceByIpIface(self,interface,timeout):assert interface=='wlan0';return '/device/1'
class Connectivity(unittest.TestCase):
 def test_device_phases(self):
  for state,phase in [(20,'unavailable'),(30,'disconnected'),(40,'associating'),(50,'associating'),(60,'associating'),(70,'acquiring'),(80,'acquiring'),(90,'acquiring'),(100,'connected'),(110,'disconnecting'),(120,'disconnected')]:
   self.assertEqual(n.normalize(manager(),dict(device(),State=state))['phase'],phase)
  self.assertEqual(n.normalize(dict(manager(),WirelessEnabled=False),device())['phase'],'disabled')
  self.assertEqual(n.normalize(dict(manager(),WirelessHardwareEnabled=False),device())['phase'],'disabled')
 def test_only_explicit_device_checks_not_global_connectivity(self):
  for values,result in [((4,1),'full'),((1,4),'full'),((2,4),'full'),((2,1),'portal'),((3,1),'limited'),((1,1),'none'),((0,1),'unknown'),((0,0),'unknown')]:
   self.assertEqual(n.normalize(manager(),dict(device(),Ip4Connectivity=values[0],Ip6Connectivity=values[1]))['connectivity'],result)
  for props in [dict(manager(),ConnectivityCheckAvailable=False),dict(manager(),ConnectivityCheckEnabled=False)]:self.assertEqual(n.normalize(props,device())['connectivity'],'unknown')
  self.assertEqual(n.normalize(manager(),dict(device(),State=70))['connectivity'],'unknown')
 def test_owner_replacement_mismatch_and_loss_drop_facts(self):
  with patch.object(n.dbus,'Interface',lambda obj,interface:obj):
   bus=Bus();self.assertEqual(n.status('wlan0',bus)['connectivity'],'full');self.assertTrue(all(owner==':1.42' for owner,path in bus.calls))
   bus=Bus();bus.owners[-1]=':1.43';self.assertIsNone(n.status('wlan0',bus))
   bus=Bus();bus.device['Interface']='wlan1';self.assertIsNone(n.status('wlan0',bus))
   bus=Bus();bus.device['Managed']=False;self.assertIsNone(n.status('wlan0',bus))
   bus=Bus();bus.device['Ip4Connectivity']=99;self.assertIsNone(n.status('wlan0',bus))
 def test_owner_calls_share_snapshot_deadline(self):
  bus=Bus();clock=[0];original=bus.GetAll
  def properties(interface,timeout):
   value=original(interface,timeout)
   if interface==n.DEVICE:clock[0]=3.1
   return value
  bus.GetAll=properties
  with patch.object(n.dbus,'Interface',lambda obj,interface:obj),patch.object(n.time,'monotonic',lambda:clock[0]):
   self.assertIsNone(n.status('wlan0',bus));self.assertEqual(bus.owners,[':1.42'])
 def test_fallback_never_claims_internet_or_hardware_absence(self):
  result=n.merge(dict(available=True,state='COMPLETED',name='Synthetic',networks=[]),None)
  self.assertEqual((result['phase'],result['connectivity']),('connected','unknown'))
  result=n.merge(dict(available=False,networks=[]),None);self.assertEqual(result['phase'],'service-unavailable');self.assertFalse(result['controlAvailable'])
  result=n.merge(dict(available=False,networks=[]),dict(phase='disabled',connectivity='unknown',source='network-manager',interface='wlan0'));self.assertEqual(result['phase'],'disabled');self.assertFalse(result['controlAvailable'])
if __name__=='__main__':unittest.main()
