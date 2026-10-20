"""Actual enlarged launcher/switcher paint, traversal and captured actions."""
import argparse,hashlib,json,os,re,shutil,signal,subprocess as sp,sys,time
from collections import Counter
from pathlib import Path
from application_fixture.isolation import run
from application_fixture.keyboard import PhysicalKeyboard
from application_fixture.provenance import FrozenSource,generated
from private_native_bus import start_private_bus
from fixture_resources import close_reaped_pipes
p=argparse.ArgumentParser();p.add_argument('--keyboard',type=Path,required=True);p.add_argument('--pointer-tool',type=Path,required=True);p.add_argument('--launcher',type=Path,required=True);p.add_argument('--sway',type=Path,required=True);p.add_argument('--font-config',type=Path,required=True);p.add_argument('--software-driver',type=Path,required=True);p.add_argument('--output',type=Path,required=True);p.add_argument('--effects',action='store_true');p.add_argument('--inside',action='store_true');a=p.parse_args();repo=Path(__file__).resolve().parents[2]
for name in ('launcher','sway','font_config','software_driver','keyboard','pointer_tool'):
 value=getattr(a,name).resolve();assert str(value).startswith('/nix/store/') and value.exists();setattr(a,name,value)
if not a.inside:
 a.output=a.output.resolve();a.output.mkdir(mode=0o700,parents=True,exist_ok=False)
 runtime=Path(sys.executable).resolve();frozen=FrozenSource(repo,__file__,[a.launcher,a.sway,a.font_config,a.keyboard,a.pointer_tool,runtime]);prefix=re.split(r'\n[ \t]*exec quickshell ',a.launcher.read_text(),maxsplit=1)[0];env=dict(v.decode().split('=',1) for v in sp.check_output(['bash','-c',prefix+'\nexec env -0']).split(b'\0') if b'=' in v)
 args=[str(runtime),'/source/tests/desktop/launcher_typography_native.py','--inside','--output','/artifacts']+sum((['--'+name.replace('_','-'),str(getattr(a,name))] for name in ('launcher','sway','font_config','software_driver','keyboard','pointer_tool')),[])+(['--effects'] if a.effects else [])
 code=1
 try:code=run(repo,a.output,args,env,software_graphics=a.effects)
 finally:frozen.finish(a.output,code)
 raise SystemExit(code)
assert repo==Path('/source') and os.getuid()!=0 and os.environ['HOME']=='/home/fixture' and not Path('/.old-root').exists();assert not any((Path('/dev')/name).exists() for name in ('dri','snd','video0','tty'))
out=a.output;processes=[];env=dict(os.environ,WLR_BACKENDS='headless',WLR_HEADLESS_OUTPUTS='1',WLR_RENDERER='gles2' if a.effects else 'pixman',WLR_RENDERER_FORCE_SOFTWARE='1' if a.effects else '0',WLR_RENDERER_ALLOW_SOFTWARE='1',WLR_LIBINPUT_NO_DEVICES='1',LIBGL_ALWAYS_SOFTWARE='1',MESA_LOADER_DRIVER_OVERRIDE='llvmpipe',LIBGL_DRIVERS_PATH=str(a.software_driver/'lib/dri'),__EGL_VENDOR_LIBRARY_FILENAMES=str(a.software_driver/'share/glvnd/egl_vendor.d/50_mesa.json'),FONTCONFIG_FILE=str(a.font_config),QT_QPA_PLATFORM='wayland',QT_QUICK_BACKEND='software',QT_QPA_PLATFORMTHEME='generic',QT_NO_XDG_DESKTOP_PORTAL='1',QT_ACCESSIBILITY='0',NO_AT_BRIDGE='1',GTK_A11Y='none',LUMEN_UNIFIED_OWNED_HEADERS='1',LUMEN_NATIVE_WINDOW_CONTROLS='0',LUMEN_NATIVE_WINDOW_ACTIONS='0',LUMEN_LAUNCH_SCOPES='0',LUMEN_NEARBY_SHARING_PROVIDER='',GLOBAL_MENU_PYTHON=sys.executable,LUMEN_COMPOSITOR_EFFECTS='1' if a.effects else '0')
for key in ('SWAYSOCK','WAYLAND_DISPLAY','DISPLAY','AT_SPI_BUS_ADDRESS'):env.pop(key,None)
assets=out/'assets';shutil.copytree(repo/'res/lumen-desktop',assets);qml=out/'overlays.qml';qml.write_bytes((repo/'tests/desktop/typography_fixture/overlays.qml').read_bytes());changes=[]
def replace_file(relative,text,scope):
 target=assets/relative;original=target.read_bytes();target.write_text(text);changes.append(dict(path=relative,originalSha256=hashlib.sha256(original).hexdigest(),modifiedSha256=hashlib.sha256(text.encode()).hexdigest(),scope=scope))
