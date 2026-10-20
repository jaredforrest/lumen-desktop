import importlib.util,json,os,re,shutil,subprocess,sys,tempfile,time,unittest
from pathlib import Path
ROOT=Path(__file__).resolve().parents[2];sys.path.insert(0,str(ROOT/'res/lumen-desktop/shell'))
from captured_resource_actions import CapturedResourceActions,ResourceUnavailable,facts,wire_facts,parse_facts
class Actions(unittest.TestCase):
 def test_wire_identity_preserves_nanoseconds_and_large_inodes(self):
  value=(2**60+1,2**60+3,33188,64,1792464592083793001,1792464592083793007)
  self.assertEqual(parse_facts(json.loads(json.dumps(wire_facts(value)))),value)
  self.assertRaises(ResourceUnavailable,parse_facts,list(value))
  self.assertEqual(parse_facts(wire_facts((1,2,33188,64,-123456789,-1))),(1,2,33188,64,-123456789,-1))
 def test_replacement_never_reaches_real_launch_observer(self):
  with tempfile.TemporaryDirectory() as temporary:
   root=Path(temporary);source=root/'file ; café.txt';source.write_text('first');broker=CapturedResourceActions();key=broker.capture(source);sink=root/'observed';observer=root/'observer.py';observer.write_text('import pathlib,sys;pathlib.Path(sys.argv[1]).write_text(sys.argv[2])')
   def run(command,**kwargs):subprocess.run([sys.executable,str(observer),str(sink),command[-1]],check=True)
   broker.dispatch(key,'open',runner=run);self.assertEqual(sink.read_text(),source.as_uri());sink.unlink();replacement=root/'new';replacement.write_text('second');replacement.replace(source)
   with self.assertRaises(ResourceUnavailable) as error:broker.dispatch(key,'open',runner=run)
   self.assertEqual(error.exception.reason,'source_changed');self.assertFalse(sink.exists())
 def test_content_uses_immutable_resource_and_rejects_replacement(self):
  with tempfile.TemporaryDirectory() as temporary:
   root=Path(temporary);source=root/'source.png';source.write_bytes(b'original');snapshot=root/'private.png';snapshot.write_bytes(b'original');broker=CapturedResourceActions();key=broker.capture(source,resource=snapshot);seen=[]
   def reader(request):seen.append(Path(request['path']).read_bytes());return {'text':'recognized'}
   self.assertEqual(broker.content(key,'ocr',{'page':1},reader),{'text':'recognized'});self.assertEqual(seen,[b'original'])
   source.write_bytes(b'changed')
   with self.assertRaises(ResourceUnavailable):broker.content(key,'ocr',{},reader)
   self.assertEqual(len(seen),1)
 def test_retirement_and_provider_scope(self):
  with tempfile.TemporaryDirectory() as temporary:
   source=Path(temporary)/'file';source.write_text('text');one=CapturedResourceActions();two=CapturedResourceActions();key=one.capture(source)
   with self.assertRaises(ResourceUnavailable):two.selected(key)
   one.retire([key]);self.assertRaises(ResourceUnavailable,one.selected,key)
 def test_capture_requires_complete_expected_identity(self):
  with tempfile.TemporaryDirectory() as temporary:
   source=Path(temporary)/'file';source.write_text('text');expected=facts(source);source.write_text('new')
   self.assertRaises(ResourceUnavailable,CapturedResourceActions().capture,source,expected)
 def test_private_registered_application_and_stale_chooser(self):
  with tempfile.TemporaryDirectory() as temporary:
   root=Path(temporary);apps=root/'data/applications';apps.mkdir(parents=True);source=root/'document.txt';source.write_text('first');sink=root/'launched'
   observer=root/'observer.py';observer.write_text('import pathlib,sys;pathlib.Path(sys.argv[1]).write_text(sys.argv[2])')
   desktop=apps/'private.desktop';desktop.write_text('[Desktop Entry]\nType=Application\nName=Private Observer\nExec='+sys.executable+' '+str(observer)+' '+str(sink)+' %U\nMimeType=text/plain;\nNoDisplay=false\n')
   (apps/'mimeinfo.cache').write_text('[MIME Cache]\ntext/plain=private.desktop;\n')
   program=root/'exercise.py';program.write_text("""import sys,time
from pathlib import Path
sys.path.insert(0,sys.argv[1])
from captured_resource_actions import CapturedResourceActions,ResourceUnavailable
source=Path(sys.argv[2]);sink=Path(sys.argv[3]);desktop=Path(sys.argv[4]);broker=CapturedResourceActions();cap=broker.capture(source)
one=broker.applications(cap)
assert any(a['id']=='private.desktop' for a in one['applications'])
two=broker.applications(cap)
try:broker.dispatch(cap,'open-with','private.desktop',choice=one['choice']);raise AssertionError('stale chooser dispatched')
except ResourceUnavailable:pass
assert not sink.exists()
broker.dispatch(cap,'open-with','private.desktop',choice=two['choice'])
end=time.monotonic()+3
while not sink.exists() and time.monotonic()<end:time.sleep(.02)
assert sink.read_text() in (source.as_uri(),str(source)),sink.read_text();sink.unlink()
three=broker.applications(cap);desktop.write_text(desktop.read_text()+'Comment=registration changed\\n')
try:broker.dispatch(cap,'open-with','private.desktop',choice=three['choice']);raise AssertionError('replaced application dispatched')
except ResourceUnavailable:pass
assert not sink.exists()
broker.retire([cap])
try:broker.dispatch(cap,'open-with','private.desktop',choice=three['choice']);raise AssertionError('retired chooser dispatched')
except ResourceUnavailable:pass
""")
   launcher=os.environ.get('LUMEN_TEST_SHELL_LAUNCHER') or shutil.which('lumen-desktop-shell');self.assertIsNotNone(launcher)
   prefix=re.split(r'\n[ \t]*exec quickshell ',Path(launcher).read_text(),maxsplit=1)[0]
   installed=dict(v.decode().split('=',1) for v in subprocess.check_output(['bash','-c',prefix+'\nexec env -0']).split(b'\0') if b'=' in v)
   env=dict(os.environ,PATH=installed['PATH'],PYTHONPATH=os.pathsep.join(sys.path),HOME=temporary,XDG_DATA_HOME=str(root/'data'),XDG_DATA_DIRS=str(root/'empty-data'),XDG_CONFIG_HOME=str(root/'config'),XDG_CONFIG_DIRS=str(root/'empty-config'),XDG_CACHE_HOME=str(root/'cache'),LUMEN_LAUNCH_SCOPES='0',DBUS_SESSION_BUS_ADDRESS='unix:path='+temporary+'/no-bus')
   result=subprocess.run([os.readlink('/proc/self/exe'),str(program),str(ROOT/'res/lumen-desktop/shell'),str(source),str(sink),str(desktop)],env=env,capture_output=True,text=True,timeout=10);self.assertEqual(result.returncode,0,result.stdout+result.stderr)
 def test_structured_timeout_and_known_snapshot_mutation(self):
  from unittest.mock import patch
  import file_preview
  from preview_snapshots import PreviewSnapshots
  with tempfile.TemporaryDirectory() as temporary,patch.dict(os.environ,{'XDG_RUNTIME_DIR':temporary}):
   source=Path(temporary)/'file.pdf';source.write_bytes(b'%PDF-1.4')
   with patch.object(file_preview,'limited_command',side_effect=subprocess.TimeoutExpired(['private renderer'],8)):
    self.assertEqual(file_preview.preview(source)['reason'],'provider_timeout')
   for fail_at in (2,3):
    calls=[0]
    def identity(path,info=None):calls[0]+=1;return 'changed' if calls[0]==fail_at else 'captured'
    resources=PreviewSnapshots(identity,lambda path,token:None)
    try:
     with self.assertRaises(ResourceUnavailable) as error:resources.capture(source,'captured')
     self.assertEqual(error.exception.reason,'source_changed')
    finally:resources.close()
if __name__=='__main__':unittest.main()
