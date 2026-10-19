"""Real native GTK zero-window lifecycle and cooperative dirty-document Quit.

All windows, session bus, process control and document writes are disposable.
"""
import argparse,json,os,re,shlex,shutil,signal,subprocess as sp,sys,tempfile,time
from pathlib import Path
from PIL import Image
p=argparse.ArgumentParser();p.add_argument('--sway',required=True);p.add_argument('--launcher',required=True,type=Path);p.add_argument('--output',required=True,type=Path);p.add_argument('--inside',action='store_true');a=p.parse_args()
if not a.inside:
 with tempfile.TemporaryDirectory(prefix='lumen-lifecycle-',ignore_cleanup_errors=True) as private:
  env=dict(os.environ,HOME=private,XDG_RUNTIME_DIR=private)
  for key in ('CONFIG','STATE','CACHE','DATA'):env['XDG_'+key+'_HOME']=str(Path(private)/key.lower())
  for key in ('WAYLAND_DISPLAY','DISPLAY','SWAYSOCK','DBUS_SESSION_BUS_ADDRESS','AT_SPI_BUS_ADDRESS'):env.pop(key,None)
  raise SystemExit(sp.call(['dbus-run-session','--',sys.executable,__file__,*sys.argv[1:],'--inside'],env=env))
out=a.output.resolve();out.mkdir(parents=True,exist_ok=False);runtime=Path(os.environ['XDG_RUNTIME_DIR']);source=Path(__file__).resolve().parents[2]/'res/lumen-desktop'
prefix=re.split(r'\n[ \t]*exec quickshell ',a.launcher.read_text(),maxsplit=1)[0]
installed=dict(v.decode().split('=',1) for v in sp.check_output(['bash','-c',prefix+'\nexec env -0'],stderr=sp.DEVNULL).split(b'\0') if b'=' in v)
env=dict(os.environ)
for key in ('PATH','GLOBAL_MENU_PYTHON','QT_PLUGIN_PATH','QML_IMPORT_PATH'):
 if key in installed:env[key]=installed[key]
