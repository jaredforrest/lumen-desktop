"""Private native disabled controls and independent window restoration."""
import argparse,json,os,re,shutil,signal,subprocess as sp,sys,tempfile,time
from pathlib import Path
from application_fixture.isolation import run as isolated_run
from private_native_bus import start_private_bus
from fixture_resources import close_reaped_pipes
p=argparse.ArgumentParser();p.add_argument('--launcher',type=Path,required=True);p.add_argument('--sway',type=Path,required=True);p.add_argument('--output',type=Path);p.add_argument('--effects',action='store_true');p.add_argument('--inside',action='store_true');p.add_argument('--python-runtime',default=shutil.which('desktop-test-python'));p.add_argument('--egl-vendor',type=Path,default=os.environ.get('__EGL_VENDOR_LIBRARY_FILENAMES'));p.add_argument('--pointer',type=Path,required=True);p.add_argument('--constraint-client',type=Path,required=True);p.add_argument('--font-config',type=Path,required=True);p.add_argument('--source-revision');p.add_argument('--parent-net');a=p.parse_args()
if not a.inside:
 if not a.output or not a.python_runtime:p.error('Fresh artifacts and pinned test runtime required')
 a.output=a.output.resolve();a.output.mkdir(mode=0o700,parents=True,exist_ok=False)
 a.launcher=a.launcher.resolve();a.sway=a.sway.resolve();a.pointer=a.pointer.resolve();a.constraint_client=a.constraint_client.resolve();a.font_config=a.font_config.resolve();runtime=Path(a.python_runtime).resolve()
 if not all(str(path).startswith('/nix/store/') for path in (a.launcher,a.sway,a.pointer,a.constraint_client,a.font_config,runtime)):raise ValueError('Pinned native tools required')
 revision=sp.check_output(['git','rev-parse','HEAD'],cwd=Path(__file__).resolve().parents[2],text=True).strip()
 args=[str(runtime),'/source/tests/desktop/window_controls_native.py','--inside','--launcher',str(a.launcher),'--sway',str(a.sway),'--pointer',str(a.pointer),'--constraint-client',str(a.constraint_client),'--font-config',str(a.font_config),'--source-revision',revision,'--parent-net',os.readlink('/proc/self/ns/net')]
 if a.effects:
  if not a.egl_vendor or not str(a.egl_vendor.resolve()).startswith('/nix/store/'):raise ValueError('Pinned Mesa vendor required')
  args+=['--effects','--egl-vendor',str(a.egl_vendor.resolve())]
 raise SystemExit(isolated_run(Path(__file__).resolve().parents[2],a.output,args,software_graphics=a.effects))
