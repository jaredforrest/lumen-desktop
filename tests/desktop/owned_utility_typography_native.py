"""Actual scaled utility keyboard reachability and cancellation in a private compositor."""
import argparse,hashlib,json,os,re,shutil,signal,subprocess as sp,sys,time
from pathlib import Path
from application_fixture import isolation
from application_fixture.keyboard import PhysicalKeyboard
from private_native_bus import start_private_bus
from fixture_resources import close_reaped_pipes
p=argparse.ArgumentParser();p.add_argument('--launcher',type=Path,required=True);p.add_argument('--sway',type=Path,required=True);p.add_argument('--keyboard',type=Path,required=True);p.add_argument('--font-config',type=Path,required=True);p.add_argument('--software-driver',type=Path,required=True);p.add_argument('--effects',action='store_true');p.add_argument('--inside',action='store_true');p.add_argument('--output',type=Path,required=True);a=p.parse_args();repo=Path(__file__).resolve().parents[2]
for name in ('launcher','sway','keyboard','font_config','software_driver'):
 value=getattr(a,name).resolve();assert str(value).startswith('/nix/store/')and value.exists(),name;setattr(a,name,value)
if not a.inside:
 assert not sp.check_output(['git','-C',str(repo),'status','--porcelain'],text=True).strip(),'Freeze source before acceptance'
 revision=sp.check_output(['git','-C',str(repo),'rev-parse','HEAD'],text=True).strip();out=a.output.resolve();out.mkdir(mode=0o700,parents=True,exist_ok=False);prefix=re.split(r'\n[ \t]*exec quickshell ',a.launcher.read_text(),maxsplit=1)[0];env=dict(v.decode().split('=',1)for v in sp.check_output(['bash','-c',prefix+'\nexec env -0']).split(b'\0')if b'='in v);env['FONTCONFIG_FILE']=str(a.font_config)
 command=[sys.executable,'/source/tests/desktop/owned_utility_typography_native.py','--inside','--output','/artifacts']+sum(([ '--'+name.replace('_','-'),str(getattr(a,name))]for name in ('launcher','sway','keyboard','font_config','software_driver')),[])+(['--effects']if a.effects else []);source_hash=hashlib.sha256(Path(__file__).read_bytes()).hexdigest();code=isolation.run(repo,out,command,env,software_graphics=a.effects);unchanged=sp.check_output(['git','-C',str(repo),'rev-parse','HEAD'],text=True).strip()==revision and not sp.check_output(['git','-C',str(repo),'status','--porcelain'],text=True).strip();(out/'provenance.json').write_text(json.dumps(dict(revision=revision,sourceSha256=source_hash,sourceUnchanged=unchanged,exitCode=code,launcher=str(a.launcher),compositor=str(a.sway),fontConfig=str(a.font_config)),indent=2));assert unchanged;raise SystemExit(code)