replace_file('shell/window_overview.py','import json,sys\nr=json.loads(sys.stdin.readline());assert r["action"]=="status"\nprint(json.dumps(dict(windows=[dict(id=101,pid=11,title="First document",app="First application",group="fixture",application="fixture",workspace="1")],workspaces=[],outputs=[],revision=1,sessionEpoch="private",autoReorder=True,assignments={})))\n','Fixed synthetic read-only window corpus; all non-status commands refuse')
replace_file('shell/system-controls.py','import json,sys\nassert sys.argv[1:]==["displays"]\nprint(json.dumps(dict(displays=[])))\n','Unavailable private brightness discovery; no hardware commands')
(out/'provider-isolation.json').write_text(json.dumps(changes,indent=2));palette=json.loads((assets/'appearance/palette.json').read_text());appearance=out/'appearance.json';env['LUMEN_APPEARANCE_STATE']=str(appearance);settings=out/'settings.json';settings.write_text(json.dumps(dict(search=dict(historyEnabled=False),clipboard=dict(enabled=False))));env['LUMEN_NIX_SETTINGS_ACTIVE']=str(settings)
def preference(mode,reduced):
 pending=appearance.with_suffix('.pending');pending.write_text(json.dumps(dict(font=palette['font'],palette=palette[mode],preferences=dict(mode=mode,reduceMotion=True,reduceTransparency=reduced),searchPreferences=dict(disabledProviders=['clipboard']))));pending.replace(appearance)
preference('light',True);env['LUMEN_FIXTURE_PALETTES']=json.dumps(palette)
def launch(command,name,**kwargs):
 with (out/name).open('w') as log:proc=sp.Popen(command,env=env,stdout=log,stderr=sp.STDOUT,start_new_session=True,**kwargs)
 processes.append(proc);return proc
