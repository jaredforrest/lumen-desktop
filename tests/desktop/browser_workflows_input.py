"""Optional actual retained-browser workflows inside a disposable application boundary."""
import argparse,base64,hashlib,io,json,os,re,shutil,signal,subprocess as sp,sys,time
from pathlib import Path
from fixture_resources import close_reaped_pipes
sys.path.insert(0,str(Path(__file__).resolve().parent/'application_fixture'))
import isolation
from keyboard import PhysicalKeyboard
from accessibility import start as start_accessibility
sys.path.insert(0,str(Path(__file__).resolve().parent/'browser_fixture'))
from protocol import PipeTransport
from server import FixtureServer
from invocation import command as browser_command,EXCLUDED_SETUP
from capabilities import Inventory
from profile import download_rows,COMPLETE
from paint import match as painted_match
from private_native_bus import start_private_bus
p=argparse.ArgumentParser();p.add_argument('--browser',required=True,type=Path);p.add_argument('--launcher',required=True,type=Path);p.add_argument('--sway',required=True,type=Path);p.add_argument('--pointer-tool',required=True,type=Path);p.add_argument('--keyboard-tool',required=True,type=Path);p.add_argument('--font-directory',required=True,type=Path);p.add_argument('--at-spi-prefix',required=True,type=Path);p.add_argument('--software-driver',required=True,type=Path);p.add_argument('--output',required=True,type=Path);p.add_argument('--effects',action='store_true');p.add_argument('--trace-wayland',action='store_true');p.add_argument('--workflow',choices=('new-save','basic','downloads','permissions','library'),default='basic');p.add_argument('--inside',action='store_true');a=p.parse_args()
repo=Path(__file__).resolve().parents[2]
for name in ('browser','launcher','sway','pointer_tool','keyboard_tool','font_directory','at_spi_prefix','software_driver'):
 value=getattr(a,name).resolve()
 if not str(value).startswith('/nix/store/') or not value.exists():p.error('Provide an existing immutable '+name+' dependency')
 setattr(a,name,value)
if not a.inside:
 output=a.output.resolve();output.mkdir(parents=True,exist_ok=False)
 prefix=re.split(r'\n[ \t]*exec quickshell ',a.launcher.read_text(),maxsplit=1)[0]
 environment=dict(part.decode().split('=',1) for part in sp.check_output(['bash','-c',prefix+'\nexec env -0']).split(b'\0') if b'=' in part)
 for tool in ('dbus-daemon','grim','wtype'):
  binary=shutil.which(tool,path=environment.get('PATH',''))
  if not binary or not str(Path(binary).resolve()).startswith('/nix/store/'):p.error('Run within the pinned nix develop runtime; missing immutable '+tool)
 command=[sys.executable,'/source/tests/desktop/browser_workflows_input.py','--inside','--browser',str(a.browser),'--launcher',str(a.launcher),'--sway',str(a.sway),'--pointer-tool',str(a.pointer_tool),'--keyboard-tool',str(a.keyboard_tool),'--font-directory',str(a.font_directory),'--at-spi-prefix',str(a.at_spi_prefix),'--software-driver',str(a.software_driver),'--output','/artifacts','--workflow',a.workflow]
 if a.effects:command.append('--effects')
 if a.trace_wayland:command.append('--trace-wayland')
 revision=sp.check_output(['git','-C',str(repo),'rev-parse','HEAD'],text=True).strip();dirty=sp.check_output(['git','-C',str(repo),'status','--porcelain'],text=True).strip()
 if dirty:raise ValueError('Freeze application fixture source before acceptance')
 sources={str(path.relative_to(repo)):hashlib.sha256(path.read_bytes()).hexdigest() for folder in ('tests/desktop/browser_fixture','tests/desktop/application_fixture') for path in sorted((repo/folder).glob('*')) if path.is_file()}
 sources[str(Path(__file__).relative_to(repo))]=hashlib.sha256(Path(__file__).read_bytes()).hexdigest()
 result=isolation.run(repo,output,command,environment,software_graphics=a.effects)
 (output/'source-provenance.json').write_text(json.dumps(dict(revision=revision,sources=sources,dependencies={name:str(getattr(a,name)) for name in ('browser','launcher','sway','pointer_tool','keyboard_tool','font_directory','at_spi_prefix','software_driver')},workflow=a.workflow,effects=a.effects,exitCode=result),indent=2))
 raise SystemExit(result)