assert repo==Path('/source')and os.environ['HOME']=='/home/fixture'and os.getuid()!=0 and not Path('/.old-root').exists();assert not any((Path('/dev')/name).exists()for name in ('dri','snd','video0','tty'))
out=a.output;env=dict(os.environ,FONTCONFIG_FILE=str(a.font_config),LANG='C.UTF-8',LC_ALL='C.UTF-8',WLR_BACKENDS='headless',WLR_HEADLESS_OUTPUTS='1',WLR_RENDERER='gles2'if a.effects else 'pixman',WLR_RENDERER_FORCE_SOFTWARE='1'if a.effects else '0',WLR_RENDERER_ALLOW_SOFTWARE='1',WLR_LIBINPUT_NO_DEVICES='1',LIBGL_ALWAYS_SOFTWARE='1',MESA_LOADER_DRIVER_OVERRIDE='llvmpipe',LIBGL_DRIVERS_PATH=str(a.software_driver/'lib/dri'),__EGL_VENDOR_LIBRARY_FILENAMES=str(a.software_driver/'share/glvnd/egl_vendor.d/50_mesa.json'),QT_QPA_PLATFORM='wayland',QT_QUICK_BACKEND='software',QT_QPA_PLATFORMTHEME='generic',QT_ACCESSIBILITY='0',NO_AT_BRIDGE='1',GTK_A11Y='none',LUMEN_UNIFIED_OWNED_HEADERS='1',LUMEN_NATIVE_WINDOW_CONTROLS='0',LUMEN_NATIVE_WINDOW_ACTIONS='0',LUMEN_FILE_SERVICE='0',LUMEN_LAUNCH_SCOPES='0',LUMEN_NEARBY_SHARING_PROVIDER='',GLOBAL_MENU_PYTHON=sys.executable)
for key in ('DISPLAY','WAYLAND_DISPLAY','SWAYSOCK'):env.pop(key,None)
assets=out/'desktop';shutil.copytree(repo/'res/lumen-desktop/shell',assets);palette=json.loads((repo/'res/lumen-desktop/appearance/palette.json').read_text());appearance=out/'appearance.json';env['LUMEN_APPEARANCE_STATE']=str(appearance);appearance.write_text(json.dumps(dict(font=palette['font'],palette=palette['light'],preferences=dict(textScale=1.5,reduceMotion=True,reduceTransparency=True))))
qml=out/'fixture.qml';qml.write_text('''import QtQuick
import QtQuick.Controls
import QtQuick.Window
import Quickshell
import Quickshell.Io
import "desktop" as D
ShellRoot {
 id:audit
 QtObject {id:stub;function claimPopup(name){}property var exportedMenu:({state:{windowId:0,appName:"Private application",menu:null}})}
 D.TextToolsWindow {id:tools;shell:stub;page:"Shortcuts"}
 D.RemoteDesktopWindow {id:remote;shell:stub;connections:[{id:"private",name:"Private computer",protocol:"RDP",host:"192.0.2.1"}]}
 D.FileInfoWindow {id:info;details:({name:"Private document.txt",size:"12 bytes",kind:"Text",location:"/private/directory",permissions:"Read and write",owner:"Synthetic owner"})}

 property var owner:null
 function walk(item,seen){if(!item || seen.includes(item))return [];seen.push(item);let result=[item];for(const child of item.children || [])result=result.concat(walk(child,seen));if(item.contentItem)result=result.concat(walk(item.contentItem,seen));return result;}
 function shown(item){while(item && item!==owner.contentItem){if(!item.visible)return false;item=item.parent;}return item===owner.contentItem;}
 IpcHandler {target:"utilityTypography"
  function open(kind:string):void {tools.visible=false;remote.visible=false;info.visible=false;audit.owner=kind==="TextTools"?tools:kind==="Remote"?remote:info;owner.visible=true;owner.implicitWidth=kind==="TextTools"?520:kind==="Remote"?480:360;owner.implicitHeight=320;Qt.callLater(()=>{owner.contentItem.Window.window.requestActivate();const name=kind==="TextTools"?"Menu command path":kind==="Remote"?"Remote computer address":"General";const item=audit.walk(owner.contentItem,[]).find(item=>item.Accessible.name===name);item.forceActiveFocus();});}
  function state():string {if(!owner)return "{}";const window=owner.contentItem.Window.window,focus=window.activeFocusItem,items=audit.walk(owner.contentItem,[]),item=items.find(item=>item.Accessible.name===(owner===tools?"Menu command path":owner===remote?"Remote computer address":"General"));let p=focus?focus.mapToItem(owner.contentItem,0,0):Qt.point(-1,-1);return JSON.stringify({visible:owner.visible,active:window.active,focus:focus?focus.Accessible.name:"",focusRect:focus?{x:p.x,y:p.y,width:focus.width,height:focus.height}:null,width:owner.width,height:owner.height,value:item.text || "",expanded:item.Accessible.description || "",sections:items.filter(item=>["General","Sharing & Permissions"].includes(item.Accessible.name)).map(item=>({name:item.Accessible.name,state:item.Accessible.description})),scale:D.Theme.preferences.textScale});}
 }
}''')
config=out/'sway.conf';config.write_text('include /source/res/lumen-desktop/sway.conf\noutput HEADLESS-1 resolution 900x700\noutput * bg #29445b solid_color\nfocus_follows_mouse no\n');processes=[];keyboard=None;records=[]
def launch(command,name):
 with (out/name).open('w')as log:child=sp.Popen(list(map(str,command)),env=env,stdout=log,stderr=sp.STDOUT,start_new_session=True)
 processes.append(child);return child
