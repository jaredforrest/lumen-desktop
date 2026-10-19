"""Isolated installed-shell smoke. Run with the normal desktop account, never root."""
import argparse,json,os,re,shlex,signal,subprocess as sp,sys,tempfile,time
from pathlib import Path

parser=argparse.ArgumentParser()
parser.add_argument('--output',type=Path,required=True)
parser.add_argument('--launcher',default='lumen-desktop-shell')
parser.add_argument('--click-pointer',type=Path,help='Optional test-only wlrctl fixture for real header button clicks')
parser.add_argument('--drag-pointer',type=Path,help='Optional test-only wlrctl fixture from drag_pointer.nix')
parser.add_argument('--sway',default='sway')
parser.add_argument('--assets',type=Path)
parser.add_argument('--effects',action='store_true')
parser.add_argument('--desktop-widgets',action='store_true')
parser.add_argument('--menu-only',action='store_true')
parser.add_argument('--theme',choices=['light','dark'],default='light')
parser.add_argument('--inside',action='store_true',help=argparse.SUPPRESS)
args=parser.parse_args()
if not args.inside:
    import shutil
    private=tempfile.mkdtemp(prefix='lumen-ui-')
    isolated=dict(os.environ,HOME=private,XDG_RUNTIME_DIR=private,DBUS_SYSTEM_BUS_ADDRESS='unix:path='+private+'/absent-system',QT_ACCESSIBILITY='0',QT_LINUX_ACCESSIBILITY_ALWAYS_ON='0',NO_AT_BRIDGE='1',GTK_A11Y='none')
    for key in ('CONFIG','STATE','CACHE','DATA'):isolated['XDG_'+key+'_HOME']=str(args.output.resolve()/key.lower())
    for key in ('AT_SPI_BUS_ADDRESS','DBUS_SESSION_BUS_ADDRESS','SWAYSOCK','WAYLAND_DISPLAY','DISPLAY'):isolated.pop(key,None)
    config=Path(private)/'bus.conf';config.write_text('<busconfig><type>session</type><listen>unix:tmpdir='+private+'</listen><auth>EXTERNAL</auth><policy context="default"><allow send_destination="*"/><allow receive_sender="*"/><allow own="*"/></policy></busconfig>')
    try:raise SystemExit(sp.call(['dbus-run-session','--config-file='+str(config),'--',sys.executable,__file__,*sys.argv[1:],'--inside'],env=isolated))
    finally:shutil.rmtree(private,ignore_errors=True)
import shutil
launcher=Path(shutil.which(args.launcher) or args.launcher)
script=launcher.read_text();prefix,tail=re.split(r'\n[ \t]*exec quickshell ',script,maxsplit=1)
assets=args.assets.resolve() if args.assets else Path(shlex.split(tail)[-1]).resolve()
run=args.output.resolve();run.mkdir(parents=True,exist_ok=False)
# Unix-domain socket paths have a short length limit, independent of output path.
runtime=Path(os.environ['XDG_RUNTIME_DIR'])
# Reuse only the pinned installed runtime exports; never run its final launcher.
raw=sp.check_output(['bash','-c',prefix+'\nexec env -0'],stderr=sp.DEVNULL)
installed=dict(v.decode().split('=',1) for v in raw.split(b'\0') if b'=' in v)
env=dict(os.environ)
for key in ('PATH','GLOBAL_MENU_PYTHON','GLOBAL_MENU_BRIDGE','QT_PLUGIN_PATH','QML_IMPORT_PATH','QT_QPA_PLATFORMTHEME','QT_LINUX_ACCESSIBILITY_ALWAYS_ON'):
    if key in installed:env[key]=installed[key]
