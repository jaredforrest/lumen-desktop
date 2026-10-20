"""Execute actual source watch expressions against private ready pipe/socket fds."""
import ast,os,socket,time,types,unittest,warnings
from pathlib import Path
from gi.repository import GLib

ROOT=Path(__file__).resolve().parents[2]
class WatchTests(unittest.TestCase):
 def test_source_watch_signatures_dispatch_without_deprecated_overload(self):
  calls=[]
  for source in (ROOT/'res').rglob('*.py'):
   for node in ast.walk(ast.parse(source.read_text())):
    if isinstance(node,ast.Call) and isinstance(node.func,ast.Attribute) and node.func.attr=='io_add_watch':calls.append((source,node))
  self.assertTrue(calls)
  for source,node in calls:
   with self.subTest(source=str(source.relative_to(ROOT)),line=node.lineno):
    self.assertGreaterEqual(len(node.args),4,'Explicit priority must be the second argument')
    self.assertIsInstance(node.args[1],ast.Attribute);self.assertEqual(node.args[1].attr,'PRIORITY_DEFAULT')
    reader,writer=socket.socketpair();file=reader.makefile('rb',buffering=0);observed=[];event=None
    def ready(channel,condition,*data):
     observed.append((channel,condition,data));self.assertEqual(os.read(reader.fileno(),1),b'x');return False
    fake=types.SimpleNamespace(glib=GLib,_events=ready,input=ready,focus_event=ready,events=types.SimpleNamespace(stdout=file))
    context=dict(GLib=GLib,self=fake,sys=types.SimpleNamespace(stdin=file),connection=reader,server=reader,readable=ready,input=ready,read=ready,command=ready,receive=ready)
    try:
     with warnings.catch_warnings():
      warnings.simplefilter('error')
      event=eval(compile(ast.Expression(node),str(source),'eval'),context)
      writer.sendall(b'x');deadline=time.monotonic()+1
      while not observed and time.monotonic()<deadline:
       GLib.MainContext.default().iteration(False);time.sleep(.001)
      self.assertEqual(len(observed),1,'Native source readiness must dispatch exactly once')
      expected=eval(compile(ast.Expression(node.args[0]),str(source),'eval'),context)
      if isinstance(expected,int):self.assertEqual(observed[0][0],expected)
      else:self.assertIs(observed[0][0],expected)
      self.assertTrue(observed[0][1]&GLib.IO_IN)
      if len(node.args)>4:self.assertEqual(observed[0][2],(reader,))
    finally:
     if event and not observed:GLib.source_remove(event)
     file.close();reader.close();writer.close()

if __name__=='__main__':unittest.main()