def call(*command):return sp.check_output(list(map(str,command)),env=env,text=True,timeout=5).strip()
def ipc(method,*args):return call('quickshell','ipc','-p',qml,'call','utilityTypography',method,*args)
def state():return json.loads(ipc('state'))
def wait(predicate,label):
 deadline=time.monotonic()+15
 while time.monotonic()<deadline:
  try:
   if predicate():return
  except (ValueError,sp.SubprocessError):pass
  time.sleep(.03)
 (out/'failure.json').write_text(json.dumps(dict(label=label,state=state()),indent=2));raise AssertionError(label)
try:
 start_private_bus(env,out,processes);launch([a.sway,'-c',config],'sway.log');runtime=Path(env['XDG_RUNTIME_DIR']);wait(lambda:next(runtime.glob('sway-ipc*.sock'),None),'private compositor');env['SWAYSOCK']=str(next(runtime.glob('sway-ipc*.sock')));env['WAYLAND_DISPLAY']=next(path.name for path in runtime.glob('wayland-*')if not path.name.endswith('.lock'));launch(['quickshell','--no-duplicate','-p',qml],'shell.log')
 with (out/'keyboard.log').open('w')as log:keyboard=PhysicalKeyboard(a.keyboard,env,log)
 processes.append(keyboard.process);wait(lambda:ipc('state')=='{}','private utility IPC ready')
 for kind,initial,after in [('TextTools','Menu command path','Save Application Shortcut'),('Remote','Remote computer address','Open'),('Info','General','Sharing & Permissions')]:
  ipc('open',kind);wait(lambda:state().get('active')and state()['focus']==initial,'actual '+kind+' initial keyboard recipient');assert state()['scale']==1.5
  if kind!='Info':keyboard.type('Private');wait(lambda:state()['value']=='Private','physical scaled input '+kind)
  keyboard.key('-k','Tab');wait(lambda:state()['focus']==after,'physical Tab reaches '+after);row=state();r=row['focusRect'];assert r['x']>=0 and r['y']>=0 and r['x']+r['width']<=row['width']+1 and r['y']+r['height']<=row['height']+1,row
  if kind=='Info':keyboard.key('-k','space');wait(lambda:any(item['name']=='Sharing & Permissions'and item['state']=='Collapsed'for item in state()['sections'])and state()['expanded']=='Expanded','physical Space collapses exact Sharing section, preserving General')
  call('grim','-o','HEADLESS-1',out/(kind+'.png'));keyboard.key('-k','Escape');wait(lambda:not state()['visible'],'physical Escape '+kind);records.append(dict(consumer=kind,before=initial,after=row))
 assert not re.search(r'ReferenceError|TypeError|binding loop',(out/'shell.log').read_text())
 (out/'result.json').write_text(json.dumps(dict(passed=True,cases=records,scope='Actual physical scaled input, Tab reachability and Escape in three constrained production utility families; no external apps or reference measurements'),indent=2))
finally:
 if keyboard:keyboard.close()
 for child in reversed(processes):
  if child.poll()is None:
   os.killpg(child.pid,signal.SIGTERM)
   try:child.wait(timeout=5)
   except sp.TimeoutExpired:os.killpg(child.pid,signal.SIGKILL);child.wait(timeout=5)
 close_reaped_pipes(processes)