if repo!=Path('/source') or a.output!=Path('/artifacts') or os.environ.get('HOME')!='/home/fixture' or os.environ.get('XDG_RUNTIME_DIR')!='/run/fixture' or os.getuid()==0 or Path('/proc/1/status').stat().st_uid!=os.getuid():raise ValueError('Private application boundary required before internal entry')
status=Path('/proc/self/status').read_text()
if not all(value in status for value in ('CapEff:\t0000000000000000','CapBnd:\t0000000000000000','NoNewPrivs:\t1')):raise ValueError('Private application capabilities were not dropped')
out=a.output;env=dict(os.environ);runtime=Path(env['XDG_RUNTIME_DIR']);processes=[];browser=None;pointer=None;keyboard=None;accessibility=None
# Public free font fixture; no host font inventory or consumer private assets.
fontconfig=runtime/'fonts.conf';fontconfig.write_text('<?xml version="1.0"?><!DOCTYPE fontconfig SYSTEM "urn:fontconfig:fonts.dtd"><fontconfig><dir>'+str(a.font_directory)+'</dir><cachedir>'+env['XDG_CACHE_HOME']+'/fontconfig</cachedir></fontconfig>');env['FONTCONFIG_FILE']=str(fontconfig)
egl=a.software_driver/'share/glvnd/egl_vendor.d/50_mesa.json';assert egl.is_file();env.update(__EGL_VENDOR_LIBRARY_FILENAMES=str(egl),LIBGL_DRIVERS_PATH=str(a.software_driver/'lib/dri'),MESA_LOADER_DRIVER_OVERRIDE='llvmpipe')
env.update(GSETTINGS_BACKEND='memory',WLR_BACKENDS='headless',WLR_RENDERER='gles2' if a.effects else 'pixman',WLR_RENDERER_FORCE_SOFTWARE='1' if a.effects else '0',WLR_RENDERER_ALLOW_SOFTWARE='1',LIBGL_ALWAYS_SOFTWARE='1',WLR_LIBINPUT_NO_DEVICES='1',QT_QPA_PLATFORM='wayland',QT_QPA_PLATFORMTHEME='generic',LANG='en_GB.UTF-8')
for key in ('DISPLAY','WAYLAND_DISPLAY','SWAYSOCK'):env.pop(key,None)

def launch(command,name):
 with (out/name).open('w') as log:process=sp.Popen(list(map(str,command)),env=env,stdout=log,stderr=sp.STDOUT,start_new_session=True)
 processes.append(process);return process

def wait(predicate,label,timeout=15):
 deadline=time.monotonic()+timeout
 while time.monotonic()<deadline:
  try:
   value=predicate()
   if value:return value
  except (OSError,ValueError,sp.SubprocessError,StopIteration):pass
  time.sleep(.05)
 try:(out/'failed-tree.json').write_text(json.dumps(tree(),indent=2));call('grim',out/'failed.png')
 except Exception:pass
 raise AssertionError('Browser acceptance deadline: '+label)

def call(*command):return sp.check_output(list(map(str,command)),env=env,text=True,timeout=5).strip()
def nodes(node):
 yield node
 for child in node.get('nodes',[])+node.get('floating_nodes',[]):yield from nodes(child)
