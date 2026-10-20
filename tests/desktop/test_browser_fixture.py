"""Actual local server and subprocess diagnostic protocol, without an application."""
import hashlib,json,os,sys,tempfile,threading,unittest,urllib.request
from pathlib import Path
sys.path.insert(0,str(Path(__file__).resolve().parent/'browser_fixture'))
from protocol import PipeTransport
from server import FixtureServer,PAYLOAD,DIGEST
class Fixture(unittest.TestCase):
 def test_gated_loopback_download_commits_exact_bytes(self):
  with FixtureServer() as server:
   opener=urllib.request.build_opener(urllib.request.ProxyHandler({}))
   with opener.open(server.origin,timeout=2) as response:self.assertIn(b'Disposable local page',response.read())
   result=[];started=threading.Event()
   def download():
    with opener.open(server.origin+'/download/fixed',timeout=3) as response:
     result.append(response.read(65536));started.set();result.append(response.read())
   worker=threading.Thread(target=download);worker.start();self.assertTrue(started.wait(2));self.assertTrue(worker.is_alive());self.assertFalse(server.snapshot()[0]['complete']);server.gates['fixed'].set();worker.join(3)
   self.assertFalse(worker.is_alive());self.assertEqual(hashlib.sha256(b''.join(result)).hexdigest(),DIGEST);self.assertEqual(len(b''.join(result)),len(PAYLOAD))
 def test_pipe_observer_handles_fragmented_frames_and_events(self):
  with tempfile.TemporaryDirectory() as temporary:
   root=Path(temporary);peer=root/'peer.py';peer.write_text('''import json,os
buffer=b''
while True:
 chunk=os.read(3,4096)
 if not chunk:break
 buffer+=chunk
 while b'\\0' in buffer:
  line,buffer=buffer.split(b'\\0',1);value=json.loads(line)
  event=json.dumps(dict(method='Fixture.observed',params=dict(method=value['method']))).encode()+b'\\0'
  response=json.dumps(dict(id=value['id'],result=dict(observed=value['params']))).encode()+b'\\0'
  for payload in (event[:3],event[3:],response[:5],response[5:]):os.write(4,payload)
''')
   with (root/'peer.log').open('w') as log,PipeTransport([sys.executable,str(peer)],dict(os.environ),log) as transport:
    result=transport.request('Fixture.inspect',dict(value='literal é'),timeout=2);self.assertEqual(result,dict(observed=dict(value='literal é')));self.assertEqual(transport.events[0]['params']['method'],'Fixture.inspect')
   self.assertEqual((root/'peer.log').read_text(),'')
if __name__=='__main__':unittest.main()