env.update(XDG_RUNTIME_DIR=str(runtime),XDG_CONFIG_HOME=str(run/'config'),XDG_STATE_HOME=str(run/'state'),XDG_CACHE_HOME=str(run/'cache'),XDG_DATA_HOME=str(run/'data'),WLR_BACKENDS='headless',WLR_RENDERER='gles2' if args.effects else 'pixman',WLR_RENDERER_ALLOW_SOFTWARE='1',WLR_LIBINPUT_NO_DEVICES='1',QT_QPA_PLATFORM='wayland',GDK_BACKEND='wayland',LUMEN_FILE_SERVICE='0',LUMEN_LAUNCH_SCOPES='0',GLOBAL_MENU_ENABLED='0')
env.update(QT_ACCESSIBILITY='0',QT_LINUX_ACCESSIBILITY_ALWAYS_ON='0',NO_AT_BRIDGE='1',GTK_A11Y='none')
env.pop('AT_SPI_BUS_ADDRESS',None)
if not args.effects:env['QT_QUICK_BACKEND']='software'
for key in ('DISPLAY','WAYLAND_DISPLAY','SWAYSOCK'):env.pop(key,None)
if args.desktop_widgets:
    widget_file=Path(env['XDG_STATE_HOME'])/'lumen-desktop/session/notification-centre.json'
    widget_file.parent.mkdir(parents=True,exist_ok=True)
    widget_file.write_text(json.dumps(dict(desktopVisible=True,desktopStyle='agenda',desktopColor='monochrome',positions={'HEADLESS-1':{'x':0.1,'y':0.2}})))
docs=run/'Documents';docs.mkdir();(docs/'example.txt').write_text('US English preview fixture')
env['LUMEN_SEARCH_ROOTS']=json.dumps([str(docs)])
# Opening Date must not activate the real user's Online Accounts via D-Bus.
fixture_bin=run/'bin';fixture_bin.mkdir()
fixture_calendar=fixture_bin/'lumen-calendar'
fixture_calendar.write_text('#!'+sys.executable+'\nimport json,sys\nr=json.loads(sys.stdin.readline());assert r["action"] in ("list","load","sync")\nprint(json.dumps(dict(configured=False,events=[],monthEvents=[],days=[],total=0,providers=[],month=r.get("month"),date=r.get("selectedDate"))))\n')
fixture_calendar.chmod(0o700);env['PATH']=str(fixture_bin)+':'+env['PATH']
procs=[]
def launch(command,name):
 p=sp.Popen(command,env=env,stdout=(run/(name+'.log')).open('w'),stderr=sp.STDOUT,start_new_session=True);procs.append(p);return p
def call(*command):return sp.check_output(command,env=env,text=True,timeout=10)
def wait(fn):
 end=time.monotonic()+20
 while time.monotonic()<end:
  try:
   if fn():return
  except (ValueError,OSError,sp.SubprocessError):pass
  time.sleep(.15)
 raise AssertionError('UI readiness timed out; inspect saved logs')
def ipc(method,*values):return call('quickshell','-p',str(assets),'ipc','call','desktop',method,*values)
def state():return json.loads(ipc('filesStatus'))
def key(*values):call('wtype','-s','150',*values)
def window_capture(title, filename):
 def nodes(node):
  yield node
  for child in node.get('nodes',[])+node.get('floating_nodes',[]):yield from nodes(child)
 def rendered():
  matches=[node for node in nodes(json.loads(call('swaymsg','-t','get_tree','-r'))) if node.get('app_id')=='org.quickshell' and node.get('name')==title and node.get('visible') and node.get('scratchpad_state')=='none']
  if not matches:return False
  rect=matches[0]['rect'];call('grim',str(run/filename))
  from PIL import Image,ImageStat
  image=Image.open(run/filename).convert('RGB')
  area=(max(0,rect['x']+20),max(0,rect['y']+50),min(image.width,rect['x']+rect['width']-20),min(image.height,rect['y']+rect['height']-20))
  if area[2]<=area[0] or area[3]<=area[1]:return False
  statistics=ImageStat.Stat(image.crop(area))
  # A mapped transparent/blank window must not pass a behavioural smoke.
  return max(statistics.mean)>10 and max(statistics.stddev)>2
 wait(rendered)
