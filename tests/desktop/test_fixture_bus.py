"""Test runners must not auto-start unrelated desktop account or mount services."""
import unittest
import dbus

class FixtureBus(unittest.TestCase):
    def test_only_bus_is_activatable(self):
        bus=dbus.SessionBus()
        names=bus.call_blocking('org.freedesktop.DBus','/org/freedesktop/DBus','org.freedesktop.DBus','ListActivatableNames','',())
        self.assertEqual(set(map(str,names)),{'org.freedesktop.DBus'})

if __name__=='__main__':unittest.main()
