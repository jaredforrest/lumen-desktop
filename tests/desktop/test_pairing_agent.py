"""Pairing decisions without real Bluetooth hardware or a system-bus agent."""
import sys,unittest
from pathlib import Path
from unittest.mock import patch
sys.path.insert(0,str(Path(__file__).resolve().parents[2]/'res/lumen-desktop/shell'))
from bluetooth_pair import Agent
class PairingTests(unittest.TestCase):
 def setUp(self):
  self.agent=Agent.__new__(Agent);self.agent.device='/org/bluez/hci0/dev_00_11_22_33_44_55';self.agent.pending=None
  self.replies=[];self.errors=[]
 def ask(self,kind):
  with patch('bluetooth_pair.emit'):
   self.agent.ask(self.agent.device,kind,lambda *args:self.replies.append(args),self.errors.append)
 def test_requires_confirmation_and_rejects_other_device(self):
  self.ask('confirm');self.assertFalse(self.replies)
  self.agent.ask('/wrong','confirm',lambda:None,self.errors.append);self.assertEqual(len(self.errors),1)
  self.agent.respond({'accept':True});self.assertEqual(self.replies,[()]);self.assertIsNone(self.agent.pending)
 def test_invalid_code_is_retryable_and_cancel_is_explicit(self):
  self.ask('passkey')
  with self.assertRaises(ValueError):self.agent.respond({'accept':True,'value':'not-a-number'})
  self.assertIsNotNone(self.agent.pending);self.assertFalse(self.replies)
  self.agent.respond({'accept':True,'value':'000123'});self.assertEqual(int(self.replies[0][0]),123)
  self.ask('pin');self.agent.respond({'accept':False});self.assertEqual(len(self.errors),1);self.assertIsNone(self.agent.pending)
if __name__=='__main__':unittest.main()
