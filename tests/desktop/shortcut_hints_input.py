"""Full installed shell switching through native keys and service-PID routing."""
import argparse,hashlib,json,os,re,shutil,signal,subprocess as sp,sys,tempfile,time
from pathlib import Path
from fixture_providers import isolate_providers
p=argparse.ArgumentParser();p.add_argument('--launcher',required=True);p.add_argument('--fixtures',type=Path,required=True);p.add_argument('--sway',required=True);p.add_argument('--effects',action='store_true');p.add_argument('--output',type=Path,required=True);p.add_argument('--inside',action='store_true');a=p.parse_args()
if not a.inside:
    private=tempfile.mkdtemp(prefix='lumen-switch-');env=dict(os.environ,HOME=private,XDG_RUNTIME_DIR=private)
    for key in ('CONFIG','STATE','CACHE','DATA'):env['XDG_'+key+'_HOME']=str(Path(private)/key.lower())
    for key in ('SWAYSOCK','WAYLAND_DISPLAY','DISPLAY','DBUS_SESSION_BUS_ADDRESS','AT_SPI_BUS_ADDRESS'):env.pop(key,None)
    env['DBUS_SYSTEM_BUS_ADDRESS']='unix:path='+private+'/no-system-bus'
    conf=Path(private)/'bus.conf';conf.write_text('<busconfig><type>session</type><listen>unix:tmpdir='+private+'</listen><policy context="default"><allow own="*"/><allow send_destination="*"/><allow receive_sender="*"/></policy></busconfig>')
    try:raise SystemExit(sp.call(['dbus-run-session','--config-file='+str(conf),'--',sys.executable,__file__,*sys.argv[1:],'--inside'],env=env))
    finally:shutil.rmtree(private,ignore_errors=True)
out=a.output.resolve();out.mkdir(parents=True,exist_ok=False)
prefix=re.split(r'\n[ \t]*exec quickshell ',Path(a.launcher).read_text(),maxsplit=1)[0]
env=dict(v.decode().split('=',1) for v in sp.check_output(['bash','-c',prefix+'\nexec env -0']).split(b'\0') if b'=' in v)
env.update(QT_QPA_PLATFORM='wayland',QT_QPA_PLATFORMTHEME='generic',QT_QUICK_BACKEND='software',QT_ACCESSIBILITY='0',NO_AT_BRIDGE='1',GTK_A11Y='none',WLR_BACKENDS='headless',WLR_RENDERER='gles2' if a.effects else 'pixman',WLR_RENDERER_FORCE_SOFTWARE='1' if a.effects else '0',WLR_RENDERER_ALLOW_SOFTWARE='1',LIBGL_ALWAYS_SOFTWARE='1',WLR_LIBINPUT_NO_DEVICES='1',GLOBAL_MENU_ENABLED='0',LUMEN_FILE_SERVICE='0',LUMEN_LAUNCH_SCOPES='0')
for key in ('WAYLAND_DISPLAY','SWAYSOCK','DISPLAY'):env.pop(key,None)
processes=[]
def launch(command,name):
    with (out/name).open('w') as stream:proc=sp.Popen(command,env=env,stdout=stream,stderr=sp.STDOUT,start_new_session=True)
    processes.append(proc);return proc

def wait(callback,description):
    end=time.monotonic()+15
    while time.monotonic()<end:
        try:
            value=callback()
            if value:return value
        except (OSError,ValueError,sp.SubprocessError):pass
        time.sleep(.05)
    raise AssertionError('Timed out: '+description)

def call(*parts):return sp.check_output(parts,env=env,text=True,timeout=8).strip()
assets=out/'assets';shutil.copytree(Path(env['LUMEN_NOTIFICATION_HELPER']).parent.parent,assets);source=assets/'shell'
isolate_providers(source,out)
text=(source/'shell.qml').read_text();head,tail=text.rsplit('}',1)
(source/'shell.qml').chmod(0o600);(source/'shell.qml').write_text(head+'''
  function textTree(object,seen){if(!object || seen.includes(object))return [];seen.push(object);let text=[];if(typeof object.text==="string" && object.visible!==false)text.push(object.text);for(const list of [object.children,object.data]){if(list && typeof list.length==="number")for(let i=0;i<list.length;i++)text=text.concat(textTree(list[i],seen));}if(object.contentItem)text=text.concat(textTree(object.contentItem,seen));return text;}
 IpcHandler {target:"shortcutAudit"
  function open():void{const bar=root.bars["HEADLESS-1"];bar.selectedWindow=root.active;bar.toggle("Window",16);}
  function status():string{const bar=root.bars["HEADLESS-1"];return JSON.stringify({ready:!!bar,available:root.shortcuts.available,hint:root.shortcuts.hint("window.fullscreen.toggle"),bindings:root.shortcuts.bindings,menu:bar?bar.menu:"",painted:bar?root.textTree(bar,[]):[],active:root.active?root.active.title:""});}
 }
}''' +tail)
palette=json.loads((assets/'appearance/palette.json').read_text());appearance=Path(env['XDG_STATE_HOME'])/'lumen-desktop/appearance.json';appearance.parent.mkdir(parents=True,exist_ok=True);appearance.write_text(json.dumps(dict(font=palette['font'],palette=palette['dark'],preferences={})))
config=out/'sway.conf';config.write_text('output HEADLESS-1 resolution 1280x900\nseat seat0 fallback true\nxwayland disable\nfocus_follows_mouse no\nfor_window [all] floating enable\n')
registry=Path(env['XDG_CONFIG_HOME'])/'lumen-desktop/shortcuts.json';registry.parent.mkdir(parents=True,exist_ok=True)
def install(name):
 bundle=json.loads((a.fixtures/(name+'.json')).read_text());document=Path(bundle['registry']);temporary=registry.with_suffix('.new');temporary.write_bytes(document.read_bytes());temporary.replace(registry)
 chord='Mod4+Control+f' if name=='default' else 'Mod4+Control+x'
 dispatch=bundle['dispatch'][chord];assert dispatch.startswith('exec /nix/store/') and dispatch.endswith('/bin/swaymsg fullscreen toggle')
 config.write_text('output HEADLESS-1 resolution 1280x900\nseat seat0 fallback true\nxwayland disable\nfocus_follows_mouse no\nfor_window [all] floating enable\nbindsym '+chord+' '+dispatch+'\n');return bundle
