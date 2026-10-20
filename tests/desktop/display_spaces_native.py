"""Private two-output native cohort transitions; never operates a live session."""
import argparse,json,os,re,shutil,signal,subprocess as sp,sys,tempfile,time
from pathlib import Path
from application_fixture.isolation import run as isolated_run
from private_native_bus import start_private_bus
from fixture_resources import close_reaped_pipes
p=argparse.ArgumentParser();p.add_argument('--launcher',type=Path,required=True);p.add_argument('--sway',type=Path,required=True);p.add_argument('--output',type=Path);p.add_argument('--effects',action='store_true');p.add_argument('--inside',action='store_true');p.add_argument('--python-runtime',default=shutil.which('desktop-test-python'));p.add_argument('--egl-vendor',type=Path,default=os.environ.get('__EGL_VENDOR_LIBRARY_FILENAMES'));p.add_argument('--client',type=Path,required=True);p.add_argument('--font-config',type=Path,required=True);p.add_argument('--source-revision');p.add_argument('--parent-net');a=p.parse_args()
if not a.inside:
 if not a.output or not a.python_runtime:p.error('Fresh artifacts and pinned test runtime required')
 a.output=a.output.resolve();a.output.mkdir(mode=0o700,parents=True,exist_ok=False)
 a.launcher=a.launcher.resolve();a.sway=a.sway.resolve();a.client=a.client.resolve();a.font_config=a.font_config.resolve();runtime=Path(a.python_runtime).resolve()
 if not all(str(path).startswith('/nix/store/') for path in (a.launcher,a.sway,a.client,a.font_config,runtime)):raise ValueError('Pinned native tools required')
 revision=sp.check_output(['git','rev-parse','HEAD'],cwd=Path(__file__).resolve().parents[2],text=True).strip()
 args=[str(runtime),'/source/tests/desktop/display_spaces_native.py','--inside','--launcher',str(a.launcher),'--sway',str(a.sway),'--client',str(a.client),'--font-config',str(a.font_config),'--source-revision',revision,'--parent-net',os.readlink('/proc/self/ns/net')]
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
  raise AssertionError('Display-space observation deadline')
 def nodes(node):
  yield node
  for child in node.get('nodes',[])+node.get('floating_nodes',[]):yield from nodes(child)
 clients={}
 def foot(app):
  qml=private/(app+'.qml');clients[app]=qml
  qml.write_text('import QtQuick\nimport Quickshell\nimport Quickshell.Io\nShellRoot {FloatingWindow {visible:true;title:'+json.dumps(app)+';implicitWidth:400;implicitHeight:240;TextEdit {id:input;anchors.fill:parent;focus:true}} IpcHandler {target:"client";function text():string{return input.text;}}}')
  launch(['quickshell','-p',str(qml)],app+'.log')
  return wait(lambda:next((n for n in nodes(tree()) if n.get('name')==app and n.get('app_id')=='org.quickshell'),None))
 def typed(app):return call('quickshell','-p',clients[app],'ipc','call','client','text')
 checks=[]
 try:
  start_private_bus(env,a.output,processes)
  config=private/'sway.conf';config.write_text('output HEADLESS-1 resolution 1280x900 position 0 0\noutput HEADLESS-2 resolution 1024x768 position 1280 0\nseat seat0 fallback true\nxwayland disable\nfocus_follows_mouse no\nfor_window [all] floating enable, border pixel 0\n')
  launch([str(a.sway),'--unsupported-gpu','-c',str(config)],'sway.log')
  env['SWAYSOCK']=str(wait(lambda:next(Path(env['XDG_RUNTIME_DIR']).glob('sway-ipc*.sock'),None)))
  env['WAYLAND_DISPLAY']=wait(lambda:next((x.name for x in Path(env['XDG_RUNTIME_DIR']).glob('wayland-*') if not x.name.endswith('.lock')),None))
  launch(['wtype','-s','120000'],'keyboard.log')
  checked('focus output HEADLESS-1; workspace "first"; move workspace to output HEADLESS-1');first=foot('display-first')
  checked('workspace "second"');second=foot('display-second')
  checked('focus output HEADLESS-2; workspace "other"; move workspace to output HEADLESS-2');other=foot('display-other')
  before=visible();state=status();(a.output/'initial-status.json').write_text(json.dumps(state,indent=2));(a.output/'initial-tree.json').write_text(json.dumps(tree(),indent=2));assert state['displaySpaces']['applied'] and state['displaySpaces']['separateDisplays'];assert visible()==before
  checked('[con_id=%d] focus'%first['id']);assert visible()['HEADLESS-2']=='other';checks.append('Default independent focus leaves the other output unchanged')
  before=visible();settings.write_text(json.dumps(dict(spaces=dict(separateDisplays=False))));state=status();assert state['displaySpaces']['coupled'],state
  assert visible()==before;assert tree()['display_spaces']['primary']=='HEADLESS-1';checks.append('Coupling preserves visible composition and chooses geometry-based main independently of focus')
  policy=json.loads((Path(env['XDG_STATE_HOME'])/'lumen-desktop/spaces.json').read_text());members=policy['display_cohorts'];assert len(members)>=2
  bykey=state['desktops'];cohort=next(c for c in members if any(bykey[k]['name']=='second' for k in c['members'].values()))
  expected={bykey[k]['output']:bykey[k]['name'] for k in cohort['members'].values()}
  checked('[con_id=%d] focus'%second['id']);assert visible()==expected,(visible(),expected)
  assert next(n['id'] for n in nodes(tree()) if n.get('focused'))==second['id'];call('wtype','-s','1','cohort keyboard recipient','-k','Return');wait(lambda:typed('display-second')=='cohort keyboard recipient');assert typed('display-first')=='' and typed('display-other')=='';checks.append('Native container focus switches the full cohort while preserving invoked keyboard focus')
  native=tree()['display_spaces'];packet=dict(session=native['session'],expectedGeneration=native['generation'],cohort=cohort['id'],output='HEADLESS-2',workspace=expected['HEADLESS-2'],workspaceId=str(next(row['id'] for row in sway('-t','get_workspaces') if row['name']==expected['HEADLESS-2'])));command='lumen_window display-spaces activate '+json.dumps(packet).encode().hex()
  checked(command);assert visible()==expected;assert next(w['output'] for w in sway('-t','get_workspaces') if w.get('focused'))=='HEADLESS-2';checks.append('Captured native cohort activation retains requested output focus')
  before=visible();stale='lumen_window display-spaces activate '+json.dumps(dict(packet,expectedGeneration=str(int(native['generation'])+1))).encode().hex();reply=sp.run([str(a.sway.parent/'swaymsg'),'-r',stale],env=env,capture_output=True,text=True,timeout=6);result=json.loads(reply.stdout);assert reply.returncode==2 and not all(row.get('success') for row in result);assert visible()==before;checks.append('Stale generation rejects without partial visibility change')
  wrong='lumen_window display-spaces activate '+json.dumps(dict(packet,workspace='wrong captured member')).encode().hex();before=visible();reply=sp.run([str(a.sway.parent/'swaymsg'),'-r',wrong],env=env,capture_output=True,text=True,timeout=6);assert reply.returncode==2 and visible()==before;checks.append('Current generation with a mismatched captured desktop member rejects without visibility changes')
  checked('[con_id=%d] focus'%other['id']);assert visible()['HEADLESS-1']=='first';checks.append('Previously empty peer is recreated without moving a user window')
  settings.write_text('{}');before=visible();state=status();assert state['displaySpaces']['separateDisplays'];assert visible()==before;assert all(next((n for n in nodes(tree()) if n.get('id')==client['id']),None) for client in [first,second,other]);checks.append('Returning to independent mode preserves desktops and exact clients')
  # Explicit preferred identity survives disconnect with a deterministic temporary main.
  settings.write_text(json.dumps(dict(spaces=dict(separateDisplays=False,primaryDisplay=dict(kind='connector',name='HEADLESS-2')))))
  state=status();assert state['displaySpaces']['primary']=='HEADLESS-2'
  keys=set(state['desktops']);checked('output HEADLESS-2 disable');state=status();assert state['displaySpaces']['applied'] and state['displaySpaces']['fallback'] and state['displaySpaces']['primary']=='HEADLESS-1',state
  assert keys <= set(state['desktops']);assert all(row['output']=='HEADLESS-1' for row in state['desktops'].values())
  # Every retained logical desktop can still be invoked after cohort repacking.
  for key in keys:
   sp.check_output([python,str(backend),'status'],env=env,text=True,timeout=8)
   state=status();item=state['desktops'][key];checked('workspace '+json.dumps(item['name']))
   assert visible()['HEADLESS-1']==item['name']
  checks.append('Output loss repacks retained logical desktops into reachable cohorts without deleting identity')
  checked('output HEADLESS-2 enable');state=status();assert state['displaySpaces']['primary']=='HEADLESS-2' and not state['displaySpaces']['fallback'];assert keys <= set(state['desktops']);checks.append('Preferred main identity returns on reconnect without moving windows back')
  # A pre-existing secondary pair survives a policy toggle and can always exit.
  settings.write_text(json.dumps(dict(spaces=dict(separateDisplays=True,primaryDisplay=dict(kind='connector',name='HEADLESS-1')))));status()
  checked('focus output HEADLESS-2; workspace "pair-source"; move workspace to output HEADLESS-2')
  pair_a=foot('display-pair-a');pair_b=foot('display-pair-b')
  def client(identifier):return next(n for n in nodes(tree()) if n.get('id')==identifier)
  original={n['id']:client(n['id'])['rect'] for n in (pair_a,pair_b)}
  checked('lumen_pair create %d %d'%(pair_a['id'],pair_b['id']));pair_id=client(pair_a['id'])['lumen_pair']['id']
  settings.write_text(json.dumps(dict(spaces=dict(separateDisplays=False,primaryDisplay=dict(kind='connector',name='HEADLESS-1')))));state=status();assert state['displaySpaces']['applied']
  assert client(pair_a['id'])['lumen_pair']['id']==pair_id
  checked('lumen_pair ratio %d 0.6'%pair_a['id']);assert client(pair_a['id'])['lumen_pair']['id']==pair_id
  checked('lumen_pair leave %d'%pair_a['id']);checked('lumen_pair leave %d'%pair_b['id'])
  wait(lambda:all(client(n['id'])['rect']==original[n['id']] for n in (pair_a,pair_b)));checks.append('Existing secondary pair survives toggle and ratio change; both members restore exact prior geometry on exit')
  status();command='lumen_pair create %d %d'%(pair_a['id'],pair_b['id'])
  reply=sp.run([str(a.sway.parent/'swaymsg'),'-r',command],env=env,capture_output=True,text=True,timeout=6);assert reply.returncode==2,reply.stdout
  assert all(not client(n['id']).get('lumen_pair',{}).get('id') for n in (pair_a,pair_b));checks.append('New secondary pair refuses safely in coupled mode')
  checked('focus output HEADLESS-1; workspace "primary-pair-source"; move workspace to output HEADLESS-1')
  primary_a=foot('display-primary-a');primary_b=foot('display-primary-b');status()
  primary_original={n['id']:client(n['id'])['rect'] for n in (primary_a,primary_b)}
  checked('lumen_pair create %d %d'%(primary_a['id'],primary_b['id']));status();assert client(primary_a['id'])['lumen_pair']['partner']==primary_b['id']
  checked('lumen_pair leave %d'%primary_a['id']);checked('lumen_pair leave %d'%primary_b['id']);wait(lambda:all(client(n['id'])['rect']==primary_original[n['id']] for n in (primary_a,primary_b)));status();checks.append('Primary-display paired layout remains available and restores both original placements in coupled mode')
  checked('[con_id=%d] fullscreen enable'%primary_a['id']);status();checked('[con_id=%d] focus'%pair_a['id']);assert client(primary_a['id'])['fullscreen_mode']==1
  checked('[con_id=%d] focus'%primary_a['id']);assert client(primary_a['id'])['fullscreen_mode']==1;checked('[con_id=%d] fullscreen disable'%primary_a['id']);wait(lambda:client(primary_a['id'])['rect']==primary_original[primary_a['id']]);checks.append('Ordinary fullscreen remains attached to its original desktop while cohort navigation leaves and returns, then restores geometry')
  settings.write_text('{}');status();launch([str(a.client)],'relationship-client.log',stdin=sp.PIPE,text=True);wait(lambda:any(n.get('app_id','').startswith('org.example.') for n in nodes(tree())));groups=tree()['window_groups'];checked('lumen_window groups %d on'%groups['revision']);groups=tree()['window_groups'];assert groups['enabled']
  group_ids=[g['id'] for g in groups['groups']];assert group_ids
  settings.write_text(json.dumps(dict(spaces=dict(separateDisplays=False))));status();groups=tree()['window_groups'];assert groups['enabled'] and groups['separate_displays'] is False;assert [g['id'] for g in groups['groups']]==group_ids and all(g['suspended'] for g in groups['groups']);checks.append('Separate-display prerequisite suspends existing groups without clearing IDs or membership')
  settings.write_text('{}');status();groups=tree()['window_groups'];assert groups['separate_displays'] is True;assert [g['id'] for g in groups['groups']]==group_ids;checked('lumen_window groups %d off'%groups['revision']);checks.append('Returning to separate spaces restores group capability and retained identities')
  palette=json.loads((repo/'res/lumen-desktop/appearance/palette.json').read_text());appearance=private/'appearance.json';appearance.write_text(json.dumps(dict(font=palette['font'],palette=palette['dark'],preferences=dict(dockMagnification=False,reduceMotion=True))));env['LUMEN_APPEARANCE_STATE']=str(appearance)
  assets=private/'assets';shutil.copytree(repo/'res/lumen-desktop',assets)
  # Fixed consumer seam records intended settings, without compiling or changing a host configuration.
  helper=assets/'shell/display_space_settings.py';original_helper=helper.read_bytes();adapter='from nix_settings import set_section'
  replacement="def set_section(section,value):\n    with open("+repr(str(private/'settings-actions.jsonl'))+",'a') as stream:stream.write(json.dumps(dict(section=section,value=value))+'\\n')"
  assert helper.read_text().count(adapter)==1;helper.write_text(helper.read_text().replace(adapter,replacement))
  import hashlib
  (a.output/'settings-adapter.json').write_text(json.dumps(dict(original=hashlib.sha256(original_helper).hexdigest(),modified=hashlib.sha256(helper.read_bytes()).hexdigest(),transform='Replace the single settings compiler import with a fixed private JSON-line observer; all validation and QML stay unchanged'),indent=2))
  qml=private/'docks.qml';qml.write_text('''import QtQuick
import QtQuick.Window
import Quickshell
import Quickshell.Io
import "assets/shell" as D
ShellRoot {id:stub
 D.DisplaySpacesState {id:policy}
 property alias displaySpaces:policy
 property string popupOwner:""
 property bool dockPreferencesBusy:false
 property var currentDate:new Date()
 property string aliasError:""
 property string applicationCommandError:""
 property var referenceDock:[{key:"synthetic",entry:{id:"synthetic",name:"Synthetic",icon:"utilities-terminal"},canvas:68,image:""}]
 signal requestDockFocus(string name)
 signal dismissPopups(string owner)
 function claimPopup(owner){popupOwner=owner;dismissPopups(owner);}
 function dockWindows(item){return [];}
 function dockLabel(item){return item.entry.name;}
 function activateDock(item){}
 function isDockPinned(entry){return true;}
 function canConfigureDockEntry(entry){return true;}
 function windowsFor(entry){return [];}
 function updateDockPin(entry,action){}
 function reorderDockPin(a,b){}
 FloatingWindow {id:preferences;visible:false;title:"Display Space Preferences";implicitWidth:520;implicitHeight:520
  D.SpaceSettingsPane {id:pane;anchors.fill:parent;anchors.margins:16;policy:stub.displaySpaces}
 }
 function find(item,name){if(item.objectName===name)return item;for(const child of item.children || []){const found=find(child,name);if(found)return found;}return null;}
 D.Dock {id:first;screen:Quickshell.screens.find(s=>s.name==="HEADLESS-1");shell:stub}
 D.Dock {id:second;screen:Quickshell.screens.find(s=>s.name==="HEADLESS-2");shell:stub}
 IpcHandler {target:"displaySpaceAudit"
  function status():string{return JSON.stringify({policy:policy.state,docks:[{name:first.outputName,visible:first.visible,keyboard:first.keyboardMode,active:first.contentItem.Window.window?first.contentItem.Window.window.active:false},{name:second.outputName,visible:second.visible,keyboard:second.keyboardMode,active:second.contentItem.Window.window?second.contentItem.Window.window.active:false}]});}
  function focus():void{stub.requestDockFocus("HEADLESS-2");}
  function preferencesOpen():void{preferences.visible=true;}
  function preferenceControl(name:string):bool{const item=find(pane,name);if(!item)return false;item.forceActiveFocus();return item.activeFocus;}
  function preferencesActive():bool{return preferences.contentItem.Window.window?preferences.contentItem.Window.window.active:false;}
  function preferencesClose():void{preferences.visible=false;}
 }
}''')
  launch(['quickshell','-p',str(qml)],'docks.log')
  def ipc(method):return call('quickshell','-p',qml,'ipc','call','displaySpaceAudit',method)
  def ui():
   value=json.loads(ipc('status'));(a.output/'last-dock-state.json').write_text(json.dumps(value,indent=2));return value
  wait(lambda:ui()['policy'].get('applied') and all(row['visible'] for row in ui()['docks']))
  settings.write_text(json.dumps(dict(spaces=dict(separateDisplays=False,primaryDisplay=dict(kind='connector',name='HEADLESS-1')))));status()
  wait(lambda:ui()['policy'].get('applied') and [row['visible'] for row in ui()['docks']]==[True,False])
  checked('bindsym Mod4+d exec quickshell -p '+str(qml)+' ipc call displaySpaceAudit focus')
  call('wtype','-M','logo','-k','d','-m','logo');wait(lambda:[row['keyboard'] for row in ui()['docks']]==[True,False] and ui()['docks'][0]['active']);call('grim',str(a.output/'coupled-dock.png'));call('wtype','-k','Escape');wait(lambda:not any(row['keyboard'] for row in ui()['docks']));checks.append('Production Dock is main-only and physical shortcut routes secondary request to main, with Escape release')
  settings.write_text('{}');status();wait(lambda:ui()['policy'].get('applied') and all(row['visible'] for row in ui()['docks']))
  call('wtype','-M','logo','-k','d','-m','logo');wait(lambda:[row['keyboard'] for row in ui()['docks']]==[False,True] and ui()['docks'][1]['active']);call('grim',str(a.output/'separate-docks.png'));call('wtype','-k','Escape');wait(lambda:not any(row['keyboard'] for row in ui()['docks']));checks.append('Production Dock restores both outputs and secondary shortcut target in independent mode')
  ipc('preferencesOpen');owner=wait(lambda:next((n for n in nodes(tree()) if n.get('name')=='Display Space Preferences'),None));checked('[con_id=%d] focus'%owner['id']);wait(lambda:ipc('preferencesActive')=='true')
  def preference_control(name):return call('quickshell','-p',qml,'ipc','call','displaySpaceAudit','preferenceControl',name)
  writes=private/'settings-actions.jsonl'
  assert preference_control('settings-control-separate-display-spaces')=='true';call('wtype','-k','space');wait(lambda:writes.exists() and len(writes.read_text().splitlines())==1)
  assert json.loads(writes.read_text().splitlines()[0])==dict(section='spaces',value=dict(separateDisplays=False,primaryDisplay=None))
  assert preference_control('settings-control-main-display')=='true';call('wtype','-k','space','-k','Home','-k','Down','-k','Return');wait(lambda:len(writes.read_text().splitlines())==2)
  saved=json.loads(writes.read_text().splitlines()[1]);assert saved['section']=='spaces' and saved['value']['primaryDisplay']['kind']=='connector';assert saved['value']['primaryDisplay']['name']==ui()['policy']['outputs'][0]['name'];assert saved['value']['separateDisplays'] is True
  ipc('preferencesClose');checks.append('Physical preference controls emit exactly two typed consumer intents through a source-hashed private compiler seam')
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
