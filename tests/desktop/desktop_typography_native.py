"""Production desktop captions, input mask and visible-layout keyboard navigation."""
import argparse,hashlib,json,math,os,re,shutil,signal,subprocess as sp,sys,time
from pathlib import Path
from application_fixture.isolation import run
from application_fixture.keyboard import PhysicalKeyboard
from application_fixture.provenance import FrozenSource,generated
from private_native_bus import start_private_bus
from fixture_resources import close_reaped_pipes
p=argparse.ArgumentParser();p.add_argument('--keyboard',type=Path,required=True);p.add_argument('--pointer-tool',type=Path,required=True);p.add_argument('--launcher',type=Path,required=True);p.add_argument('--sway',type=Path,required=True);p.add_argument('--font-config',type=Path,required=True);p.add_argument('--software-driver',type=Path,required=True);p.add_argument('--output',type=Path,required=True);p.add_argument('--effects',action='store_true');p.add_argument('--text-scale',type=float,default=1.5);p.add_argument('--navigation-only',action='store_true');p.add_argument('--inside',action='store_true');a=p.parse_args();repo=Path(__file__).resolve().parents[2]
for name in ('launcher','sway','font_config','software_driver','keyboard','pointer_tool'):
 value=getattr(a,name).resolve();assert str(value).startswith('/nix/store/') and value.exists();setattr(a,name,value)
if not a.inside:
 a.output=a.output.resolve();a.output.mkdir(mode=0o700,parents=True,exist_ok=False)
 runtime=Path(sys.executable).resolve();frozen=FrozenSource(repo,__file__,[a.launcher,a.sway,a.font_config,a.keyboard,a.pointer_tool,runtime]);prefix=re.split(r'\n[ \t]*exec quickshell ',a.launcher.read_text(),maxsplit=1)[0];env=dict(v.decode().split('=',1) for v in sp.check_output(['bash','-c',prefix+'\nexec env -0']).split(b'\0') if b'=' in v)
 args=[str(runtime),'/source/tests/desktop/desktop_typography_native.py','--inside','--output','/artifacts']+sum((['--'+name.replace('_','-'),str(getattr(a,name))] for name in ('launcher','sway','font_config','software_driver','keyboard','pointer_tool')),[])+['--text-scale',str(a.text_scale)]+(['--navigation-only']if a.navigation_only else [])+(['--effects'] if a.effects else [])
 code=1
 try:code=run(repo,a.output,args,env,software_graphics=a.effects)
 finally:frozen.finish(a.output,code)
 raise SystemExit(code)
assert repo==Path('/source') and os.getuid()!=0 and os.environ['HOME']=='/home/fixture' and not Path('/.old-root').exists();assert not any((Path('/dev')/name).exists() for name in ('dri','snd','video0','tty'))
out=a.output;processes=[];env=dict(os.environ,WLR_BACKENDS='headless',WLR_HEADLESS_OUTPUTS='1',WLR_RENDERER='gles2' if a.effects else 'pixman',WLR_RENDERER_FORCE_SOFTWARE='1' if a.effects else '0',WLR_RENDERER_ALLOW_SOFTWARE='1',WLR_LIBINPUT_NO_DEVICES='1',LIBGL_ALWAYS_SOFTWARE='1',MESA_LOADER_DRIVER_OVERRIDE='llvmpipe',LIBGL_DRIVERS_PATH=str(a.software_driver/'lib/dri'),__EGL_VENDOR_LIBRARY_FILENAMES=str(a.software_driver/'share/glvnd/egl_vendor.d/50_mesa.json'),FONTCONFIG_FILE=str(a.font_config),QT_QPA_PLATFORM='wayland',QT_QUICK_BACKEND='software',QT_QPA_PLATFORMTHEME='generic',QT_NO_XDG_DESKTOP_PORTAL='1',QT_ACCESSIBILITY='0',NO_AT_BRIDGE='1',GTK_A11Y='none',LUMEN_UNIFIED_OWNED_HEADERS='1',LUMEN_NATIVE_WINDOW_CONTROLS='0',LUMEN_NATIVE_WINDOW_ACTIONS='0',LUMEN_LAUNCH_SCOPES='0',LUMEN_NEARBY_SHARING_PROVIDER='',GLOBAL_MENU_PYTHON=sys.executable,LUMEN_COMPOSITOR_EFFECTS='1' if a.effects else '0')
for key in ('SWAYSOCK','WAYLAND_DISPLAY','DISPLAY','AT_SPI_BUS_ADDRESS'):env.pop(key,None)
assets=out/'assets';shutil.copytree(repo/'res/lumen-desktop',assets);qml=out/'overlays.qml';qml.write_bytes((repo/'tests/desktop/typography_fixture/desktop.qml').read_bytes());changes=[]
def replace_file(relative,text,scope):
 target=assets/relative;original=target.read_bytes();target.write_text(text);changes.append(dict(path=relative,originalSha256=hashlib.sha256(original).hexdigest(),modifiedSha256=hashlib.sha256(text.encode()).hexdigest(),scope=scope))