def tree():return json.loads(call(a.sway.parent/'swaymsg','-r','-t','get_tree'))
def windows():return [node for node in nodes(tree()) if node.get('pid') and (node.get('app_id') or node.get('window'))]
checks=[]
try:
 start_private_bus(env,out,processes)
 if a.workflow in ('permissions','library'):accessibility=start_accessibility(a.at_spi_prefix,env,launch,wait)
 config=runtime/'sway.conf';config.write_text('include /source/res/lumen-desktop/sway.conf\noutput HEADLESS-1 resolution 1400x1000\nxwayland disable\nseat seat0 fallback true\nfocus_follows_mouse no\n')
 launch([a.sway,'--unsupported-gpu','-d','-c',config],'sway.log');env['SWAYSOCK']=str(wait(lambda:next(runtime.glob('sway-ipc*.sock'),None),'compositor IPC'));env['WAYLAND_DISPLAY']=wait(lambda:next((path.name for path in runtime.glob('wayland-*') if not path.name.endswith('.lock')),None),'private display')
 profile=Path(env['HOME'])/'browser-profile';downloads=Path(env['HOME'])/'Downloads';downloads.mkdir()
 configdir=Path(env['XDG_CONFIG_HOME']);(configdir/'user-dirs.dirs').write_text('XDG_DOWNLOAD_DIR="'+str(downloads)+'"\n')
 with FixtureServer() as server:
  command=browser_command(a.browser,profile,server.origin+'/page',accessibility=a.workflow in ('permissions','library'))
  with (out/'browser.log').open('w') as log:browser=PipeTransport(command,dict(env,WAYLAND_DEBUG='client') if a.trace_wayland else env,log)
  version=browser.request('Browser.getVersion',timeout=10);inventory=Inventory(version);target=wait(lambda:next((value for value in browser.request('Target.getTargets')['targetInfos'] if value.get('url')==server.origin+'/page'),None),'synthetic page target');session=browser.request('Target.attachToTarget',dict(targetId=target['targetId'],flatten=True))['sessionId']
  def read_session(expression,session_id):return browser.request('Runtime.evaluate',dict(expression=expression,returnByValue=True),session=session_id)['result'].get('value')
  def evaluate(expression):return read_session(expression,session)
  assert wait(lambda:evaluate('document.title')=='Fixture browser workflow','painted document title')
  main=wait(lambda:next((node for node in windows() if 'Fixture browser workflow' in (node.get('name') or '')),None),'actual browser native window')
  (out/'main-tree.json').write_text(json.dumps(tree(),indent=2))
  assert main.get('lumen_relationships',{}).get('parent') is None,main
  assert main.get('border')=='normal' and main.get('deco_rect',{}).get('height',0)>0,main
  preferences=json.loads((profile/'Default/Preferences').read_text());assert preferences['browser']['custom_chrome_frame'] is False,'Explicit private profile did not receive production native frame preference'
  checks.append('Actual pinned retained browser opened private synthetic page with production native-frame preference')
  with (out/'keyboard.log').open('w') as log:keyboard=PhysicalKeyboard(a.keyboard_tool,env,log)
  processes.append(keyboard.process)
  pointer=sp.Popen([str(a.pointer_tool),'pointer','move','0','0'],env=env,stdin=sp.PIPE,text=True,start_new_session=True);processes.append(pointer);time.sleep(.2)
  def key(*keys):keyboard.key(*keys)
  def click_accessible(row):
   bounds=accessibility.bounds(row);native=current_main();origin=(native['rect']['x']+native['window_rect']['x'],native['rect']['y']+native['window_rect']['y']);bounds=(bounds[0]+origin[0],bounds[1]+origin[1],bounds[2],bounds[3]);(out/'accessible-pointer-target.json').write_text(json.dumps(dict(control=row,bounds=bounds),indent=2));positioned=json.loads(call(a.sway.parent/'swaymsg','seat','seat0','cursor','set',str(bounds[0]+bounds[2]//2),str(bounds[1]+bounds[3]//2)));assert all(value['success'] for value in positioned),positioned
   for operation in ('m 0.5 0.5','p','r'):pointer.stdin.write(operation+'\n');pointer.stdin.flush();time.sleep(.12)
  def capture_presented(expression,name,session_id=None,window=None):
   from PIL import Image
   def ready():
    info=read_session('(()=>{const element='+expression+';if(!element)return null;let r=element.getBoundingClientRect();if(!element.childElementCount){const range=document.createRange();range.selectNodeContents(element);r=range.getBoundingClientRect();}return {x:r.x,y:r.y,width:r.width,height:r.height,text:element.innerText,innerWidth,innerHeight,dpr:devicePixelRatio};})()',session_id or session)
    if not info or info['width']<=0 or info['height']<=0:return False
    native=current_main() if window is None else next(row for row in windows() if row['id']==window['id'] and row['pid']==window['pid']);client=native['window_rect'];scale=client['width']/info['innerWidth']
    if abs(scale-info['dpr'])>=.000001:
     (out/(Path(name).stem+'-paint.json')).write_text(json.dumps(dict(painted=False,reason='Native configure and application viewport have not committed together',content=info,native=native),indent=2));return False
    viewport=Image.open(io.BytesIO(base64.b64decode(browser.request('Page.captureScreenshot',dict(format='png',fromSurface=True,captureBeyondViewport=False),session=session_id or session,timeout=3)['data']))).convert('RGB')
    if viewport.size!=(round(info['innerWidth']*info['dpr']),round(info['innerHeight']*info['dpr'])):return False
    left,top=round(info['x']*info['dpr']),round(info['y']*info['dpr']);reference=viewport.crop((left,top,left+round(info['width']*info['dpr']),top+round(info['height']*info['dpr'])));reference.save(out/(Path(name).stem+'-reference.png'))
    call('grim',out/name)
    with Image.open(out/name) as screen:result=painted_match(reference,screen,(native['rect']['x']+client['x']+info['x']*scale,native['rect']['y']+client['y']+client['height']-info['innerHeight']*scale+info['y']*scale))
    (out/(Path(name).stem+'-paint.json')).write_text(json.dumps(dict(result=result,content=info,native=native),indent=2));return result['painted']
   wait(ready,'actual presented application content: '+name)
  def current_main():return next(node for node in windows() if node['id']==main['id'] and node['pid']==main['pid'])
  def focus_main():
   current_main();result=json.loads(call(a.sway.parent/'swaymsg','-r','[con_id='+str(main['id'])+'] focus'));assert all(row['success'] for row in result)
  def navigate(url):
   focus_main();key('-M','ctrl','-k','l','-m','ctrl');key(url);key('-k','Return')
  def click_page(selector):return click_expression('document.querySelector('+json.dumps(selector)+')')
  def click_expression(expression,read=None,window=None):
   geometry=(read or evaluate)('(()=>{const element='+expression+';if(!element)return null;const r=element.getBoundingClientRect();return {x:r.x+r.width/2,y:r.y+r.height/2,width:r.width,height:r.height,innerWidth,innerHeight};})()');assert geometry and geometry['width']>0 and geometry['height']>0,expression
   native=current_main() if window is None else next(row for row in windows() if row['id']==window['id'] and row['pid']==window['pid']);client=native['window_rect'];rect=native['rect'];scale=client['width']/geometry['innerWidth'];x=rect['x']+client['x']+geometry['x']*scale;y=rect['y']+client['y']+client['height']-geometry['innerHeight']*scale+geometry['y']*scale
   call(a.sway.parent/'swaymsg','seat','seat0','cursor','set',str(round(x)),str(round(y)))
   for operation in ('m 0.5 0.5','p','r'):pointer.stdin.write(operation+'\n');pointer.stdin.flush();time.sleep(.12)
  outputs=json.loads(call(a.sway.parent/'swaymsg','-r','-t','get_outputs'));assert len(outputs)==1 and outputs[0]['scale']==1; (out/'outputs.json').write_text(json.dumps(outputs,indent=2));capture_presented("document.querySelector('#sample')",'main.png')
  navigate('brave://sandbox');sandbox=wait(lambda:evaluate('document.body.innerText') if 'sandbox' in str(evaluate('location.href')) else None,'actual browser sandbox status')
  (out/'sandbox-status.txt').write_text(sandbox);call('grim',out/'sandbox.png')
  assert re.search(r'Seccomp-BPF sandbox\s+Yes',sandbox,re.I),sandbox
  assert re.search(r'(?:Namespace sandbox|PID namespaces|User namespaces)\s+Yes',sandbox,re.I),sandbox
  checks.append('Actual browser status reports namespace and seccomp sandbox enabled without disabling sandbox flags')
  navigate(server.origin+'/page');wait(lambda:evaluate('document.title')=='Fixture browser workflow','return to synthetic page');click_page('#sample');assert evaluate('document.hasFocus()')
  existing={node['id'] for node in windows()};key('-M','ctrl','-k','s','-m','ctrl')
  chooser=wait(lambda:next((node for node in windows() if node['id'] not in existing),None),'actual retained browser Save As chooser')
  (out/'save-chooser-tree.json').write_text(json.dumps(tree(),indent=2));call('grim',out/'save-chooser.png')
  assert chooser.get('lumen_relationships',{}).get('parent')==main['id'],chooser
  assert chooser['id']!=main['id'];key('-k','Escape');wait(lambda:all(node['id']!=chooser['id'] for node in windows()),'Save As cancellation');assert not list(downloads.iterdir()) and not list(Path(env['HOME']).glob('*.html'));assert wait(lambda:evaluate('document.hasFocus()'),'page focus after Save cancellation')
  checks.append('Actual browser Save As uses a separate correctly parented chooser; Escape commits no file and returns page focus')
  def finish_result():
   if a.effects:
    required=('Creating scenefx FX renderer','Using OpenGL ES','GL renderer: llvmpipe','FX RENDERER: Shaders Initialized Successfully','Trying udmabuf allocator','Created GL FBO for buffer');assert all(value in (out/'sway.log').read_text() for value in required),'Actual software effects renderer proof missing'
    (out/'software-effects-proof.json').write_text(json.dumps(list(required),indent=2))
   actions={'APP-BROWSE-02':('edit_url','select_url','navigate')}
   by_workflow={
    'basic':{'APP-BROWSE-01':('new_tab','close_tab','reopen_tab'),'APP-BROWSE-08':('zoom_in','zoom_reset'),'APP-BROWSE-09':('browser_fullscreen','page_fullscreen','escape_fullscreen','resume_page_input')},
    'downloads':{'APP-BROWSE-04':('running_download','completed_download_hash','pause_download','resume_download','cancel_download','downloads_manager')},
    'permissions':{'APP-BROWSE-05':('notifications_allow','notifications_deny','notifications_revoke')},
    'library':{'APP-BROWSE-03':('bookmark_add','bookmark_edit','bookmark_delete','private_history_isolation'),'APP-BROWSE-06':('private_window','cookie_isolation','history_isolation')},
    'new-save':{},
   }
   actions.update(by_workflow[a.workflow])
   for capability,names in actions.items():
    for name in names:inventory.record(capability,name,'verified_compatible',dict(workflow=a.workflow,checks=checks))
   (out/'result.json').write_text(json.dumps(dict(passed=True,workflow=a.workflow,checks=checks,version=version,browser=str(a.browser),command=command,main=main,profile=str(profile),capabilities=inventory.snapshot(),excludedSetup=EXCLUDED_SETUP,fontFixture=str(a.font_directory),isolation='Private non-root filesystem/PID/network/IPC/UTS namespaces; loopback only; explicit private buses and profile',notClaimed=['Whole multi-action requirement completion','Reference-exact appearance','Consumer private assets','Overwrite/read-only/disconnected-mount workflows','Physical devices, real accounts or external networks']),indent=2))
  chooser_mapped={}
  def open_save():
   click_page('#sample');existing={node['id'] for node in windows()};key('-M','ctrl','-k','s','-m','ctrl')
   value=wait(lambda:next((node for node in windows() if node['id'] not in existing),None),'real Save As chooser')
   assert value.get('lumen_relationships',{}).get('parent')==main['id'];assert value.get('border')=='csd' and value.get('deco_rect',{}).get('height')==0
   chooser_mapped[value['id']]=time.monotonic();return value
  def choose_path(path,chooser):
   chooser_mapped.setdefault(chooser['id'],time.monotonic());key('-M','ctrl','-k','l','-m','ctrl');key('-M','ctrl','-k','a','-m','ctrl');assert path.parent==downloads;key(path.name)
   # Chromium ignores acceptance inside its documented 500ms display guard.
   # Observe that readiness boundary; do not bypass it or retry a mutation.
   wait(lambda:time.monotonic()-chooser_mapped[chooser['id']]>=.5,'provider minimum chooser display duration')
   # The retained provider does not export this genuine chooser through AT-SPI.
   # Locate its painted header buttons within the captured native window, then
   # press the rightmost wide button. Retain exact pixels and measured bounds.
   from PIL import Image
   picture=out/'chooser-accept.png';call('grim',picture);image=Image.open(picture).convert('RGB');rect=chooser['rect'];seen=set();components=[]
   x0,y0=rect['x'],rect['y'];x1=x0+rect['width'];y1=min(image.height,y0+100)
   def eligible(x,y):
    r,g,b=image.getpixel((x,y));return min(r,g,b)>225 and max(r,g,b)-min(r,g,b)<8
   for y in range(y0,y1):
    for x in range(x0,x1):
     if (x,y) in seen or not eligible(x,y):continue
     queue=[(x,y)];seen.add((x,y));points=[]
     while queue:
      point=queue.pop();points.append(point)
      for nx,ny in ((point[0]-1,point[1]),(point[0]+1,point[1]),(point[0],point[1]-1),(point[0],point[1]+1)):
       if x0<=nx<x1 and y0<=ny<y1 and (nx,ny) not in seen and eligible(nx,ny):seen.add((nx,ny));queue.append((nx,ny))
     left=min(v[0] for v in points);right=max(v[0] for v in points);top=min(v[1] for v in points);bottom=max(v[1] for v in points)
     if 50<=right-left<=150 and 20<=bottom-top<=60 and top-y0<20:components.append((left,top,right,bottom))
   assert len(components)==2,components
   bounds=max(components,key=lambda item:item[0]);(out/'chooser-accept-bounds.json').write_text(json.dumps(dict(native=chooser,buttons=components,target=bounds)))
   positioned=json.loads(call(a.sway.parent/'swaymsg','seat','seat0','cursor','set',str((bounds[0]+bounds[2])//2),str((bounds[1]+bounds[3])//2)));assert all(value['success'] for value in positioned),positioned
   for operation in ('m 0.5 0.5','p','r'):pointer.stdin.write(operation+'\n');pointer.stdin.flush();time.sleep(.12)
   call('grim','-c',out/'chooser-click-cursor.png')
  def ui_downloads():return evaluate("document.querySelector('downloads-manager')?.items_?.map(row=>({id:row.id,filePath:row.filePath,fileName:row.fileName,state:row.state,percent:row.percent,total:String(row.total)}))") or []
  if a.workflow=='library':
   from types import SimpleNamespace
   from library import run
   run(SimpleNamespace(click_page=click_page,click_expression=click_expression,capture_presented=capture_presented,navigate=navigate,wait=wait,evaluate=evaluate,accessibility=accessibility,main=main,key=key,server=server,profile=profile,checks=checks,capture=lambda name:call('grim',out/name),out=out,windows=windows,targets=lambda:browser.request('Target.getTargets')['targetInfos'],original_target=target,click_accessible=click_accessible,browser_exited=lambda:browser.process.poll() is not None,attach=lambda identity:browser.request('Target.attachToTarget',dict(targetId=identity,flatten=True))['sessionId'],read_session=read_session))
   finish_result();raise SystemExit(0)
  if a.workflow=='permissions':
   from types import SimpleNamespace
   from permissions import run
   run(SimpleNamespace(click_page=click_page,wait=wait,evaluate=evaluate,accessibility=accessibility,main=main,key=key,server=server,checks=checks,capture=lambda name:call('grim',out/name),out=out))
   finish_result();raise SystemExit(0)
  if a.workflow=='downloads':
   from types import SimpleNamespace
   from downloads import run
   run(SimpleNamespace(click_page=click_page,navigate=navigate,wait=wait,evaluate=evaluate,capture_presented=capture_presented,ui_downloads=ui_downloads,downloads=downloads,server=server,checks=checks,click_expression=click_expression,windows=windows,main=main,choose_path=choose_path,capture=lambda name:call('grim',out/name),out=out))
   finish_result();raise SystemExit(0)
  if a.workflow=='basic':
   initial={row['targetId'] for row in browser.request('Target.getTargets')['targetInfos'] if row['type']=='page'}
   key('-M','ctrl','-k','t','-m','ctrl');new=wait(lambda:next((row for row in browser.request('Target.getTargets')['targetInfos'] if row['type']=='page' and row['targetId'] not in initial),None),'actual new tab')
   key(server.origin+'/page');key('-k','Return');wait(lambda:any(row['targetId']==new['targetId'] and row['url']==server.origin+'/page' for row in browser.request('Target.getTargets')['targetInfos']),'native new-tab navigation')
   key('-M','ctrl','-k','w','-m','ctrl');wait(lambda:all(row['targetId']!=new['targetId'] for row in browser.request('Target.getTargets')['targetInfos']),'close captured tab')
   assert any(row['targetId']==target['targetId'] for row in browser.request('Target.getTargets')['targetInfos'])
   key('-M','ctrl','-M','shift','-k','t','-m','shift','-m','ctrl');reopened=wait(lambda:next((row for row in browser.request('Target.getTargets')['targetInfos'] if row['type']=='page' and row['targetId'] not in initial and row['targetId']!=new['targetId'] and row['url']==server.origin+'/page'),None),'reopen closed tab')
   key('-M','ctrl','-k','w','-m','ctrl');wait(lambda:all(row['targetId']!=reopened['targetId'] for row in browser.request('Target.getTargets')['targetInfos']),'close reopened tab')
   assert wait(lambda:evaluate('document.hasFocus()'),'original page input after tab lifecycle');checks.append('Native new, close and reopen affect captured tab identities and preserve original tab/input')
   ratio=evaluate('devicePixelRatio');key('-M','ctrl','-k','=','-m','ctrl');larger=wait(lambda:evaluate('devicePixelRatio') if evaluate('devicePixelRatio')>ratio else None,'native page zoom increase');key('-M','ctrl','-k','0','-m','ctrl');wait(lambda:evaluate('devicePixelRatio')==ratio,'native zoom reset');checks.append('Native zoom increases actual page pixel ratio and reset restores it')
   key('-k','F11');wait(lambda:current_main()['fullscreen_mode']==1,'browser fullscreen');capture_presented("document.querySelector('#sample')",'browser-fullscreen.png');key('-k','F11');wait(lambda:current_main()['fullscreen_mode']==0,'browser fullscreen return');assert wait(lambda:evaluate('document.hasFocus()'),'input restored after browser fullscreen');checks.append('Native browser fullscreen enters/exits actual compositor state and restores page input')
   click_page('#fullscreen');wait(lambda:evaluate('!!document.fullscreenElement'),'page fullscreen request');wait(lambda:current_main()['fullscreen_mode']==1,'page fullscreen native state');capture_presented("document.querySelector('#sample')",'page-fullscreen.png');key('-k','Escape');wait(lambda:not evaluate('!!document.fullscreenElement') and current_main()['fullscreen_mode']==0,'page fullscreen Escape');assert evaluate('document.hasFocus()');checks.append('Actual page gesture fullscreen and native Escape reconcile page/native state and input')
   finish_result();raise SystemExit(0)
  fresh=downloads/'new-saved-page.html';assert not fresh.exists();chooser=open_save();choose_path(fresh,chooser)
  wait(lambda:all(node['id']!=chooser['id'] for node in windows()),'new-file Save provider completion')
  (out/'new-save-observed.json').write_text(json.dumps(dict(rows=download_rows(profile),files=[str(path) for path in downloads.iterdir()]),indent=2))
  if fresh.exists():shutil.copyfile(fresh,out/'new-saved-page.html')
  navigate('brave://downloads');wait(lambda:evaluate("!!document.querySelector('downloads-manager')"),'actual downloads provider UI')
  committed_new=wait(lambda:next((row for row in ui_downloads() if row['filePath']==str(fresh) and row['state']==2),None),'new saved-page provider UI complete state')
  assert b'Disposable local page' in fresh.read_bytes();(out/'new-save-provider.json').write_text(json.dumps(dict(download=committed_new,sha256=hashlib.sha256(fresh.read_bytes()).hexdigest()),indent=2));checks.append('Actual new-file Save commits exact captured target and completed provider record')
  shutil.copyfile(fresh,out/'new-saved-page.html');finish_result()

finally:
 if accessibility is not None:accessibility.close()
 if browser is not None:browser.close()
 if keyboard is not None:keyboard.close()
 for process in reversed(processes):
  if process.poll() is None:os.killpg(process.pid,signal.SIGTERM)
 for process in reversed(processes):
  try:process.wait(timeout=5)
  except sp.TimeoutExpired:os.killpg(process.pid,signal.SIGKILL);process.wait(timeout=5)
 close_reaped_pipes(processes)
