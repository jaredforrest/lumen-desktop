"""Real pointer, keyboard and session-lock protocol on a disposable compositor."""
import argparse,json,os,select,socket,struct,subprocess as sp,tempfile,time
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--sway',required=True);p.add_argument('--pointer',required=True);p.add_argument('--keyboard',required=True);p.add_argument('--lock',required=True);p.add_argument('--output',type=Path,required=True);p.add_argument('--effects',action='store_true');a=p.parse_args()
a.output.mkdir(parents=True,exist_ok=False);processes=[];events=[]
def wait(fn):
 end=time.monotonic()+10
 while time.monotonic()<end:
  value=fn()
  if value:return value
  time.sleep(.03)
 raise AssertionError('Timed out waiting for private compositor')
with tempfile.TemporaryDirectory(prefix='lumen-corners-') as private:
 env=dict(os.environ,HOME=private,XDG_RUNTIME_DIR=private,WLR_BACKENDS='headless',WLR_HEADLESS_OUTPUTS='2',WLR_RENDERER='gles2' if a.effects else 'pixman',WLR_RENDERER_ALLOW_SOFTWARE='1',WLR_LIBINPUT_NO_DEVICES='1',LIBGL_ALWAYS_SOFTWARE='1',QT_ACCESSIBILITY='0',QT_LINUX_ACCESSIBILITY_ALWAYS_ON='0',NO_AT_BRIDGE='1',GTK_A11Y='none')
 for key in ('DISPLAY','WAYLAND_DISPLAY','SWAYSOCK','AT_SPI_BUS_ADDRESS','DBUS_SESSION_BUS_ADDRESS'):env.pop(key,None)
 for key in ('CONFIG','STATE','DATA','CACHE'):env['XDG_'+key+'_HOME']=str(Path(private)/key.lower())
 def launch(args,name,**kwargs):
  proc=sp.Popen(args,env=env,stdout=(a.output/name).open('w'),stderr=sp.STDOUT,start_new_session=True,**kwargs);processes.append(proc);return proc
 def ipc(command):return json.loads(sp.check_output(['swaymsg','-r',command],env=env,text=True))
 bus_config=a.output/'bus.conf'
 bus_config.write_text('<busconfig><type>session</type><listen>unix:tmpdir='+private+'</listen><policy context="default"><allow own="*"/><allow send_destination="*"/><allow receive_sender="*"/></policy></busconfig>')
 bus_log=(a.output/'bus.log').open('w')
 bus=sp.Popen(['dbus-daemon','--nofork','--print-address=1','--config-file='+str(bus_config)],env=env,stdout=sp.PIPE,stderr=bus_log,text=True,start_new_session=True)
 processes.append(bus)
 assert select.select([bus.stdout],[],[],5)[0], 'Private bus failed to start'
 env['DBUS_SESSION_BUS_ADDRESS']=bus.stdout.readline().strip()
 assert env['DBUS_SESSION_BUS_ADDRESS'].startswith('unix:')
 config=a.output/'sway.conf';config.write_text('output HEADLESS-1 resolution 1280x720 position 0 0\noutput HEADLESS-2 resolution 1280x720 position 1280 0\nseat seat0 fallback true\n')
 try:
  launch([a.sway,'--unsupported-gpu','-c',str(config)],'sway.log')
  env['SWAYSOCK']=str(wait(lambda:next(Path(private).glob('sway-ipc.*.sock'),None)));env['WAYLAND_DISPLAY']=wait(lambda:next((p.name for p in Path(private).glob('wayland-*') if not p.name.endswith('.lock')),None))
  version=json.loads(sp.check_output(['swaymsg','-r','-t','get_version'],env=env,text=True));assert version['native_hot_corners'] is True
  subscriber=socket.socket(socket.AF_UNIX);subscriber.connect(env['SWAYSOCK']);payload=b'["tick"]';subscriber.sendall(b'i3-ipc'+struct.pack('II',len(payload),2)+payload);buffer=b''
  def collect(duration=.15):
   global buffer
   end=time.monotonic()+duration
   while time.monotonic()<end:
    if not select.select([subscriber],[],[],max(0,end-time.monotonic()))[0]:break
    buffer+=subscriber.recv(65536)
    while len(buffer)>=14:
     assert buffer[:6]==b'i3-ipc';length,kind=struct.unpack('II',buffer[6:14])
     if len(buffer)<14+length:break
     message=json.loads(buffer[14:14+length]);buffer=buffer[14+length:]
     if kind==(1<<31)|7:
      try:event=json.loads(message['payload'])
      except (ValueError,KeyError):continue
      if event.get('kind')=='lumen-corner':events.append(event)
   return events
  collect()
  pointer=launch([a.pointer,'pointer','move','0','0'],'pointer.log',stdin=sp.PIPE,text=True)
  time.sleep(.2)
  def motion(x,y):pointer.stdin.write(f'm {x} {y}\n');pointer.stdin.flush();collect()
  def button(value):pointer.stdin.write(value+'\n');pointer.stdin.flush();time.sleep(.15)
  def warp(x,y):ipc(f'seat seat0 cursor set {x} {y}');collect()
  def enter(x,y,corner,**fields):
   before=len(events);warp(x+.25,y+.25);assert len(events)==before,"Warp cannot activate or leave a corner";motion(-.25,-.25);assert len(events)==before+1,(corner,events[before:]);event=events[-1];assert event['corner']==corner,event
   for key,value in fields.items():assert event[key]==value,event
   return event
  warp(100,100);assert not events,'Warps must not generate corner events'
  enter(0,0,'top-left',locked=False,fullscreen=False,adjacent=False)
  before=len(events);motion(5,5);motion(-5,-5);assert len(events)==before,'Hysteresis should latch'
  motion(10,10);assert events[-1]['corner'] is None
  enter(1279.5,0,'top-right',adjacent=True)
  motion(-20,20);assert events[-1]['corner'] is None
  enter(0,719.5,'bottom-left',adjacent=False,output='HEADLESS-1');motion(20,-20)
  enter(1279.5,719.5,'bottom-right',adjacent=True,output='HEADLESS-1');motion(-20,-20)
  enter(2559.5,0,'top-right',adjacent=False,output='HEADLESS-2');motion(-20,20)
  # Drag entry occupies the corner but cannot dispatch an action after release.
  warp(20,20);button('p');before=len(events);warp(.25,.25);motion(-.25,-.25);assert len(events)==before+1 and events[-1]['corner'] is None
  button('r');before=len(events);motion(.1,.1);assert len(events)==before;motion(20,20)
  # Modifiers are sampled at entry, not a keyboard-triggered activation.
  keyboard=launch([a.keyboard,'-M','shift','-s','1200','-m','shift'],'keyboard.log');time.sleep(.25)
  enter(0,0,'top-left',modifiers=['shift']);before=len(events);keyboard.wait(timeout=3);collect();assert len(events)==before
  motion(20,20)
  # Fullscreen is an inhibition fact, with policy left to the backend.
  foot=launch(['foot','--app-id=corner-test','sleep','120'],'foot.log');time.sleep(.5);ipc('[app_id="corner-test"] fullscreen enable');time.sleep(.2)
  enter(0,0,'top-left',fullscreen=True);motion(20,20);ipc('[app_id="corner-test"] fullscreen disable')
  enter(0,0,'top-left',fullscreen=False)
  locker=launch([a.lock],'lock.log',stdin=sp.PIPE,text=True);time.sleep(.3);collect();assert events[-1]['corner'] is None and events[-1]['locked'] is True,events[-1]
  locker.stdin.write('u\n');locker.stdin.flush();locker.wait(timeout=3);collect();assert events[-1]['corner'] is None and events[-1]['locked'] is False
  before=len(events);motion(.1,.1);assert len(events)==before,'Unlock in occupied corner cannot reactivate'
  motion(20,20);enter(0,0,'top-left',locked=False)
  motion(20,20)
  locker=launch([a.lock],'locked-entry.log',stdin=sp.PIPE,text=True);time.sleep(.3);collect()
  enter(0,0,None,locked=True)
  locker.stdin.write('u\n');locker.stdin.flush();locker.wait(timeout=3);collect()
  before=len(events);motion(.1,.1);assert len(events)==before,'Suppressed locked entry must remain latched'
  motion(20,20);enter(0,0,'top-left',locked=False)

  for event in events:
   assert set(event)=={'kind','version','corner','output','modifiers','locked','fullscreen','adjacent'},event
   assert event['version']==1 and event['output'] and all(type(event[k]) is bool for k in ('locked','fullscreen','adjacent'))
  (a.output/'result.json').write_text(json.dumps({'passed':True,'events':events,'version':version},indent=2));print('PASS native corner pointer/modifier/drag/fullscreen/adjacency/lock lifecycle')
 finally:
  for proc in reversed(processes):
   if proc.poll() is None:proc.terminate()
  for proc in reversed(processes):
   try:proc.wait(timeout=3)
   except sp.TimeoutExpired:proc.kill();proc.wait()
