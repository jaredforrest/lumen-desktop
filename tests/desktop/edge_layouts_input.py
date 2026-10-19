"""Physical drag previews and placement on a private two-output compositor."""
import argparse,json,os,select,signal,socket,struct,subprocess as sp,tempfile,time
from pathlib import Path
from private_native_bus import start_private_bus
p=argparse.ArgumentParser();p.add_argument('--sway',required=True);p.add_argument('--pointer',required=True);p.add_argument('--keyboard',required=True);p.add_argument('--lock',required=True);p.add_argument('--output',type=Path,required=True);p.add_argument('--effects',action='store_true');p.add_argument('--quickshell');p.add_argument('--constraint-client');p.add_argument('--constraints-only',action='store_true');a=p.parse_args();a.output.mkdir(parents=True,exist_ok=False);processes=[];events=[]
def wait(fn):
 end=time.monotonic()+10
 while time.monotonic()<end:
  value=fn()
  if value:return value
  time.sleep(.03)
 raise AssertionError('Timed out')
with tempfile.TemporaryDirectory(prefix='lumen-edge-') as private:
 env=dict(os.environ,HOME=private,XDG_RUNTIME_DIR=private,WLR_BACKENDS='headless',WLR_HEADLESS_OUTPUTS='2',WLR_RENDERER='gles2' if a.effects else 'pixman',WLR_RENDERER_FORCE_SOFTWARE='1' if a.effects else '0',WLR_RENDERER_ALLOW_SOFTWARE='1',WLR_LIBINPUT_NO_DEVICES='1',LIBGL_ALWAYS_SOFTWARE='1',QT_QPA_PLATFORM='wayland',QT_QUICK_BACKEND='software',QT_QPA_PLATFORMTHEME='',QT_ACCESSIBILITY='0',QT_LINUX_ACCESSIBILITY_ALWAYS_ON='0',NO_AT_BRIDGE='1',GTK_A11Y='none')
 for key in ('DISPLAY','WAYLAND_DISPLAY','SWAYSOCK','AT_SPI_BUS_ADDRESS','DBUS_SESSION_BUS_ADDRESS'):env.pop(key,None)
 for key in ('CONFIG','STATE','DATA','CACHE'):env['XDG_'+key+'_HOME']=str(Path(private)/key.lower())
 def launch(args,name,**kwargs):
  proc=sp.Popen(args,env=env,stdout=(a.output/name).open('w'),stderr=sp.STDOUT,start_new_session=True,**kwargs);processes.append(proc);return proc
 def sway(*args):return json.loads(sp.check_output(['swaymsg','-r',*args],env=env,text=True))
 def ipc(command):
  result=sway(command);assert all(x.get('success') for x in result),result;return result
 def nodes(node):
  yield node
  for child in node.get('nodes',[])+node.get('floating_nodes',[]):yield from nodes(child)
 subject_app='edge-test'
 def window():return next((x for x in nodes(sway('-t','get_tree')) if x.get('app_id')==subject_app),None)
 config=a.output/'sway.conf';config.write_text('output HEADLESS-1 resolution 1280x720 position 0 0\noutput HEADLESS-2 resolution 1000x800 position 1280 0\nseat seat0 fallback true\nfloating_modifier Mod4 normal\ndefault_border normal 2\n')
 try:
  start_private_bus(env,a.output,processes)
  launch([a.sway,'--unsupported-gpu','-c',str(config)],'sway.log');env['SWAYSOCK']=str(wait(lambda:next(Path(private).glob('sway-ipc.*.sock'),None)));env['WAYLAND_DISPLAY']=wait(lambda:next((p.name for p in Path(private).glob('wayland-*') if not p.name.endswith('.lock')),None));assert sway('-t','get_version')['native_edge_tiling']
  if a.quickshell:
   fixture=a.output/'bar.qml';fixture.write_text('import QtQuick\nimport Quickshell\nimport Quickshell.Wayland\nShellRoot {Variants {model:Quickshell.screens;PanelWindow {required property var modelData;screen:modelData;anchors {top:true;left:true;right:true}implicitHeight:40;exclusiveZone:40;color:"#242424";WlrLayershell.layer:WlrLayer.Top}}}')
   launch([a.quickshell,'-p',str(fixture)],'bar.log');time.sleep(.5)
  subscriber=socket.socket(socket.AF_UNIX);subscriber.connect(env['SWAYSOCK']);payload=b'["tick"]';subscriber.sendall(b'i3-ipc'+struct.pack('II',len(payload),2)+payload);buffer=b''
  def collect(duration=.12):
   global buffer
   end=time.monotonic()+duration
   while time.monotonic()<end:
    if not select.select([subscriber],[],[],max(0,end-time.monotonic()))[0]:break
    buffer+=subscriber.recv(65536)
    while len(buffer)>=14:
     length,kind=struct.unpack('II',buffer[6:14])
     if len(buffer)<14+length:break
     message=json.loads(buffer[14:14+length]);buffer=buffer[14+length:]
     if kind==(1<<31)|7:
      try:event=json.loads(message['payload'])
      except (ValueError,KeyError):continue
      if event.get('lumen_drag_layout') or event.get('lumen_drag_overview'):events.append(event)
   return events
  pointer=launch([a.pointer,'pointer','move','0','0'],'pointer.log',stdin=sp.PIPE,text=True);time.sleep(.15)
  def motion(dx,dy):pointer.stdin.write(f'm {dx} {dy}\n');pointer.stdin.flush();collect()
  def button(value):pointer.stdin.write(value+'\n');pointer.stdin.flush();collect(.16)
  def at(x,y):ipc(f'seat seat0 cursor set {x} {y}')
  launch(['foot','-o','resize-by-cells=no','--app-id=edge-test','sleep','180'],'foot.log');wait(window);cid=window()['id']
  def outer():
   node=window();box=dict(node['rect']);box['y']-=node['deco_rect']['height'];box['height']+=node['deco_rect']['height'];return box
  def reset():
   ipc(f'[con_id={cid}] floating enable, resize set 400 px 300 px, move absolute position 200 px 200 px');time.sleep(.15);return outer()
  def start():
   box=reset();(a.output/'window-before.json').write_text(json.dumps(window(),indent=2));sp.run(['grim','-o','HEADLESS-1',str(a.output/'before.png')],env=env,check=True);at(box['x']+150,box['y']+10);button('p');motion(1,1);return box
  def to(x,y):
   at(x+.2,y+.2);motion(-.2,-.2)
  def phases(start):return [x.get('phase') for x in events[start:]]
  def release():button('r');time.sleep(.15)
  if not a.constraints_only:
   original=start();begin=len(events);to(0,360);collect(.32);assert 'preview' in phases(begin),events[begin:]
   preview=next(x for x in events[begin:] if x.get('phase')=='preview');assert preview['id']==cid and preview['target']==1
   if a.quickshell:assert preview['y']>=40 and preview['height']<=680,preview
   # Native preview is visible while the client retains its ordinary dimensions.
   before=outer();assert before['width']==original['width']
   sp.run(['grim','-o','HEADLESS-1',str(a.output/'preview.png')],env=env,check=True)
   to(100,360);assert 'leave' in phases(begin);release();assert 'commit' not in phases(begin)
   # Valid release commits only the captured window and preserves pre-drag restore.
   original=start();begin=len(events);to(0,360);collect(.30);release();assert 'commit' in phases(begin)
   placed=outer();assert placed=={k:preview[k] for k in ('x','y','width','height')},(placed,preview)
   ipc(f'[con_id={cid}] lumen_window restore');time.sleep(.15);assert outer()==original,(outer(),original)
   # Every corner and side captures the dragged ID, leaving another client untouched.
   launch(['foot','-o','resize-by-cells=no','--app-id=edge-bystander','sleep','180'],'bystander.log')
   other=wait(lambda:next((x for x in nodes(sway('-t','get_tree')) if x.get('app_id')=='edge-bystander'),None))
   other_id=other['id'];ipc(f'[con_id={other_id}] floating enable, resize set 240 px 180 px, move absolute position 800 px 400 px')
   time.sleep(.2)
   def bystander():return next(x['rect'] for x in nodes(sway('-t','get_tree')) if x['id']==other_id)
   other_box=bystander()
   for x,y,target in [(1279.5,360,2),(0,0,3),(1279.5,0,4),(0,719.5,5),(1279.5,719.5,6)]:
    original=start();begin=len(events);to(x,y);collect(.32)
    candidate=next(e for e in events[begin:] if e.get('phase')=='preview')
    assert candidate['id']==cid and candidate['target']==target,candidate
    to(600,360);assert 'leave' in phases(begin);release();assert 'commit' not in phases(begin)
    start();begin=len(events);to(x,y);collect(.32);release()
    assert outer()=={k:candidate[k] for k in ('x','y','width','height')}
    assert bystander()==other_box
    ipc(f'[con_id={cid}] lumen_window restore');time.sleep(.2);assert outer()==original
   # Escape cancels without a late commit on the subsequent button release.
   original=start();begin=len(events);to(0,360);collect(.30);sp.run([a.keyboard,'-k','Escape'],env=env,check=True);collect();release();assert 'cancel' in phases(begin) and 'commit' not in phases(begin);assert outer()==original
   # Disabling edge tiling removes both preview and commit.
   ipc('lumen_window edge-tiling disable');start();begin=len(events);to(0,360);collect(.35);release();assert 'preview' not in phases(begin) and 'commit' not in phases(begin);ipc('lumen_window edge-tiling enable')
   # Alternate modifier permits immediate tiling inside the wider activation band.
   start();keyboard=launch([a.keyboard,'-M','alt','-s','1200','-m','alt'],'option.log');time.sleep(.2);begin=len(events);to(20,360);assert 'preview' in phases(begin);release();assert 'commit' in phases(begin);keyboard.wait(timeout=3)
   ipc('lumen_window option-tiling disable');start();keyboard=launch([a.keyboard,'-M','alt','-s','1200','-m','alt'],'option-off.log');time.sleep(.2);begin=len(events);to(20,360);collect(.35);release();assert 'preview' not in phases(begin),events[begin:];keyboard.wait(timeout=3);ipc('lumen_window option-tiling enable')
   # A target on the differently sized second output uses that output's workarea.
   start();begin=len(events);to(2279.5,400);collect(.32);preview2=next(x for x in events[begin:] if x.get('phase')=='preview');release();assert outer()=={k:preview2[k] for k in ('x','y','width','height')},(outer(),preview2);assert preview2['x']>=1280 and preview2['height']<=800
   # Configurable gap and margins have matching preview and placement geometry.
   ipc('lumen_window tile-gap 16');start();begin=len(events);to(0,360);collect(.32);preview_gap=next(x for x in events[begin:] if x.get('phase')=='preview');assert preview_gap['x']==16;release();ipc('lumen_window tile-gap 8')
   # Slow top dwell can fill; a held-through gesture chooses overview exclusively.
   ipc('lumen_window top-edge-overview disable');start();begin=len(events);to(600,0);collect(.32);release();assert 'commit' in phases(begin)
   ipc('lumen_window top-edge-overview enable');start();begin=len(events);to(600,0);collect(1.0);release();assert any(x.get('lumen_drag_overview') and x['id']==cid for x in events[begin:]);assert 'commit' not in phases(begin)
   ipc('lumen_window top-edge-fill disable');start();begin=len(events);to(600,0);collect(1.0);release()
   assert any(e.get('lumen_drag_overview') for e in events[begin:]) and 'preview' not in phases(begin) and 'commit' not in phases(begin)
   ipc('lumen_window top-edge-overview disable');start();begin=len(events);to(600,0);collect(1.0);release()
   assert not any(e.get('lumen_drag_overview') for e in events[begin:]) and 'preview' not in phases(begin) and 'commit' not in phases(begin)
   ipc('lumen_window top-edge-fill enable');ipc('lumen_window top-edge-overview enable')
   # Lock cancels the pending drag immediately and cannot commit on release.
   original=start();begin=len(events);to(0,360);collect(.32);locker=launch([a.lock],'lock.log',stdin=sp.PIPE,text=True);collect(.3);assert 'cancel' in phases(begin);locker.stdin.write('u\n');locker.stdin.flush();locker.wait(timeout=3);release();assert 'commit' not in phases(begin);assert outer()==original
   # Destroying the captured target cannot retarget a commit onto another client.
   start();begin=len(events);to(0,360);collect(.32);ipc(f'[con_id={cid}] kill');wait(lambda:window() is None);release()
   assert 'commit' not in phases(begin) and bystander()==other_box
   assert sway('-t','get_version')['native_edge_tiling']
  else:
   ipc(f"[con_id={cid}] kill");wait(lambda:window() is None);preview=None;preview2=None
  constraints_checked=[]
  if a.constraint_client:
   motion(0,0)  # Flush the virtual device registration before binding wl_seat.
   subject_app='constraint-test'
   for kind in ('fixed','bounded','free'):
    client=launch([a.constraint_client,kind],f'constraint-{kind}.log',stdin=sp.PIPE,text=True)
    wait(window);cid=window()['id'];ipc(f'[con_id={cid}] border normal 2');original=reset();assert window()['deco_rect']['height']>0
    if kind!='free':
     # IPC and physical drag both refuse layouts outside the client constraints.
     if kind=='fixed':
      at(original['x']+60,original['y']+16);button('p');release();assert outer()==original and not window()['lumen_state']['filled']
     ipc(f'[con_id={cid}] lumen_window left');time.sleep(.2);assert outer()==original,(kind,outer(),original);assert not window()['lumen_state']['filled']
     original=start();begin=len(events);to(0,360);collect(.35)
     assert 'preview' not in phases(begin),events[begin:]
     sp.run([a.keyboard,'-k','Escape'],env=env,check=True);release();assert outer()==original
     if kind=='bounded':
      # The narrower second-output tile is allowed by this same maximum size.
      start();begin=len(events);to(2279.5,400);collect(.32);release()
      assert 'commit' in phases(begin),events[begin:]
      ipc(f'[con_id={cid}] lumen_window restore');time.sleep(.2);assert outer()==original,(outer(),original)
    else:
     for command,active in [('l','locked'),('c','confined')]:
      original=reset();at(original['x']+150,original['y']+100);motion(1,1)
      client.stdin.write(command+'\n');client.stdin.flush()
      wait(lambda:active in (a.output/f'constraint-{kind}.log').read_text().splitlines())
      begin=len(events);button('p');motion(-2000,-2000);collect(.35);release()
      assert not any(e.get('phase') in ('preview','commit') for e in events[begin:]),events[begin:]
      assert outer()==original
      reports=[line.split()[1:] for line in (a.output/f'constraint-{kind}.log').read_text().splitlines() if line.startswith('motion ')]
      assert reports and all(0<=float(x)<=400 and 0<=float(y)<=300 for x,y in reports),reports
      client.stdin.write('u\n');client.stdin.flush();collect(.2)
     # Once the client releases its pointer constraint a valid titlebar gesture works.
     start();begin=len(events);to(0,360);collect(.32);release();assert 'commit' in phases(begin)
    constraints_checked.append(kind);client.stdin.write('q\n');client.stdin.flush();client.wait(timeout=3);wait(lambda:window() is None)
  (a.output/'result.json').write_text(json.dumps({'passed':True,'fullDragSuite':not a.constraints_only,'constraints':constraints_checked,'events':events,'capturedID':cid,'preview':preview,'secondOutput':preview2},indent=2));print('PASS native fixed/bounded sizes, pointer lock/confinement and released-constraint drag' if a.constraints_only else 'PASS native drag preview/leave/cancel/commit/restore/modifiers/top arbitration/lock'+(' and client constraints' if a.constraint_client else ''))
 finally:
  for proc in reversed(processes):
   try:os.killpg(proc.pid,signal.SIGTERM)
   except ProcessLookupError:pass
  for proc in reversed(processes):
   try:proc.wait(timeout=3)
   except sp.TimeoutExpired:
    try:os.killpg(proc.pid,signal.SIGKILL)
    except ProcessLookupError:pass
    proc.wait()
