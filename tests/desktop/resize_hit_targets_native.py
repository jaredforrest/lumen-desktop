"""Unmodified pointer resizing on observed native frame edges and corners."""
import argparse,json,os,re,shutil,signal,socket,subprocess as sp,sys,tempfile,time
from pathlib import Path
from application_fixture.isolation import run
from application_fixture.provenance import FrozenSource,generated
from private_native_bus import start_private_bus
from fixture_resources import close_reaped_pipes

p=argparse.ArgumentParser()
for name in ('launcher','sway','pointer','font-config','output'):p.add_argument('--'+name,type=Path,required=True)
p.add_argument('--rapid',action='store_true');p.add_argument('--effects',action='store_true');p.add_argument('--egl-vendor',type=Path,default=os.environ.get('__EGL_VENDOR_LIBRARY_FILENAMES'));p.add_argument('--python-runtime',default=shutil.which('desktop-test-python'));p.add_argument('--inside',action='store_true');p.add_argument('--parent-net');a=p.parse_args();repo=Path(__file__).resolve().parents[2]
if not a.inside:
 a.output=a.output.resolve();a.output.mkdir(mode=0o700,parents=True,exist_ok=False)
 tools=[a.launcher.resolve(),a.sway.resolve(),a.pointer.resolve(),a.font_config.resolve(),Path(a.python_runtime).resolve()]
 if a.effects:tools.append(a.egl_vendor.resolve())
 frozen=FrozenSource(repo,__file__,tools)
 argv=[str(tools[4]),'/source/tests/desktop/resize_hit_targets_native.py','--inside','--launcher',str(tools[0]),'--sway',str(tools[1]),'--pointer',str(tools[2]),'--font-config',str(tools[3]),'--output','/artifacts/result','--parent-net',os.readlink('/proc/self/ns/net')]
 if a.rapid:argv+=['--rapid']
 if a.effects:argv+=['--effects','--egl-vendor',str(tools[5])]
 code=1
 try:code=run(repo,a.output,argv,software_graphics=a.effects)
 finally:frozen.finish(a.output,code)
 raise SystemExit(code)