directory=Path('/home/fixture/Desktop');directory.mkdir();names=['0 Private document.txt','1 Private image.png','2 Another private document.txt','3 Another private image.png']
from PIL import Image
for index,name in enumerate(names):
 path=directory/name
 if name.endswith('.png'):Image.new('RGB',(8,8),(20+index*20,80,120)).save(path)
 else:path.write_text('Disposable private desktop text '+str(index))
configHome=Path(env['XDG_CONFIG_HOME']);configHome.mkdir(parents=True,exist_ok=True);(configHome/'user-dirs.dirs').write_text('XDG_DESKTOP_DIR="/home/fixture/Desktop"\n')
import importlib.util
spec=importlib.util.spec_from_file_location('private_desktop_layout',assets/'shell/desktop_layout.py');layout=importlib.util.module_from_spec(spec);spec.loader.exec_module(layout)
keys=[str((directory/name).stat().st_dev)+':'+str((directory/name).stat().st_ino)for name in names];positions=dict(zip(keys,[[.8,.2],[.8,.7],[.2,.2],[.2,.7]]));layout.handle(dict(action='save',directory=str(directory),items=keys,layout=dict(grouping='none',positions=positions)))
fileHashes={name:hashlib.sha256((directory/name).read_bytes()).hexdigest()for name in names}
backend=assets/'shell/files.py';before=backend.read_bytes();text=before.decode();needle="    if action=='volumes':\n";assert text.count(needle)==1;replace_file('shell/files.py',text.replace(needle,needle+'        return dict(volumes=[])\n'),'Namespace-only real file listing; volume discovery excluded, no mutation methods exercised')
(out/'provider-isolation.json').write_text(json.dumps(changes,indent=2));palette=json.loads((assets/'appearance/palette.json').read_text());appearance=out/'appearance.json';env['LUMEN_APPEARANCE_STATE']=str(appearance);settings=out/'settings.json';settings.write_text(json.dumps(dict(search=dict(historyEnabled=False),clipboard=dict(enabled=False))));env['LUMEN_NIX_SETTINGS_ACTIVE']=str(settings)
def preference(mode,reduced):
 pending=appearance.with_suffix('.pending');pending.write_text(json.dumps(dict(font=palette['font'],palette=palette[mode],preferences=dict(mode=mode,reduceMotion=True,reduceTransparency=reduced),searchPreferences=dict(disabledProviders=['clipboard']))));pending.replace(appearance)
preference('light',True);env['LUMEN_FIXTURE_PALETTES']=json.dumps(palette)
def launch(command,name,**kwargs):
 with (out/name).open('w') as log:proc=sp.Popen(command,env=env,stdout=log,stderr=sp.STDOUT,start_new_session=True,**kwargs)
 processes.append(proc);return proc
def call(*args):return sp.check_output(list(map(str,args)),env=env,text=True,timeout=6).strip()
def ipc(method,*args):return call('quickshell','-p',qml,'ipc','call','desktopText',method,*args)
def state():return json.loads(ipc('status'))
def tree():return json.loads(call(a.sway.parent/'swaymsg','-r','-t','get_tree'))
def nodes(node):
 yield node
 for child in node.get('nodes',[])+node.get('floating_nodes',[]):yield from nodes(child)
def wait(fn):
 end=time.monotonic()+15
 while time.monotonic()<end:
  try:value=fn()
  except (sp.SubprocessError,OSError,ValueError):value=None
  if value:return value
  time.sleep(.04)
 (out/'timeout-tree.json').write_text(json.dumps(tree(),indent=2))
 try:(out/'timeout-state.json').write_text(json.dumps(state(),indent=2))
 except (sp.SubprocessError,OSError,ValueError):pass
 call('grim',out/'timeout.png');raise AssertionError('Live overlay observation deadline')
