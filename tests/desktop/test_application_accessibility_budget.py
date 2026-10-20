"""Absolute private observation budgets apply to every provider roundtrip."""
import sys,unittest
from pathlib import Path
from unittest.mock import Mock,patch
sys.path.insert(0,str(Path(__file__).parent/'application_fixture'))
from accessibility import bounded_call
class Budget(unittest.TestCase):
 def test_remaining_budget_bounds_last_roundtrip(self):
  method=Mock(return_value='row')
  with patch('accessibility.time.monotonic',side_effect=[4.8,4.9]):self.assertEqual(bounded_call(method,'field',deadline=5),'row')
  self.assertAlmostEqual(method.call_args.kwargs['timeout'],.2)
 def test_late_response_never_becomes_success(self):
  method=Mock(return_value='late')
  with patch('accessibility.time.monotonic',side_effect=[4.8,5.01]):
   with self.assertRaisesRegex(ValueError,'budget'):bounded_call(method,deadline=5)
 def test_expired_budget_never_calls_provider(self):
  method=Mock()
  with patch('accessibility.time.monotonic',return_value=5):
   with self.assertRaisesRegex(ValueError,'budget'):bounded_call(method,deadline=5)
  method.assert_not_called()
 def test_individual_cap_unchanged(self):
  method=Mock(return_value=True)
  with patch('accessibility.time.monotonic',side_effect=[1,1.2]):self.assertTrue(bounded_call(method,deadline=5))
  self.assertEqual(method.call_args.kwargs['timeout'],.5)
if __name__=='__main__':unittest.main()
