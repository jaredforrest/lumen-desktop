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
 argv=[str(tools[4]),'/source/tests/desktop/force_quit_native.py','--inside','--launcher',str(tools[0]),'--sway',str(tools[1]),'--font-config',str(tools[2]),'--keyboard',str(tools[3]),'--output','/artifacts/result','--parent-net',os.readlink('/proc/self/ns/net')]
 if a.effects:argv+=['--effects','--egl-vendor',str(tools[5])]
 code=1
 try:code=run(repo,a.output,argv,software_graphics=a.effects)
 finally:frozen.finish(a.output,code)
 raise SystemExit(code)
assert os.getuid()!=0 and os.readlink('/proc/self/ns/net')!=a.parent_net
assert [name for _,name in socket.if_nameindex()]==['lo']
assert not any((Path('/dev')/name).exists() for name in ('dri','snd','video0')) and Path('/dev/udmabuf').exists()==a.effects
out=a.output;out.mkdir(mode=0o700);processes=[];checks=[];receipts=[];original_socket=os.environ.get('SWAYSOCK')
with tempfile.TemporaryDirectory(prefix='force-quit-native-') as temporary:
 private=Path(temporary);env=dict(os.environ)
 for key in ('HOME','XDG_RUNTIME_DIR','XDG_STATE_HOME','XDG_CONFIG_HOME','XDG_DATA_HOME','XDG_CACHE_HOME'):
  env[key]=str(private/key);Path(env[key]).mkdir(mode=0o700)
 prefix=re.split(r'\n[ \t]*exec quickshell ',a.launcher.read_text(),maxsplit=1)[0]
 env.update(dict(v.decode().split('=',1) for v in sp.check_output(['bash','-c',prefix+'\nexec env -0'],env=env).split(b'\0') if b'=' in v))
 for key in ('DISPLAY','WAYLAND_DISPLAY','SWAYSOCK','DBUS_SESSION_BUS_ADDRESS','AT_SPI_BUS_ADDRESS'):env.pop(key,None)
 env.update(WLR_BACKENDS='headless',WLR_RENDERER='gles2' if a.effects else 'pixman',WLR_RENDERER_FORCE_SOFTWARE='1' if a.effects else '0',WLR_RENDERER_ALLOW_SOFTWARE='1',LIBGL_ALWAYS_SOFTWARE='1',WLR_LIBINPUT_NO_DEVICES='1',QT_QPA_PLATFORM='wayland',QT_QPA_PLATFORMTHEME='generic',QT_QUICK_BACKEND='software',QT_ACCESSIBILITY='0',NO_AT_BRIDGE='1',GTK_A11Y='none',GDK_BACKEND='wayland',FONTCONFIG_FILE=str(a.font_config))
 if a.effects:env['__EGL_VENDOR_LIBRARY_FILENAMES']=str(a.egl_vendor)
 assets=Path(re.search(r'exec quickshell .*--path (\S+)/shell',a.launcher.read_text()).group(1))
 for name in ('shell/ForceQuitDialog.qml','runtime/application_control.py','runtime/state.py','runtime/protocol.py'):
  assert (assets/name).read_bytes()==(repo/'res/lumen-desktop'/name).read_bytes(),name
 shutil.copytree(assets,private/'assets');assets=private/'assets'
 assert shutil.which('lumen-application-control',path=env['PATH'])
 palette=json.loads((assets/'appearance/palette.json').read_text());appearance=private/'appearance.json';appearance.write_text(json.dumps(dict(font=palette['font'],palette=palette['dark'],preferences=dict(reduceMotion=True))));env['LUMEN_APPEARANCE_STATE']=str(appearance)
 shell=private/'confirmation.qml';shell.write_text('''import QtQuick
import Quickshell
import Quickshell.Io
import "'''+'assets/shell'+'''" as D
ShellRoot {id:fixture
 property string popupOwner:""
 signal dismissPopups(string owner)
 function preferredOutput(){return "HEADLESS-1";}
 function claimPopup(owner){popupOwner=owner;dismissPopups(owner);}
 D.ForceQuitDialog {id:dialog;shell:fixture}
 function focused(item){if(item.activeFocus && typeof item.text==="string")return item.text;for(let i=0;i<(item.children||[]).length;++i){let found=focused(item.children[i]);if(found)return found;}return "";}
 IpcHandler {target:"confirmationProof"
  function openConfirmation(appId:string,title:string):void {dialog.show("Disposable document",[{appId:appId,title:title}]);}
  function status():string {return JSON.stringify({visible:dialog.visible,phase:dialog.phase,error:dialog.error,windowCount:dialog.windowCount,focus:fixture.focused(dialog.contentItem)});}
 }
}''')
 client=private/'client.py';client.write_text('''import gi,json,os,sys
from pathlib import Path
gi.require_version('Gtk','3.0')
from gi.repository import Gtk,Gio,GLib
appid,title,state=sys.argv[1:];state=Path(state);GLib.set_prgname(appid)
app=Gtk.Application(application_id=appid,flags=Gio.ApplicationFlags.FLAGS_NONE);ticks=0;editor=None
def publish():
 global ticks
 ticks+=1;value=dict(pid=os.getpid(),ticks=ticks,text=editor.get_text() if editor else '',dirty=True)
 temporary=state.with_suffix('.tmp');temporary.write_text(json.dumps(value));temporary.replace(state);return True
def activate(app):
 global editor
 window=Gtk.ApplicationWindow(application=app,title=title);window.set_default_size(430,230)
 editor=Gtk.Entry();editor.set_text('unsaved-synthetic-');window.add(editor);window.show_all();editor.grab_focus();editor.set_position(-1);publish();GLib.timeout_add(40,publish)
app.connect('activate',activate);app.run([])
''')
 def launch(argv,name):
  with (out/name).open('w') as log:proc=sp.Popen(list(map(str,argv)),env=env,stdout=log,stderr=sp.STDOUT,start_new_session=True)
  processes.append(proc);return proc
 def call(*argv):return sp.check_output(list(map(str,argv)),env=env,text=True,timeout=6).strip()
 def sway(*argv):return json.loads(call(a.sway.parent/'swaymsg','-r',*argv))
 def nodes(node):
  yield node
  for child in node.get('nodes',[])+node.get('floating_nodes',[]):yield from nodes(child)
 def mapped(proc):return next((n for n in nodes(sway('-t','get_tree')) if n.get('pid')==proc.pid and n.get('app_id') and n.get('rect',{}).get('width',0)>0),None)
 def ipc(method,*args):return call('quickshell','-p',shell,'ipc','call','confirmationProof',method,*args)
 def state():return json.loads(ipc('status'))
 def wait(fn,label):
  deadline=time.monotonic()+15
  while time.monotonic()<deadline:
   try:value=fn()
   except (OSError,ValueError,sp.SubprocessError):value=None
   if value:return value
   time.sleep(.04)
  (out/'timeout-tree.json').write_text(json.dumps(sway('-t','get_tree'),indent=2))
  try:(out/'timeout-dialog.json').write_text(json.dumps(state(),indent=2))
  except (OSError,ValueError,sp.SubprocessError):pass
  raise AssertionError(label)
 def checked(command):
  result=sway(command);assert result and all(row.get('success') for row in result),result
 def client_state(path):return json.loads(path.read_text())
 def identity(proc,node):
  status=Path('/proc',str(proc.pid),'stat').read_text().rsplit(')',1)[1].split();exe=Path('/proc',str(proc.pid),'exe').stat()
  return dict(pid=proc.pid,start=status[19],device=str(exe.st_dev),inode=str(exe.st_ino),conId=node['id'],appId=node['app_id'],title=node['name'])
 def show(node):
  ipc('openConfirmation',node['app_id'],node['name']);return wait(lambda:(s if (s:=state())['phase']=='ready' and s['focus']=='Cancel' else None),'Production confirmation ready and Cancel focused')
 def confirm():
  keyboard.key('-k','Tab');wait(lambda:state()['focus']=='Force Quit','Force Quit keyboard focus');keyboard.key('-k','space')
 def bystander_live():
  before=client_state(bystander_path);checked('[con_id=%d] focus'%bystander_node['id']);wait(lambda:any(n.get('focused') and n.get('id')==bystander_node['id'] for n in nodes(sway('-t','get_tree'))),'Bystander native focus')
  keyboard.type('still-dirty-');after=wait(lambda:(v if (v:=client_state(bystander_path))['ticks']>before['ticks'] and len(v['text'])>len(before['text']) else None),'Bystander physical text and heartbeat')
  assert after['dirty'] and after['text'].startswith('unsaved-synthetic-');receipts.append(dict(bystander=after))
 try:
  start_private_bus(env,out,processes)
  config=private/'sway.conf';config.write_text('output HEADLESS-1 resolution 1100x800\nxwayland disable\nseat seat0 fallback true\nfocus_follows_mouse no\nfor_window [all] floating enable\n')
  compositor=launch([a.sway,'--unsupported-gpu','-c',config],'sway.log')
  env['SWAYSOCK']=str(wait(lambda:next(Path(env['XDG_RUNTIME_DIR']).glob('sway-ipc*.sock'),None),'Private compositor socket'))
  env['WAYLAND_DISPLAY']=wait(lambda:next((x.name for x in Path(env['XDG_RUNTIME_DIR']).glob('wayland-*') if not x.name.endswith('.lock')),None),'Private Wayland socket')
  with (out/'keyboard.log').open('w') as log:keyboard=PhysicalKeyboard(a.keyboard,env,log)
  processes.append(keyboard.process)
  desktop=launch(['quickshell','-p',shell],'shell.log');wait(lambda:state()['phase']=='closed','Production dialog loaded')
  bystander_path=private/'bystander.json';target_path=private/'target.json'
  bystander=launch([env['GLOBAL_MENU_PYTHON'],client,'org.example.DirtyBystander','Dirty bystander',bystander_path],'bystander.log');bystander_node=wait(lambda:mapped(bystander),'Bystander mapping');bystander_live()
  target=launch([env['GLOBAL_MENU_PYTHON'],client,'org.example.HungDocument','Hung document',target_path],'target.log');target_node=wait(lambda:mapped(target),'Target mapping');wait(lambda:client_state(target_path)['ticks']>1,'Target heartbeat')
  captured=identity(target,target_node);os.kill(target.pid,signal.SIGSTOP);wait(lambda:Path('/proc',str(target.pid),'status').read_text().split('State:')[1].lstrip().startswith('T'),'Owned target stopped');receipts.append(dict(hung=captured))
  show(target_node);call('grim',out/'cancel-ready.png');keyboard.key('-k','Escape');wait(lambda:not state()['visible'],'Escape cancellation')
  assert target.poll() is None and Path('/proc',str(target.pid),'status').read_text().split('State:')[1].lstrip().startswith('T');bystander_live();checks.append('Physical Escape cancels production confirmation without signalling the captured stopped native client')
  ready=show(target_node);assert ready['windowCount']==1;confirm();wait(lambda:not state()['visible'],'Confirmed dialog closed');assert target.wait(timeout=5)==-signal.SIGKILL;wait(lambda:not mapped(target),'Terminated window removed');bystander_live();checks.append('Physical Tab/Space confirms production pidfd termination of exact stopped client while dirty bystander remains interactive')
  replacement=launch([env['GLOBAL_MENU_PYTHON'],client,'org.example.HungDocument','Hung document',target_path],'retired-target.log');old_node=wait(lambda:mapped(replacement),'Retired target mapping');old_identity=identity(replacement,old_node);show(old_node)
  os.kill(replacement.pid,signal.SIGKILL);replacement.wait(timeout=5);wait(lambda:not mapped(replacement),'Retired target removal')
  survivor=launch([env['GLOBAL_MENU_PYTHON'],client,'org.example.HungDocument','Hung document',target_path],'replacement.log');new_node=wait(lambda:mapped(survivor),'Same-title replacement mapping');assert new_node['id']!=old_node['id'] and survivor.pid!=replacement.pid
  confirm();error=wait(lambda:(s if (s:=state())['phase']=='error' and s['focus']=='Close' else None),'Stale confirmation rejected with Close focused');assert survivor.poll() is None;receipts.append(dict(retired=old_identity,replacement=identity(survivor,new_node),refusal=error));call('grim',out/'stale-refusal.png');keyboard.key('-k','Escape');wait(lambda:not state()['visible'],'Error dismissed');bystander_live();checks.append('Captured confirmation rejects retired conId and same-title new process with zero replacement/bystander termination')
  # The broker must refuse a mixed application selection rather than partially act.
  result=sp.run(['lumen-application-control','--request-id','mixed','prepare',json.dumps([dict(appId=new_node['app_id'],title=new_node['name']),dict(appId=bystander_node['app_id'],title=bystander_node['name'])])],env=env,text=True,capture_output=True,timeout=6)
  mixed=json.loads(result.stdout);assert result.returncode!=0 and 'one application' in mixed['error'] and survivor.poll() is None and bystander.poll() is None;mixed.pop('requestId',None);receipts.append(dict(mixedRefusal=mixed));checks.append('Actual production broker refuses mixed-app selection before any process signal')
  generated(private,out)
  (out/'client-source.sha256').write_text(hashlib.sha256(client.read_bytes()).hexdigest()+'\n')
  (out/'receipts.json').write_text(json.dumps(receipts,indent=2)+'\n')
  (out/'result.json').write_text(json.dumps(dict(passed=True,checks=checks,effects=a.effects,scope='Production dialog and broker; synthetic GTK dirty state, actual SIGSTOP/pidfd lifecycle. No application-save protocol or automatic unresponsiveness detection claim.',liveSocketUnchanged=os.environ.get('SWAYSOCK')==original_socket),indent=2)+'\n')
 finally:
  for proc in reversed(processes):
   if proc.poll() is None:
    os.killpg(proc.pid,signal.SIGCONT);os.killpg(proc.pid,signal.SIGTERM)
    try:proc.wait(timeout=4)
    except sp.TimeoutExpired:os.killpg(proc.pid,signal.SIGKILL);proc.wait(timeout=4)
  close_reaped_pipes(processes)
  warnings={}
  for log in out.glob('*.log'):
   found=[line for line in log.read_text(errors='replace').splitlines() if re.search(r'\bWARN\b|QtWARN|Warning:|ResourceWarning|Traceback|Ignoring unexpected wl_keyboard|Failed to load configuration',re.sub(r'\x1b\[[0-9;]*m','',line))]
   if found:warnings[log.name]=found
  (out/'warning-scan.json').write_text(json.dumps(warnings,indent=2)+'\n')
  assert not warnings,warnings
print(json.dumps(dict(passed=True,checks=len(checks))))
