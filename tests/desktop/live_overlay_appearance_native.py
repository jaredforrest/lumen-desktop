"""Live palette/material updates on production overlay families in a private session."""
import argparse,hashlib,json,os,re,shutil,signal,subprocess as sp,sys,time
from collections import Counter
from pathlib import Path
from application_fixture.isolation import run
from application_fixture.keyboard import PhysicalKeyboard
from application_fixture.provenance import FrozenSource,generated
from private_native_bus import start_private_bus
from fixture_resources import close_reaped_pipes
p=argparse.ArgumentParser();p.add_argument('--keyboard',type=Path,required=True);p.add_argument('--launcher',type=Path,required=True);p.add_argument('--sway',type=Path,required=True);p.add_argument('--font-config',type=Path,required=True);p.add_argument('--software-driver',type=Path,required=True);p.add_argument('--output',type=Path,required=True);p.add_argument('--effects',action='store_true');p.add_argument('--inside',action='store_true');a=p.parse_args();repo=Path(__file__).resolve().parents[2]
for name in ('launcher','sway','font_config','software_driver','keyboard'):
 value=getattr(a,name).resolve();assert str(value).startswith('/nix/store/') and value.exists();setattr(a,name,value)
if not a.inside:
 a.output=a.output.resolve();a.output.mkdir(mode=0o700,parents=True,exist_ok=False)
 runtime=Path(sys.executable).resolve();frozen=FrozenSource(repo,__file__,[a.launcher,a.sway,a.font_config,a.keyboard,runtime]);prefix=re.split(r'\n[ \t]*exec quickshell ',a.launcher.read_text(),maxsplit=1)[0];env=dict(v.decode().split('=',1) for v in sp.check_output(['bash','-c',prefix+'\nexec env -0']).split(b'\0') if b'=' in v)
 args=[str(runtime),'/source/tests/desktop/live_overlay_appearance_native.py','--inside','--output','/artifacts']+sum((['--'+name.replace('_','-'),str(getattr(a,name))] for name in ('launcher','sway','font_config','software_driver','keyboard')),[])+(['--effects'] if a.effects else [])
 code=1
 try:code=run(repo,a.output,args,env,software_graphics=a.effects)
 finally:frozen.finish(a.output,code)
 raise SystemExit(code)
assert repo==Path('/source') and os.getuid()!=0 and os.environ['HOME']=='/home/fixture' and not Path('/.old-root').exists();assert not any((Path('/dev')/name).exists() for name in ('dri','snd','video0','tty'))
out=a.output;processes=[];env=dict(os.environ,WLR_BACKENDS='headless',WLR_HEADLESS_OUTPUTS='1',WLR_RENDERER='gles2' if a.effects else 'pixman',WLR_RENDERER_FORCE_SOFTWARE='1' if a.effects else '0',WLR_RENDERER_ALLOW_SOFTWARE='1',WLR_LIBINPUT_NO_DEVICES='1',LIBGL_ALWAYS_SOFTWARE='1',MESA_LOADER_DRIVER_OVERRIDE='llvmpipe',LIBGL_DRIVERS_PATH=str(a.software_driver/'lib/dri'),__EGL_VENDOR_LIBRARY_FILENAMES=str(a.software_driver/'share/glvnd/egl_vendor.d/50_mesa.json'),FONTCONFIG_FILE=str(a.font_config),QT_QPA_PLATFORM='wayland',QT_QUICK_BACKEND='software',QT_QPA_PLATFORMTHEME='generic',QT_NO_XDG_DESKTOP_PORTAL='1',QT_ACCESSIBILITY='0',NO_AT_BRIDGE='1',GTK_A11Y='none',LUMEN_UNIFIED_OWNED_HEADERS='1',LUMEN_NATIVE_WINDOW_CONTROLS='0',LUMEN_NATIVE_WINDOW_ACTIONS='0',LUMEN_LAUNCH_SCOPES='0',LUMEN_NEARBY_SHARING_PROVIDER='',GLOBAL_MENU_PYTHON=sys.executable,LUMEN_COMPOSITOR_EFFECTS='1' if a.effects else '0')
for key in ('SWAYSOCK','WAYLAND_DISPLAY','DISPLAY','AT_SPI_BUS_ADDRESS'):env.pop(key,None)
assets=out/'assets';shutil.copytree(repo/'res/lumen-desktop',assets);qml=out/'overlays.qml';qml.write_bytes((repo/'tests/desktop/appearance_fixture/overlays.qml').read_bytes());changes=[]
def replace_file(relative,text,scope):
 target=assets/relative;original=target.read_bytes();target.write_text(text);changes.append(dict(path=relative,originalSha256=hashlib.sha256(original).hexdigest(),modifiedSha256=hashlib.sha256(text.encode()).hexdigest(),scope=scope))
