"""Physical production settings search/navigation on private synthetic preferences."""
import argparse,hashlib,json,os,re,shutil,subprocess as sp,sys,time
from pathlib import Path
from application_fixture.isolation import run as isolated_run
from private_native_bus import start_private_bus
from fixture_resources import close_reaped_pipes
p=argparse.ArgumentParser();p.add_argument('--launcher',type=Path,required=True);p.add_argument('--sway',required=True);p.add_argument('--pointer-tool',required=True);p.add_argument('--python-runtime',default=shutil.which('desktop-test-python'));p.add_argument('--egl-vendor',type=Path,default=os.environ.get('__EGL_VENDOR_LIBRARY_FILENAMES'));p.add_argument('--effects',action='store_true');p.add_argument('--output',type=Path);p.add_argument('--inside',action='store_true');p.add_argument('--source-revision');p.add_argument('--parent-net');a=p.parse_args()
if not a.inside:
 if not a.output or not a.python_runtime:p.error('Fresh output and pinned runtime required')
 out=a.output.resolve();out.mkdir(mode=0o700,parents=True,exist_ok=False)
 a.launcher=a.launcher.resolve();a.sway=str(Path(a.sway).resolve());a.pointer_tool=str(Path(a.pointer_tool).resolve());a.python_runtime=str(Path(a.python_runtime).resolve())
 for value in (a.launcher,a.sway,a.pointer_tool,a.python_runtime):
  if not str(value).startswith('/nix/store/'):raise ValueError('Pinned immutable tools required')
 revision=sp.check_output(['git','-C',str(Path(__file__).resolve().parents[2]),'rev-parse','HEAD'],text=True).strip()
 args=[a.python_runtime,'/source/tests/desktop/settings_search_input.py','--inside','--launcher',str(a.launcher),'--sway',a.sway,'--pointer-tool',a.pointer_tool,'--source-revision',revision,'--parent-net',os.readlink('/proc/self/ns/net')]
 if a.effects:
  if not a.egl_vendor or not str(a.egl_vendor.resolve()).startswith('/nix/store/'):raise ValueError('Pinned Mesa vendor required')
  args+=['--effects','--egl-vendor',str(a.egl_vendor.resolve())]
 raise SystemExit(isolated_run(Path(__file__).resolve().parents[2],out,args,software_graphics=a.effects))
assert os.getuid()!=0 and os.readlink('/proc/self/ns/net')!=a.parent_net
assert not any((Path('/dev')/n).exists() for n in ('dri','snd','video0')) and Path('/dev/udmabuf').exists()==a.effects
out=Path('/artifacts');runtime=Path(os.environ['XDG_RUNTIME_DIR']);env=dict(os.environ);children=[];checks=[]
prefix=re.split(r'\n[ \t]*exec quickshell ',a.launcher.read_text(),maxsplit=1)[0]
raw=sp.check_output(['bash','-c',prefix+'\nexec env -0'],env=env);env.update(dict(v.decode().split('=',1) for v in raw.split(b'\0') if b'=' in v))
env.update(QT_QPA_PLATFORM='wayland',QT_QPA_PLATFORMTHEME='generic',QT_QUICK_BACKEND='software',QT_ACCESSIBILITY='0',NO_AT_BRIDGE='1',GTK_A11Y='none',GLOBAL_MENU_ENABLED='0',GLOBAL_MENU_PYTHON=sys.executable,LUMEN_NEARBY_SHARING_PROVIDER='',WLR_BACKENDS='headless',WLR_RENDERER='gles2' if a.effects else 'pixman',WLR_RENDERER_ALLOW_SOFTWARE='1',WLR_RENDERER_FORCE_SOFTWARE='1',LIBGL_ALWAYS_SOFTWARE='1',WLR_LIBINPUT_NO_DEVICES='1')
if a.effects:env['__EGL_VENDOR_LIBRARY_FILENAMES']=str(a.egl_vendor)
for name in ('DISPLAY','WAYLAND_DISPLAY','SWAYSOCK','AT_SPI_BUS_ADDRESS'):env.pop(name,None)
def launch(args,name,**kw):
 with (out/name).open('w') as stream:child=sp.Popen(args,env=env,stdout=stream,stderr=sp.STDOUT,**kw)
 children.append(child);return child
