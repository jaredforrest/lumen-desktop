"""Real pointer sweeps and CSD file-chooser ownership in an isolated compositor."""
import argparse,json,os,re,shutil,signal,subprocess as sp,sys,tempfile,time
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--output',type=Path,required=True);p.add_argument('--source',type=Path,default=Path(__file__).resolve().parents[2]);p.add_argument('--pointer-tool',default='wlrctl');p.add_argument('--inside',action='store_true');a=p.parse_args()
if not a.inside:
 a.output.mkdir(parents=True,exist_ok=False)
 runtime=tempfile.mkdtemp(prefix='lumen-interaction-')
 env=dict(os.environ,XDG_RUNTIME_DIR=runtime)
 for k in ('DISPLAY','WAYLAND_DISPLAY','SWAYSOCK','DBUS_SESSION_BUS_ADDRESS'):env.pop(k,None)
 try:raise SystemExit(sp.call(['dbus-run-session','--',sys.executable,__file__,*sys.argv[1:],'--inside'],env=env))
 finally:shutil.rmtree(runtime,ignore_errors=True)
out=a.output.resolve();source=a.source.resolve();assets=out/'assets';shutil.copytree(source/'res/lumen-desktop/shell',assets)
prefix=re.split(r'\n[ \t]*exec quickshell ',Path(shutil.which('lumen-desktop-shell')).read_text(),maxsplit=1)[0]
raw=sp.check_output(['bash','-c',prefix+'\nexec env -0'],stderr=sp.DEVNULL)
env=dict(os.environ);env.update(dict(v.decode().split('=',1) for v in raw.split(b'\0') if b'=' in v))
for k in ('CONFIG','STATE','CACHE','DATA'):env['XDG_'+k+'_HOME']=str(out/k.lower())
env.update(WLR_BACKENDS='headless',WLR_RENDERER='pixman',WLR_LIBINPUT_NO_DEVICES='1',QT_QUICK_BACKEND='software',QT_QPA_PLATFORM='wayland',GDK_BACKEND='wayland',GTK_USE_PORTAL='0',GSETTINGS_BACKEND='memory',NO_AT_BRIDGE='1',QT_ACCESSIBILITY='0',XDG_CURRENT_DESKTOP='sway')
for k in ('DISPLAY','WAYLAND_DISPLAY','SWAYSOCK'):env.pop(k,None)
portal=out/'config/xdg-desktop-portal';portal.mkdir(parents=True,exist_ok=True);(portal/'portals.conf').write_text('[preferred]\ndefault=none\n')
procs=[]
def launch(cmd,name):
 x=sp.Popen(cmd,env=env,stdout=(out/(name+'.log')).open('w'),stderr=sp.STDOUT,start_new_session=True);procs.append(x);return x
def call(*cmd):return sp.check_output(cmd,env=env,text=True,timeout=15)
def wait(fn):
 end=time.monotonic()+20
 while time.monotonic()<end:
  try:
   if fn():return
  except (OSError,ValueError,sp.SubprocessError):pass
  time.sleep(.1)
 raise AssertionError('Timed out; inspect '+str(out))
def state():return json.loads(call('quickshell','-p',str(assets/'fixture.qml'),'ipc','call','fixture','state'))
def nodes(n):return ([n] if n.get('app_id') or n.get('window') else [])+sum([nodes(c) for c in n.get('nodes',[])+n.get('floating_nodes',[])],[])
position=[960,540]
def move(x,y):
 pointer.stdin.write(f'm {x-position[0]} {y-position[1]}\n');pointer.stdin.flush()
 position[:]=[x,y]
 time.sleep(.08)