replace_file('shell/window_overview.py','import json,sys\nr=json.loads(sys.stdin.readline());assert r["action"]=="status"\nprint(json.dumps(dict(windows=[dict(id=101,pid=11,title="First document",app="First application",group="fixture",application="fixture",workspace="1")],workspaces=[],outputs=[],revision=1,sessionEpoch="private",autoReorder=True,assignments={})))\n','Fixed synthetic read-only window corpus; all non-status commands refuse')
replace_file('shell/system-controls.py','import json,sys\nassert sys.argv[1:]==["displays"]\nprint(json.dumps(dict(displays=[])))\n','Unavailable private brightness discovery; no hardware commands')
(out/'provider-isolation.json').write_text(json.dumps(changes,indent=2));palette=json.loads((assets/'appearance/palette.json').read_text());appearance=out/'appearance.json';env['LUMEN_APPEARANCE_STATE']=str(appearance);settings=out/'settings.json';settings.write_text(json.dumps(dict(search=dict(historyEnabled=False),clipboard=dict(enabled=False))));env['LUMEN_NIX_SETTINGS_ACTIVE']=str(settings)
def preference(mode,reduced):
 pending=appearance.with_suffix('.pending');pending.write_text(json.dumps(dict(font=palette['font'],palette=palette[mode],preferences=dict(mode=mode,reduceMotion=True,reduceTransparency=reduced),searchPreferences=dict(disabledProviders=['files','clipboard']))));pending.replace(appearance)
preference('light',False)
def launch(command,name):
 with (out/name).open('w') as log:proc=sp.Popen(command,env=env,stdout=log,stderr=sp.STDOUT,start_new_session=True)
 processes.append(proc);return proc
def call(*args):return sp.check_output(list(map(str,args)),env=env,text=True,timeout=6).strip()
def ipc(method,*args):return call('quickshell','-p',qml,'ipc','call','liveOverlay',method,*args)
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
 processes.append(keyboard.process)
 from PIL import Image
 checks=[]
 for family in ('launcher','switcher','overview','controls','notifications'):
  ipc('open',family);wait(lambda:state()['family']==family and state()['visible'] and state()['body']['width']>0);initial=state();identity=None
  if family=='switcher':keyboard.key('-k','Right');wait(lambda:state()['selected']==1)
  else:
   wait(lambda:ipc('focusTarget')=='true')
   if family in ('launcher','overview'):keyboard.type('First');wait(lambda:state()['query']=='First')
  initial=state();text_geometry=[(row['text'],row['rect']) for row in initial['texts']]
  if family=='overview':identity=wait(lambda:next((n['id'] for n in nodes(tree()) if n.get('pid')==shell.pid and n.get('name')=='Window Overview' and n.get('visible')),None));wait(lambda:state()['overview']['count']==1);initial=state();text_geometry=[(row['text'],row['rect']) for row in initial['texts']]
  for step,(mode,reduced) in enumerate((('light',False),('dark',False),('dark',True),('light',True),('light',False))):
   preference(mode,reduced);wait(lambda:state()['dark']==(mode=='dark') and state()['reduced']==reduced);snapshot=state();assert snapshot['visible'] and snapshot['body']==initial['body'] and snapshot['width']==initial['width'] and snapshot['height']==initial['height'],(family,snapshot,initial)
   assert [(row['text'],row['rect']) for row in snapshot['texts']]==text_geometry,(family,'Text geometry changed during appearance update')
   if family=='switcher':assert snapshot['selected']==1
   else:
    assert snapshot['targetFocus'],(family,snapshot['focus'])
    if family in ('launcher','overview'):assert snapshot['query']=='First'
   if identity:assert any(n.get('id')==identity and n.get('visible') for n in nodes(tree()))
   stale={palette['light' if mode=='dark' else 'dark'][key] for key in ('text','muted')};fresh_text={palette[mode][key] for key in ('text','muted')};stale-=fresh_text;assert not any(row['color'] in stale for row in snapshot['texts']),(family,mode,snapshot['texts'])
   if reduced or not a.effects:assert all(row['alpha']>=.999 or row['alpha']==0 for row in snapshot['rectangles']),(family,snapshot['rectangles'])
   # Wait for actual rendered palette bytes, not just singleton property notification.
   material=max(snapshot['rectangles'],key=lambda row:row['width']*row['height']);alpha=material['alpha'];expected=tuple(round(int(palette[mode]['background'][i:i+2],16)*alpha) for i in (1,3,5));old=tuple(round(int(palette['light' if mode=='dark' else 'dark']['background'][i:i+2],16)*alpha) for i in (1,3,5));capture=out/f'{family}-{step}.png'
   def painted():
    call('grim',capture)
    with Image.open(capture) as image:counts=Counter(image.convert('RGB').get_flattened_data())
    fresh=sum(count for color,count in counts.items() if max(abs(color[i]-expected[i]) for i in range(3))<=2);stale=sum(count for color,count in counts.items() if max(abs(color[i]-old[i]) for i in range(3))<=2)
    foreground=tuple(int(palette[mode]['text'][i:i+2],16) for i in (1,3,5));ink=sum(count for color,count in counts.items() if max(abs(color[i]-foreground[i]) for i in range(3))<=2)
    return fresh>500 and stale<500 and ink>20
   wait(painted);checks.append(dict(family=family,mode=mode,reduced=reduced,state=snapshot,capture=str(capture),nativeId=identity))
  assert not state()['actions'],state()
  if family in ('controls','notifications'):
   keyboard.key('-k','Return');wait(lambda:state()['actions']==['settings' if family=='controls' else 'default'])
  else:
   keyboard.key('-k','Escape');wait(lambda:not state()['visible'])
 logs=(out/'shell.log').read_text();assert not any(marker in logs for marker in ('ReferenceError:','TypeError:','Binding loop','Failed to load configuration','Cannot assign')),logs
 (out/'result.json').write_text(json.dumps(dict(passed=True,checks=checks,scope='Five production owned overlay families kept visible across live atomic palette/material updates with captured geometry, visible text colors and rendered palette bytes; synthetic read-only providers, no external actions.'),indent=2));print(json.dumps(dict(passed=True,checks=len(checks))))
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