env.update(WLR_BACKENDS='headless',WLR_RENDERER='pixman',WLR_LIBINPUT_NO_DEVICES='1',QT_QPA_PLATFORM='wayland',QT_QUICK_BACKEND='software',QT_QPA_PLATFORMTHEME='generic',QT_ACCESSIBILITY='0',QT_LINUX_ACCESSIBILITY_ALWAYS_ON='0',NO_AT_BRIDGE='1',GTK_A11Y='none',GLOBAL_MENU_ENABLED='0',LUMEN_FILE_SERVICE='0',LUMEN_LAUNCH_SCOPES='0',GDK_BACKEND='wayland')
for key in ('WAYLAND_DISPLAY','DISPLAY','SWAYSOCK'):env.pop(key,None)
assets=out/'assets';shutil.copytree(source,assets);shell=assets/'shell';original=(shell/'shell.qml').read_text();head,tail=original.rsplit('\n}',1)
(shell/'shell.qml').write_text(head+'''
 IpcHandler {target:"lifecycleAudit"
  function status():string {const e=DesktopEntries.byId("org.example.Lifecycle");return JSON.stringify({command:applicationLifecycle.observer.command,observerRunning:applicationLifecycle.observer.running,available:applicationLifecycle.available,observerError:applicationLifecycle.error,apps:root.runningApplications,entry:!!e,running:e?root.runningFor(e):false,error:root.applicationCommandError,pending:applicationLifecycle.pending,extras:root.runningExtras.map(e=>e.id)});}
  function quit():void{root.quitApplication(DesktopEntries.byId("org.example.Lifecycle"));}
 }
}'''+tail)
palette=json.loads((assets/'appearance/palette.json').read_text());appearance=out/'appearance.json';appearance.write_text(json.dumps({'font':palette['font'],'palette':palette['dark'],'preferences':{'mode':'dark','reduceMotion':True}}));env['LUMEN_APPEARANCE_STATE']=str(appearance)
assert shutil.which('lumen-application-lifecycle',path=env['PATH']), 'Build a launcher containing the lifecycle observer first'
helpers=out/'helpers';helpers.mkdir();calendar=helpers/'lumen-calendar';calendar.write_text('#!'+sys.executable+'\nimport json,sys\nr=json.loads(sys.stdin.readline());print(json.dumps(dict(configured=False,events=[],monthEvents=[],days=[],total=0,providers=[])))\n');calendar.chmod(0o700);env['PATH']=str(helpers)+':'+env['PATH']
applications=Path(env['XDG_DATA_HOME'])/'applications';applications.mkdir(parents=True)
gapplication=shutil.which('gapplication',path=env['PATH']);assert gapplication
(applications/'org.example.Lifecycle.desktop').write_text('[Desktop Entry]\nType=Application\nName=Lifecycle Fixture\nExec='+gapplication+' launch org.example.Lifecycle\nIcon=application-x-executable\nStartupWMClass=org.example.Lifecycle\n')
client=out/'client.py';client.write_text('''import gi,json,os,sys
from pathlib import Path
gi.require_version('Gtk','3.0')
from gi.repository import Gtk,Gio,GLib
GLib.set_prgname('org.example.Lifecycle')
root=Path(sys.argv[1]);documents=['dirty first','dirty second'];requests=0;last='';windows=[]
app=Gtk.Application(application_id='org.example.Lifecycle',flags=Gio.ApplicationFlags.FLAGS_NONE)
def status():
 (root/'client-state.json').write_text(json.dumps({'windows':len(windows),'requests':requests,'documents':documents}))
def activate(app):
 if not windows:
  for i,text in enumerate(documents):
   window=Gtk.ApplicationWindow(application=app,title='Lifecycle document '+str(i+1));window.set_default_size(300,200);window.add(Gtk.Label(label=text));window.show_all();windows.append(window)
 status()
def quit_action(action,parameter):
 global requests
 requests+=1;choice=(root/'choice').read_text() if (root/'choice').exists() else 'cancel'
 if choice=='save':
  for i,text in enumerate(documents):(root/('saved-'+str(i)+'.txt')).write_text(text)
 status()
 if choice in ('save','discard'):app.quit()
def poll():
 global last
 command=root/'command'
 if command.exists() and command.read_text()!=last:
  last=command.read_text()
  if last.startswith('hide'):
   for window in windows:window.destroy()
   windows.clear();status()
 return True
def startup(app):
 app.hold();action=Gio.SimpleAction.new('quit',None);action.connect('activate',quit_action);app.add_action(action);GLib.timeout_add(30,poll)
app.connect('startup',startup);app.connect('activate',activate);app.run([])
''')
processes=[]
def launch(command,name):
 stream=(out/name).open('w');proc=sp.Popen(command,env=env,stdout=stream,stderr=sp.STDOUT,start_new_session=True);stream.close();processes.append(proc);return proc
def call(*command):return sp.check_output(command,env=env,text=True,timeout=10).strip()
def wait(predicate):
 deadline=time.monotonic()+20
 while time.monotonic()<deadline:
  try:
   value=predicate()
   if value:return value
  except (OSError,ValueError,sp.SubprocessError):pass
  time.sleep(.05)
 raise AssertionError('Lifecycle fixture timeout; inspect '+str(out))
def ipc(method,*values,target='lifecycleAudit'):return call('quickshell','-p',str(shell),'ipc','call',target,method,*values)
def state():
 value=json.loads(ipc('status'));(out/'last-lifecycle.json').write_text(json.dumps(value,indent=2));return value
