"""Exact native pressed-surface lifetime, multibutton and surviving input checks."""
import argparse,hashlib,json,os,re,shutil,subprocess as sp,tempfile,time
from pathlib import Path
from private_native_bus import start_private_bus
from fixture_resources import close_reaped_pipes
p=argparse.ArgumentParser();p.add_argument('--launcher',type=Path,required=True);p.add_argument('--sway',required=True);p.add_argument('--pointer-tool',required=True);p.add_argument('--effects',action='store_true');p.add_argument('--output',type=Path,required=True);a=p.parse_args();out=a.output.resolve();out.mkdir(parents=True,exist_ok=False);children=[];checks=[]
with tempfile.TemporaryDirectory(prefix='lumen-pointer-retirement-') as temporary:
 home=Path(temporary);env=dict(os.environ,HOME=temporary,XDG_RUNTIME_DIR=temporary,DBUS_SYSTEM_BUS_ADDRESS='unix:path='+temporary+'/absent-system')
 for key in ('CONFIG','CACHE','DATA','STATE'):env['XDG_'+key+'_HOME']=str(home/key.lower())
 for key in ('DBUS_SESSION_BUS_ADDRESS','WAYLAND_DISPLAY','SWAYSOCK','DISPLAY','AT_SPI_BUS_ADDRESS'):env.pop(key,None)
 prefix=re.split(r'\n[ \t]*exec quickshell ',a.launcher.read_text(),maxsplit=1)[0];env.update(dict(v.decode().split('=',1) for v in sp.check_output(['bash','-c',prefix+'\nexec env -0'],env=env).split(b'\0') if b'=' in v))
 env.update(QT_QPA_PLATFORM='wayland',QT_QPA_PLATFORMTHEME='generic',QT_QUICK_BACKEND='software',QT_ACCESSIBILITY='0',NO_AT_BRIDGE='1',GTK_A11Y='none',WLR_BACKENDS='headless',WLR_RENDERER='gles2' if a.effects else 'pixman',WLR_RENDERER_ALLOW_SOFTWARE='1',WLR_RENDERER_FORCE_SOFTWARE='1' if a.effects else '0',LIBGL_ALWAYS_SOFTWARE='1',WLR_LIBINPUT_NO_DEVICES='1')
 def launch(command,name,**kwargs):
  with (out/name).open('w') as log:child=sp.Popen(command,env=env,stdout=log,stderr=sp.STDOUT,**kwargs)
  children.append(child);return child
 def call(*args):return sp.check_output(args,env=env,text=True,timeout=5).strip()
 def wait(fn,label):
  end=time.monotonic()+12
  while time.monotonic()<end:
   try:value=fn()
   except (ValueError,OSError,sp.SubprocessError):value=None
   if value:return value
   time.sleep(.025)
  raise AssertionError('Timed out: '+label)
 def nodes(n):return [n]+sum((nodes(c) for c in n.get('nodes',[])+n.get('floating_nodes',[])),[])
 def tree():return nodes(json.loads(call('swaymsg','-r','-t','get_tree')))
 def native(name):return next((n for n in tree() if n.get('name')=='Pointer '+name and n.get('pid')==shell.pid),None)
 def ipc(*args):return call('quickshell','ipc','--pid',str(shell.pid),'call','fixture',*args)
 def state():return json.loads(ipc('status'))
 def send(command):pointer.stdin.write(command+'\n');pointer.stdin.flush()
 def move(name):
  node=wait(lambda:native(name),'native '+name);call('swaymsg','[con_id='+str(node['id'])+']','focus');x=node['rect']['x']+node['window_rect']['x']+150;y=node['rect']['y']+node['window_rect']['y']+100;call('swaymsg','seat','seat0','cursor','set',str(x-24),str(y));send('m 24 0');wait(lambda:state()[name]['hovered'],'genuine fresh '+name+' hover')
 def button(code,pressed):send('b '+str(code)+' '+str(int(pressed)))
 def hidden(name):return not native(name)
 qml=out/'shell.qml';qml.write_text('''import QtQuick
import QtQuick.Window
import Quickshell
import Quickshell.Io
ShellRoot {
 Window { id:a;title:"Pointer A";visible:true;width:300;height:220
  Rectangle {anchors.fill:parent;color:"#394b60"}
  MouseArea {id:ma;anchors.fill:parent;hoverEnabled:true;acceptedButtons:Qt.AllButtons;property int releases:0;property int moves:0;onReleased:releases++;onPositionChanged:if(pressedButtons)moves++}
 }
 Window { id:b;title:"Pointer B";visible:true;width:300;height:220
  Rectangle {anchors.fill:parent;color:"#506f45"}
  MouseArea {id:mb;anchors.fill:parent;hoverEnabled:true;acceptedButtons:Qt.AllButtons;property int releases:0;property int moves:0;onReleased:releases++;onPositionChanged:if(pressedButtons)moves++}
 }
 IpcHandler {target:"fixture"
  function status():string{return JSON.stringify({A:{hovered:ma.containsMouse,buttons:Number(ma.pressedButtons),releases:ma.releases,moves:ma.moves},B:{hovered:mb.containsMouse,buttons:Number(mb.pressedButtons),releases:mb.releases,moves:mb.moves}});}
  function hideA():void{a.visible=false;} function showA():void{a.visible=true;}
  function hideB():void{b.visible=false;}
 }
}
''')
 try:
  start_private_bus(env,out,children);config=out/'sway.conf';config.write_text('output HEADLESS-1 resolution 1000x700\nseat seat0 fallback true\nxwayland disable\nfocus_follows_mouse no\nfor_window [all] floating enable\n');launch([a.sway,'--unsupported-gpu','-c',str(config)],'sway.log');env['SWAYSOCK']=str(wait(lambda:next(home.glob('sway-ipc*.sock'),None),'private compositor'));env['WAYLAND_DISPLAY']=wait(lambda:next((p.name for p in home.glob('wayland-*') if not p.name.endswith('.lock')),None),'private display');pointer=launch([a.pointer_tool,'pointer','move','0','0'],'pointer.log',stdin=sp.PIPE,text=True);shell=launch(['quickshell','-p',str(qml)],'shell.log');wait(lambda:native('A') and native('B'),'both actual windows');
  for name,x in [('A',60),('B',550)]:call('swaymsg','[con_id='+str(native(name)['id'])+']',f'move position {x} 160')
  runtime=(Path(shutil.which('quickshell',path=env['PATH'])).resolve().parent.parent/'share/lumen-desktop/qt-wayland-runtime').read_text().strip();loaded=sorted({line.split()[-1] for line in Path('/proc/'+str(shell.pid)+'/maps').read_text().splitlines() if 'libQt6WaylandClient.so' in line});assert loaded and all(path.startswith(runtime+'/') for path in loaded)
  move('A');button(272,True);button(273,True);wait(lambda:state()['A']['buttons']==3,'two actual held buttons');button(272,False);wait(lambda:state()['A']['buttons']==2,'right button remains held after left release');ipc('hideA');wait(lambda:hidden('A'),'actual first surface unmapped');button(273,False);move('B');ipc('showA');wait(lambda:native('A'),'retained Window reopens');move('A');button(272,True);wait(lambda:state()['A']['buttons']==1,'press before hide and reopen');ipc('hideA');wait(lambda:hidden('A'),'retained Window hidden while held');ipc('showA');wait(lambda:native('A'),'same owned Window reopened while old button physically held');button(272,False);move('A');move('B');checks.append('Retiring exact surface after first of two button releases clears remaining state before hide-without-destroy/reopen while still held and fresh hover')
  before=state()['B'];button(272,True);wait(lambda:state()['B']['buttons']==1,'ordinary left press');send('m 30 0');wait(lambda:state()['B']['moves']>before['moves'] and state()['B']['buttons']==1,'ordinary held motion');button(272,False);wait(lambda:state()['B']['buttons']==0 and state()['B']['releases']>before['releases'],'ordinary physical release');button(272,True);wait(lambda:state()['B']['buttons']==1,'press before ordinary focus transfer');call('swaymsg','[con_id='+str(native('A')['id'])+']','focus');wait(lambda:native('A')['focused'] and native('B'),'ordinary focus transfer keeps both native surfaces mapped');button(272,False);move('B');checks.append('Ordinary held motion and release preserve delivery; focus transfer without unmap retains reachable fresh hover after physical release')
  ipc('showA');wait(lambda:native('A'),'new native A surface');move('A');button(272,True);wait(lambda:state()['A']['buttons']==1,'old guard press');button(272,False);wait(lambda:state()['A']['buttons']==0,'old guard normal release');move('B');button(272,True);wait(lambda:state()['B']['buttons']==1,'new target press');before=state()['B']['moves'];ipc('hideA');wait(lambda:hidden('A'),'retired old surface gone');send('m 20 0');wait(lambda:state()['B']['buttons']==1 and state()['B']['moves']>before,'old surface cannot clear new held target');button(272,False);wait(lambda:state()['B']['buttons']==0,'new target physical release');checks.append('A normally retired old guard cannot clear a subsequent different live target press')
  button(272,True);wait(lambda:state()['B']['buttons']==1,'press before pointer teardown');pointer.stdin.close();pointer.terminate();pointer.wait(timeout=5);wait(lambda:not any(d.get('type')=='pointer' for seat in json.loads(call('swaymsg','-r','-t','get_seats')) for d in seat.get('devices',[])),'actual pointer device removal');ipc('hideB');wait(lambda:hidden('B'),'surface teardown after Pointer destruction');assert shell.poll() is None;ipc('status');checks.append('Pointer teardown disconnects guard before its former surface is destroyed; shell remains responsive')
  (out/'result.json').write_text(json.dumps(dict(passed=True,checks=checks,launcher=str(a.launcher),runtime=runtime,loaded=loaded,source=sp.check_output(['git','rev-parse','HEAD'],text=True).strip(),fixtureSha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest()),indent=2)+'\n')
 except BaseException:
  diagnostics={"checks":checks}
  for label,reader in (("state",state),("tree",tree)):
   try:diagnostics[label]=reader()
   except (OSError,ValueError,sp.SubprocessError,NameError) as error:diagnostics[label+"Error"]=str(error)
  (out/'failure-state.json').write_text(json.dumps(diagnostics,indent=2)+'\n')
  raise
 finally:
  if 'pointer' in locals() and pointer.stdin and not pointer.stdin.closed:
   try:pointer.stdin.close()
   except BrokenPipeError:pass
  for child in reversed(children):
   if child.poll() is None:child.terminate()
  for child in reversed(children):
   try:child.wait(timeout=5)
   except sp.TimeoutExpired:child.kill();child.wait(timeout=5)
  close_reaped_pipes(children)
