"""Production confirmation and pidfd termination of isolated native clients."""
import argparse,hashlib,json,os,re,shutil,signal,socket,subprocess as sp,sys,tempfile,time
from pathlib import Path
from application_fixture.isolation import run
from application_fixture.provenance import FrozenSource,generated
from application_fixture.keyboard import PhysicalKeyboard
from private_native_bus import start_private_bus
from fixture_resources import close_reaped_pipes

p=argparse.ArgumentParser()
for name in ('launcher','sway','font-config','keyboard','output'):p.add_argument('--'+name,type=Path,required=True)
p.add_argument('--effects',action='store_true');p.add_argument('--egl-vendor',type=Path,default=os.environ.get('__EGL_VENDOR_LIBRARY_FILENAMES'));p.add_argument('--python-runtime',default=shutil.which('desktop-test-python'));p.add_argument('--inside',action='store_true');p.add_argument('--parent-net');a=p.parse_args()
repo=Path(__file__).resolve().parents[2]
if not a.inside:
 a.output=a.output.resolve();a.output.mkdir(mode=0o700,parents=True,exist_ok=False)
 tools=[a.launcher.resolve(),a.sway.resolve(),a.font_config.resolve(),a.keyboard.resolve(),Path(a.python_runtime).resolve()]
 if a.effects:tools.append(a.egl_vendor.resolve())
 frozen=FrozenSource(repo,__file__,tools)
 argv=[str(tools[4]),'/source/tests/desktop/essential_dialog_typography_native.py','--inside','--launcher',str(tools[0]),'--sway',str(tools[1]),'--font-config',str(tools[2]),'--keyboard',str(tools[3]),'--output','/artifacts/result','--parent-net',os.readlink('/proc/self/ns/net')]
 if a.effects:argv+=['--effects','--egl-vendor',str(tools[5])]
 code=1
 try:code=run(repo,a.output,argv,software_graphics=a.effects)
 finally:frozen.finish(a.output,code)
 raise SystemExit(code)
