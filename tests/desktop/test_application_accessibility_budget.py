"""Absolute private observation budgets apply to every provider roundtrip."""
import sys,unittest
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import Mock,patch
sys.path.insert(0,str(Path(__file__).parent/'application_fixture'))
from accessibility import bounded_call,Observer,role_name
import dbus
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
 def test_unavailable_last_row_cannot_return_after_deadline(self):
  observer=Observer.__new__(Observer);observer.describe=Mock(side_effect=dbus.DBusException('unavailable'))
  with patch('accessibility.time.monotonic',side_effect=[0,0,5.01]):
   with self.assertRaisesRegex(ValueError,'budget'):observer.snapshot()
 def test_owner_list_roundtrip_is_bounded(self):
  observer=Observer.__new__(Observer);method=Mock(return_value=[]);observer.interface=Mock(return_value=SimpleNamespace(ListNames=method))
  with patch('accessibility.time.monotonic',side_effect=[0,0,.1]):self.assertEqual(observer.diagnostics(),[])
  self.assertEqual(method.call_args.kwargs['timeout'],.5)
 def test_oversized_owner_list_rejected_before_pid_probe(self):
  observer=Observer.__new__(Observer);observer.interface=Mock(return_value=SimpleNamespace(ListNames=Mock(return_value=[':1.'+str(i) for i in range(257)])));observer.pid=Mock()
  with patch('accessibility.time.monotonic',side_effect=[0,0,.1]):
   with self.assertRaisesRegex(ValueError,'count budget'):observer.diagnostics()
  observer.pid.assert_not_called()
 def test_late_owner_error_does_not_escape_budget(self):
  observer=Observer.__new__(Observer);observer.interface=Mock(return_value=SimpleNamespace(ListNames=Mock(return_value=[':1.2'])));observer.pid=Mock(side_effect=dbus.DBusException('late'))
  with patch('accessibility.time.monotonic',side_effect=[0,0,.1,5.01]):
   with self.assertRaisesRegex(ValueError,'budget'):observer.diagnostics()
  self.assertEqual(observer.pid.call_args.args,(':1.2',5))
 def test_numeric_role_fallback_only_for_unsupported_name_method(self):
  missing=dbus.DBusException('no role-name method',name='org.freedesktop.DBus.Error.UnknownMethod')
  accessible=SimpleNamespace(GetRoleName=Mock(side_effect=missing),GetRole=Mock(return_value=43))
  role=Mock(return_value=SimpleNamespace(value_nick='button'));gi=SimpleNamespace(require_version=Mock())
  with patch.dict(sys.modules,{'gi':gi,'gi.repository':SimpleNamespace(Atspi=SimpleNamespace(Role=role))}):self.assertEqual(role_name(accessible),'button')
  accessible.GetRole.assert_called_once();role.assert_called_once_with(43);gi.require_version.assert_called_once_with('Atspi','2.0')
 def test_role_failure_never_falls_back_to_different_target(self):
  failure=dbus.DBusException('gone',name='org.freedesktop.DBus.Error.UnknownObject')
  accessible=SimpleNamespace(GetRoleName=Mock(side_effect=failure),GetRole=Mock())
  with self.assertRaises(dbus.DBusException):role_name(accessible)
  accessible.GetRole.assert_not_called()
 def test_individual_cap_unchanged(self):
  method=Mock(return_value=True)
  with patch('accessibility.time.monotonic',side_effect=[1,1.2]):self.assertTrue(bounded_call(method,deadline=5))
  self.assertEqual(method.call_args.kwargs['timeout'],.5)
if __name__=='__main__':unittest.main()
