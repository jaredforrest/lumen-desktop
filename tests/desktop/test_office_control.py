"""Protocol behavior only; these synthetic peers are never application proof."""
import sys,tempfile,unittest
from pathlib import Path
sys.path.insert(0,str(Path(__file__).parent))
from office_fixture.control import Control,ControlError
class OfficeControl(unittest.TestCase):
 def test_notification_and_unrelated_reply_cannot_ack_current_request(self):
  with tempfile.TemporaryDirectory() as temporary:
   root=Path(temporary);peer=root/'peer.py';peer.write_text('import json,sys\nfor line in sys.stdin:\n r=json.loads(line)\n if "id" not in r:continue\n print(json.dumps(dict(jsonrpc="2.0",method="progress")),flush=True)\n print(json.dumps(dict(jsonrpc="2.0",id=r["id"]+10,result="unrelated")),flush=True)\n print(json.dumps(dict(jsonrpc="2.0",id=r["id"],result=dict(answer=r["method"]))),flush=True)\n')
   control=Control([sys.executable,str(peer)],root/'stderr')
   try:self.assertEqual(control.request('one'),dict(answer='one'));self.assertEqual(control.request('two'),dict(answer='two'))
   finally:control.close()
   self.assertTrue(control.process.stdin.closed and control.process.stdout.closed)
 def test_error_response_is_failure_without_automatic_retry(self):
  with tempfile.TemporaryDirectory() as temporary:
   root=Path(temporary);peer=root/'peer.py';peer.write_text('import json,sys\nfor line in sys.stdin:\n r=json.loads(line);print(json.dumps(dict(jsonrpc="2.0",id=r["id"],error=dict(code=-1,message="synthetic"))),flush=True)\n')
   control=Control([sys.executable,str(peer)],root/'stderr')
   try:
    with self.assertRaises(ControlError):control.request('mutation')
    self.assertEqual(control.serial,1)
   finally:control.close()
if __name__=='__main__':unittest.main()