def call(*args):return sp.check_output(list(map(str,args)),env=env,text=True,timeout=6).strip()
def ipc(method,*args):return call('quickshell','-p',qml,'ipc','call','launcherText',method,*args)
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
 start_private_bus(env,out,processes);config=out/'sway.conf';config.write_text('output HEADLESS-1 resolution 1200x900\nxwayland disable\nfocus_follows_mouse no\nfor_window [app_id="org.quickshell"] floating enable, border none\n');launch([str(a.sway),'--unsupported-gpu','-c',str(config)],'sway.log');env['SWAYSOCK']=str(wait(lambda:next(Path(env['XDG_RUNTIME_DIR']).glob('sway-ipc*.sock'),None)));env['WAYLAND_DISPLAY']=wait(lambda:next((p.name for p in Path(env['XDG_RUNTIME_DIR']).glob('wayland-*') if not p.name.endswith('.lock')),None));shell=launch(['quickshell','-p',str(qml)],'shell.log');wait(lambda:state()['width']>0)
 with (out/'keyboard.log').open('w') as log:keyboard=PhysicalKeyboard(a.keyboard,env,log)
 processes.append(keyboard.process);pointer=launch([str(a.pointer_tool),'pointer','move','0','0'],'pointer.log',stdin=sp.PIPE,text=True);pointer.stdin.write('m 0 0\n');pointer.stdin.flush();wait(lambda:any(seat.get('capabilities',0)&1 for seat in json.loads(call(a.sway.parent/'swaymsg','-r','-t','get_seats'))))
 from PIL import Image
 checks=[]
 def presented(label,selected_only=False):
  captured=state();capture=out/(label+'.png');receipts=[];frames=[]
  def painted():
   snapshot=state();assert (snapshot['family'],snapshot['currentKey'],snapshot['switcherLabel'])==(captured['family'],captured['currentKey'],captured['switcherLabel']),'Paint subject changed';origin=snapshot['body'];visible=[row for row in snapshot['texts']if row['rect']['width']>0 and row['rect']['height']>0];frames[:]=[snapshot]
   call('grim',capture)
   with Image.open(capture)as image:
    image=image.convert('RGB');receipts.clear()
    if snapshot['family']=='switcher':
     plate=max(snapshot['rectangles'],key=lambda row:row['width']*row['height']);r=plate['rect'];expected=tuple(int(plate['color'][i:i+2],16)for i in [1,3,5]);points=[(round(r['x']+2),round(r['y']+r['height']/2)),(round(r['x']+r['width']-3),round(r['y']+r['height']/2))]
     if any(max(abs(a-b)for a,b in zip(image.getpixel(point),expected))>2 for point in points):return False
    for row in visible:
     box=row['rect'];crop=image.crop(tuple(round(v)for v in [origin['x']+box['x'],origin['y']+box['y'],origin['x']+box['x']+box['width'],origin['y']+box['y']+box['height']]));color=tuple(int(row['color'][i:i+2],16)for i in [1,3,5]);ink=sum(max(abs(a-b)for a,b in zip(pixel,color))<=32 for pixel in crop.get_flattened_data());receipts.append(dict(text=row['text'],inkPixels=ink,cropPixels=crop.width*crop.height,rect=row['rect']))
   (out/(label+'-paint.json')).write_text(json.dumps(dict(state=snapshot,receipts=receipts),indent=2));required=[r for r in receipts if (snapshot['family']=='switcher'and(not selected_only or r['text']==snapshot['switcherLabel'])) or r['text']==(snapshot['wanted'] if snapshot['wanted']in('Open','Show Folder') else snapshot['currentLabel'])];
   if snapshot['family']=='switcher':
    plate=max(snapshot['rectangles'],key=lambda row:row['width']*row['height'])['rect']
    if any(r['rect']['x']<plate['x']-1 or r['rect']['y']<plate['y']-1 or r['rect']['x']+r['rect']['width']>plate['x']+plate['width']+1 or r['rect']['y']+r['rect']['height']>plate['y']+plate['height']+1 for r in required):return False
   return bool(required)and all(4<=r['inkPixels']<r['cropPixels']*.6 for r in required)
  wait(painted);return dict(capture=str(capture),state=frames[0],ink=receipts)
 def focus(label):
  ipc('focusLabel',label);wait(lambda:state()['control']and state()['control']['focus']);row=state()['control'];assert row['clipped']['width']>=row['rect']['width']-1 and row['clipped']['height']>=row['rect']['height']-1,row
 for small in [False,True]:
  call(a.sway.parent/'swaymsg','output HEADLESS-1 resolution '+('480x360'if small else'1200x900'))
  for mode in ['light','dark']:
   ipc('configure',mode,'1.5');ipc('open','launcher');wait(lambda:state()['family']=='launcher'and state()['visible']and state()['windowActive']and state()['width']==(480 if small else 1200));ipc('layout','list');wait(lambda:ipc('focusTarget')=='true');keyboard.type('application');wait(lambda:state()['query']=='application'and state()['launcher']['count']==2);keyboard.key('-k','Down');wait(lambda:state()['launcher']['selected']==1);receipt=presented(mode+'-'+str(small)+'-list');keyboard.key('-k','Return');wait(lambda:state()['actions']==['fixture.second']);checks.append(dict(mode=mode,small=small,case='list-selection-return',painted=receipt))
   ipc('open','launcher');ipc('layout','preview');wait(lambda:ipc('focusTarget')=='true');keyboard.type('application');wait(lambda:state()['query']=='application'and state()['launcher']['count']==2);focus('Search result preview');keyboard.key('-k','End');wait(lambda:state()['control']['contentY']>0 if small else True);keyboard.key('-k','Tab');wait(lambda:'Open'in state()['focus']);focus('Open');receipt=presented(mode+'-'+str(small)+'-preview');keyboard.key('-k','space');wait(lambda:state()['actions']==['fixture.first']);checks.append(dict(mode=mode,small=small,case='preview-space',painted=receipt))
   ipc('open','launcher');ipc('layout','preview');wait(lambda:ipc('focusTarget')=='true');keyboard.type('application');wait(lambda:state()['query']=='application'and state()['launcher']['count']==2);focus('Open');captured=state()['currentKey'];ipc('select','1');wait(lambda:state()['currentKey']!=captured);keyboard.key('-k','Return');wait(lambda:ipc('focusTarget')=='true');keyboard.type('z');wait(lambda:state()['query']=='applicationz');assert state()['actions']==[];checks.append(dict(mode=mode,small=small,case='changed-selection-refuses-return',captured=captured,state=state()));keyboard.key('-k','Escape');wait(lambda:not state()['visible'])
   ipc('open','launcher');ipc('layout','preview');wait(lambda:ipc('focusTarget')=='true');keyboard.type('application');wait(lambda:state()['query']=='application'and state()['launcher']['count']==2);focus('Open');captured=state()['currentKey'];ipc('removeSelected');wait(lambda:state()['currentKey']!=captured);keyboard.key('-k','space');wait(lambda:ipc('focusTarget')=='true');keyboard.type('z');wait(lambda:state()['query']=='applicationz');assert state()['actions']==[];checks.append(dict(mode=mode,small=small,case='removed-result-refuses-space',captured=captured,state=state()));keyboard.key('-k','Escape');wait(lambda:not state()['visible'])
   ipc('open','launcher');ipc('layout','preview');wait(lambda:ipc('focusTarget')=='true');keyboard.type('application');wait(lambda:state()['query']=='application'and state()['launcher']['count']==2);focus('Open');r=state()['control']['rect'];call(a.sway.parent/'swaymsg','seat seat0 cursor set '+str(round(r['x']+r['width']/2))+' '+str(round(r['y']+r['height']/2)));pointer.stdin.write('m 0 0\np\nr\n');pointer.stdin.flush();wait(lambda:state()['actions']==['fixture.first']);checks.append(dict(mode=mode,small=small,case='preview-pointer-click',state=state()))
   ipc('open','launcher');ipc('layout','preview');wait(lambda:ipc('focusTarget')=='true');keyboard.type('application');wait(lambda:state()['query']=='application'and state()['launcher']['count']==2);focus('Open');captured=state()['currentKey'];r=state()['control']['rect'];call(a.sway.parent/'swaymsg','seat seat0 cursor set '+str(round(r['x']+r['width']/2))+' '+str(round(r['y']+r['height']/2)));pointer.stdin.write('m 0 0\np\n');pointer.stdin.flush();wait(lambda:state()['control']['pressed']);ipc('select','1');wait(lambda:state()['currentKey']!=captured);pointer.stdin.write('r\n');pointer.stdin.flush();wait(lambda:not state()['control']['pressed']);wait(lambda:ipc('focusTarget')=='true');keyboard.type('z');wait(lambda:state()['query']=='applicationz');assert state()['actions']==[];checks.append(dict(mode=mode,small=small,case='changed-selection-refuses-pointer-release',captured=captured,state=state()));keyboard.key('-k','Escape');wait(lambda:not state()['visible'])
   ipc('open','launcher');ipc('openFiles');wait(lambda:ipc('focusTarget')=='true');keyboard.type('private');wait(lambda:state()['launcher']['count']==1);focus('Open');keyboard.key('-k','Tab');wait(lambda:'Show Folder'in state()['focus']);focus('Show Folder');receipt=presented(mode+'-'+str(small)+'-show-folder');keyboard.key('-k','Return');wait(lambda:bool(state()['actions']));assert state()['actions']==[dict(path='/home/fixture/document.txt',parent=True,token='private-token',browse=True)],state();checks.append(dict(mode=mode,small=small,case='captured-file-show-folder',painted=receipt,state=state()));keyboard.key('-k','Escape');wait(lambda:not state()['visible'])
   ipc('resetEntries');ipc('configure',mode,'1');ipc('open','switcher');wait(lambda:state()['family']=='switcher'and state()['visible']and state()['windowActive']and bool(state()['focus']));wait(lambda:state()['captionSize']and all(size==12 for size in state()['captionSize']));default_paint=presented(mode+'-'+str(small)+'-switcher-default');baseline=state();assert baseline['tileWidth']==104 and all(row['resolvedFamily']==palette['font']for row in baseline['captionMetrics']),baseline;scale_paints=[]
   for scale in [1.15,1.3,1.5]:
    ipc('configure',mode,str(scale));wait(lambda:state()['tileWidth']>=round(104*scale)and state()['captionSize']and all(size==round(12*scale)for size in state()['captionSize']));paint=presented(mode+'-'+str(small)+'-switcher-'+str(scale));enlarged=state();assert len(baseline['captionMetrics'])==len(enlarged['captionMetrics'])==2;assert all(row['resolvedFamily']==palette['font']for row in enlarged['captionMetrics'])and enlarged['iconSizes']and all(row==dict(width=66,height=66)for row in enlarged['iconSizes']),enlarged
    for row in enlarged['captionMetrics']:
     before=next(item for item in baseline['captionMetrics']if item['text']==row['text']);(out/'capacity-last.json').write_text(json.dumps(dict(scale=scale,baseline=baseline,enlarged=enlarged),indent=2));assert before['naturalWidth']>0 and row['naturalWidth']>0 and (row['width']+1)/row['naturalWidth']>=before['width']/before['naturalWidth'],(before,row)
    scale_paints.append(dict(scale=scale,painted=paint,postFrameState=enlarged))
   keyboard.key('-k','Right');wait(lambda:state()['selected']==1);receipt=presented(mode+'-'+str(small)+'-switcher');keyboard.key('-k','Escape');wait(lambda:not state()['visible']);assert state()['actions']==[];ipc('open','switcher');wait(lambda:state()['visible']and state()['windowActive']and bool(state()['focus']));keyboard.key('-k','Right');wait(lambda:state()['selected']==1);keyboard.key('-k','Return');wait(lambda:state()['actions']==['fixture.second']);checks.append(dict(mode=mode,small=small,case='switcher-selection-cancel-commit',defaultTypography=baseline,defaultPaint=default_paint,scalePaints=scale_paints,enlargedTypography=enlarged,painted=receipt,state=state()))
   if small:
    ipc('manySwitcher');wait(lambda:state()['visible']and state()['windowActive']and bool(state()['focus'])and state()['selected']==0)
    for index in range(1,8):keyboard.key('-k','Right');wait(lambda:state()['selected']==index)
    assert state()['tileWidth']>=156 and state()['switcherLabel']=='Application 8',state();receipt=presented(mode+'-many-switcher',selected_only=True);keyboard.key('-k','Return');wait(lambda:state()['actions']==['fixture.7']);checks.append(dict(mode=mode,small=small,case='narrow-horizontal-switcher-selection',painted=receipt,state=state()))
 logs=(out/'shell.log').read_text();assert not any(marker in logs for marker in ('ReferenceError:','TypeError:','Binding loop','Failed to load configuration','Cannot assign')),logs
 (out/'result.json').write_text(json.dumps(dict(passed=True,checks=checks,scope='Owned launcher/switcher enlarged text, bounded keyboard traversal, captured preview intents and painted labels; private stubs only.'),indent=2));print(json.dumps(dict(passed=True,checks=len(checks))))
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
  text=(out/'shell.log').read_text();warnings=[line for line in text.splitlines()if 'WARN'in line];path=out/'result.json'
  if path.exists():data=json.loads(path.read_text());data['runtimeWarnings']=warnings;path.write_text(json.dumps(data,indent=2))
  assert not warnings and not any(marker in text for marker in ('ReferenceError:','TypeError:','Binding loop','Failed to load configuration','Cannot assign')),text
