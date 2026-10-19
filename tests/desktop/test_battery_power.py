"""Power loss, malformed values and replacement owners clear the percentage."""
import importlib.util
from pathlib import Path
import unittest
from unittest.mock import patch
import dbus

ROOT=Path(__file__).resolve().parents[2]
spec=importlib.util.spec_from_file_location('battery_power',ROOT/'res/lumen-desktop/shell/battery_power.py')
power=importlib.util.module_from_spec(spec);spec.loader.exec_module(power)

def properties():
    return dict(IsPresent=dbus.Boolean(True),PowerSupply=dbus.Boolean(True),Type=dbus.UInt32(2),
                Percentage=dbus.Double(42),State=dbus.UInt32(2),TimeToEmpty=dbus.Int64(5400),TimeToFull=dbus.Int64(0))

class Bus:
    def __init__(self,props):self.props=props;self.owners=[':1.42',':1.42'];self.calls=[]
    def get_name_owner(self,name):
        if not self.owners:raise dbus.DBusException('Service unavailable')
        return self.owners.pop(0)
    def get_object(self,owner,path,introspect=False):self.calls.append((owner,path));return self
    def GetDisplayDevice(self,timeout):return '/org/freedesktop/UPower/devices/DisplayDevice'
    def GetAll(self,interface,timeout):return self.props

class PowerFacts(unittest.TestCase):
    def test_charging_discharging_low_full_and_empty(self):
        for state,percentage,name,low,remaining in [(1,42,'charging',False,1200),(2,42,'discharging',False,5400),(2,12,'discharging',True,5400),(3,0,'empty',True,None),(4,100,'charged',False,None)]:
            with self.subTest(state=state):
                values=properties();values.update(State=state,Percentage=percentage,TimeToFull=1200)
                actual=power.normalize(values)
                self.assertEqual((actual['state'],actual['percentage'],actual['low'],actual['secondsRemaining']),(name,percentage,low,remaining))
    def test_missing_device_and_non_supply_hide_percentage(self):
        for values in [dict(IsPresent=False),{**properties(),'PowerSupply':False},{**properties(),'Type':5}]:
            result=power.normalize(values);self.assertTrue(result['available']);self.assertFalse(result['present']);self.assertIsNone(result['percentage'])
    def test_zero_is_real_charge_and_missing_estimate_is_not_zero_minutes(self):
        result=power.normalize({**properties(),'Percentage':0,'TimeToEmpty':0})
        self.assertEqual(result['percentage'],0);self.assertIsNone(result['secondsRemaining'])
    def test_bad_service_data_fails_closed(self):
        cases=[('Percentage',float('nan')),('Percentage',float('inf')),('Percentage',True),('Percentage',101),('Percentage',-1),('State',7),('State',1.5),('IsPresent','yes'),('TimeToEmpty',-1),('PowerSupply',1)]
        for key,value in cases:
            with self.subTest(key=key,value=value),patch.object(power.dbus,'Interface',lambda obj,interface:obj):
                self.assertEqual(power.status(Bus({**properties(),key:value})),power.unavailable())
    def test_owner_replacement_during_snapshot_is_rejected(self):
        bus=Bus(properties());bus.owners=[':1.42',':1.43']
        with patch.object(power.dbus,'Interface',lambda obj,interface:obj):self.assertEqual(power.status(bus),power.unavailable())
        self.assertTrue(all(owner==':1.42' for owner,path in bus.calls))
    def test_service_disappearance_never_reuses_previous_facts(self):
        bus=Bus(properties())
        with patch.object(power.dbus,'Interface',lambda obj,interface:obj):
            self.assertEqual(power.status(bus)['percentage'],42)
            self.assertEqual(power.status(bus),power.unavailable())

if __name__=='__main__':unittest.main()
