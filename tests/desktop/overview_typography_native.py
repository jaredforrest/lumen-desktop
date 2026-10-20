"""Actual Overview enlarged text, keyboard reachability and inert captured focus."""
import argparse,hashlib,json,os,re,shutil,signal,subprocess as sp,sys,time
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
 args=[str(runtime),'/source/tests/desktop/overview_typography_native.py','--inside','--output','/artifacts']+sum((['--'+name.replace('_','-'),str(getattr(a,name))] for name in ('launcher','sway','font_config','software_driver','keyboard','pointer_tool')),[])+(['--effects'] if a.effects else [])
 code=1
 try:code=run(repo,a.output,args,env,software_graphics=a.effects)
 finally:frozen.finish(a.output,code)
 raise SystemExit(code)
assert repo==Path('/source') and os.getuid()!=0 and os.environ['HOME']=='/home/fixture' and not Path('/.old-root').exists();assert not any((Path('/dev')/name).exists() for name in ('dri','snd','video0','tty'))
out=a.output;processes=[];env=dict(os.environ,WLR_BACKENDS='headless',WLR_HEADLESS_OUTPUTS='1',WLR_RENDERER='gles2' if a.effects else 'pixman',WLR_RENDERER_FORCE_SOFTWARE='1' if a.effects else '0',WLR_RENDERER_ALLOW_SOFTWARE='1',WLR_LIBINPUT_NO_DEVICES='1',LIBGL_ALWAYS_SOFTWARE='1',MESA_LOADER_DRIVER_OVERRIDE='llvmpipe',LIBGL_DRIVERS_PATH=str(a.software_driver/'lib/dri'),__EGL_VENDOR_LIBRARY_FILENAMES=str(a.software_driver/'share/glvnd/egl_vendor.d/50_mesa.json'),FONTCONFIG_FILE=str(a.font_config),QT_QPA_PLATFORM='wayland',QT_QUICK_BACKEND='software',QT_QPA_PLATFORMTHEME='generic',QT_NO_XDG_DESKTOP_PORTAL='1',QT_ACCESSIBILITY='0',NO_AT_BRIDGE='1',GTK_A11Y='none',LUMEN_UNIFIED_OWNED_HEADERS='1',LUMEN_NATIVE_WINDOW_CONTROLS='0',LUMEN_NATIVE_WINDOW_ACTIONS='0',LUMEN_LAUNCH_SCOPES='0',LUMEN_NEARBY_SHARING_PROVIDER='',GLOBAL_MENU_PYTHON=sys.executable,LUMEN_COMPOSITOR_EFFECTS='1' if a.effects else '0')
for key in ('SWAYSOCK','WAYLAND_DISPLAY','DISPLAY','AT_SPI_BUS_ADDRESS'):env.pop(key,None)
assets=out/'assets';shutil.copytree(repo/'res/lumen-desktop',assets);qml=out/'overlays.qml';qml.write_bytes((repo/'tests/desktop/typography_fixture/overview.qml').read_bytes());changes=[]
def replace_file(relative,text,scope):
 target=assets/relative;original=target.read_bytes();target.write_text(text);changes.append(dict(path=relative,originalSha256=hashlib.sha256(original).hexdigest(),modifiedSha256=hashlib.sha256(text.encode()).hexdigest(),scope=scope))
