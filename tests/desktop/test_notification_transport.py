"""Out-of-order startup replies and old service instances never reach UI stdout."""
import importlib.util,unittest
from pathlib import Path
spec=importlib.util.spec_from_file_location('notification_transport',Path(__file__).resolve().parents[2]/'res/lumen-desktop/notifications/transport.py');module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)
class OrderTests(unittest.TestCase):
 def test_late_startup_reply_and_replaced_owner_are_rejected(self):
  order=module.TransportOrder();frame=lambda revision,instance='a'*32:{'instance':instance,'revision':revision}
  self.assertTrue(order.accept(frame(7)))
  self.assertFalse(order.accept(frame(6)))
  self.assertFalse(order.accept(frame(7)))
  self.assertFalse(order.accept(frame(8,'b'*32)))
  self.assertTrue(order.accept(frame(8)))
  for invalid in ({},[],None,frame(True),frame(0),frame(1.5),frame(2**53),frame(9,'unknown')):
   self.assertFalse(order.accept(invalid))
  self.assertEqual((order.instance,order.revision),('a'*32,8))
if __name__=='__main__':unittest.main()
