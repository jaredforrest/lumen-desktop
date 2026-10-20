"""Native captured Dock removal with private persisted settings transport."""
from fixture_resources import close_reaped_pipes
import argparse,hashlib,json,os,re,shutil,signal,subprocess as sp,sys,tempfile,time
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--output',type=Path,required=True);p.add_argument('--source',type=Path,default=Path(__file__).resolve().parents[2]);p.add_argument('--pointer-tool',default='wlrctl');p.add_argument('--launcher',required=True);p.add_argument('--sway',default='sway');p.add_argument('--effects',action='store_true');p.add_argument('--inside',action='store_true');a=p.parse_args()
if not a.inside:
 a.output.mkdir(parents=True,exist_ok=False)
 runtime=tempfile.mkdtemp(prefix='lumen-interaction-')
 env=dict(os.environ,HOME=runtime,XDG_RUNTIME_DIR=runtime)
 for key in ('CONFIG','STATE','CACHE','DATA'):env['XDG_'+key+'_HOME']=str(Path(runtime)/key.lower())
 for k in ('DISPLAY','WAYLAND_DISPLAY','SWAYSOCK','DBUS_SESSION_BUS_ADDRESS'):env.pop(k,None)
 try:raise SystemExit(sp.call(['dbus-run-session','--',sys.executable,__file__,*sys.argv[1:],'--inside'],env=env))
 finally:shutil.rmtree(runtime,ignore_errors=True)
out=a.output.resolve();source=a.source.resolve();assets=out/'assets';shutil.copytree(source/'res/lumen-desktop/shell',assets)
prefix=re.split(r'\n[ \t]*exec quickshell ',Path(a.launcher).read_text(),maxsplit=1)[0]
raw=sp.check_output(['bash','-c',prefix+'\nexec env -0'],stderr=sp.DEVNULL)
env=dict(os.environ);env.update(dict(v.decode().split('=',1) for v in raw.split(b'\0') if b'=' in v))
for k in ('CONFIG','STATE','CACHE','DATA'):env['XDG_'+k+'_HOME']=str(out/k.lower())
env.update(WLR_BACKENDS='headless',WLR_RENDERER='pixman',WLR_LIBINPUT_NO_DEVICES='1',QT_QUICK_BACKEND='software',QT_QPA_PLATFORM='wayland',GDK_BACKEND='wayland',GTK_USE_PORTAL='0',GSETTINGS_BACKEND='memory',NO_AT_BRIDGE='1',QT_ACCESSIBILITY='0',XDG_CURRENT_DESKTOP='sway')
env.update(QT_ACCESSIBILITY='0',QT_LINUX_ACCESSIBILITY_ALWAYS_ON='0',NO_AT_BRIDGE='1',GTK_A11Y='none')
env.pop('AT_SPI_BUS_ADDRESS',None)
if a.effects:env.update(WLR_RENDERER='gles2',WLR_RENDERER_ALLOW_SOFTWARE='1',WLR_RENDERER_FORCE_SOFTWARE='1',QT_QUICK_BACKEND='',QSG_RHI_BACKEND='opengl',LIBGL_ALWAYS_SOFTWARE='1',LUMEN_COMPOSITOR_EFFECTS='1')
for k in ('DISPLAY','WAYLAND_DISPLAY','SWAYSOCK'):env.pop(k,None)
portal=out/'config/xdg-desktop-portal';portal.mkdir(parents=True,exist_ok=True);(portal/'portals.conf').write_text('[preferred]\ndefault=none\n')
procs=[]
def launch(cmd,name):
 with (out/(name+'.log')).open('w') as log:x=sp.Popen(cmd,env=env,stdout=log,stderr=sp.STDOUT,start_new_session=True)
 procs.append(x);return x
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
def ipc(method,*args):return call('quickshell','-p',str(assets/'fixture.qml'),'ipc','call','fixture',method,*args)
def button(code):pointer.stdin.write(code+'\n');pointer.stdin.flush();time.sleep(.1)
def write_count():return len((out/'writes.jsonl').read_text().splitlines()) if (out/'writes.jsonl').exists() else 0
def center(key):
 d=state();index=d['keys'].index(key);return (1920-d['width'])/2+12+(index+.5)*d['slot'],1030
def press(key):move(*center(key));button('p')
def outside():move(30,500)
def reset():
 ipc('reset');wait(lambda:state()['ready'] and not state()['busy']);time.sleep(.15)