assert os.getuid()!=0 and os.readlink('/proc/self/ns/net')!=a.parent_net
assert [name for _,name in socket.if_nameindex()]==['lo']
assert not any((Path('/dev')/name).exists()for name in ('dri','snd','video0')) and Path('/dev/udmabuf').exists()==a.effects
out=a.output;out.mkdir(mode=0o700);processes=[];receipts=[];checks=[]
with tempfile.TemporaryDirectory(prefix='resize-hit-targets-')as temporary:
 private=Path(temporary);env=dict(os.environ)
 for key in ('HOME','XDG_RUNTIME_DIR','XDG_STATE_HOME','XDG_CONFIG_HOME','XDG_DATA_HOME','XDG_CACHE_HOME'):
  env[key]=str(private/key);Path(env[key]).mkdir(mode=0o700)
 prefix=re.split(r'\n[ \t]*exec quickshell ',a.launcher.read_text(),maxsplit=1)[0]
 env.update(dict(v.decode().split('=',1)for v in sp.check_output(['bash','-c',prefix+'\nexec env -0'],env=env).split(b'\0')if b'='in v))
 for key in ('DISPLAY','WAYLAND_DISPLAY','SWAYSOCK','DBUS_SESSION_BUS_ADDRESS','AT_SPI_BUS_ADDRESS'):env.pop(key,None)
 env.update(WLR_BACKENDS='headless',WLR_HEADLESS_OUTPUTS='2',WLR_RENDERER='gles2'if a.effects else'pixman',WLR_RENDERER_FORCE_SOFTWARE='1'if a.effects else'0',WLR_RENDERER_ALLOW_SOFTWARE='1',LIBGL_ALWAYS_SOFTWARE='1',WLR_LIBINPUT_NO_DEVICES='1',GDK_BACKEND='wayland',GTK_CSD='0',GTK_A11Y='none',NO_AT_BRIDGE='1',FONTCONFIG_FILE=str(a.font_config))
 if a.effects:env['__EGL_VENDOR_LIBRARY_FILENAMES']=str(a.egl_vendor)
 client=private/'client.py';client.write_text('''import gi,json,os,sys
from pathlib import Path
gi.require_version('Gtk','3.0')
from gi.repository import Gtk,Gio,GLib,Gdk
appid,title,path=sys.argv[1:];path=Path(path);GLib.set_prgname(appid);app=Gtk.Application(application_id=appid,flags=Gio.ApplicationFlags.FLAGS_NONE)
window=None;presses=0;releases=0;motions=0;ticks=0;last_request=None
def publish():
 global ticks,last_request
 request=path.with_suffix('.resize')
 if window and request.exists():
  wanted=request.read_text()
  if wanted!=last_request:
   last_request=wanted;width,height=json.loads(wanted);window.resize(width,height)
 ticks+=1;value=dict(pid=os.getpid(),ticks=ticks,presses=presses,releases=releases,motions=motions,width=window.get_allocated_width()if window else 0,height=window.get_allocated_height()if window else 0)
 temporary=path.with_suffix('.tmp');temporary.write_text(json.dumps(value));temporary.replace(path);return True
def pressed(widget,event):
 global presses
 presses+=1;publish();return True
def released(widget,event):
 global releases
 releases+=1;publish();return True
def motion(widget,event):
 global motions
 motions+=1;publish();return True
def activate(app):
 global window
 window=Gtk.ApplicationWindow(application=app,title=title);window.set_default_size(300,200)
 hints=Gdk.Geometry();hints.min_width=180;hints.min_height=120;window.set_geometry_hints(None,hints,Gdk.WindowHints.MIN_SIZE)
 area=Gtk.DrawingArea();area.add_events(Gdk.EventMask.BUTTON_PRESS_MASK|Gdk.EventMask.BUTTON_RELEASE_MASK|Gdk.EventMask.POINTER_MOTION_MASK);area.connect('button-press-event',pressed);area.connect('button-release-event',released);area.connect('motion-notify-event',motion)
 window.add(area);window.show_all();publish();GLib.timeout_add(30,publish)
app.connect('activate',activate);app.run([])
''')
 def launch(argv,name,**kw):
  with(out/name).open('w')as log:proc=sp.Popen(list(map(str,argv)),env=env,stdout=log,stderr=sp.STDOUT,start_new_session=True,**kw)
  processes.append(proc);return proc
 def call(*argv):return sp.check_output(list(map(str,argv)),env=env,text=True,timeout=6).strip()
 def sway(*argv):return json.loads(call(a.sway.parent/'swaymsg','-r',*argv))
 def checked(command):
  result=sway(command);assert result and all(row.get('success')for row in result),(command,result)
 def nodes(node):
  yield node
  for child in node.get('nodes',[])+node.get('floating_nodes',[]):yield from nodes(child)
 def native(cid):return next((n for n in nodes(sway('-t','get_tree'))if n.get('id')==cid),None)
 def mapped(proc):return next((n for n in nodes(sway('-t','get_tree'))if n.get('pid')==proc.pid and n.get('app_id')and n.get('deco_rect',{}).get('height',0)>0),None)
 def state(path):return json.loads(path.read_text())
 def frame(node):
  r=node['rect'];d=node['deco_rect'];assert node['current_border_width']>0 and d['height']>0
  return dict(x=r['x'],y=r['y']-d['height'],width=r['width'],height=r['height']+d['height'])
 def edges(f):return dict(left=f['x'],right=f['x']+f['width'],top=f['y'],bottom=f['y']+f['height'])
 def wait(fn,label):
  deadline=time.monotonic()+15
  while time.monotonic()<deadline:
   try:value=fn()
   except(OSError,ValueError,sp.SubprocessError):value=None
   if value:return value
   time.sleep(.04)
  (out/'timeout-tree.json').write_text(json.dumps(sway('-t','get_tree'),indent=2));call('grim',out/'timeout.png');raise AssertionError(label)
 def configured(cid,path):
  n=native(cid);s=state(path)
  return n if n and s['width']==n['window_rect']['width']and s['height']==n['window_rect']['height']else None
 def ack(cid,path):return wait(lambda:configured(cid,path),'Client configure acknowledgement')
 def move(cid,x,y,width,height):
  checked('[con_id=%d] resize set width %d px height %d px, move absolute position %d px %d px'%(cid,width,height,x,y))
 def focus(cid):checked('[con_id=%d] focus'%cid);wait(lambda:native(cid).get('focused'),'Exact native focus')
 def send(commands):pointer.stdin.write(commands);pointer.stdin.flush()
 def position(x,y):checked('seat seat0 cursor set %.3f %.3f'%(x,y));send('m 0 0\n')
 def drag(x,y,dx,dy):
  position(x,y)
  if a.rapid:send('p\nm %.3f %.3f\nm %.3f %.3f\nr\n'%(dx/2,dy/2,dx/2,dy/2))
  else:send('p\nm %.3f %.3f\nr\n'%(dx,dy))
 def counts(path):v=state(path);return(v['presses'],v['releases'])
 def sample(node,horizontal,vertical):
  f=frame(node);e=edges(f);half=node['current_border_width']/2
  return(e['left']+half if horizontal<0 else e['right']-half if horizontal>0 else(e['left']+e['right'])/2,e['top']+half if vertical<0 else e['bottom']-half if vertical>0 else(e['top']+e['bottom'])/2)
 def captured(node):return dict(id=node['id'],pid=node['pid'],appId=node['app_id'],rect=node['rect'],deco=node['deco_rect'],window=node['window_rect'])
 try:
  start_private_bus(env,out,processes);config=private/'sway.conf';config.write_text('output HEADLESS-1 resolution 1600x1200 position 0 0 scale 1\noutput HEADLESS-2 resolution 1600x1200 position 1600 0 scale 1.5\nxwayland disable\nseat seat0 fallback true\nfocus_follows_mouse no\ndefault_floating_border normal 2\nfor_window [all] floating enable\n')
  launch([a.sway,'--unsupported-gpu','-c',config],'sway.log');env['SWAYSOCK']=str(wait(lambda:next(Path(env['XDG_RUNTIME_DIR']).glob('sway-ipc*.sock'),None),'Compositor socket'));env['WAYLAND_DISPLAY']=wait(lambda:next((x.name for x in Path(env['XDG_RUNTIME_DIR']).glob('wayland-*')if not x.name.endswith('.lock')),None),'Wayland socket')
  pointer=launch([a.pointer,'pointer','move','0','0'],'pointer.log',stdin=sp.PIPE,text=True);send('m 0 0\n')
  target_path=private/'target.json';neighbor_path=private/'neighbor.json'
  target=launch([env['GLOBAL_MENU_PYTHON'],client,'org.example.ResizeTarget','Resize target',target_path],'target.log');tn=wait(lambda:mapped(target),'Target mapping');tid=tn['id']
  neighbor=launch([env['GLOBAL_MENU_PYTHON'],client,'org.example.ResizeNeighbor','Resize neighbor',neighbor_path],'neighbor.log');nn=wait(lambda:mapped(neighbor),'Neighbor mapping');nid=nn['id']
  directions=[('left',-1,0),('right',1,0),('top',0,-1),('bottom',0,1),('top-left',-1,-1),('top-right',1,-1),('bottom-left',-1,1),('bottom-right',1,1)]
  for output,scale in [('HEADLESS-1',1),('HEADLESS-2',1.5),('HEADLESS-2',2)]:
   checked('output %s scale %s'%(output,scale));actual=wait(lambda:next((v for v in sway('-t','get_outputs')if v['name']==output and v['scale']==scale),None),'Output scale acknowledgement');ox=actual['rect']['x'];oy=actual['rect']['y']
   checked('[con_id=%d] move container to output %s'%(tid,output));checked('[con_id=%d] move container to output %s'%(nid,output))
   for label,h,v in directions:
    move(tid,ox+220,oy+180,300,230);tn=ack(tid,target_path);tf=frame(tn)
    move(nid,ox+10,oy+20,184,154);nn=ack(nid,neighbor_path);nf=frame(nn)
    nx=tf['x']-nf['width']-8 if h<0 else tf['x']+tf['width']+8 if h>0 else tf['x']
    ny=tf['y']-nf['height']-8 if v<0 else tf['y']+tf['height']+8 if v>0 else tf['y']
    move(nid,nx,ny,184,154);nn=ack(nid,neighbor_path);focus(tid);tn=ack(tid,target_path);before=edges(frame(tn));neighbor_before=frame(nn);target_count=counts(target_path);neighbor_count=counts(neighbor_path);x,y=sample(tn,h,v)
    (out/'active-case.json').write_text(json.dumps(dict(output=output,scale=scale,direction=label,point=dict(x=x,y=y),before=captured(tn),neighbor=captured(nn),inputMode='two-motion-rapid-release'if a.rapid else'single-motion'),indent=2)+'\n')
    drag(x,y,h*32,v*24)
    stationary=['right']if h<0 else['left']if h>0 else['left','right'];stationary+=['bottom']if v<0 else['top']if v>0 else['top','bottom']
    def resized():
     node=native(tid);allocation=state(target_path)
     if not node or allocation['width']!=node['window_rect']['width']or allocation['height']!=node['window_rect']['height']:return None
     after=edges(frame(node));horizontal=(after['left']<before['left']if h<0 else after['right']>before['right']if h>0 else True);vertical=(after['top']<before['top']if v<0 else after['bottom']>before['bottom']if v>0 else True)
     return node if horizontal and vertical and all(abs(after[k]-before[k])<=1 for k in stationary)else None
    after_node=wait(resized,'Unmodified native '+label+' resize');after=edges(frame(after_node))
    assert all(abs(after[k]-before[k])<=1 for k in stationary),(label,before,after)
    assert frame(native(nid))==neighbor_before and native(tid)['focused']and counts(target_path)==target_count and counts(neighbor_path)==neighbor_count
    receipts.append(dict(output=output,scale=scale,direction=label,point=dict(x=x,y=y),before=captured(tn),after=captured(after_node),neighborBefore=captured(nn),neighborAfter=captured(native(nid)),clientCounts=dict(target=target_count,neighbor=neighbor_count)))
    checks.append('%s scale%s: exact %s frame resize, stationary opposite edges and no neighbor/client button leak'%(output,scale,label))
   move(tid,ox+220,oy+180,300,230);tn=ack(tid,target_path);tf=frame(tn);move(nid,ox+10,oy+20,184,154);ack(nid,neighbor_path);focus(tid);before=frame(tn);n_before=frame(native(nid));c=counts(target_path);x=tn['rect']['x']+tn['window_rect']['x']+40;y=tn['rect']['y']+40;position(x,y);send('p\nr\n');wait(lambda:counts(target_path)==(c[0]+1,c[1]+1),'Positive target content input');assert frame(native(tid))==before and frame(native(nid))==n_before;checks.append('%s scale%s: content click reaches actual target without resize'%(output,scale))
   # Place the neighbor's content over the target's old right-border point.
   x,y=sample(tn,1,0);move(nid,int(x)-40,int(y)-65,184,154);nn=ack(nid,neighbor_path);focus(nid);nb=frame(nn);tb=frame(native(tid));c=counts(neighbor_path);drag(x,y,16,0);wait(lambda:counts(neighbor_path)==(c[0]+1,c[1]+1),'Topmost neighbor content owns covered border');assert frame(native(tid))==tb and frame(native(nid))==nb and native(nid)['focused'];checks.append('%s scale%s: topmost neighbor owns covered target edge without underlying resize'%(output,scale))
   move(nid,ox+10,oy+20,184,154);ack(nid,neighbor_path);focus(tid);tn=ack(tid,target_path);x,y=sample(tn,1,1);drag(x,y,-200,-180);small=wait(lambda:(n if(n:=configured(tid,target_path))and n['window_rect']['width']<=200 and n['window_rect']['height']<=140 else None),'Client minimum clamp');assert small['window_rect']['width']>=180 and small['window_rect']['height']>=120;checks.append('%s scale%s: corner shrinking preserves actual client minimum constraints'%(output,scale));call('grim',out/('scale-'+str(scale)+'.png'))
  if a.rapid:
   before=frame(ack(tid,target_path));target_path.with_suffix('.resize').write_text(json.dumps([400,260]))
   grown=wait(lambda:(n if(n:=configured(tid,target_path))and n['window_rect']['width']==400 and n['window_rect']['height']==260 else None),'Client-authored resize after completed interactive acknowledgement')
   assert frame(grown)['x']==before['x']and frame(grown)['y']==before['y'];checks.append('Later client-authored resize remains operable after exact interactive configure acknowledgement')
  generated(private,out);(out/'client-source.py').write_text(client.read_text());(out/'receipts.json').write_text(json.dumps(receipts,indent=2)+'\n');(out/'result.json').write_text(json.dumps(dict(passed=True,checks=checks,effects=a.effects,inputMode='two-motion-rapid-release'if a.rapid else'single-motion',scope='Observed native two-logical-pixel border centres at3 actual output scales; not an unmeasured outside-border target-size or reference parity claim'),indent=2)+'\n')
 finally:
  (out/'completed-checks.json').write_text(json.dumps(checks,indent=2)+'\n');(out/'receipts.json').write_text(json.dumps(receipts,indent=2)+'\n')
  for proc in reversed(processes):
   if proc.poll()is None:
    os.killpg(proc.pid,signal.SIGTERM)
    try:proc.wait(timeout=4)
    except sp.TimeoutExpired:os.killpg(proc.pid,signal.SIGKILL);proc.wait(timeout=4)
  close_reaped_pipes(processes);warnings={}
  for log in out.glob('*.log'):
   found=[line for line in log.read_text(errors='replace').splitlines()if re.search(r'\bWARN\b|QtWARN|Warning:|ResourceWarning|Traceback|Ignoring unexpected wl_keyboard',re.sub(r'\x1b\[[0-9;]*m','',line))]
   if found:warnings[log.name]=found
  (out/'warning-scan.json').write_text(json.dumps(warnings,indent=2)+'\n');assert not warnings,warnings
print(json.dumps(dict(passed=True,checks=len(checks))))
