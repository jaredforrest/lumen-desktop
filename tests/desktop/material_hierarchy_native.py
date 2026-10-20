"""Patterned material hierarchy using production consumers in a full private boundary."""
import argparse,hashlib,json,os,re,shutil,signal,subprocess as sp,sys,time
from pathlib import Path
from fixture_resources import close_reaped_pipes
from private_native_bus import start_private_bus
sys.path.insert(0,str(Path(__file__).resolve().parent/'application_fixture'))
import isolation
p=argparse.ArgumentParser();p.add_argument('--launcher',type=Path,required=True);p.add_argument('--sway',type=Path,required=True);p.add_argument('--font-directory',type=Path,required=True);p.add_argument('--software-driver',type=Path,required=True);p.add_argument('--effects',action='store_true');p.add_argument('--inside',action='store_true');p.add_argument('--output',type=Path,required=True);a=p.parse_args();repo=Path(__file__).resolve().parents[2]
for name in ('launcher','sway','font_directory','software_driver'):
 value=getattr(a,name).resolve();assert str(value).startswith('/nix/store/') and value.exists(),name;setattr(a,name,value)
if not a.inside:
 out=a.output.resolve();out.mkdir(mode=0o700,parents=True,exist_ok=False)
 prefix=re.split(r'\n[ \t]*exec quickshell ',a.launcher.read_text(),maxsplit=1)[0]
 environment=dict(value.decode().split('=',1) for value in sp.check_output(['bash','-c',prefix+'\nexec env -0']).split(b'\0') if b'=' in value)
 command=[sys.executable,'/source/tests/desktop/material_hierarchy_native.py','--inside','--launcher',str(a.launcher),'--sway',str(a.sway),'--font-directory',str(a.font_directory),'--software-driver',str(a.software_driver),'--output','/artifacts']+(['--effects'] if a.effects else [])
 assert not sp.check_output(['git','-C',str(repo),'status','--porcelain'],text=True).strip(),'Freeze source before acceptance'
 code=isolation.run(repo,out,command,environment,software_graphics=a.effects)
 (out/'source-provenance.json').write_text(json.dumps(dict(revision=sp.check_output(['git','-C',str(repo),'rev-parse','HEAD'],text=True).strip(),exitCode=code,sources={str(path.relative_to(repo)):hashlib.sha256(path.read_bytes()).hexdigest() for path in [Path(__file__),repo/'tests/desktop/material_fixture/hierarchy.qml',repo/'tests/desktop/application_fixture/isolation.py']},launcher=str(a.launcher),compositor=str(a.sway),fontDirectory=str(a.font_directory),softwareDriver=str(a.software_driver),effects=a.effects),indent=2));raise SystemExit(code)
