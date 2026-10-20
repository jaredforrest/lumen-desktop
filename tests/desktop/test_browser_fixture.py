"""Actual local server and subprocess diagnostic protocol, without an application."""
import hashlib,json,os,subprocess,sys,tempfile,threading,unittest,urllib.request
from pathlib import Path
sys.path.insert(0,str(Path(__file__).resolve().parent/'browser_fixture'))
sys.path.insert(0,str(Path(__file__).resolve().parent/'application_fixture'))
from protocol import PipeTransport
from server import FixtureServer,PAYLOAD,DIGEST
from invocation import command
from capabilities import Inventory
from keyboard import text_events,SPECIAL
from profile import download_rows
class Fixture(unittest.TestCase):
 def test_painted_readback_requires_ink_and_rejects_blank_or_wrong_frames(self):
  from PIL import Image,ImageDraw
  from paint import match
  reference=Image.new('RGB',(64,32),'white');ImageDraw.Draw(reference).rectangle((8,8,30,20),fill='#21384a')
  screen=Image.new('RGB',(100,80),'white');self.assertFalse(match(reference,screen,(10,12))['painted']);self.assertFalse(match(Image.new('RGB',(64,32),'white'),screen,(10,12))['painted'])
  screen.paste(reference,(10,12));self.assertTrue(match(reference,screen,(10,12))['painted'])
  shifted=Image.new('RGB',(100,80),'white');shifted.paste(reference,(11,12));self.assertTrue(match(reference,shifted,(10,12))['painted'])
  shifted=Image.new('RGB',(100,80),'white');shifted.paste(reference,(12,12));self.assertFalse(match(reference,shifted,(10,12))['painted'])
 def test_internal_entry_rejects_ambient_execution_before_creating_state(self):
  with tempfile.TemporaryDirectory(prefix='lumen-browser-entry-') as temporary:
   root=Path(temporary);arguments=[sys.executable,str(Path(__file__).with_name('browser_workflows_input.py')),'--inside','--output',str(root/'artifacts')]
   for name in ('browser','launcher','sway','pointer-tool','keyboard-tool'):arguments.extend(['--'+name,sys.executable])
   for name in ('font-directory','at-spi-prefix','software-driver'):arguments.extend(['--'+name,sys.prefix])
   result=subprocess.run(arguments,env=dict(os.environ,HOME=str(root),XDG_RUNTIME_DIR=str(root/'runtime')),text=True,capture_output=True,timeout=5)
   self.assertNotEqual(result.returncode,0);self.assertIn('Private application boundary required',result.stderr);self.assertEqual(list(root.iterdir()),[])
 def test_invocation_cannot_select_host_profile_or_external_origin(self):
  actual=command('/nix/store/fixture-browser/bin/brave','/home/fixture/Literal # profile','http://127.0.0.1:8080/page')
  self.assertIn('--user-data-dir=/home/fixture/Literal # profile',actual)
  self.assertFalse(any(value.startswith(('--no-sandbox','--disable-setuid-sandbox','--disable-seccomp','--use-fake-ui')) for value in actual))
  for profile,url in [('/home/other/profile','http://127.0.0.1:8080/page'),('/home/fixture/../other','http://127.0.0.1:8080/page'),('/home/fixture/profile','https://example.invalid/')]:
   with self.assertRaises(ValueError):command('/nix/store/fixture-browser/bin/brave',profile,url)
 def test_one_passing_action_never_closes_a_multi_action_capability(self):
  inventory=Inventory({'product':'synthetic version'});inventory.record('APP-BROWSE-01','new_tab','verified_compatible',{'capture':'synthetic'})
  row=inventory.snapshot()[0];self.assertEqual(row['status'],'unassessed');self.assertEqual(row['supported_actions'],['new_tab']);self.assertIn('close_tab',row['remaining_actions'])
  with self.assertRaises(ValueError):inventory.record('APP-BROWSE-01','new_tab','verified_compatible',{'capture':'replacement'})
 def test_stable_physical_map_covers_printable_us_input(self):
  for char in map(chr,range(32,127)):
   events=text_events(char);self.assertEqual(events[-1],('m',0,0));self.assertEqual(events[1][0],'k');self.assertEqual(events[1][1],events[2][1]);self.assertEqual(events[1][2],1);self.assertEqual(events[2][2],0)
  self.assertEqual(SPECIAL['Escape'],1)
  with self.assertRaises(ValueError):text_events('é')
 def test_download_observer_opens_only_explicit_profile_readonly(self):
  import sqlite3
  from contextlib import closing
  with tempfile.TemporaryDirectory(prefix='lumen-browser-history-') as temporary:
   profile=Path(temporary)/'Literal # é';default=profile/'Default';default.mkdir(parents=True);path=default/'History'
   with closing(sqlite3.connect(path)) as connection:
    connection.execute('CREATE TABLE downloads (id INTEGER,guid TEXT,current_path TEXT,target_path TEXT,received_bytes INTEGER,total_bytes INTEGER,state INTEGER,interrupt_reason INTEGER)')
    connection.execute('INSERT INTO downloads VALUES (1,?,?,?,?,?,?,?)',('synthetic-guid',b'/synthetic/partial',b'/synthetic/complete',12,24,0,0));connection.commit()
   before=path.read_bytes();rows=download_rows(profile);self.assertEqual(rows[0]['state'],0);self.assertEqual(rows[0]['target_path'],'/synthetic/complete');self.assertEqual(path.read_bytes(),before)
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
