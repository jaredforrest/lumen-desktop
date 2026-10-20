"""Bounded pipe-only browser diagnostic transport; no listening endpoint."""
import json,os,select,signal,subprocess,sys,time
from pathlib import Path
MAX_FRAME_BYTES=8*1024**2
MAX_QUEUED_EVENTS=2000
MAX_EVENT_BYTES=16*1024**2

class PipeTransport:
    def __init__(self,command,environment,log):
        incoming,self.write=os.pipe2(os.O_CLOEXEC);self.read,outgoing=os.pipe2(os.O_CLOEXEC)
        self.sequence=0;self.buffer=b'';self.events=[];self.event_bytes=0;self.closed=False
        try:
            self.process=subprocess.Popen([sys.executable,str(Path(__file__).with_name('pipe_child.py')),str(incoming),str(outgoing),*map(str,command)],env=environment,pass_fds=(incoming,outgoing),stdout=log,stderr=subprocess.STDOUT,start_new_session=True)
        except BaseException:
            os.close(self.read);os.close(self.write);raise
        finally:os.close(incoming);os.close(outgoing)
        os.set_blocking(self.read,False);os.set_blocking(self.write,False)

    def _send(self,value,deadline):
        payload=json.dumps(value).encode()+b'\0'
        if len(payload)>MAX_FRAME_BYTES:raise ValueError('Diagnostic request exceeds frame budget')
        while payload:
            if not select.select([],[self.write],[],max(0,deadline-time.monotonic()))[1]:raise TimeoutError('Diagnostic command pipe did not become writable')
            try:payload=payload[os.write(self.write,payload):]
            except BlockingIOError:pass

    def _receive(self,deadline):
        while b'\0' not in self.buffer:
            if not select.select([self.read],[],[],max(0,deadline-time.monotonic()))[0]:raise TimeoutError('Diagnostic response deadline reached')
            chunk=os.read(self.read,65536)
            if not chunk:raise EOFError('Application diagnostic pipe closed')
            self.buffer+=chunk
            if len(self.buffer)>MAX_FRAME_BYTES:raise ValueError('Diagnostic response exceeds frame budget')
        frame,self.buffer=self.buffer.split(b'\0',1)
        value=json.loads(frame)
        if not isinstance(value,dict):raise ValueError('Diagnostic response is not an object')
        return value

    def request(self,method,params=None,session=None,timeout=5):
        self.sequence+=1;ident=self.sequence;deadline=time.monotonic()+timeout
        value=dict(id=ident,method=method,params={} if params is None else params)
        if session is not None:value['sessionId']=session
        self._send(value,deadline)
        while True:
            response=self._receive(deadline)
            if 'id' not in response:
                self.event_bytes+=len(json.dumps(response).encode())
                if len(self.events)>=MAX_QUEUED_EVENTS or self.event_bytes>MAX_EVENT_BYTES:raise ValueError('Diagnostic event queue exceeds its budget')
                self.events.append(response);continue
            if response['id']!=ident:raise ValueError('Unexpected diagnostic response identity')
            if 'error' in response:raise RuntimeError(str(response['error']))
            return response.get('result',{})

    def close(self):
        if self.closed:return
        self.closed=True
        os.close(self.write);os.close(self.read)
        if self.process.poll() is None:
            try:self.process.wait(timeout=3)
            except subprocess.TimeoutExpired:
                os.killpg(self.process.pid,signal.SIGTERM)
                try:self.process.wait(timeout=3)
                except subprocess.TimeoutExpired:os.killpg(self.process.pid,signal.SIGKILL);self.process.wait(timeout=3)
        else:self.process.wait()

    def __enter__(self):return self
    def __exit__(self,*_):self.close()
