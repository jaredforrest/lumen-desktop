"""Real private provider/launch-observer protocol; no desktop service or account access."""
import hashlib,json,os,queue,select,shutil,subprocess,sys,tempfile,threading,time,unittest
from pathlib import Path
ROOT=Path(__file__).resolve().parents[2];SHELL=ROOT/'res/lumen-desktop/shell'
class Protocol(unittest.TestCase):
 def test_captured_open_replacement_and_unsupported_reason(self):
  with tempfile.TemporaryDirectory(prefix='lumen-action-protocol-') as temporary:
   home=Path(temporary);bin=home/'bin';bin.mkdir();sink=home/'opened';observer=bin/'xdg-open';observer.write_text('#!'+sys.executable+'\nimport os,pathlib,sys\npathlib.Path(os.environ["PRIVATE_OBSERVER"]).write_text(sys.argv[1])\n');observer.chmod(0o700)
   env=dict(os.environ,HOME=temporary,XDG_RUNTIME_DIR=temporary,PATH=str(bin)+':'+os.environ['PATH'],LUMEN_LAUNCH_SCOPES='0',PRIVATE_OBSERVER=str(sink))
   for key in ('CONFIG','CACHE','DATA','STATE'):env['XDG_'+key+'_HOME']=str(home/key.lower())
   child=subprocess.Popen([sys.executable,str(SHELL/'owned_preview.py')],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True,env=env)
   def read():
    self.assertTrue(select.select([child.stdout],[],[],8)[0],'provider deadline');return json.loads(child.stdout.readline())
   try:
    ready=read();generation=ready['generation'];source=home/'original.txt';source.write_text('first')
    def send(data):child.stdin.write(json.dumps(dict(data,generation=generation))+'\n');child.stdin.flush();return read()
    preview=send(dict(id=1,action='preview',path=str(source)));cap=preview['actionCapability'];self.assertEqual(preview['type'],'text');self.assertIsNone(preview['reason'])
    opened=send(dict(id=2,action='resource-action',operation='open',capability=cap));self.assertTrue(opened['dispatched']);self.assertEqual(sink.read_text(),source.as_uri());sink.unlink()
    replacement=home/'replacement';replacement.write_text('second');replacement.replace(source)
    refused=send(dict(id=3,action='resource-action',operation='open',capability=cap));self.assertEqual(refused['reason'],'source_changed');self.assertFalse(sink.exists())
    # Failure is a separate action response; it never substitutes content into the old preview.
    self.assertEqual(preview['text'],'first');self.assertNotEqual(refused['action'],'preview')
    large=home/'large.txt'
    with large.open('wb') as stream:stream.truncate(64*1024*1024+1)
    limited=send(dict(id=4,action='preview',path=str(large)));self.assertEqual(limited['reason'],'policy_limit');self.assertTrue(limited['actionCapability'])
    opened=send(dict(id=5,action='resource-action',operation='open',capability=limited['actionCapability']));self.assertTrue(opened['dispatched']);self.assertEqual(sink.read_text(),large.as_uri())
    release=send(dict(id=6,action='retire-preview',requests=[1,4]));self.assertTrue(release['retired'])
    refused=send(dict(id=7,action='resource-action',operation='open',capability=cap));self.assertEqual(refused['reason'],'source_changed')
    # UI allocates the next preview before its older-resource release command.
    self.assertTrue(send(dict(id=9,action='retire-preview',requests=[]))['retired'])
    reordered=send(dict(id=8,action='preview',path=str(source)));self.assertEqual(reordered['text'],'second')
    replay=send(dict(id=8,action='preview',path=str(source)));self.assertEqual(replay['reason'],'invalid_request')
    self.assertTrue(send(dict(id=200,action='retire-preview',requests=[]))['retired'])
    retired=send(dict(id=8,action='preview',path=str(source)));self.assertEqual(retired['reason'],'invalid_request')
   finally:
    child.terminate();child.wait(timeout=5)
    for stream in (child.stdin,child.stdout,child.stderr):stream.close()
 def test_exact_content_cancellation_keeps_visible_snapshot(self):
  with tempfile.TemporaryDirectory() as temporary:
   home=Path(temporary);assets=home/'shell';shutil.copytree(SHELL,assets);marker=home/'helper.json'
   (assets/'context_tools.py').chmod(0o600);(assets/'context_tools.py').write_text("""import hashlib,json,os,subprocess,sys,time
from pathlib import Path
data=json.loads(sys.stdin.readline());source=Path(data['path']);descendant=subprocess.Popen([sys.executable,'-c','import time;time.sleep(120)'])
Path(os.environ['PRIVATE_CONTENT_MARKER']).write_text(json.dumps(dict(pid=os.getpid(),descendant=descendant.pid,path=str(source),sha256=hashlib.sha256(source.read_bytes()).hexdigest())))
time.sleep(120)
""")
   source=home/'source.txt';source.write_text('captured bytes');env=dict(os.environ,HOME=temporary,XDG_RUNTIME_DIR=temporary,PRIVATE_CONTENT_MARKER=str(marker));child=subprocess.Popen([sys.executable,str(assets/'owned_preview.py')],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True,env=env)
   messages=queue.Queue()
   def collect():
    for line in child.stdout:messages.put(json.loads(line))
   collector=threading.Thread(target=collect);collector.start()
   def read():return messages.get(timeout=8)
   try:
    generation=read()['generation']
    def send(data):child.stdin.write(json.dumps(dict(data,generation=generation))+'\n');child.stdin.flush()
    send(dict(id=1,action='preview',path=str(source)));preview=read();cap=preview['actionCapability'];resource=Path(preview['resourcePath']);send(dict(id=2,action='resource-action',capability=cap,operation='ocr'))
    deadline=time.monotonic()+5
    while not marker.exists() and time.monotonic()<deadline:time.sleep(.02)
    helper=json.loads(marker.read_text());self.assertEqual(helper['path'],str(resource));self.assertEqual(helper['sha256'],hashlib.sha256(b'captured bytes').hexdigest())
    send(dict(id=3,action='cancel-action',target=999,capability=cap));self.assertFalse(read()['accepted'])
    send(dict(id=4,action='cancel-action',target=2,capability='wrong'));self.assertFalse(read()['accepted']);self.assertTrue(resource.exists())
    send(dict(id=5,action='cancel-action',target=2,capability=cap));responses=[read(),read()];canceled=next(value for value in responses if value['action']=='resource-action');self.assertEqual(canceled['reason'],'canceled');self.assertFalse(Path('/proc/'+str(helper['pid'])).exists());self.assertTrue(resource.exists());self.assertEqual(resource.read_bytes(),b'captured bytes')
    send(dict(id=6,action='retire-preview',requests=[1]));self.assertTrue(read()['retired']);self.assertFalse(resource.exists())
   finally:
    child.terminate();child.wait(timeout=5);collector.join(timeout=3);self.assertFalse(collector.is_alive())
    for stream in (child.stdin,child.stdout,child.stderr):stream.close()
if __name__=='__main__':unittest.main()
