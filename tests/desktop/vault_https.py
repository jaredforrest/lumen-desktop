"""Bounded HTTPS front end for a loopback-only disposable compatibility server."""
import http.client,http.server,socket,ssl,threading,time

HOP={'connection','keep-alive','proxy-authenticate','proxy-authorization','te','trailer','transfer-encoding','upgrade','content-length'}
MAX_BODY=4*1024*1024
MAX_RESPONSE=8*1024*1024
READ_TIMEOUT=2
CLOSE_TIMEOUT=3

class PrivateHttps:
    def __init__(self,certificate,key,backend_port):
        if backend_port!=8238:raise ValueError('Fixed disposable backend required')
        self.requests=0;owner=self
        self.lock=threading.Lock();self.connections=set();self.workers=set()
        class Server(http.server.ThreadingHTTPServer):
            def process_request(self,request,address):
                # Register before starting, so close cannot miss a new worker.
                worker=threading.Thread(target=self.process_request_thread,args=(request,address),daemon=True,name="private-vault-request")
                with owner.lock:
                    owner.workers={active for active in owner.workers if active.is_alive()}
                    owner.connections.add(request);owner.workers.add(worker)
                try:worker.start()
                except BaseException:
                    with owner.lock:owner.connections.discard(request);owner.workers.discard(worker)
                    request.close();raise
            def process_request_thread(self,request,address):
                try:super().process_request_thread(request,address)
                finally:
                    with owner.lock:owner.connections.discard(request)
        class Handler(http.server.BaseHTTPRequestHandler):
            protocol_version='HTTP/1.1'
            def setup(self):
                # TLS handshake happens on first worker I/O, never in accept().
                self.request.settimeout(READ_TIMEOUT)
                super().setup()
            def handle(self):
                try:super().handle()
                except (ConnectionResetError,BrokenPipeError,ssl.SSLEOFError):
                    # Expected peer disconnect or our own close() interruption.
                    return
            def log_message(self,*args):pass # Request URLs can contain disposable registration tokens.
            def forward(self):
                self.close_connection=True
                if not self.path.startswith('/')or self.headers.get('Transfer-Encoding'):
                    self.send_error(400);return
                try:length=int(self.headers.get('Content-Length','0'))
                except ValueError:self.send_error(400);return
                if not 0<=length<=MAX_BODY:self.send_error(413);return
                data=self.rfile.read(length)
                if len(data)!=length:self.send_error(400);return
                connection=http.client.HTTPConnection('127.0.0.1',backend_port,timeout=10)
                try:
                    headers={k:v for k,v in self.headers.items()if k.lower()not in HOP}
                    connection.request(self.command,self.path,body=data if length else None,headers=headers)
                    response=connection.getresponse();payload=response.read(MAX_RESPONSE+1)
                    if len(payload)>MAX_RESPONSE:raise ValueError('Disposable response exceeds budget')
                    self.send_response(response.status)
                    for name,value in response.getheaders():
                        if name.lower()not in HOP|{'strict-transport-security'}:self.send_header(name,value)
                    self.send_header('Strict-Transport-Security','max-age=31536000')
                    self.send_header('Content-Length',str(len(payload)));self.send_header('Connection','close');self.end_headers()
                    if self.command!='HEAD':self.wfile.write(payload)
                    owner.requests+=1
                except (OSError,ValueError,http.client.HTTPException):
                    self.send_error(502,'Disposable backend unavailable')
                finally:connection.close()
            do_GET=do_POST=do_PUT=do_PATCH=do_DELETE=do_OPTIONS=do_HEAD=forward
        self.server=Server(('127.0.0.1',8237),Handler)
        self.server.daemon_threads=True;self.server.block_on_close=False
        context=ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER);context.minimum_version=ssl.TLSVersion.TLSv1_2
        context.load_cert_chain(certificate,key);self.server.socket=context.wrap_socket(self.server.socket,server_side=True,do_handshake_on_connect=False)
        self.thread=threading.Thread(target=self.server.serve_forever,name='private-vault-https',daemon=True)
        self.thread.start()
    def close(self):
        self.server.shutdown();self.server.server_close();self.thread.join(timeout=3)
        if self.thread.is_alive():raise RuntimeError('Disposable HTTPS front end did not stop')
        with self.lock:connections=list(self.connections);workers=list(self.workers)
        for connection in connections:
            try:connection.shutdown(socket.SHUT_RDWR)
            except OSError as error:
                import errno
                if error.errno not in (errno.ENOTCONN,errno.EBADF):raise
        deadline=time.monotonic()+CLOSE_TIMEOUT
        for worker in workers:worker.join(timeout=max(0,deadline-time.monotonic()))
        if any(worker.is_alive()for worker in workers):raise RuntimeError('Disposable HTTPS request workers did not stop')
        with self.lock:self.workers.difference_update(workers)