assert os.getuid()!=0 and os.readlink('/proc/self/ns/net')!=a.parent_net
assert not any((Path('/dev')/name).exists() for name in ('dri','snd','video0')) and Path('/dev/udmabuf').exists()==a.effects
a.output=Path('/artifacts')
repo=Path(__file__).resolve().parents[2];processes=[];original_socket=os.environ.get('SWAYSOCK')
with tempfile.TemporaryDirectory(prefix='lumen-display-spaces-') as temporary:
 private=Path(temporary);prefix=re.split(r'\n[ \t]*exec quickshell ',a.launcher.read_text(),maxsplit=1)[0]
 env=dict(v.decode().split('=',1) for v in sp.check_output(['bash','-c',prefix+'\nexec env -0']).split(b'\0') if b'=' in v)
 for key in ('HOME','XDG_RUNTIME_DIR','XDG_STATE_HOME','XDG_DATA_HOME','XDG_CONFIG_HOME','XDG_CACHE_HOME'):
  env[key]=str(private/key);Path(env[key]).mkdir(mode=0o700)
 for key in ('SWAYSOCK','WAYLAND_DISPLAY','DISPLAY','DBUS_SESSION_BUS_ADDRESS','AT_SPI_BUS_ADDRESS'):env.pop(key,None)
 env.update(WLR_BACKENDS='headless',WLR_HEADLESS_OUTPUTS='2',WLR_RENDERER='gles2' if a.effects else 'pixman',WLR_RENDERER_FORCE_SOFTWARE='1' if a.effects else '0',WLR_RENDERER_ALLOW_SOFTWARE='1',LIBGL_ALWAYS_SOFTWARE='1',WLR_LIBINPUT_NO_DEVICES='1',QT_QPA_PLATFORM='wayland',QT_QUICK_BACKEND='software',QT_QPA_PLATFORMTHEME='generic',QT_NO_XDG_DESKTOP_PORTAL='1',QT_ACCESSIBILITY='0',NO_AT_BRIDGE='1',GTK_A11Y='none')
 if a.effects:env['__EGL_VENDOR_LIBRARY_FILENAMES']=str(a.egl_vendor)
 env['FONTCONFIG_FILE']=str(a.font_config)
 settings=private/'settings.json';settings.write_text('{}');env['LUMEN_NIX_SETTINGS_ACTIVE']=str(settings)
 python=env.get('GLOBAL_MENU_PYTHON') or 'python3';backend=repo/'res/lumen-desktop/runtime/spaces.py'
 def launch(args,name,**kwargs):
  with (a.output/name).open('w') as log:proc=sp.Popen(args,env=env,stdout=log,stderr=sp.STDOUT,start_new_session=True,**kwargs)
  processes.append(proc);return proc
 def call(*args):return sp.check_output(list(map(str,args)),env=env,text=True,timeout=6).strip()
 def sway(*args):return json.loads(call(a.sway.parent/'swaymsg','-r',*args))
 def checked(command):
  result=sway(command);assert result and all(row.get('success') for row in result),(command,result)
 def tree():return sway('-t','get_tree')
 def visible():return {row['output']:row['name'] for row in sway('-t','get_workspaces') if row.get('visible')}
 def status():return json.loads(call(python,backend,'status'))
 def wait(fn):
  deadline=time.monotonic()+15
  while time.monotonic()<deadline:
   try:value=fn()
   except (sp.SubprocessError,OSError,ValueError):value=None
   if value:return value
   time.sleep(.04)
  (a.output/'timeout-tree.json').write_text(json.dumps(tree(),indent=2))
  (a.output/'timeout-seats.json').write_text(json.dumps(sway('-t','get_seats'),indent=2))
  raise AssertionError('Window-control observation deadline')
 def nodes(node):
  yield node
  for child in node.get('nodes',[])+node.get('floating_nodes',[]):yield from nodes(child)

 def native(cid):return next((n for n in nodes(tree()) if n.get('id')==cid),None)
 def named(title):return next((n for n in nodes(tree()) if n.get('name')==title and n.get('rect',{}).get('width',0)>0),None)
 def settled(cid):return wait(lambda:(n if (n:=native(cid)) and n.get('deco_rect',{}).get('height',0)>=32 and n['rect']['width']>100 else None))
 def click_control(cid,index):
  checked('[con_id=%d] focus'%cid);wait(lambda:native(cid).get('focused'));n=settled(cid);r=n['rect'];d=n['deco_rect'];x=r['x']+20+index*20;y=r['y']-d['height']+16
  checked('seat seat0 cursor set %d %d'%(x,y));pointer.stdin.write('m 0 0\np\nr\n');pointer.stdin.flush()
 def image(name):call('grim',a.output/name)
 def make(title):
  qml=private/(title.replace(' ','-')+'.qml');qml.write_text('import QtQuick\nimport Quickshell\nShellRoot {FloatingWindow {visible:true;title:'+json.dumps(title)+';implicitWidth:400;implicitHeight:240;TextEdit {anchors.fill:parent;focus:true;text:"Synthetic document"}}}')
  launch(['quickshell','-p',str(qml)],title.replace(' ','-')+'.log');return wait(lambda:named(title))
 checks=[]
 try:
  start_private_bus(env,a.output,processes)
  config=private/'sway.conf';config.write_text('output HEADLESS-1 resolution 1280x900\noutput HEADLESS-2 disable\nseat seat0 fallback true\nxwayland disable\nfocus_follows_mouse no\nfor_window [all] floating enable, border normal 1\n')
  launch([str(a.sway),'--unsupported-gpu','-c',str(config)],'sway.log')
  env['SWAYSOCK']=str(wait(lambda:next(Path(env['XDG_RUNTIME_DIR']).glob('sway-ipc*.sock'),None)))
  env['WAYLAND_DISPLAY']=wait(lambda:next((x.name for x in Path(env['XDG_RUNTIME_DIR']).glob('wayland-*') if not x.name.endswith('.lock')),None))
  pointer=launch([str(a.pointer),'pointer','move','0','0'],'pointer.log',stdin=sp.PIPE,text=True);pointer.stdin.write('m 0 0\n');pointer.stdin.flush()
  launch(['wtype','-s','120000'],'keyboard.log');wait(lambda:any(seat.get('capabilities',0)&1 for seat in sway('-t','get_seats')))
  fixed=launch([str(a.constraint_client),'fixed'],'fixed.log',stdin=sp.PIPE,text=True);f=wait(lambda:named('Constraint fixture'));cid=f['id'];f=settled(cid)
  original=dict(f['rect']);click_control(cid,2);call('wtype','-k','Escape');assert native(cid)['rect']==original and not native(cid)['fullscreen_mode'];image('fixed-disabled.png');checks.append('Physical green press/release on protocol fixed-size client leaves native geometry/fullscreen unchanged')
  for action in ('fullscreen','fill','zoom','left','right','top','bottom','top-left','top-right','bottom-left','bottom-right'):
   reply=sp.run([str(a.sway.parent/'swaymsg'),'-r','[con_id=%d] lumen_window '%cid+action],env=env,capture_output=True,text=True,timeout=6);assert reply.returncode==2 and all(not row.get('success') for row in json.loads(reply.stdout));assert native(cid)['rect']==original and not native(cid)['fullscreen_mode']
  checks.append('All eleven disabled presentation command entries reject explicitly without geometry/fullscreen mutation')
  click_control(cid,1);wait(lambda:native(cid)['lumen_state']['minimised']);checked('[con_id=%d] scratchpad show'%cid);wait(lambda:not native(cid)['lumen_state']['minimised']);assert native(cid)['rect']==original;checks.append('Fixed size does not incorrectly disable supported minimize/restore')
  fixed.stdin.write('q\n');fixed.stdin.flush();fixed.wait(timeout=3);wait(lambda:native(cid) is None)
  panel=make('Quick Look');cid=panel['id'];panel=settled(cid);original=dict(panel['rect'])
  for index in (1,2):click_control(cid,index);call('wtype','-k','Escape');assert native(cid)['rect']==original and not native(cid)['fullscreen_mode'] and not native(cid)['lumen_state']['minimised']
  for action in ('minimize','fullscreen'):
   reply=sp.run([str(a.sway.parent/'swaymsg'),'-r','[con_id=%d] lumen_window '%cid+action],env=env,capture_output=True,text=True,timeout=6);assert reply.returncode==2 and native(cid)['rect']==original and not native(cid)['lumen_state']['minimised']
  image('utility-disabled.png');checks.append('Physical yellow/green utility controls remain unavailable without hiding/resizing the captured panel')
  click_control(cid,0);wait(lambda:native(cid) is None);checks.append('Utility supported Close remains operable independently of unavailable actions')
  first=make('Document One');second=make('Document Two');ids=[first['id'],second['id']]
  for index,cid in enumerate(ids):checked('[con_id=%d] move position %d px %d px, resize set width %d px height %d px'%(cid,100+index*500,120+index*100,400+index*20,260+index*20))
  before={cid:dict(settled(cid)['rect']) for cid in ids}
  for cid in ids:click_control(cid,1);wait(lambda:native(cid)['lumen_state']['minimised'])
  assert all(native(cid)['lumen_state']['minimised'] for cid in ids)
  assets=private/'assets';shutil.copytree(repo/'res/lumen-desktop',assets)
  palette=json.loads((assets/'appearance/palette.json').read_text());appearance=private/'appearance.json';appearance.write_text(json.dumps(dict(font=palette['font'],palette=palette['dark'],preferences=dict(dockMagnification=False,reduceMotion=True))));env['LUMEN_APPEARANCE_STATE']=str(appearance)
  dockqml=private/'dock.qml';dockqml.write_text('''import QtQuick
import Quickshell
import Quickshell.Io
import Quickshell.Wayland
import "assets/shell" as D
ShellRoot {id:stub
 property var referenceDock:[{key:"documents",canvas:68,entry:{id:"synthetic-documents",name:"Documents",icon:"application-x-executable"}}]
 property string popupOwner:""
 property bool dockPreferencesBusy:false
 property string aliasError:""
 property string applicationCommandError:""
 property date currentDate:new Date()
 property var allWindows:ToplevelManager.toplevels.values
 property var focuses:[]
 signal requestDockFocus(string name)
 signal dismissPopups(string owner)
 function claimPopup(owner){popupOwner=owner;dismissPopups(owner);}
 function windowsFor(entry){return allWindows.filter(w=>["Document One","Document Two"].includes(w.title)).sort((a,b)=>a.title.localeCompare(b.title));}
 function dockWindows(item){return windowsFor(item.entry);}
 function dockLabel(item){return item.entry.name;}
 function runningFor(entry){return windowsFor(entry).length>0;}
 function canConfigureDockEntry(entry){return false;}
 function isDockPinned(entry){return false;}
 function canForceQuit(entry){return false;}
 function focusWindow(window){focuses=focuses.concat([window.title]);window.activate();}
 function activate(entry,fresh){throw new Error("Unexpected application launch");}
 function activateDock(item,fresh,output){throw new Error("Unexpected application activation");}
 function updateDockPin(entry,action){}
 function reorderDockPin(a,b){}
 D.Dock {id:dock;screen:Quickshell.screens[0];shell:stub}
 IpcHandler {target:"windowContracts"
 function status():string{return JSON.stringify({open:dock.contextEntry!==null,index:dock.contextIndex,selected:dock.contextSelection?dock.contextSelection.row.label:"",windows:stub.windowsFor(stub.referenceDock[0].entry).map(w=>({title:w.title,minimized:w.minimized})),focuses:stub.focuses});}
 function open():void{dock.openContext(stub.referenceDock[0],0);}
 }
}''')
  launch(['quickshell','-p',str(dockqml)],'dock.log')
  def dock(action):return call('quickshell','-p',dockqml,'ipc','call','windowContracts',action)
  def dockstate():return json.loads(dock('status'))
  wait(lambda:len(dockstate()['windows'])==2 and all(w['minimized'] for w in dockstate()['windows']))
  for cid in reversed(ids):
   title='Document One' if cid==ids[0] else 'Document Two';dock('open');wait(lambda:dockstate()['open'])
   # The production menu has Open New followed by the captured sorted window rows.
   call('wtype','-k','Down','-k','Down',*(['-k','Down'] if cid==ids[1] else []));wait(lambda:dockstate()['selected']=='Minimized — '+title)
   call('wtype','-k','Return');wait(lambda:not native(cid)['lumen_state']['minimised'] and native(cid)['focused']);assert native(cid)['rect']==before[cid] and native(cid)['name']==title
  assert dockstate()['focuses']==['Document Two','Document One'];image('restored-documents.png')
  checks.append('Two physical minimized documents restore through production application Dock rows with exact container/title/geometry and keyboard focus')
  (a.output/'result.json').write_text(json.dumps(dict(passed=True,checks=checks,source=a.source_revision,compositor=str(a.sway),liveSocketUnchanged=os.environ.get('SWAYSOCK')==original_socket),indent=2));print(json.dumps(dict(passed=True,checks=len(checks))))
 finally:
  for proc in reversed(processes):
   if proc.poll() is None:
    try:os.killpg(proc.pid,signal.SIGTERM)
    except ProcessLookupError:pass
  for proc in reversed(processes):
   try:proc.wait(timeout=5)
   except sp.TimeoutExpired:os.killpg(proc.pid,signal.SIGKILL);proc.wait(timeout=5)
  close_reaped_pipes(processes)