try:
 conf=run/'sway.conf';conf.write_text('output HEADLESS-1 resolution 1440x1000\nxwayland disable\ndefault_border none\nfor_window [app_id="org.quickshell"] floating enable\n')
 if args.effects:
  with conf.open('a') as stream:stream.write('corner_radius 12\nshadows enable\nshadow_blur_radius 28\nanimation_duration_ms 0\nblur enable\nblur_xray disable\nlayer_effects "lumen-dock" "blur enable; corner_radius 22"\nlayer_effects "lumen-menu-popup" "blur enable; blur_ignore_transparent enable"\n')
 launch([args.sway,'--unsupported-gpu','-c',str(conf)],'sway');wait(lambda:list(runtime.glob('sway-ipc*.sock')))
 env['SWAYSOCK']=str(next(runtime.glob('sway-ipc*.sock')));env['WAYLAND_DISPLAY']=next(p.name for p in runtime.glob('wayland-*') if not p.name.endswith('.lock'))
 capabilities=json.loads(call('swaymsg','-t','get_version','-r'))
 env['LUMEN_NATIVE_WINDOW_CONTROLS']='1' if capabilities.get('native_window_controls') else '0'
 env['LUMEN_NATIVE_WINDOW_ACTIONS']='1' if capabilities.get('native_window_actions') else '0'
 env['LUMEN_UNIFIED_OWNED_HEADERS']='1' if capabilities.get('native_window_actions') else '0'
 env['LUMEN_COMPOSITOR_EFFECTS']='1' if capabilities.get('compositor_effects') else '0'
 palettes=json.loads((assets.parent/'appearance/palette.json').read_text())
 appearance=run/'appearance.json';appearance.write_text(json.dumps({'font':palettes['font'],'palette':palettes[args.theme],'preferences':{'mode':args.theme,'reduceMotion':True}}))
 env['LUMEN_APPEARANCE_STATE']=str(appearance)
 launch(['quickshell','-p',str(assets)],'shell');wait(lambda:json.loads(ipc('status')))
 if args.menu_only:
  layouts=[]
  for width in (1080,720,480):
   call('swaymsg',f'output HEADLESS-1 resolution {width}x1000')
   wait(lambda:json.loads(ipc('menuLayoutStatus'))[0]['width']==width)
   layout=json.loads(ipc('menuLayoutStatus'))[0];assert layout['menuEnd']<=layout['statusStart']-4,layout
   if width<=720:assert layout['overflow']>0,layout
   if layout['overflow']:
    ipc('menu','Overflow');wait(lambda:json.loads(ipc('layout'))[0]['menu']=='Overflow')
    key('-k','Down');wait(lambda:json.loads(ipc('layout'))[0]['selectedIndex']>=0)
    time.sleep(.3);call('grim',str(run/f'menu-overflow-{width}.png'))
    key('-k','Return');wait(lambda:json.loads(ipc('layout'))[0]['menu']!='Overflow')
    key('-k','Escape')
   layouts.append(layout)
  log=(run/'shell.log').read_text();assert not any(v in log for v in ('Failed to load configuration','ReferenceError:','TypeError:','Unable to assign')),log
  (run/'result.json').write_text(json.dumps(dict(passed=True,checks=['1080/720/480 no menu-status collision','hidden menus and status actions in overflow','real keyboard selection, activation and dismissal'],layouts=layouts),indent=2))
  print(run/'result.json');raise SystemExit(0)

 if args.desktop_widgets:
  wait(lambda:json.loads(ipc('desktopWidgetStatus'))['outputs'])
  widget_state=json.loads(ipc('desktopWidgetStatus'));assert not widget_state['error'],widget_state
  assert any(w['visible'] and w['widgetWidth']>100 and w['widgetHeight']>100 and w['monochrome'] for w in widget_state['outputs']),widget_state
  (run/'desktop-widgets.json').write_text(json.dumps(widget_state,indent=2))
  time.sleep(.5)
  call('grim',str(run/'desktop-widgets.png'))

 ipc('files',str(docs));wait(lambda:state()['count']==1 and not state()['busy'])
 for number,view in ((1,'grid'),(2,'list'),(3,'columns'),(4,'gallery')):
  key('-M','ctrl','-k',str(number),'-m','ctrl');wait(lambda:state()['view']==view)
 key('-k','Down','-k','space');wait(lambda:json.loads(ipc('quickLookStatus'))['type']=='text');key('-k','Escape')
 ipc('textTools');wait(lambda:json.loads(ipc('textToolsStatus'))['visible'])
 # wtype exiting does not acknowledge Qt processing the injected key events.
 # Verify every transition instead of racing the last key with a single read.
 for page in ('Speech','Replacements','Shortcuts','Spelling','Dictation','Services','Fonts','Colours'):
  key('-M','ctrl','-k','Tab','-m','ctrl')
  wait(lambda:json.loads(ipc('textToolsStatus'))['page']==page)
 key('-k','Escape');ipc('focusDock');time.sleep(.3);key('-k','End','-k','Return');wait(lambda:state()['path']=='trash:')
 window_capture('Files — Trash','files.png')
 ipc('settings','Appearance');wait(lambda:not json.loads(ipc('settingsStatus'))['busy']);window_capture('System Settings','settings.png')
 ipc('settings','VPN');wait(lambda:json.loads(ipc('settingsStatus'))['page']=='VPN');window_capture('System Settings','vpn-settings.png')
 if capabilities.get('native_window_actions') and args.click_pointer:
  def allnodes(node):
   yield node
   for c in node.get('nodes',[])+node.get('floating_nodes',[]):yield from allnodes(c)
  def settings_node():return next(n for n in allnodes(json.loads(call('swaymsg','-t','get_tree'))) if n.get('app_id')=='org.quickshell' and n.get('name')=='System Settings')
  def press_control(index):
   r=settings_node()['rect'];call('swaymsg',f"seat seat0 cursor set {r['x']+20+20*index} {r['y']+24}");call(str(args.click_pointer.resolve()),'pointer','click','left')
  before=settings_node()['rect'];press_control(2);wait(lambda:settings_node()['rect']!=before)
  assert settings_node()['fullscreen_mode']==0
  press_control(2);wait(lambda:settings_node()['rect']==before)
  press_control(1);wait(lambda:settings_node()['scratchpad_state']!='none')
  ipc('activate','lumen-system-settings');wait(lambda:settings_node()['scratchpad_state']=='none')
  assert settings_node()['rect']==before
  press_control(0);wait(lambda:not json.loads(ipc('settingsStatus'))['visible'])
  print('PASS owned Settings header buttons and Dock activation restore placement',flush=True)

 key('-k','Escape');ipc('menu','Controls');wait(lambda:json.loads(ipc('layout'))[0]['menu']=='Controls')
 assert json.loads(ipc('layout'))[0]['scrollY']==0,'Control Centre retained the previous panel scroll offset'
 time.sleep(.5);call('grim',str(run/'controls.png'));key('-k','Escape')
 ipc('menu','Launcher');time.sleep(.5);call('grim',str(run/'search.png'));key('-k','Escape')
 ipc('settings','Internet Accounts');wait(lambda:json.loads(ipc('settingsStatus'))['page']=='Internet Accounts');call('grim',str(run/'calendar-accounts.png'));key('-k','Escape')
 # A high-frequency background makes accidental full-screen blur observable.
 pattern=run/'checker.ppm';pattern.write_bytes(b'P6\n64 64\n255\n'+bytes(v for y in range(64) for x in range(64) for v in ((40,90,130) if (x//4+y//4)%2 else (220,180,140))))
 call('swaymsg','output HEADLESS-1 bg '+str(pattern)+' tile');time.sleep(.3);call('grim',str(run/'calendar-before.png'))
 ipc('notificationCentre','open');ipc('notificationCentre','open');time.sleep(.5)
 assert json.loads(ipc('layout'))[0]['menu']=='Date'
 ipc('notificationCentre','close');wait(lambda:json.loads(ipc('layout'))[0]['menu']=='')
 ipc('notificationCentre','open');time.sleep(.3)
 for scale in ('1','2'):
  call('swaymsg','output HEADLESS-1 scale '+scale);time.sleep(.3)
  layout=json.loads(ipc('layout'))[0]
  assert layout['scale']==1,layout
  assert layout['panelHeight']<=layout['height'] and layout['panelWidth']<=layout['width'],layout
  capture=run/('calendar-popover-scale-'+scale+'.png');call('grim',str(capture))
  if args.effects and scale=='1':
   from PIL import Image
   area=(16,800,96,880)
   assert Image.open(run/'calendar-before.png').crop(area).tobytes()==Image.open(capture).crop(area).tobytes(),'Popup blur changed pixels outside its content'
 key('-k','Escape')
 for scale in ('1','1.25','1.5','2'):
  call('swaymsg','output HEADLESS-1 scale '+scale);time.sleep(.5);assert state()['visible']
  call('grim',str(run/('scale-'+scale+'.png')))
 checks=['Files four views','Quick Look text','Text Tools keyboard pages','Dock keyboard Trash','100/125/150/200 percent scales','Calendar Accounts UI','Notification Centre idempotent open/close and unscaled viewport at 100/200 percent']
 if args.effects:checks.append('Transparent popup pixels do not blur the desktop outside content')
 if args.drag_pointer:
  call('swaymsg','output HEADLESS-1 scale 1');time.sleep(.4)
  def nodes(tree):
   yield tree
   for child in tree.get('nodes',[])+tree.get('floating_nodes',[]):yield from nodes(child)
  for number,view,start,delta in ((1,'grid',(366,150),(-112,0)),(2,'list',(300,177),(0,-31))):
   folder=run/('drag-'+view);folder.mkdir();(folder/'Target').mkdir();payload=folder/'payload.txt';payload.write_text('stationary spring-drop fixture')
   ipc('files',str(folder));wait(lambda:state()['path']==str(folder) and state()['count']==2 and not state()['busy'])
   key('-M','ctrl','-k',str(number),'-m','ctrl');wait(lambda:state()['view']==view);time.sleep(.4)
   window=next(n for n in nodes(json.loads(call('swaymsg','-t','get_tree'))) if n.get('focused') and n.get('app_id')=='org.quickshell')
   rect=window['rect'];call('swaymsg',f"seat seat0 cursor set {rect['x']+start[0]} {rect['y']+start[1]}")
   call(str(args.drag_pointer.resolve()),'pointer','move',str(delta[0]),str(delta[1]))
   wait(lambda:(folder/'Target/payload.txt').exists() and not state()['working'])
   assert state()['path']==str(folder/'Target')
   assert payload.read_bytes()==(folder/'Target/payload.txt').read_bytes()
   checks.append(view+' spring-open/drop without further pointer movement; source retained')
 if args.desktop_widgets:
  call('swaymsg','[app_id="org.quickshell"] move scratchpad')
  time.sleep(.5)
  call('grim',str(run/'desktop-widgets-painted.png'))
 log=(run/'shell.log').read_text()
 assert not any(s in log for s in ('Failed to load configuration','ReferenceError:','TypeError:')),log
 sway_log=(run/'sway.log').read_text()
 assert 'Unknown/invalid command' not in sway_log,sway_log
 if args.effects:
  from PIL import ImageStat
  for capture,area in (('files.png',(470,260,1000,700)),('controls.png',(1130,100,1420,450))):
   mean=ImageStat.Stat(Image.open(run/capture).convert('RGB').crop(area)).mean
   assert sum(mean)/3 > 60,(capture,mean,'incomplete headless rendering')
  checks.append('Effects captures contain rendered window and popup interiors')
 (run/'result.json').write_text(json.dumps(dict(passed=True,assets=str(assets),checks=checks),indent=2))
 print(run/'result.json')
finally:
 for process in reversed(procs):
  if process.poll() is None:
   os.killpg(process.pid,signal.SIGTERM)
   try:process.wait(timeout=5)
   except sp.TimeoutExpired:os.killpg(process.pid,signal.SIGKILL)
 shutil.rmtree(runtime,ignore_errors=True)