try:
 conf=out/'sway.conf';conf.write_text('output * resolution 1920x1080\ndefault_border normal 1\ndefault_floating_border normal 1\n'+(source/'res/lumen-desktop/sway.conf').read_text())
 launch([a.sway,'--unsupported-gpu','-c',str(conf)],'sway');runtime=Path(env['XDG_RUNTIME_DIR']);wait(lambda:list(runtime.glob('sway-ipc*.sock')))
 env['SWAYSOCK']=str(next(runtime.glob('sway-ipc*.sock')));env['WAYLAND_DISPLAY']=next(x.name for x in runtime.glob('wayland-*') if not x.name.endswith('.lock'))
 bin=out/'bin';bin.mkdir();service=bin/'lumen-appearance'
 service.write_text('#!'+sys.executable+'\n'+r'''import json,sys,pathlib,time
root=pathlib.Path(__file__).resolve().parents[1];store=root/'preferences.json'
initial={"dockPins":["fixture-a","fixture-b","unknown-retained"],"dockFiles":[str(root/'Original report.txt')],"dockFolders":[{"path":str(root/'Original folder'),"view":"list"}]}
data=json.loads(store.read_text()) if store.exists() else initial
if sys.argv[1]=='--set':
 key=sys.argv[2];value=json.loads(sys.argv[3]);data[key]=value
 with (root/'writes.jsonl').open('a') as log:log.write(json.dumps({"key":key,"value":value})+'\n')
 store.write_text(json.dumps(data))
print(json.dumps({"preferences":data,"appearance":{},"system":{}}))
''');service.chmod(0o755);env['PATH']=str(bin)+os.pathsep+env['PATH']
 (out/'Fixture A.desktop').write_text('[Desktop Entry]\nName=Fixture A\nType=Application\nExec=true\n');(out/'Original report.txt').write_text('original file remains');(out/'Original folder').mkdir()
 qml=r'''import QtQuick
import Quickshell
import Quickshell.Io
import "DockModel.js" as Model
ShellRoot {
 SettingsState {id:settings;active:true}
 QtObject {id:stub
  property var apps:[{id:"fixture-a",name:"Fixture A",icon:"application-x-executable"},{id:"fixture-b",name:"Fixture B",icon:"application-x-executable"}]
  property var reference:[{key:"files",action:"files",image:"files.svg",canvas:68},{key:"a",appId:"fixture-a",canvas:68},{key:"b",appId:"fixture-b",canvas:68},{key:"downloads",action:"downloads",image:"files.svg",canvas:68},{key:"trash",action:"trash",image:"files.svg",canvas:68}]
  property bool forcedBusy:false
  property bool runningA:false
  property bool replaced:false
  property bool dockPreferencesBusy:settings.busy || forcedBusy
  property var referenceDock:Model.build(reference,apps,runningA?[apps[0]]:[],settings.preferences.dockPins,[],false,settings.preferences.dockFolders,settings.preferences.dockFiles)
  property string popupOwner:""
  property date currentDate:new Date()
  property var activations:[]
  property string aliasError:""
  property string applicationCommandError:""
  signal requestDockFocus(string name)
  signal dismissPopups(string owner)
  function claimPopup(owner){popupOwner=owner;dismissPopups(owner);}
  function dockWindows(item){return [];}
  function dockLabel(item){return item.key;}
  function runningFor(entry){return runningA && entry.id==="fixture-a";}
  function activateDock(item,shift,name){activations=activations.concat(item.key);}
  function canConfigureDockEntry(entry){return entry.id!=="lumen-files";}
  function isDockPinned(entry){return (settings.preferences.dockPins || []).includes(entry.id);}
  function updateDockPin(entry,action){if(!dockPreferencesBusy)settings.setPreference("dockPins",Model.updatePins(settings.preferences.dockPins,entry.id,action));}
  function reorderDockPin(source,target){if(!dockPreferencesBusy)settings.setPreference("dockPins",Model.movePin(settings.preferences.dockPins,source.id,target.id));}
  function removeDockAlias(item){if(dockPreferencesBusy)return;if(item.action==="file")settings.setPreference("dockFiles",settings.preferences.dockFiles.filter(path=>path!==item.path));else if(item.action==="folder")settings.setPreference("dockFolders",settings.preferences.dockFolders.filter(folder=>folder.path!==item.folder.path));}
 }
 Dock {id:dock;shell:stub;screen:Quickshell.screens[0]}
 IpcHandler {target:"fixture"
  function state():string{return JSON.stringify({keys:stub.referenceDock.map(x=>x.key),width:dock.width,slot:dock.slot,dragging:dock.dragging,hovered:dock.hovered,activations:stub.activations,busy:stub.dockPreferencesBusy,ready:settings.available,pins:settings.preferences.dockPins,files:settings.preferences.dockFiles,folders:settings.preferences.dockFolders});}
  function busy(value:bool):void{stub.forcedBusy=value;}
  function running():void{stub.runningA=true;}
  function replace():void{stub.apps=stub.apps.map(x=>Object.assign({},x));}
  function remove():void{stub.apps=stub.apps.filter(x=>x.id!=="fixture-a");}
  function reset():void{stub.forcedBusy=false;stub.runningA=false;stub.apps=[{id:"fixture-a",name:"Fixture A",icon:"application-x-executable"},{id:"fixture-b",name:"Fixture B",icon:"application-x-executable"}];settings.refresh();}
 }
}'''
 (assets/'fixture.qml').write_text(qml)
 shell=launch(['quickshell','-p',str(assets/'fixture.qml')],'dock');wait(lambda:state()['ready'] and not state()['busy']);time.sleep(.5)
 pointer=sp.Popen([a.pointer_tool,'pointer','move','0','0'],env=env,stdin=sp.PIPE,text=True,start_new_session=True);procs.append(pointer)
 time.sleep(.1);call('swaymsg','seat seat0 cursor set 960 540')
 checks=[]
 # No wrong-neighbour activation even for a required, non-draggable icon.
 press('files');move(*center('b'));button('r');assert state()['activations']==[],state();checks.append('Required-item press across neighbours cannot activate a bystander')
 # Escape consumes the whole captured gesture.
 n=write_count();press('a');move(*center('b'));wait(lambda:state()['dragging']);call('wtype','-k','Escape');outside();button('r');assert write_count()==n and state()['activations']==[],state();checks.append('Escape cancels held drag without writes or activation')
 # Busy, removed, and same-ID replacement all invalidate the original press.
 for action,args in [('busy',('true',)),('replace',()),('remove',())]:
  reset();n=write_count();press('a');move(*center('b'));wait(lambda:state()['dragging']);ipc(action,*args);outside();button('r');assert write_count()==n and state()['activations']==[],(action,state())
 checks.append('Busy, removed, and same-ID replacement cancel captured pointer intent')
 reset();n=write_count();press('a');move(*center('b'));outside();call('grim',str(out/'removal-cue.png'));button('r');wait(lambda:write_count()==n+1 and not state()['busy']);assert state()['pins']==['fixture-b','unknown-retained'],state();assert 'a' not in state()['keys'];checks.append('Cross-neighbour app drag-out persists only intended pin removal and preserves unknown pins')
 # Restart actual owned Dock and SettingsState against the private persisted transport.
 os.killpg(shell.pid,signal.SIGTERM);shell.wait(timeout=10);shell=launch(['quickshell','-p',str(assets/'fixture.qml')],'dock-restart');wait(lambda:state()['ready'] and not state()['busy']);assert state()['pins']==['fixture-b','unknown-retained'];checks.append('Removal survives private shell restart')
 # Re-add pin using the same protocol, then demonstrate running item survives unpin.
 launch(['foot','--app-id=fixture-a','--title=Fixture Original App','sleep','120'],'original-app');wait(lambda:any(n.get('app_id')=='fixture-a' for n in nodes(json.loads(call('swaymsg','-t','get_tree')))));original_window=next(n['id'] for n in nodes(json.loads(call('swaymsg','-t','get_tree'))) if n.get('app_id')=='fixture-a')
 call(str(service),'--set','dockPins',json.dumps(['fixture-a','fixture-b','unknown-retained']));reset();ipc('running');wait(lambda:'a' in state()['keys']);n=write_count();press('a');outside();button('r');wait(lambda:write_count()==n+1 and not state()['busy']);assert 'fixture-a' in state()['keys'] and 'fixture-a' not in state()['pins'],state();assert any(n['id']==original_window for n in nodes(json.loads(call('swaymsg','-t','get_tree'))));checks.append('Running application remains available after its pin is removed, with its actual native window unchanged')
 n=write_count();press('fixture-a');outside();button('r');assert write_count()==n and 'fixture-a' in state()['keys'],state();checks.append('An unpinned running application cannot be removed by dragging out')
 for key in ['file:'+str(out/'Original report.txt'),'folder:'+str(out/'Original folder')]:
  n=write_count();press(key);outside();button('r');wait(lambda:write_count()==n+1 and not state()['busy']);assert key not in state()['keys'],state()
 assert (out/'Fixture A.desktop').is_file();assert (out/'Original report.txt').read_text()=='original file remains' and (out/'Original folder').is_dir();assert state()['activations']==[],state();checks.append('File and folder drag-out changes only persisted aliases; original resources survive')
 call('grim',str(out/'accepted.png'))
 log=''.join(p.read_text() for p in out.glob('dock*.log'));assert not any(x in log for x in ['ReferenceError','TypeError','Binding loop','Failed to load']),log
 (out/'result.json').write_text(json.dumps(dict(passed=True,checks=checks,sourceDigest=hashlib.sha256((source/'res/lumen-desktop/shell/Dock.qml').read_bytes()).hexdigest(),finalState=state(),sourceRevision=call('git','-C',str(source),'rev-parse','HEAD').strip(),writes=write_count(),liveSocketUnchanged=os.environ.get('SWAYSOCK')!=env['SWAYSOCK']),indent=2))
except BaseException:
 try:call('grim',str(out/'failure.png'))
 except Exception:pass
 raise
finally:
 for x in reversed(procs):
  if x.poll() is None:os.killpg(x.pid,signal.SIGTERM)
  x.wait(timeout=10)
 if 'pointer' in globals() and pointer.stdin:pointer.stdin.close()
 for _fixture_process in procs:
     if _fixture_process.poll() is None:_fixture_process.wait(timeout=5)
 close_reaped_pipes(procs)