assert repo==Path('/source') and a.output==Path('/artifacts') and os.environ['HOME']=='/home/fixture' and os.getuid()!=0
out=a.output;assert set(Path('/home').iterdir())=={Path('/home/fixture')} and not Path('/.old-root').exists()
(out/'namespace-proof.json').write_text(json.dumps(dict(namespaces={name:os.readlink('/proc/self/ns/'+name) for name in ('user','pid','mnt','net','ipc','uts')},status=Path('/proc/self/status').read_text(),mounts=Path('/proc/self/mountinfo').read_text()),indent=2))
env=dict(os.environ);env.update(WLR_BACKENDS='headless',WLR_HEADLESS_OUTPUTS='1',WLR_RENDERER='gles2' if a.effects else 'pixman',WLR_RENDERER_FORCE_SOFTWARE='1' if a.effects else '0',WLR_RENDERER_ALLOW_SOFTWARE='1',WLR_LIBINPUT_NO_DEVICES='1',LIBGL_ALWAYS_SOFTWARE='1',QT_QPA_PLATFORM='wayland',QT_QUICK_BACKEND='software',QT_QPA_PLATFORMTHEME='generic',QT_NO_XDG_DESKTOP_PORTAL='1',QT_ACCESSIBILITY='0',NO_AT_BRIDGE='1',GTK_A11Y='none',LUMEN_COMPOSITOR_EFFECTS='1' if a.effects else '0',LUMEN_UNIFIED_OWNED_HEADERS='1',LUMEN_NATIVE_WINDOW_CONTROLS='0',LUMEN_FILE_SERVICE='0',LUMEN_LAUNCH_SCOPES='0',LUMEN_NEARBY_SHARING_LABEL='Nearby Sharing',LUMEN_NEARBY_SHARING_PROVIDER='',GLOBAL_MENU_PYTHON=sys.executable)
for key in ('SWAYSOCK','WAYLAND_DISPLAY','DISPLAY','AT_SPI_BUS_ADDRESS'):env.pop(key,None)
env['MESA_LOADER_DRIVER_OVERRIDE']='llvmpipe'
env['LIBGL_DRIVERS_PATH']=str(a.software_driver/'lib/dri');env['__EGL_VENDOR_LIBRARY_FILENAMES']=str(a.software_driver/'share/glvnd/egl_vendor.d/50_mesa.json')
font=out/'fonts.conf';font.write_text('<fontconfig><dir>'+str(a.font_directory)+'</dir><cachedir>/home/fixture/.cache/fontconfig</cachedir></fontconfig>');env['FONTCONFIG_FILE']=str(font)
assets=out/'assets';shutil.copytree(repo/'res/lumen-desktop',assets);changes=[]
def copied_change(relative,old,new):
 path=assets/relative;original=path.read_text();assert original.count(old)==1,relative;modified=original.replace(old,new);path.write_text(modified);changes.append(dict(path=relative,originalSha256=hashlib.sha256(original.encode()).hexdigest(),modifiedSha256=hashlib.sha256(modified.encode()).hexdigest(),old=old,new=new))
copied_change('shell/files.py',"    if action=='volumes':\n","    if action=='volumes':\n        return dict(volumes=[])\n")
copied_change('shell/BrightnessControl.qml','    id: root','    id: root\n    readonly property alias materialAuditTip:brightnessTip')
copied_change('shell/BrightnessControl.qml','running:root.visible;repeat:true;triggeredOnStart:true;','running:false;repeat:true;triggeredOnStart:true;')
(out/'provider-isolation.json').write_text(json.dumps(changes,indent=2))
folder=Path('/home/fixture/Documents');folder.mkdir();(folder/'Document sample.txt').write_text('Self-authored disposable document.\n')
qml=out/'hierarchy.qml';qml.write_bytes((repo/'tests/desktop/material_fixture/hierarchy.qml').read_bytes());palette=json.loads((assets/'appearance/palette.json').read_text());appearance=out/'appearance.json';env['LUMEN_APPEARANCE_STATE']=str(appearance)
def theme(mode):
 pending=appearance.with_suffix('.pending');pending.write_text(json.dumps(dict(font=palette['font'],palette=palette[mode],preferences=dict(mode=mode,reduceMotion=True,reduceTransparency=False,dockAutoHide=False))));pending.replace(appearance)
theme('light');processes=[];results=[]
def launch(argv,name):
 with (out/name).open('w') as log:proc=sp.Popen(argv,env=env,stdout=log,stderr=sp.STDOUT,start_new_session=True)
 processes.append(proc);return proc
def call(*argv):return sp.check_output(argv,env=env,text=True,timeout=5).strip()
def ipc(method,*args):return call('quickshell','-p',str(qml),'ipc','call','materialHierarchy',method,*args)
def state():return json.loads(ipc('state'))
def nodes(node):
 yield node
 for child in node.get('nodes',[])+node.get('floating_nodes',[]):yield from nodes(child)
