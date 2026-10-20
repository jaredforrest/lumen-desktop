"""Bounded standard-input client for an owned installed office control bridge."""
import json,os,select,subprocess,time
from fixture_resources import close_reaped_pipes

class ControlError(RuntimeError):pass
class Control:
 def __init__(self,command,log,environment=None):
  with log.open('w') as stream:self.process=subprocess.Popen(command,env=environment,stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=stream)
  self.buffer=b'';self.serial=0;self.frames=[]
 def request(self,method,params=None,timeout=10):
  self.serial+=1;received=0;request=dict(jsonrpc='2.0',id=self.serial,method=method,params={} if params is None else params)
  self.process.stdin.write(json.dumps(request,separators=(',',':')).encode()+b'\n');self.process.stdin.flush();deadline=time.monotonic()+timeout
  while time.monotonic()<deadline:
   while b'\n' in self.buffer:
    line,self.buffer=self.buffer.split(b'\n',1)
    try:reply=json.loads(line)
    except ValueError:raise ControlError('Control bridge emitted malformed JSON')
    if not isinstance(reply,dict) or reply.get('jsonrpc')!='2.0':raise ControlError('Control bridge emitted an invalid envelope')
    self.frames.append(reply)
    if len(self.frames)>2048:raise ControlError('Control metadata frame budget exceeded')
    if type(reply.get('id')) is int and reply['id']==self.serial:
     if 'error' in reply:raise ControlError('Control request failed: '+method)
     return reply.get('result')
   if not select.select([self.process.stdout],[],[],max(0,deadline-time.monotonic()))[0]:break
   chunk=os.read(self.process.stdout.fileno(),65536)
   if not chunk:raise ControlError('Owned control bridge exited before acknowledging '+method)
   received+=len(chunk)
   if received>8388608:raise ControlError('Control request metadata budget exceeded')
   self.buffer+=chunk
   if len(self.buffer)>8388608:raise ControlError('Control response exceeded fixture metadata budget')
  raise ControlError('Owned control request timed out: '+method)
 def initialize(self):
  result=self.request('initialize',dict(protocolVersion='2024-11-05',capabilities={},clientInfo=dict(name='Fixture',version='1')))
  self.process.stdin.write(b'{"jsonrpc":"2.0","method":"notifications/initialized"}\n');self.process.stdin.flush();return result
 def tool(self,name,arguments=None):
  result=self.request('tools/call',dict(name=name,arguments={} if arguments is None else arguments))
  if not isinstance(result,dict) or result.get('isError'):raise ControlError('Owned application tool failed: '+name)
  content=result.get('content',[]);text='\n'.join(part.get('text','') for part in content if part.get('type')=='text')
  try:return json.loads(text)
  except ValueError:return text
 def command(self,app,name,params=None):
  tool={'writer':'execute','sheet':'execute_command','slides':'run_command'}[app]
  return self.tool(tool,dict(command=name,params={} if params is None else params))
 def close(self):
  if self.process.poll() is None:self.process.terminate()
  try:self.process.wait(timeout=5)
  except subprocess.TimeoutExpired:self.process.kill();self.process.wait(timeout=5)
  close_reaped_pipes([self.process])