data=dict(windows=[dict(id=101,pid=11,title='A private document title with several words',app='Private application',group='private',application='private',workspace='1',workspaceLabel='Private desktop'),dict(id=102,pid=12,title='Another private document',app='Private application',group='private',application='private',workspace='1',workspaceLabel='Private desktop')],workspaces=[dict(logicalId='desktop1',name='1',label='Private desktop',outputId='display1',available=True,focused=True)],outputs=[dict(logicalId='display1',name='HEADLESS-1',label='Private display')],revision=1,sessionEpoch='private',autoReorder=True,assignments={},windowGroups=dict(enabled=True,locked=False,separate_displays=True,revision=1,groups=[dict(id=1,members=[dict(id=101,pid=11)],selected=True,suspended=False,workspace='1')]))
actions=out/'actions.jsonl'
backend='import json,sys\nfrom pathlib import Path\nr=json.loads(sys.stdin.readline());assert r["action"]in("status","focus")\nif r["action"]=="focus":\n assert r==dict(action="focus",id=102,pid=12)\n with Path('+repr(str(actions))+').open("a")as f:f.write(json.dumps(r)+"\\n")\nprint('+repr(json.dumps(data))+')\n'
replace_file('shell/window_overview.py',backend,'Synthetic status and exact inert focus recorder only; all other actions refuse')
replace_file('shell/system-controls.py','import json,sys\nassert sys.argv[1:]==["displays"]\nprint(json.dumps(dict(displays=[])))\n','Unavailable private brightness discovery; no hardware commands')
(out/'provider-isolation.json').write_text(json.dumps(changes,indent=2));palette=json.loads((assets/'appearance/palette.json').read_text());appearance=out/'appearance.json';env['LUMEN_APPEARANCE_STATE']=str(appearance);settings=out/'settings.json';settings.write_text(json.dumps(dict(search=dict(historyEnabled=False),clipboard=dict(enabled=False))));env['LUMEN_NIX_SETTINGS_ACTIVE']=str(settings)
def preference(mode,reduced):
 pending=appearance.with_suffix('.pending');pending.write_text(json.dumps(dict(font=palette['font'],palette=palette[mode],preferences=dict(mode=mode,reduceMotion=True,reduceTransparency=reduced),searchPreferences=dict(disabledProviders=['clipboard']))));pending.replace(appearance)
preference('light',True);env['LUMEN_FIXTURE_PALETTES']=json.dumps(palette)
def launch(command,name,**kwargs):
 with (out/name).open('w') as log:proc=sp.Popen(command,env=env,stdout=log,stderr=sp.STDOUT,start_new_session=True,**kwargs)
 processes.append(proc);return proc
