"""Synthetic loopback pages and gated downloads; no upstream fetching."""
import hashlib,http.server,json,threading,time
from urllib.parse import urlsplit
PAYLOAD=(b'Local disposable download.\n'*8192)*8
DIGEST=hashlib.sha256(PAYLOAD).hexdigest()
PAGE=b'''<!doctype html><meta charset="utf-8"><title>Fixture browser workflow</title>
<style>body{font:20px sans-serif;padding:32px}button,a{display:block;margin:20px;padding:10px}</style>
<h1>Disposable local page</h1><p id="sample">Selectable fixture text: 12:34, punctuation &amp; accented caf\xc3\xa9.</p>
<a id="fixed" href="/download/fixed">Download gated fixture</a><a id="cancel" href="/download/cancel">Download cancellable fixture</a>
<button id="set-marker" onclick="const marker=new URLSearchParams(location.search).get('marker')||'normal';document.cookie='fixture_marker='+marker+'; path=/; SameSite=Lax';localStorage.setItem('fixture_marker',marker);document.querySelector('#status').textContent='Storage marker: '+marker">Set disposable storage marker</button>
<button id="permission" onclick="Notification.requestPermission().then(value=>document.querySelector('#status').textContent=value)">Request notification permission</button>
<button id="fullscreen" onclick="document.documentElement.requestFullscreen()">Enter page fullscreen</button><p id="status">No permission requested</p>'''

class FixtureServer:
    def __init__(self):
        self.gates={name:threading.Event() for name in ('fixed','cancel')};self.lock=threading.Lock();self.requests=[];self.stopping=threading.Event();owner=self
        class Handler(http.server.BaseHTTPRequestHandler):
            def log_message(self,*_):pass
            def do_GET(self):
                if self.headers.get('Host')!='127.0.0.1:'+str(self.server.server_port):self.send_error(400,'Synthetic loopback origin required');return
                path=urlsplit(self.path).path
                if path in ('/','/page'):
                    self.send_response(200);self.send_header('Content-Type','text/html; charset=utf-8');self.send_header('Content-Length',str(len(PAGE)));self.end_headers();self.wfile.write(PAGE);return
                if path not in ('/download/fixed','/download/cancel'):self.send_error(404);return
                name=path.rsplit('/',1)[-1];record=dict(name=name,sent=0,complete=False,disconnected=False)
                with owner.lock:owner.requests.append(record)
                self.send_response(200);self.send_header('Content-Type','application/octet-stream');self.send_header('Content-Disposition','attachment; filename="fixture-'+name+'.bin"');self.send_header('Content-Length',str(len(PAYLOAD)));self.end_headers()
                try:
                    first=PAYLOAD[:65536];self.wfile.write(first);self.wfile.flush();record['sent']=len(first)
                    while not owner.gates[name].wait(.05):
                        if owner.stopping.is_set():return
                    for start in range(len(first),len(PAYLOAD),65536):
                        if owner.stopping.is_set():return
                        chunk=PAYLOAD[start:start+65536];self.wfile.write(chunk);self.wfile.flush();record['sent']+=len(chunk)
                        if name=='cancel':time.sleep(.01)
                    record['complete']=True
                except (BrokenPipeError,ConnectionResetError):record['disconnected']=True
        self.server=http.server.ThreadingHTTPServer(('127.0.0.1',0),Handler);self.server.daemon_threads=False;self.worker=threading.Thread(target=self.server.serve_forever);self.worker.start()
    @property
    def origin(self):return 'http://127.0.0.1:'+str(self.server.server_port)
    def snapshot(self):
        with self.lock:return json.loads(json.dumps(self.requests))
    def close(self):
        self.stopping.set()
        for gate in self.gates.values():gate.set()
        self.server.shutdown();self.server.server_close();self.worker.join(timeout=3)
        if self.worker.is_alive():raise RuntimeError('Synthetic server did not stop')
    def __enter__(self):return self
    def __exit__(self,*_):self.close()