def call(*args):return sp.check_output(args,env=env,text=True,stderr=sp.PIPE,timeout=5).strip()
def wait(predicate,label):
 end=time.monotonic()+15
 while time.monotonic()<end:
  try:
   value=predicate()
   if value:return value
  except (ValueError,OSError,sp.SubprocessError):pass
  time.sleep(.03)
 raise AssertionError('Timed out: '+label)
def nodes(node):
 yield node
 for child in node.get('nodes',[])+node.get('floating_nodes',[]):yield from nodes(child)
try:
 start_private_bus(env,out,children)
 assets=out/'assets';shutil.copytree(Path(env['LUMEN_NOTIFICATION_HELPER']).parent.parent,assets)
 for name in ('SettingsWindow.qml','SettingsSearch.js'):
  assert (assets/'shell'/name).read_bytes()==(Path('/source/res/lumen-desktop/shell')/name).read_bytes(),('Installed source mismatch',name)
 template=Path('/source/tests/desktop/fixtures/settings_search_window.qml');qml=out/'shell.qml';shutil.copyfile(template,qml)
 palette=json.loads((assets/'appearance/palette.json').read_text());appearance=runtime/'appearance.json';appearance.write_text(json.dumps(dict(font=palette['font'],palette=palette['dark'],preferences=dict(reduceMotion=True))));env['LUMEN_APPEARANCE_STATE']=str(appearance)
 header_rule=next(line for line in Path('/source/res/lumen-desktop/sway.conf').read_text().splitlines() if line.startswith('for_window [app_id=') and 'System Settings' in line and line.endswith('border pixel 0'))
 config=out/'sway.conf';config.write_text('output HEADLESS-1 resolution 1280x900\nseat seat0 fallback true\nfocus_follows_mouse no\nfor_window [all] floating enable\n'+header_rule+'\n')
 launch([a.sway,'-d','--unsupported-gpu','-c',str(config)],'sway.log');env['SWAYSOCK']=str(wait(lambda:next(runtime.glob('sway-ipc.*.sock'),None),'private compositor'));env['WAYLAND_DISPLAY']=wait(lambda:next((v.name for v in runtime.glob('wayland-*') if not v.name.endswith('.lock')),None),'private display')
 version=json.loads(call('swaymsg','-r','-t','get_version'))
 for variable,capability in [('LUMEN_NATIVE_WINDOW_CONTROLS','native_window_controls'),('LUMEN_NATIVE_WINDOW_ACTIONS','native_window_actions'),('LUMEN_COMPOSITOR_EFFECTS','compositor_effects')]:env[variable]='1' if version.get(capability) is True else '0'
 launch(['wtype','-s','60000'],'keyboard.log')
 pointer=launch([a.pointer_tool,'pointer','move','0','0'],'pointer.log',stdin=sp.PIPE,text=True);pointer.stdin.write('m 0 0\n');pointer.stdin.flush();shell=launch(['quickshell','-p',str(qml)],'shell.log')
 def ipc(method,*args):return call('quickshell','-p',str(qml),'ipc','call','settingsAudit',method,*args)
 def state():
  v=json.loads(ipc('state'));(out/'last-state.json').write_text(json.dumps(v,indent=2));return v
 def owner():
  matches=[n for n in nodes(json.loads(call('swaymsg','-r','-t','get_tree'))) if n.get('pid')==shell.pid and n.get('app_id')=='org.quickshell' and n.get('name')=='System Settings' and n.get('window_rect',{}).get('width',0)>0 and n.get('window_rect',{}).get('height',0)>0]
  return matches[0] if len(matches)==1 else None
 window=wait(owner,'captured settings owner map');con_id=window['id'];call('swaymsg','[con_id='+str(con_id)+'] focus');wait(lambda:owner().get('focused'),'captured owner focused')
 wait(lambda:state()['visible'] and state()['active'],'production settings visible and active');assert state()['zoomChoices']==['Fill','Zoom','Minimize','Do nothing'];checks.append('Installed production settings retains Zoom choice and all existing titlebar alternatives')
 def chord(key,modifier):call('wtype','-M',modifier,'-k',key,'-m',modifier)
 def query(text):
  chord('f','ctrl');wait(lambda:state()['searchGeometry']['focus'],'physical Ctrl+F search focus');chord('a','ctrl');call('wtype',text);wait(lambda:state()['search']==text,'actual typed query acknowledgement')
 query('Title bar double-click action');call('wtype','-k','Return');wait(lambda:state()['control']=='title-bar-double-click-action' and state()['selected'] and state()['selected']['focus'],'exact titlebar result focus');checks.append('Physical Ctrl+F and typed exact visible label select, focus and reveal the exact control')
 selected=state();assert selected['page']=='Desktop & Dock' and selected['selected']['y']>=0 and selected['selected']['y']+selected['selected']['h']<=selected['windowHeight'];call('grim',str(out/'titlebar-result.png'))
 call('wtype','-k','space','-k','Down','-k','Return');wait(lambda:state()['windowCalls']==[{'key':'titlebarDoubleClick','value':'zoom'}],'physical Zoom choice typed canonical preference');checks.append('Physical titlebar combo choice submits only canonical zoom to synthetic preference owner')
 query('Reduce');call('wtype','-k','Down');wait(lambda:state()['searchIndex']==1,'physical next result');assert state()['results'][1]['control']=='reduce-motion';call('wtype','-k','Return');wait(lambda:state()['control']=='reduce-motion' and state()['selected']['focus'],'keyboard selected second result');checks.append('Down and Enter select the captured second control rather than first category match')
 chord('Left','alt');wait(lambda:state()['control']=='title-bar-double-click-action' and state()['selected']['focus'],'physical Back exact control');checks.append('Alt+Left restores exact control and focus across categories')
 chord('w','ctrl');wait(lambda:not state()['visible'],'physical window close');ipc('reopen');wait(lambda:state()['visible'] and state()['control']=='title-bar-double-click-action' and state()['selected']['focus'],'same-window reopen preserved control');con_id=wait(lambda:owner() if owner() and owner().get('focused') else None,'new mapped settings generation')['id'];checks.append('Close/reopen preserves selected control and usable navigation history')
 query('No such setting label');call('wtype','-k','Return');assert state()['control']=='title-bar-double-click-action';checks.append('Unmatched physical search cannot retarget the current page/control')
 query('Dock size')
 def point_result():
  def stable():
   n=owner();v=state();return (n,v) if n and n['id']==con_id and n.get('focused') and n['window_rect']['width']==v['windowWidth'] and n['window_rect']['height']==v['windowHeight'] else None
  n,v=wait(stable,'exact client configure/focus acknowledged');rect=json.loads(ipc('result','dock-size'));assert rect and rect['w']>0 and rect['h']>0
  call('swaymsg','seat','seat0','cursor','set',str(round(n['rect']['x']+n['window_rect']['x']+rect['x']+rect['w']/2)),str(round(n['rect']['y']+n['window_rect']['y']+rect['y']+rect['h']/2)));pointer.stdin.write('m 0.5 0.5\n');pointer.stdin.flush();wait(lambda:json.loads(ipc('result','dock-size'))['hovered'],'exact physical result hover')
 point_result();pointer.stdin.write('p\n');pointer.stdin.flush();query('Reduce motion');pointer.stdin.write('r\n');pointer.stdin.flush();assert state()['control']=='title-bar-double-click-action';checks.append('Held physical result press cannot activate a replacement query result on release')
 # A fresh intended click still works after the canceled press.
 query('Dock size');point_result();pointer.stdin.write('p\nr\n');pointer.stdin.flush();wait(lambda:state()['control']=='dock-size' and state()['selected']['focus'],'fresh exact physical result click');checks.append('Fresh pointer press/release selects the intended indexed control')
 assert state()['writes']==1 and state()['windowCalls']==[{'key':'titlebarDoubleClick','value':'zoom'}],'Search/navigation must never change another preference'
 call('grim',str(out/'final.png'))
 if a.effects:
  log=(out/'sway.log').read_text();assert all(v in log for v in ('Creating scenefx FX renderer','GL renderer: llvmpipe','FX RENDERER: Shaders Initialized Successfully','Trying udmabuf allocator','Created GL FBO for buffer'))
 (out/'result.json').write_text(json.dumps(dict(passed=True,checks=checks,effects=a.effects,sourceRevision=a.source_revision,fixtureSha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),templateSha256=hashlib.sha256(template.read_bytes()).hexdigest(),productionHeaderRule=header_rule,packages=dict(launcher=str(a.launcher),compositor=a.sway,pointer=a.pointer_tool),scope='Actual installed owned settings; physical Wayland keyboard/pointer; synthetic preference service; no host writes/provider rows/reference metrics claim'),indent=2));print('PASS settings search',len(checks))
finally:
 for child in reversed(children):
  if child.poll() is None:
   child.terminate()
   try:child.wait(timeout=5)
   except sp.TimeoutExpired:child.kill();child.wait(timeout=5)
 close_reaped_pipes(children)