def call(*args):return sp.check_output(list(map(str,args)),env=env,text=True,timeout=6).strip()
def ipc(method,*args):return call('quickshell','-p',qml,'ipc','call','overviewText',method,*args)
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
 start_private_bus(env,out,processes);config=out/'sway.conf';config.write_text('output HEADLESS-1 resolution 1200x900\nxwayland disable\nfocus_follows_mouse no\nfor_window [app_id="org.quickshell"] floating enable, border none\n');launch([str(a.sway),'--unsupported-gpu','-c',str(config)],'sway.log');env['SWAYSOCK']=str(wait(lambda:next(Path(env['XDG_RUNTIME_DIR']).glob('sway-ipc*.sock'),None)));env['WAYLAND_DISPLAY']=wait(lambda:next((p.name for p in Path(env['XDG_RUNTIME_DIR']).glob('wayland-*') if not p.name.endswith('.lock')),None));shell=launch(['quickshell','-p',str(qml)],'shell.log');wait(lambda:ipc('ready')=='true');ipc('open','light','false');wait(lambda:state()['width']>0 and state()['active'] and state()['count']==2)
 with (out/'keyboard.log').open('w') as log:keyboard=PhysicalKeyboard(a.keyboard,env,log)
 processes.append(keyboard.process);pointer=launch([str(a.pointer_tool),'pointer','move','0','0'],'pointer.log',stdin=sp.PIPE,text=True);pointer.stdin.write('m 0 0\n');pointer.stdin.flush();wait(lambda:any(seat.get('capabilities',0)&1 for seat in json.loads(call(a.sway.parent/'swaymsg','-r','-t','get_seats'))))
 from PIL import Image
 checks=[]
 def recorded():return [json.loads(line)for line in actions.read_text().splitlines()]if actions.exists()else []
 def focus(name):
  ipc('focus',name);wait(lambda:state()['focus']==name or name=='card'and state()['focus'].startswith('A private document'))
  r=state();assert r['rect'] and r['clipped']['width']>=r['rect']['width']-1 and r['clipped']['height']>=r['rect']['height']-1 and r['rect']['y']>=-1 and r['rect']['y']+r['rect']['height']<=r['height']+1,r
 def fully_visible():
  r=state();return r['rect'] and r['clipped']['width']>=r['rect']['width']-1 and r['clipped']['height']>=r['rect']['height']-1 and r['rect']['y']>=-1 and r['rect']['y']+r['rect']['height']<=r['height']+1
 def painted(name):
  snapshot=state();capture=out/(name+'.png')
  def frame():
   current=state();assert(current['currentId'],current['currentPid'],current['focus'])==(snapshot['currentId'],snapshot['currentPid'],snapshot['focus']);node=next(n for n in nodes(tree())if n.get('pid')==shell.pid and n.get('name')=='Window Overview');text=current['text'];assert text and text['resolvedFamily']==palette['font'];box=text['rect'];origin=node['rect'];call('grim',capture)
   with Image.open(capture)as image:
    crop=image.convert('RGB').crop(tuple(round(v)for v in[origin['x']+box['x'],origin['y']+box['y'],origin['x']+box['x']+box['width'],origin['y']+box['y']+box['height']]));color=tuple(int(text['color'][i:i+2],16)for i in[1,3,5]);ink=sum(max(abs(a-b)for a,b in zip(pixel,color))<=32 for pixel in crop.get_flattened_data());(out/(name+'-paint.json')).write_text(json.dumps(dict(state=current,ink=ink,pixels=crop.width*crop.height),indent=2));return 4<=ink<crop.width*crop.height*.6
  wait(fully_visible);wait(frame);return dict(state=state(),capture=str(capture))
 for small in [False,True]:
  for mode in ['light','dark']:
   ipc('open',mode,str(small).lower());wait(lambda:state()['visible']and state()['active']and state()['count']==2 and state()['width']==(560 if small else 880));focus('Find a window');keyboard.type('private');wait(lambda:state()['query']=='private'and state()['count']==2);keyboard.key('-k','Down');before=len(recorded());keyboard.key('-k','Return');wait(lambda:len(recorded())==before+1 and not state()['visible']);assert recorded()[-1]==dict(action='focus',id=102,pid=12);checks.append(dict(mode=mode,small=small,case='physical-search-selection-captured-focus',action=recorded()[-1]))
   ipc('open',mode,str(small).lower());wait(lambda:state()['active']and state()['visible']);focus('Assign Application');receipt=painted(mode+'-'+str(small)+'-footer');keyboard.key('-k','Tab');wait(lambda:state()['focus']=='Order desktops by recent use');order_receipt=painted(mode+'-'+str(small)+'-recent-order');keyboard.key('-M','shift','-k','Tab','-m','shift');wait(lambda:state()['focus']=='Assign Application');checks.append(dict(mode=mode,small=small,case='footer-tab-and-backtab-revealed',paint=receipt,recentOrderingPaint=order_receipt))
   if small:
    old=state()['viewport']['contentY'];assert old>0;r=state()['viewport']['rect'];node=next(n for n in nodes(tree())if n.get('pid')==shell.pid and n.get('name')=='Window Overview');call(a.sway.parent/'swaymsg','seat seat0 cursor set '+str(round(node['rect']['x']+r['x']+r['width']/2))+' '+str(round(node['rect']['y']+r['y']+r['height']/2)));pointer.stdin.write('m 0 0\nw -1\n');pointer.stdin.flush();wait(lambda:state()['viewport']['contentY']<old);checks.append(dict(mode=mode,small=small,case='real-wheel-scroll',before=old,after=state()))
   focus('Manage window groups');keyboard.key('-k','space');wait(lambda:state()['groupOpen']and state()['focus']=='Window group');
   for name in ['Select window group 1','Add selected window to group 1','Create group from selected window','Close window groups']:
    keyboard.key('-k','Tab');wait(lambda:state()['focus']==name)
   receipt=painted(mode+'-'+str(small)+'-groups');keyboard.key('-k','Escape');wait(lambda:not state()['groupOpen']and state()['visible']and state()['focus']=='Manage window groups');keyboard.key('-k','Escape');wait(lambda:not state()['visible']);checks.append(dict(mode=mode,small=small,case='physical-group-dialog-tab-escape-focus-return',paint=receipt))
 (out/'result.json').write_text(json.dumps(dict(passed=True,checks=checks,scope='Production Overview150% physical search, captured inert focus, focused footer and modal traversal/escape; private synthetic status only'),indent=2));print(json.dumps(dict(passed=True,checks=len(checks))))
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