results=[]
try:
 before=install('default');launch([a.sway,'--unsupported-gpu','-c',str(config)],'sway.log');runtime=Path(env['XDG_RUNTIME_DIR'])
 env['SWAYSOCK']=str(wait(lambda:next(runtime.glob('sway-ipc*.sock'),None),'private compositor socket'))
 env['WAYLAND_DISPLAY']=wait(lambda:next((v.name for v in runtime.glob('wayland-*') if not v.name.endswith('.lock')),None),'private Wayland display')
 launch(['wtype','-s','60000'],'keyboard-seat.log');shell=launch(['quickshell','-p',str(source)],'shell.log')
 def audit(*args):return call('quickshell','ipc','--pid',str(shell.pid),'call','shortcutAudit',*args)
 def state():return json.loads(audit('status'))
 def nodes(node):return [node]+sum((nodes(child) for child in node.get('nodes',[])+node.get('floating_nodes',[])),[])
 def window():return next((node for node in nodes(json.loads(call('swaymsg','-r','-t','get_tree'))) if node.get('app_id')=='lumen-shortcut-fixture'),None)
 def key(key):call('wtype','-M','logo','-M','ctrl','-k',key,'-m','ctrl','-m','logo')
 wait(lambda:state()['ready'] and state()['available'],'effective registry readiness');client=launch(['foot','--app-id=lumen-shortcut-fixture','--title=Shortcut Fixture','sh','-c','sleep 600'],'client.log');target=wait(window,'native fixture window');identity=[target['id'],target['pid']];assert target['pid']==client.pid
 def focus():call('swaymsg','[con_id='+str(identity[0])+'] focus');wait(lambda:state()['active']=='Shortcut Fixture','owned native focus')
 focus();assert state()['hint']=='Ctrl+Super+F';key('f');wait(lambda:window()['fullscreen_mode']>0,'default exact dispatch');key('f');wait(lambda:window()['fullscreen_mode']==0,'default reverse toggle');results.append('Actual module default registry hint and physical configured dispatch agree')
 after=install('rebound');call('swaymsg','reload');wait(lambda:state()['hint']=='Ctrl+Super+X','consumer rebind watched');focus();key('f');time.sleep(.25);assert window()['fullscreen_mode']==0;key('x');wait(lambda:window()['fullscreen_mode']>0,'new physical binding dispatch');key('x');wait(lambda:window()['fullscreen_mode']==0,'new binding reverse toggle');assert before['dispatch']['Mod4+Control+f']==after['dispatch']['Mod4+Control+x'];results.append('Consumer moved exact dispatch command; old key is inert and new key toggles exact native window')
 focus();audit('open');wait(lambda:state()['menu']=='Window' and 'Ctrl+Super+X' in state()['painted'],'actual owned menu painted effective hint');call('grim',str(out/'menu-hint.png'));call('wtype','-k','Escape');wait(lambda:state()['menu']=='','menu dismissed');results.append('Actual owned Window menu renders the rebound hint and Escape restores normal input')
 registry.write_text('{bad');wait(lambda:not state()['available'] and state()['hint']=='','invalid metadata clears hints');assert [window()['id'],window()['pid']]==identity;results.append('Invalid registry cannot retain a stale hint or retarget a native window')
 (out/'result.json').write_text(json.dumps({'passed':True,'checks':results,'target':identity,'source':sp.check_output(['git','rev-parse','HEAD'],text=True).strip(),'fixture':str(a.fixtures)},indent=2)+'\n')
finally:
 for process in reversed(processes):
  if process.poll() is None:process.terminate()
 for process in reversed(processes):
  try:process.wait(timeout=5)
  except sp.TimeoutExpired:process.kill();process.wait(timeout=3)