try:
 start_private_bus(env,out,processes);config=out/'sway.conf';config.write_text('output HEADLESS-1 resolution 1200x900\nxwayland disable\nfocus_follows_mouse no\nfor_window [app_id="org.quickshell"] floating enable, border none\n');launch([str(a.sway),'--unsupported-gpu','-c',str(config)],'sway.log');env['SWAYSOCK']=str(wait(lambda:next(Path(env['XDG_RUNTIME_DIR']).glob('sway-ipc*.sock'),None)));env['WAYLAND_DISPLAY']=wait(lambda:next((p.name for p in Path(env['XDG_RUNTIME_DIR']).glob('wayland-*') if not p.name.endswith('.lock')),None));shell=launch(['quickshell','-p',str(qml)],'shell.log');wait(lambda:ipc('ready')=='true');wait(lambda:state()['ready'] and state()['count']==4 and all(key in state()['layout']['positions']for key in keys))
 with (out/'keyboard.log').open('w') as log:keyboard=PhysicalKeyboard(a.keyboard,env,log)
 processes.append(keyboard.process);pointer=launch([str(a.pointer_tool),'pointer','move','0','0'],'pointer.log',stdin=sp.PIPE,text=True);pointer.stdin.write('m 0 0\n');pointer.stdin.flush();wait(lambda:any(seat.get('capabilities',0)&1 for seat in json.loads(call(a.sway.parent/'swaymsg','-r','-t','get_seats'))))
 from PIL import Image,ImageColor
 checks=[]
 def row(index):return next(item for item in state()['items']if item['path']==str(directory/names[index]))
 def click(item):
  r=item['rect'];call(a.sway.parent/'swaymsg','seat seat0 cursor set '+str(round(r['x']+r['width']/2))+' '+str(round(r['y']+25)));pointer.stdin.write('m 0 0\np\nr\n');pointer.stdin.flush();wait(lambda:state()['active']and state()['currentPath']==item['path'])
 ipc('configure','light',str(a.text_scale));wait(lambda:all(item['caption']and item['caption']['resolvedFamily']==palette['font']for item in state()['items']if not item['overflow']))
 if not a.navigation_only:
  snapshot=state();(out/'caption-baseline.json').write_text(json.dumps(snapshot,indent=2));assert all(item['caption']['size']==math.floor(11*a.text_scale+.5)for item in snapshot['items']if not item['overflow']),snapshot
 click(row(2));before=state();keyboard.key('-k','Right');keyboard.key('-k','Down');wait(lambda:state()['currentPath']!=before['currentPath']);after=state();(out/'navigation-receipt.json').write_text(json.dumps(dict(before=before,after=after),indent=2));assert after['currentPath']==str(directory/names[1]),after;checks.append(dict(case='ordered-physical-right-down-layout-neighbours',before=before,after=after))
 # A failed current request must not turn preserved rows into an authoritative empty list.
 ipc('navigate',str(directory/'missing-folder'));wait(lambda:state()['error'] and not state()['listingCurrent']);assert state()['layout']['positions']==positions,state();checks.append(dict(case='failed-list-retains-four-placement-identities',state=state()))
 ipc('navigate',str(directory));wait(lambda:state()['listingComplete'] and state()['count']==4)
 for dimensions in ('1200x900','700x600'):
  call(a.sway.parent/'swaymsg','output HEADLESS-1 resolution '+dimensions);wait(lambda:state()['width']==int(dimensions.split('x')[0]))
  for mode in ('light','dark'):
   for scale in (1,1.15,1.3,1.5):
    ipc('configure',mode,str(scale));wait(lambda:all(item['caption'] and item['caption']['size']==math.floor(11*scale+.5)for item in state()['items']if not item['overflow']))
    snapshot=state();assert all(item['icon']==dict(width=46,height=46)for item in snapshot['items']if not item['overflow']),snapshot
    if scale==1:baselineWidths={item['key']:item['caption']['naturalWidth']/item['caption']['rect']['width']for item in snapshot['items']if not item['overflow']}
    else:
     for item in snapshot['items']:
      if not item['overflow']:assert item['caption']['naturalWidth']/item['caption']['rect']['width']<=baselineWidths[item['key']]+.015,item
    capture=out/(dimensions+'-'+mode+'-'+str(scale)+'-desktop.png')
    def painted():
     call('grim',capture)
     with Image.open(capture)as image:
      for item in snapshot['items']:
       if item['overflow']:continue
       r=item['caption']['rect'];crop=image.crop((math.floor(r['x']),math.floor(r['y']),math.ceil(r['x']+r['width']),math.ceil(r['y']+r['height']))).convert('RGB')
       ink=ImageColor.getrgb(item['caption']['color']);pixels=crop.convert('RGB').get_flattened_data()
       if sum(max(abs(pixel[channel]-ink[channel])for channel in range(3))<30 for pixel in pixels)<10:return False
     return True
    wait(painted);checks.append(dict(case='painted-caption-capacity-and-fixed-optical-icon',mode=mode,scale=scale,state=snapshot,capture=str(capture)))
 # Caption-only input outside the old108px tile must remain inside the production mask.
 target=row(0);r=target['caption']['rect'];beforeClicks=state()['backgroundClicks'];assert target['rect']['width']>120
 call(a.sway.parent/'swaymsg','seat seat0 cursor set '+str(round(target['rect']['x']+120))+' '+str(round(r['y']+10)));pointer.stdin.write('m 0 0\np\nr\n');pointer.stdin.flush();wait(lambda:state()['currentPath']==target['path']);assert state()['backgroundClicks']==beforeClicks
 keyboard.key('-k','space');wait(lambda:state()['quickLooks']==[target['path']]);checks.append(dict(case='expanded-caption-mask-and-captured-quicklook',state=state()))
 ipc('group','kind');wait(lambda:any(item['stack']for item in state()['items']));header=next(item for item in state()['items']if item['stack']and not item['overflow']);r=header['rect']
 call(a.sway.parent/'swaymsg','seat seat0 cursor set '+str(round(r['x']+r['width']/2))+' '+str(round(r['y']+25)));pointer.stdin.write('m 0 0\np\nr\n');pointer.stdin.flush();wait(lambda:state()['currentKey']==header['key']and len(state()['items'])>2)
 keyboard.key('-k','space');wait(lambda:len(state()['items'])==2);assert state()['currentPath']=='' and state()['selected']==[];checks.append(dict(case='stack-header-physical-space-collapse-without-file-action',state=state()))
 ipc('group','none');wait(lambda:len(state()['items'])==4 and state()['layout']['grouping']=='none')
 # Restart the production surface and both real providers against the same private durable state.
 os.killpg(shell.pid,signal.SIGTERM);shell.wait(timeout=5)
 shell=launch(['quickshell','-p',str(qml)],'shell-restart.log');wait(lambda:ipc('ready')=='true');wait(lambda:state()['listingComplete'] and state()['count']==4)
 assert state()['layout']['positions']==positions,state();checks.append(dict(case='restart-retains-four-placement-identities',state=state()))
 for name,expected in fileHashes.items():assert hashlib.sha256((directory/name).read_bytes()).hexdigest()==expected
 (out/'result.json').write_text(json.dumps(dict(passed=True,checks=checks,scope='Real disposable desktop files, production surface/mask and physical keys; no external app actions'),indent=2));print(json.dumps(dict(passed=True,checks=len(checks))))
finally:
 generated(out,out)
 for proc in reversed(processes):
  if proc.poll() is None:
   try:os.killpg(proc.pid,signal.SIGTERM)
   except ProcessLookupError:pass
 for proc in reversed(processes):
  try:proc.wait(timeout=5)
  except sp.TimeoutExpired:os.killpg(proc.pid,signal.SIGKILL);proc.wait(timeout=5)
 close_reaped_pipes(processes)
 if (out/'shell.log').exists():
  text='\n'.join(path.read_text()for path in out.glob('shell*.log'));warnings=[line for line in text.splitlines()if 'WARN'in line];path=out/'result.json'
  if path.exists():data=json.loads(path.read_text());data['runtimeWarnings']=warnings;path.write_text(json.dumps(data,indent=2))
  assert not warnings and not any(marker in text for marker in ('ReferenceError:','TypeError:','Binding loop','Failed to load configuration','Cannot assign')),text