def app_state():return json.loads((out/'client-state.json').read_text())
try:
 config=out/'sway.conf';config.write_text('output HEADLESS-1 resolution 1440x1000\nxwayland disable\nseat seat0 fallback true\n')
 launch([a.sway,'--unsupported-gpu','-c',str(config)],'sway.log');wait(lambda:list(runtime.glob('sway-ipc*.sock')))
 env['SWAYSOCK']=str(next(runtime.glob('sway-ipc*.sock')));env['WAYLAND_DISPLAY']=next(p.name for p in runtime.glob('wayland-*') if not p.name.endswith('.lock'))
 launch(['quickshell','-p',str(shell)],'desktop.log');wait(lambda:state()['entry'])
 application=launch([env['GLOBAL_MENU_PYTHON'],str(client),str(out)],'client.log');wait(lambda:app_state()['windows']==2)
 (out/'native-tree.json').write_text(call('swaymsg','-t','get_tree'));(out/'initial-lifecycle.json').write_text(json.dumps(state(),indent=2))
 first=wait(lambda:next((app for app in state()['apps'] if app['appId']=='org.example.Lifecycle' and app['windowCount']==2),None));instances=first['instanceIds'];assert state()['running']
 (out/'command').write_text('hide all windows');wait(lambda:app_state()['windows']==0)
 zero=wait(lambda:next((app for app in state()['apps'] if app['appId']=='org.example.Lifecycle' and app['zeroWindows']),None));assert zero['instanceIds']==instances and application.poll() is None and state()['running'];assert 'org.example.Lifecycle' in state()['extras']
 def painted_zero():
  call('grim',str(out/'zero-windows.png'))
  with Image.open(out/'zero-windows.png') as image:return sum(image.convert('RGB').getpixel((720,500)))<80
 wait(painted_zero)
 picker=json.loads(ipc('appSwitcher','begin',target='desktop'));assert 'org.example.Lifecycle' in picker['items'];ipc('appSwitcher','commit',target='desktop');wait(lambda:app_state()['windows']==2)
 reopened=wait(lambda:next((app for app in state()['apps'] if app['appId']=='org.example.Lifecycle' and app['windowCount']==2),None));assert reopened['instanceIds']==instances and application.poll() is None
 # Two dirty documents: cooperative cancellation leaves both documents/process alive.
 (out/'choice').write_text('cancel');ipc('quit');wait(lambda:app_state()['requests']==1);wait(lambda:not state()['pending']);assert application.poll() is None and app_state()['documents']==['dirty first','dirty second'];assert not list(out.glob('saved-*.txt'))
 (out/'choice').write_text('save');ipc('quit');application.wait(timeout=8);assert application.returncode==0;wait(lambda:not any(app['appId']=='org.example.Lifecycle' for app in state()['apps']));assert not state()['running'];assert [(out/('saved-'+str(i)+'.txt')).read_text() for i in range(2)]==['dirty first','dirty second']
 # A second independent instance chooses Discard through the same app action.
 for index in range(2):(out/('saved-'+str(index)+'.txt')).unlink()
 (out/'command').unlink()
 application=launch([env['GLOBAL_MENU_PYTHON'],str(client),str(out)],'discard-client.log')
 wait(lambda:any(app['appId']=='org.example.Lifecycle' and app['windowCount']==2 and app['instanceIds']!=instances for app in state()['apps']))
 (out/'choice').write_text('discard');ipc('quit');application.wait(timeout=8);assert application.returncode==0;assert not list(out.glob('saved-*.txt'));wait(lambda:not state()['running'])
 log=(out/'desktop.log').read_text();assert not any(error in log for error in ('TypeError:','ReferenceError:','Failed to load configuration')),log
 result={'passed':True,'checks':['actual native compositor process identity','pidfd-retained zero-window process','Dock running entry retained without windows','switcher zero-window item and activation reopen same process','advertised GTK Quit action only','dirty two-document cancel preserves process/data','explicit save Quit commits both synthetic documents','actual process exit removes running indicator/state','separate instance explicit discard does not write documents']};(out/'result.json').write_text(json.dumps(result,indent=2)+'\n');print(json.dumps(result))
finally:
 for proc in reversed(processes):
  if proc.poll() is None:
   os.killpg(proc.pid,signal.SIGTERM)
   try:proc.wait(timeout=3)
   except sp.TimeoutExpired:os.killpg(proc.pid,signal.SIGKILL);proc.wait()