def tree():return json.loads(call('swaymsg','-r','-t','get_tree'))
def native(title):return next((row for row in nodes(tree()) if row.get('name')==title and row.get('app_id')=='org.quickshell'),None)
def wait(fn,label):
 deadline=time.monotonic()+15
 while time.monotonic()<deadline:
  try:value=fn()
  except (OSError,ValueError,sp.CalledProcessError):value=None
  if value:return value
  time.sleep(.04)
 try:observed=state()
 except (OSError,ValueError,sp.CalledProcessError) as error:observed=dict(unavailable=str(error))
 (out/'failed-state.json').write_text(json.dumps(dict(state=observed,tree=tree()),indent=2));raise AssertionError(label)
def origin(region,base):
 r=dict(region['rect']);r['x']+=base[0];r['y']+=base[1];return r
def rgb(color):return tuple(round(color[key]*255) for key in ('r','g','b'))
def luminance(value):
 def linear(c):
  c=c/255;return c/12.92 if c<=.04045 else ((c+.055)/1.055)**2.4
 return sum(linear(c)*w for c,w in zip(value,(.2126,.7152,.0722)))
def contrast(a,b):
 x,y=sorted((luminance(a),luminance(b)));return (y+.05)/(x+.05)
try:
 from PIL import Image
 start_private_bus(env,out,processes);config=out/'sway.conf';config.write_text('output HEADLESS-1 resolution 1600x1000\noutput * bg #29445b solid_color\nxwayland disable\nfor_window [all] floating enable, border pixel 0, move position 70 70\n')
 launch([str(a.sway),'-d','--unsupported-gpu','-c',str(config)],'compositor.log');wait(lambda:list(Path(env['XDG_RUNTIME_DIR']).glob('sway-ipc.*.sock')),'private compositor');env['SWAYSOCK']=str(next(Path(env['XDG_RUNTIME_DIR']).glob('sway-ipc.*.sock')));env['WAYLAND_DISPLAY']=next(path.name for path in Path(env['XDG_RUNTIME_DIR']).glob('wayland-*') if not path.name.endswith('.lock'));launch(['quickshell','-p',str(qml)],'shell.log');wait(state,'owned material fixture IPC ready');output_rect=next(row['rect'] for row in json.loads(call('swaymsg','-r','-t','get_outputs')) if row.get('active') and row['name']=='HEADLESS-1')
 for mode in ('light','dark'):
  theme(mode);ipc('open','base');current=wait(lambda:state() if state()['ready'] and state()['dark']==(mode=='dark') and len(state()['regions'])==3 else None,'actual file document material regions');window=wait(lambda:native(current['filesTitle']),'actual owned Files frame');base_origin=(window['rect']['x']+window['window_rect']['x'],window['rect']['y']+window['window_rect']['y'])
  baseline=out/(mode+'-document-baseline.png');call('grim',str(baseline))
  with Image.open(baseline) as loaded:under=loaded.convert('RGB')
  for phase in ('base','popover','menu','notification','tooltip','hud'):
   ipc('open',phase);current=wait(lambda:state() if state()['phase']==phase and (phase=='base' or state()['overlay']) else None,'production '+phase+' maps')
   def painted():
    observed=state()
    capture=out/(mode+'-'+phase+'.png');call('grim',str(capture))
    with Image.open(capture) as loaded:image=loaded.convert('RGB')
    regions=observed['regions'] if phase=='base' else [observed['overlay']];records=[]
    for region in regions:
     if phase=='base':offset=base_origin
     else:
      w=region['window'];offset=(output_rect['x']+(output_rect['width']-w['width'])/2,output_rect['y']+output_rect['height']-w['height']-w['bottom']) if phase=='hud' else (output_rect['x']+output_rect['width']-w['width']-w['right'],output_rect['y']+w['top'])
     box=origin(region,offset);bounds=tuple(round(v) for v in (box['x'],box['y'],box['x']+box['width'],box['y']+box['height']));crop=image.crop(bounds);crop.save(out/(mode+'-'+phase+'-'+region['role']+'-crop.png'))
     points=[];color=rgb(region['color']);alpha=region['color']['a']
     for fy in (.2,.4,.6,.8):
      for fx in (.15,.3,.5,.7,.85):
       x=round(box['x']+box['width']*fx);y=round(box['y']+box['height']*fy)
       underlying=under.getpixel((output_rect['x']+(x-output_rect['x'])%64,output_rect['y']+(y-output_rect['y'])%64)) if phase=='base' and region['role']=='sidebar' else under.getpixel((x,y))
       expected=tuple(round(color[i]*alpha+underlying[i]*(1-alpha)) for i in range(3));actual=image.getpixel((x,y));points.append(dict(x=x,y=y,actual=actual,expected=expected,error=max(abs(a-b) for a,b in zip(actual,expected))))
     matches=[point for point in points if point['error']<=3]
     foreground=rgb(observed['foreground']);ink=sum(max(abs(pixel[i]-foreground[i]) for i in range(3))<=3 for pixel in crop.get_flattened_data());measured=min((contrast(foreground,point['actual']) for point in matches),default=0)
     records.append(dict(role=region['role'],rect=box,matches=len(matches),samples=points,textPixels=ink,minimumContrast=measured))
     (out/'last-paint.json').write_text(json.dumps(records,indent=2))
     if len(matches)<8 or ink<8:return None
     assert measured>=4.5,(mode,phase,region['role'],measured)
    unchanged=[]
    if phase!='base':
     document=origin(next(region for region in observed['regions'] if region['role']=='document'),base_origin);w=observed['overlay']['window']
     host=(offset[0],offset[1],offset[0]+w['width'],offset[1]+w['height'])
     for fy in (.25,.5,.75):
      for fx in (.1,.3,.5,.7,.9):
       x=round(document['x']+document['width']*fx);y=round(document['y']+document['height']*fy)
       if host[0]-16<=x<=host[2]+16 and host[1]-16<=y<=host[3]+16:continue
       before=under.getpixel((x,y))
       if max(abs(a-b) for a,b in zip(before,rgb(observed['content'])))>3:continue
       after=image.getpixel((x,y));error=max(abs(a-b) for a,b in zip(before,after));unchanged.append(dict(x=x,y=y,before=before,after=after,error=error))
     if len(unchanged)<6 or any(row['error']>3 for row in unchanged):
      (out/'last-occlusion.json').write_text(json.dumps(unchanged,indent=2));return None
    (out/'last-paint.json').write_text(json.dumps(records,indent=2));return dict(state=observed,regions=records,unoccludedDocument=unchanged)
   painted_records=wait(painted,'actual painted hierarchy '+mode+' '+phase)
   if phase=='base':
    with Image.open(out/(mode+'-base.png')) as loaded:under=loaded.convert('RGB')
    under.save(baseline)
   results.append(dict(mode=mode,phase=phase,state=current,paint=painted_records))
  assert len({tuple(rgb(region['color'])) for region in current['regions']})>=2,'Document and navigation roles collapsed'
 if a.effects:assert all(text in (out/'compositor.log').read_text() for text in ('Creating scenefx FX renderer','GL renderer: llvmpipe','FX RENDERER: Shaders Initialized Successfully','Trying udmabuf allocator','Created GL FBO for buffer'))
 (out/'result.json').write_text(json.dumps(dict(passed=True,cases=results,isolation='Private user/PID/network/mount/IPC/UTS namespaces, detached host root, private buses and document',notClaimed=['Reference-exact material measurements','All application interiors','Tooltip hover interaction','Replacement of existing seven-surface live transparency/input corpus']),indent=2))
finally:
 for proc in reversed(processes):
  if proc.poll() is None:
   os.killpg(proc.pid,signal.SIGTERM)
   try:proc.wait(timeout=5)
   except sp.TimeoutExpired:os.killpg(proc.pid,signal.SIGKILL);proc.wait(timeout=5)
 close_reaped_pipes(processes)
