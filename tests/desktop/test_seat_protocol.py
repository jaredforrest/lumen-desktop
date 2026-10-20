"""Focus trace checks reject duplicated and mismatched leaves per resource."""
import unittest
from application_fixture.seat_protocol import inspect,require_ordered
class SeatProtocol(unittest.TestCase):
 def test_complete_independent_resources(self):
  text='\n'.join(f'{kind}#{identifier}.{action}(1, wl_surface#{surface})' for kind,identifier,surface in [('wl_keyboard',24,29),('wl_pointer',25,29),('wl_keyboard',30,41)] for action in ('enter','leave'))
  self.assertFalse(require_ordered(text)['errors'])
 def test_duplicate_leave_is_not_hidden_by_another_resource(self):
  text='wl_keyboard#24.enter(1, wl_surface#29)\nwl_keyboard#30.enter(2, wl_surface#29)\nwl_keyboard#24.leave(3, wl_surface#29)\nwl_keyboard#24.leave(4, wl_surface#29)'
  result=inspect(text);self.assertEqual(len(result['errors']),1);self.assertEqual(result['errors'][0]['expected'],None)
 def test_mismatched_surface(self):
  self.assertEqual(inspect('wl_pointer#1.enter(1, wl_surface#2)\nwl_pointer#1.leave(2, wl_surface#3)')['errors'][0]['expected'],'2')
 def test_recreated_resource_can_enter_again(self):
  self.assertFalse(inspect('wl_keyboard#24.enter(1, wl_surface#29)\nwl_keyboard#24.leave(2, wl_surface#29)\nwl_keyboard#24.enter(3, wl_surface#41)\nwl_keyboard#24.leave(4, wl_surface#41)')['errors'])
 def test_empty_or_keyboard_only_is_not_acceptance(self):
  for text in ('','wl_keyboard#24.enter(1, wl_surface#29)\nwl_keyboard#24.leave(2, wl_surface#29)'):
   with self.subTest(text=text),self.assertRaises(AssertionError):require_ordered(text)
if __name__=='__main__':unittest.main()
