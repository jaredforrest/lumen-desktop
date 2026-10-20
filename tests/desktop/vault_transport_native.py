"""Actual stalled TLS/body cleanup, exclusively in a disposable namespace."""
import argparse,json,os,select,signal,socket,ssl,subprocess as sp,sys,time,urllib.request
from pathlib import Path
from application_fixture.isolation import run
from application_fixture.provenance import FrozenSource
from application_fixture.offscreen import immutable_file
from fixture_resources import close_reaped_pipes

p=argparse.ArgumentParser();p.add_argument('--tls-tool',type=Path,required=True);p.add_argument('--output',type=Path,required=True);p.add_argument('--inside',action='store_true');a=p.parse_args();repo=Path(__file__).resolve().parents[2];a.tls_tool=immutable_file(a.tls_tool)
if not a.inside:
 a.output=a.output.resolve();a.output.mkdir(mode=0o700,parents=True,exist_ok=False)
 frozen=FrozenSource(repo,__file__,[a.tls_tool,immutable_file(sys.executable)]);code=1
 try:code=run(repo,a.output,[str(immutable_file(sys.executable)),'/source/tests/desktop/vault_transport_native.py','--inside','--tls-tool',str(a.tls_tool),'--output','/artifacts'])
 finally:frozen.finish(a.output,code)
 raise SystemExit(code)
assert repo==Path('/source') and Path.home()==Path('/home/fixture') and not Path('/.old-root').exists()
assert [name for _,name in socket.if_nameindex()]==['lo']
os.umask(0o077);certificate=Path.home()/'certificate.pem';key=Path.home()/'key.pem'
sp.run([str(a.tls_tool),'req','-x509','-newkey','rsa:2048','-nodes','-days','1','-subj','/CN=Disposable transport','-addext','subjectAltName=IP:127.0.0.1','-keyout',str(key),'-out',str(certificate)],check=True,stdout=sp.DEVNULL,stderr=sp.PIPE,timeout=10)
context=ssl.create_default_context(cafile=str(certificate));child=Path.home()/'transport-child.py'
child.write_text('''import http.server,json,sys,threading
sys.path.insert(0,'/source/tests/desktop')
from vault_https import PrivateHttps
class Backend(http.server.BaseHTTPRequestHandler):
 def log_message(self,*args):pass
 def do_GET(self):
  payload=b'{"disposable":true}';self.send_response(200);self.send_header('Content-Length',str(len(payload)));self.end_headers();self.wfile.write(payload)
backend=http.server.ThreadingHTTPServer(('127.0.0.1',8238),Backend);backend.daemon_threads=True
worker=threading.Thread(target=backend.serve_forever,daemon=True);worker.start()
proxy=PrivateHttps(sys.argv[1],sys.argv[2],8238)
print(json.dumps(dict(ready=True)),flush=True)
try:
 for command in sys.stdin:
  if command.strip()=='close':
   proxy.close();print(json.dumps(dict(closed=True,workers=len(proxy.workers),connections=len(proxy.connections),listenerAlive=proxy.thread.is_alive())),flush=True);break
finally:
 backend.shutdown();backend.server_close();worker.join(timeout=2)
 if worker.is_alive():raise RuntimeError('Private backend did not stop')
''')
checks=[];receipts=[];processes=[]
def read(proc,seconds,label):
 if not select.select([proc.stdout],[],[],seconds)[0]:raise TimeoutError(label)
 line=proc.stdout.readline()
 if not line:raise RuntimeError('Private transport child exited before '+label)
 return json.loads(line)
def start(name):
 with (a.output/(name+'.log')).open('w')as log:proc=sp.Popen([sys.executable,str(child),str(certificate),str(key)],stdout=sp.PIPE,stdin=sp.PIPE,stderr=log,text=True,start_new_session=True)
 processes.append(proc);assert read(proc,5,'ready')==dict(ready=True);return proc
def close(proc):
 started=time.monotonic();proc.stdin.write('close\n');proc.stdin.flush();result=read(proc,4,'bounded close');assert result==dict(closed=True,workers=0,connections=0,listenerAlive=False),result
 proc.wait(timeout=3);assert proc.returncode==0;result['seconds']=time.monotonic()-started;assert result['seconds']<4;return result
try:
 proc=start('handshake')
 with socket.create_connection(('127.0.0.1',8237),timeout=2)as stalled:
  # A valid parallel request proves accept is not stuck on this raw TCP peer.
  with urllib.request.urlopen('https://127.0.0.1:8237/positive',context=context,timeout=3)as response:
   assert json.loads(response.read())==dict(disposable=True) and response.headers['Strict-Transport-Security']=='max-age=31536000'
  receipts.append(dict(case='stalled TLS handshake',close=close(proc)))
 checks.append('Raw TCP without TLS cannot block verified parallel HTTPS or bounded listener/worker shutdown')
 proc=start('body-timeout')
 with context.wrap_socket(socket.create_connection(('127.0.0.1',8237),timeout=2),server_hostname='127.0.0.1')as stalled:
  stalled.sendall(b'POST /body HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Length: 64\r\nConnection: close\r\n\r\nx');stalled.settimeout(3);started=time.monotonic();assert stalled.recv(1)==b'';elapsed=time.monotonic()-started;assert elapsed<3
  receipts.append(dict(case='incomplete body expires',seconds=elapsed,close=close(proc)))
 checks.append('Server-verified TLS connection with incomplete body expires through actual socket read timeout')
 proc=start('body-close')
 with context.wrap_socket(socket.create_connection(('127.0.0.1',8237),timeout=2),server_hostname='127.0.0.1')as stalled:
  stalled.sendall(b'POST /body HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Length: 64\r\n\r\nx');receipts.append(dict(case='incomplete body interrupted',close=close(proc)))
 checks.append('Explicit close interrupts incomplete TLS body and joins all owned workers')
 (a.output/'result.json').write_text(json.dumps(dict(passed=True,checks=checks,receipts=receipts,scope='Private loopback HTTP backend and verified TLS front end, no real vault/account/browser or host listener'),indent=2)+'\n')
finally:
 for proc in processes:
  if proc.poll()is None:
   os.killpg(proc.pid,signal.SIGTERM)
   try:proc.wait(timeout=3)
   except sp.TimeoutExpired:os.killpg(proc.pid,signal.SIGKILL);proc.wait(timeout=3)
 close_reaped_pipes(processes)
 warnings={p.name:p.read_text()for p in a.output.glob('*.log')if p.read_text().strip()}
 (a.output/'warning-scan.json').write_text(json.dumps(warnings,indent=2)+'\n')
 assert not warnings,warnings
print(json.dumps(dict(passed=True,checks=len(checks))))