assert os.getuid()!=0 and os.readlink('/proc/self/ns/net')!=a.parent_net
assert [name for _,name in socket.if_nameindex()]==['lo']
assert not any((Path('/dev')/name).exists() for name in ('dri','snd','video0')) and Path('/dev/udmabuf').exists()==a.effects
out=a.output;out.mkdir(mode=0o700);processes=[];checks=[];receipts=[];original_socket=os.environ.get('SWAYSOCK')
with tempfile.TemporaryDirectory(prefix='essential-dialog-native-') as temporary:
 private=Path(temporary);env=dict(os.environ)
 for key in ('HOME','XDG_RUNTIME_DIR','XDG_STATE_HOME','XDG_CONFIG_HOME','XDG_DATA_HOME','XDG_CACHE_HOME'):
  env[key]=str(private/key);Path(env[key]).mkdir(mode=0o700)
 prefix=re.split(r'\n[ \t]*exec quickshell ',a.launcher.read_text(),maxsplit=1)[0]
 env.update(dict(v.decode().split('=',1) for v in sp.check_output(['bash','-c',prefix+'\nexec env -0'],env=env).split(b'\0') if b'=' in v))
 for key in ('DISPLAY','WAYLAND_DISPLAY','SWAYSOCK','DBUS_SESSION_BUS_ADDRESS','AT_SPI_BUS_ADDRESS'):env.pop(key,None)
 env.update(WLR_BACKENDS='headless',WLR_RENDERER='gles2' if a.effects else 'pixman',WLR_RENDERER_FORCE_SOFTWARE='1' if a.effects else '0',WLR_RENDERER_ALLOW_SOFTWARE='1',LIBGL_ALWAYS_SOFTWARE='1',WLR_LIBINPUT_NO_DEVICES='1',QT_QPA_PLATFORM='wayland',QT_QPA_PLATFORMTHEME='generic',QT_QUICK_BACKEND='software',QT_ACCESSIBILITY='0',NO_AT_BRIDGE='1',GTK_A11Y='none',GDK_BACKEND='wayland',FONTCONFIG_FILE=str(a.font_config))
 if a.effects:env['__EGL_VENDOR_LIBRARY_FILENAMES']=str(a.egl_vendor)
 assets=Path(re.search(r'exec quickshell .*--path (\S+)/shell',a.launcher.read_text()).group(1))
 names=('ForceQuitDialog.qml','WindowPairPicker.qml','DisplaySettingsPane.qml','Theme.qml','FocusFlickable.qml','DialogButton.qml')
 for name in names:assert (assets/'shell'/name).read_bytes()==(repo/'res/lumen-desktop/shell'/name).read_bytes(),name
 shutil.copytree(repo/'res/lumen-desktop/shell',private/'desktop');desktop=private/'desktop'
 source=dict(id=101,pid=501,title='Private captured document with a long descriptive title',output='HEADLESS-1',pair=dict(id=0,eligible=True,survivor=False))
 candidates=[dict(id=102,pid=502,title='Compatible private partner with a descriptive document title',app='Private application')]
 seams=[]
 for filename,payload in [('window_pair.py',dict(source=source,candidates=candidates)),('display_settings.py',dict(outputs=[dict(name='HEADLESS-1',label='Private display',mode='700x480@60',modes=['700x480@60'],scale=1,x=0,y=0)],pending='',seconds=0))]:
  helper=desktop/filename;before=helper.read_bytes();helper.write_text('import json,sys\nr=json.loads(sys.stdin.readline());assert r["action"]=="status","Mutations forbidden"\nprint('+repr(json.dumps(payload))+')\n');seams.append(dict(path=filename,beforeSha256=hashlib.sha256(before).hexdigest(),afterSha256=hashlib.sha256(helper.read_bytes()).hexdigest()))
 (out/'provider-seams.json').write_text(json.dumps(dict(scope='Read-only synthetic status; mutations rejected',providers=seams),indent=2))
 palette=json.loads((assets/'appearance/palette.json').read_text());appearance=private/'appearance.json';appearance.write_text(json.dumps(dict(font=palette['font'],palette=palette['dark'],preferences=dict(textScale=1.5,reduceMotion=True))));env['LUMEN_APPEARANCE_STATE']=str(appearance);env['GLOBAL_MENU_PYTHON']=sys.executable
 shell=private/'fixture.qml';shell.write_text('''import QtQuick
import QtQuick.Controls
import QtQuick.Window
import Quickshell
import Quickshell.Io
import "desktop" as D
ShellRoot {
 id:audit
 property string popupOwner:""
 signal dismissPopups(string owner)
 function preferredOutput(){return "HEADLESS-1";}
 function claimPopup(owner){popupOwner=owner;}
 property var owner:null
 D.ForceQuitDialog {id:force;shell:audit}
 D.WindowPairPicker {id:pair;shell:audit}
 FloatingWindow {id:displays;visible:false;implicitWidth:340;implicitHeight:440;color:D.Theme.colors.background
  D.FocusFlickable {id:displayScroll;anchors.fill:parent;anchors.margins:16;contentHeight:pane.height;clip:true
   D.DisplaySettingsPane {id:pane;width:displayScroll.width}
   ScrollBar.vertical:D.DesktopScrollBar {}
  }
 }
 function walk(item,seen){seen=seen || [];if(!item || seen.includes(item))return [];seen.push(item);let items=[item];for(const child of item.children || [])items=items.concat(walk(child,seen));if(item.contentItem)items=items.concat(walk(item.contentItem,seen));return items;}
 function focusLabel(item){while(item){if(item.Accessible && item.Accessible.name)return item.Accessible.name;if(item.text)return item.text;item=item.parent;}return "";}
 IpcHandler {target:"essentialDialogs"
  function open(kind:string):void {force.visible=false;pair.visible=false;displays.visible=false;if(kind==="Force"){audit.owner=force;force.visible=true;force.applicationName="Private captured document application with a long descriptive name";force.fail("The captured application changed while preparing this request. Close and select the intended application again.");}else if(kind==="Pair"){audit.owner=pair;pair.open(101,"left");}else{audit.owner=displays;displays.visible=true;displays.contentItem.Window.window.requestActivate();}}
  function focus(name:string):void {const item=audit.walk(audit.owner.contentItem).find(item=>item.Accessible && item.Accessible.name===name);if(!item)throw Error("No exact focus target "+name);item.forceActiveFocus();}
  function status():string {if(!audit.owner)return "{}";const window=audit.owner.contentItem.Window.window;if(!window)return JSON.stringify({visible:audit.owner.visible});const focus=window.activeFocusItem,p=focus?focus.mapToItem(audit.owner.contentItem,0,0):Qt.point(-1,-1);return JSON.stringify({visible:audit.owner.visible,active:window.active,focus:audit.focusLabel(focus),width:audit.owner.width,height:audit.owner.height,rect:focus?{x:p.x,y:p.y,width:focus.width,height:focus.height}:null,selected:pair.selected,pairReady:pair.canChoose && pair.candidates.length===1,scale:D.Theme.textScale,phase:force.phase,positionX:audit.walk(pane).find(item=>item.objectName==="displayX").text,labels:audit.walk(audit.owner.contentItem).filter(item=>item.text && item.font).map(item=>({text:item.text,size:item.font.pixelSize}))});}
 }
}
''')
 def launch(argv,name):
  with (out/name).open('w')as log:child=sp.Popen(list(map(str,argv)),env=env,stdout=log,stderr=sp.STDOUT,start_new_session=True)
  processes.append(child);return child
 def call(*args):return sp.check_output(list(map(str,args)),env=env,text=True,timeout=6).strip()
 def ipc(method,*args):return call('quickshell','-p',shell,'ipc','call','essentialDialogs',method,*args)
 def state():return json.loads(ipc('status'))
 def wait(predicate,label):
  deadline=time.monotonic()+15
  while time.monotonic()<deadline:
   try:
    value=predicate()
    if value:return value
   except (ValueError,OSError,sp.SubprocessError):pass
   time.sleep(.04)
  try:(out/'failure.json').write_text(json.dumps(dict(label=label,state=state()),indent=2))
  except (ValueError,OSError,sp.SubprocessError):pass
  raise AssertionError(label)
 def contained():
  value=state();r=value['rect'];assert r and r['y']>=-1 and r['x']>=-1 and r['x']+r['width']<=value['width']+1 and r['y']+r['height']<=value['height']+1,value
  assert value['scale']==1.5;return value
 keyboard=None
 try:
  start_private_bus(env,out,processes)
  config=private/'sway.conf';config.write_text('include /source/res/lumen-desktop/sway.conf\noutput HEADLESS-1 resolution 700x480\noutput * bg #29445b solid_color\nfocus_follows_mouse no\n')
  launch([a.sway,'--unsupported-gpu','-c',config],'sway.log');runtime=Path(env['XDG_RUNTIME_DIR']);wait(lambda:next(runtime.glob('sway-ipc*.sock'),None),'private compositor');env['SWAYSOCK']=str(next(runtime.glob('sway-ipc*.sock')));env['WAYLAND_DISPLAY']=next(path.name for path in runtime.glob('wayland-*')if not path.name.endswith('.lock'))
  launch(['quickshell','--no-duplicate','-p',shell],'shell.log');wait(lambda:ipc('status')=='{}','owned fixture IPC')
  with (out/'keyboard.log').open('w')as log:keyboard=PhysicalKeyboard(a.keyboard,env,log)
  processes.append(keyboard.process)
  for key in ('Return','Escape'):
   ipc('open','Force');wait(lambda:state().get('focus')=='Close' and state()['visible'],'actual error Close focus');receipt=contained();assert any(row['text'].startswith('Force Quit Private')and row['size']==26 for row in receipt['labels']);call('grim','-o','HEADLESS-1',out/('Force-'+key+'.png'));keyboard.key('-k',key);wait(lambda:not state()['visible'],'physical error '+key+' cancels');checks.append('Force Quit error '+key+' closes captured decision without confirmation');receipts.append(receipt)
  ipc('open','Pair');wait(lambda:state().get('pairReady'),'actual compatible partner list');name=candidates[0]['title']+' · '+candidates[0]['app'];ipc('focus',name);wait(lambda:state()['focus']==name,'captured candidate keyboard recipient');keyboard.key('-k','space');wait(lambda:state()['selected']==0,'physical Space selects exact partner');checks.append('Pair candidate physical Space selects exact captured row')
  ipc('focus','Cancel');wait(lambda:state()['focus']=='Cancel','actual Cancel focus');keyboard.key('-k','Tab');wait(lambda:state()['focus']=='Create Pair','physical Tab reaches final pair action');receipts.append(contained());call('grim','-o','HEADLESS-1',out/'Pair-final-action.png');keyboard.key('-k','Escape');wait(lambda:not state()['visible'],'physical Pair Escape');checks.append('Pair final action remains revealed at150 percent and physical Escape cancels without dispatch')
  ipc('open','Displays');wait(lambda:state().get('active'),'actual display window active');ipc('focus','Display x position');wait(lambda:state()['focus']=='Display x position','position field recipient');keyboard.key('-M','ctrl','-k','a','-m','ctrl');keyboard.type('12');wait(lambda:state()['positionX']=='12','actual physical position edit');checks.append('Display position field accepts physical editing without Apply')
  seen=[]
  for _ in range(12):
   keyboard.key('-k','Tab');value=state();seen.append(value['focus']);contained()
   if value['focus']=='Forget Saved Display Layout':break
  assert seen[-1]=='Forget Saved Display Layout',seen;receipts.append(state());call('grim','-o','HEADLESS-1',out/'Displays-final-action.png');checks.append('Physical display Tab traverses Apply and reaches final Forget action through focus-aware scroll without invoking mutation')
 finally:
  if keyboard:keyboard.close()
  for child in reversed(processes):
   if child.poll()is None:
    os.killpg(child.pid,signal.SIGTERM)
    try:child.wait(timeout=5)
    except sp.TimeoutExpired:os.killpg(child.pid,signal.SIGKILL);child.wait(timeout=5)
  close_reaped_pipes(processes)
 warnings={path.name:re.findall(r'^.*(?:WARN|WARNING|ResourceWarning|Exception ignored|TypeError|ReferenceError|ERROR).*$',path.read_text(),re.M)for path in out.glob('*.log')};warnings={key:value for key,value in warnings.items()if value};(out/'warning-scan.json').write_text(json.dumps(warnings,indent=2));assert not warnings,warnings
 (out/'result.json').write_text(json.dumps(dict(passed=True,checks=checks,receipts=receipts,effects=a.effects,scope='Actual native layer-shell Force/Pairs; constrained display window with private read-only status. Physical keys cancel/select/traverse; no provider mutation or all application/ref parity claim'),indent=2));print(json.dumps(dict(passed=True,checks=len(checks))))
