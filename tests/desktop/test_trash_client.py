"""Actual private service/client confirmation and durable restart protocol."""
import json,os,select,subprocess,sys,tempfile,time,unittest
from pathlib import Path
from urllib.parse import quote
ROOT=Path(__file__).resolve().parents[2]
SHELL=ROOT/'res/lumen-desktop/shell'
sys.path.insert(0,str(SHELL))
import file_identity

class TrashClient(unittest.TestCase):
 def test_confirmed_service_outlives_client_and_restart_never_replays(self):
  with tempfile.TemporaryDirectory(prefix='lumen-trash-client-') as temporary:
   base=Path(temporary);home=base/'home';runtime=base/'runtime';data=base/'data';state=base/'state'
   for path in (home,runtime,data,state):path.mkdir(mode=0o700)
   trash=data/'Trash';trash.mkdir(mode=0o700)
   for name in ('files','info'):(trash/name).mkdir(mode=0o700)
   item=trash/'files'/'Literal # é.txt';item.write_text('disposable')
   record=trash/'info'/(item.name+'.trashinfo');record.write_text('[Trash Info]\nPath='+quote(str(home/item.name),safe='/')+'\n');record.chmod(0o600)
   env=dict(os.environ,HOME=str(home),XDG_RUNTIME_DIR=str(runtime),XDG_DATA_HOME=str(data),XDG_STATE_HOME=str(state),LUMEN_FILE_SERVICE='1',PYTHONWARNINGS='error',DBUS_SESSION_BUS_ADDRESS='unix:path='+str(runtime/'inert-session'),DBUS_SYSTEM_BUS_ADDRESS='unix:path='+str(runtime/'inert-system'))
   processes=[]
   try:
    def launch_server():
     with (base/'server.log').open('a') as log:process=subprocess.Popen([sys.executable,str(SHELL/'file_jobs.py')],env=env,stdout=log,stderr=log)
     processes.append(process);deadline=time.monotonic()+3
     while not (runtime/'lumen-file-operations.sock').exists() and time.monotonic()<deadline:
      if process.poll() is not None:self.fail((base/'server.log').read_text())
      time.sleep(.01)
     self.assertTrue((runtime/'lumen-file-operations.sock').exists());return process
    def launch_client():
     process=subprocess.Popen([sys.executable,str(SHELL/'trash_client.py')],env=env,stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True);processes.append(process);return process
    def send(client,ident,action,**values):
     client.stdin.write(json.dumps(dict(id=ident,action=action,**values))+'\n');client.stdin.flush()
     self.assertTrue(select.select([client.stdout],[],[],3)[0]);return json.loads(client.stdout.readline())
    def close_client(client):
     client.stdin.close();client.wait(timeout=3);self.assertEqual(client.returncode,0);self.assertEqual(client.stderr.read(),'');client.stdout.close();client.stderr.close()
    server=launch_server();client=launch_client()
    preparation=send(client,1,'trash-prepare',mode='selected',items=[dict(path=str(item),identity=file_identity.wire(item.lstat()))]);self.assertNotIn('error',preparation)
    self.assertEqual(preparation['count'],1);self.assertTrue(item.exists())
    result=send(client,2,'trash-confirm',token=preparation['token']);self.assertNotIn('error',result);close_client(client)
    client=launch_client();deadline=time.monotonic()+4
    while True:
     result=send(client,3,'trash-status')
     if not result['busy']:break
     self.assertLess(time.monotonic(),deadline);time.sleep(.01)
    self.assertEqual(result['operations'][-1]['state'],'complete');self.assertFalse(item.exists());close_client(client)
    server.terminate();server.wait(timeout=3)
    replacement=trash/'files'/item.name;replacement.write_text('later arrival');server=launch_server();client=launch_client()
    result=send(client,4,'trash-confirm',token=preparation['token']);self.assertEqual(result['operations'][-1]['state'],'complete');self.assertEqual(replacement.read_text(),'later arrival')
    invalid=send(client,5,'trash-confirm',token=preparation['token'],path=str(home));self.assertIn('error',invalid)
    close_client(client)
   finally:
    for process in reversed(processes):
     if process.poll() is None:process.terminate()
     process.wait(timeout=5)
     for stream in (process.stdin,process.stdout,process.stderr):
      if stream is not None and not stream.closed:stream.close()
   self.assertNotIn('Warning',(base/'server.log').read_text());self.assertNotIn('Traceback',(base/'server.log').read_text())

if __name__=='__main__':unittest.main()