try:
 conf=out/'sway.conf';conf.write_text('output * resolution 1920x1080\ndefault_border normal 1\ndefault_floating_border normal 1\n'+(source/'res/lumen-desktop/sway.conf').read_text())
 launch(['sway','--unsupported-gpu','-c',str(conf)],'sway');runtime=Path(env['XDG_RUNTIME_DIR']);wait(lambda:list(runtime.glob('sway-ipc*.sock')))
 env['SWAYSOCK']=str(next(runtime.glob('sway-ipc*.sock')));env['WAYLAND_DISPLAY']=next(x.name for x in runtime.glob('wayland-*') if not x.name.endswith('.lock'))
 call('dbus-update-activation-environment','WAYLAND_DISPLAY','XDG_CURRENT_DESKTOP','XDG_CONFIG_HOME','XDG_DATA_HOME','XDG_CACHE_HOME','XDG_STATE_HOME')
 (assets/'fixture.qml').write_text('''import QtQuick
import Quickshell
import Quickshell.Io
ShellRoot {
 QtObject {id:stub
  property var referenceDock: Array.from({length:8},(_,i)=>({key:"item"+i,image:"files.svg",canvas:68,entry:null}))
  property string popupOwner:""
  property date currentDate:new Date()
  signal requestDockFocus(string name)
  signal dismissPopups(string owner)
  function claimPopup(owner){popupOwner=owner;dismissPopups(owner);}
  function dockWindows(item){return [];}
  function dockLabel(item){return item.key;}
  function activateDock(item,shift,name){}
 }
 Dock {id:dock;shell:stub;screen:Quickshell.screens[0]}
 IpcHandler {target:"fixture"
  function state():string{return JSON.stringify({hovered:dock.hovered,slot:dock.pointerSlot,width:dock.width});}
 }
}''')
 launch(['quickshell','-p',str(assets/'fixture.qml')],'dock');wait(lambda:state()['width']>0);time.sleep(1)
 pointer=sp.Popen([a.pointer_tool,'pointer','move','0','0'],env=env,stdin=sp.PIPE,text=True,start_new_session=True);procs.append(pointer)
 time.sleep(.1);call('swaymsg','seat seat0 cursor set 960 540')
 width=state()['width'];slot=(width-24)/8;left=(1920-width)/2
 for held in (False,True):
  move(left+12+slot/2,1030)
  if held:pointer.stdin.write('p\n');pointer.stdin.flush()
  for y in (1030,975,995,1030):
   for index in list(range(8))+list(reversed(range(8))):
    move(left+12+(index+.5)*slot,y)
    assert state()['hovered']==index,(held,y,index,state())
  move(20,500);assert state()['hovered']==-1,state()
  if held:pointer.stdin.write('r\n');pointer.stdin.flush()
 call('grim',str(out/'dock.png'))
 fixture=out/'chooser.py';fixture.write_text('''import gi
gi.require_version("Gtk","3.0")
from gi.repository import Gtk
w=Gtk.FileChooserDialog(title="Save Dialog Fixture",action=Gtk.FileChooserAction.SAVE,use_header_bar=True)
w.add_button("Cancel",Gtk.ResponseType.CANCEL);w.add_button("Save",Gtk.ResponseType.ACCEPT)
w.set_default_size(760,520);w.set_current_name("fixture.html");w.show();Gtk.main()
''')
 launch([sys.executable,str(fixture)],'chooser');wait(lambda:any(n.get('name')=='Save Dialog Fixture' for n in nodes(json.loads(call('swaymsg','-t','get_tree')))))
 node=next(n for n in nodes(json.loads(call('swaymsg','-t','get_tree'))) if n.get('name')=='Save Dialog Fixture')
 assert node['border']=='csd',node
 assert node['deco_rect']['height']==0,node
 assert node['rect']['y']>=0 and node['rect']['height']<=999,node
 call('grim',str(out/'chooser.png'))
 (out/'result.json').write_text(json.dumps(dict(passed=True,checks=['bidirectional Dock hover sweeps','held-button sweeps and exit clear','CSD save dialog has no compositor titlebar']),indent=2));print(out/'result.json')
except BaseException:
 try:call('grim',str(out/'failure.png'))
 except Exception:pass
 raise
finally:
 for x in reversed(procs):
  if x.poll() is None:os.killpg(x.pid,signal.SIGTERM)
