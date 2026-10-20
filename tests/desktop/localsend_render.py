"""Render LocalSend pages in a private network namespace and synthetic profile."""
import os,subprocess as sp,tempfile,time,json,signal,argparse,sys
from PIL import Image
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--app',required=True);p.add_argument('--sway',default='sway');p.add_argument('--output',type=Path,required=True);p.add_argument('--inside',action='store_true');p.add_argument('--dark',action='store_true');p.add_argument('--click-pointer',required=True);a=p.parse_args()
if not a.inside:raise SystemExit(sp.call(['unshare','-Urn','dbus-run-session','--',sys.executable,__file__,*sys.argv[1:],'--inside']))
assert set(line.split(':')[0].strip() for line in Path('/proc/net/dev').read_text().splitlines()[2:]) <= {'lo'}, 'Network isolation required'
out=a.output.resolve();out.mkdir(parents=True,exist_ok=False)
env=dict(os.environ,HOME=str(out/'home'),XDG_CONFIG_HOME=str(out/'config'),XDG_DATA_HOME=str(out/'data'),XDG_CACHE_HOME=str(out/'cache'),XDG_STATE_HOME=str(out/'state'),XDG_RUNTIME_DIR=tempfile.mkdtemp(prefix='ls-ui-'),WLR_BACKENDS='headless',WLR_RENDERER='pixman',WLR_LIBINPUT_NO_DEVICES='1',GDK_BACKEND='wayland',LIBGL_ALWAYS_SOFTWARE='1')
for k in ('DISPLAY','WAYLAND_DISPLAY','SWAYSOCK'):env.pop(k,None)
for name in ('home','config','data','cache','state'):(out/name).mkdir()
prefs=out/'data/org.localsend.localsend_app/shared_preferences.json';prefs.parent.mkdir(parents=True);prefs.write_text(json.dumps({'flutter.ls_theme':'dark' if a.dark else 'light','flutter.ls_alias':'Test Computer'}))
env.update(QT_ACCESSIBILITY='0',QT_LINUX_ACCESSIBILITY_ALWAYS_ON='0',NO_AT_BRIDGE='1',GTK_A11Y='none')
env.pop('AT_SPI_BUS_ADDRESS',None)
procs=[]
def launch(args,name):
 with (out/name).open('w') as _fixture_log0:
     p=sp.Popen(args,env=env,stdout=_fixture_log0,stderr=sp.STDOUT,start_new_session=True);procs.append(p);return p
def wait(fn):
 for i in range(150):
  try:
   if fn():return
  except Exception:pass
  time.sleep(.1)
 raise AssertionError('Timed out')
def call(*args):return sp.check_output(args,env=env,text=True)
try:
 conf=out/'sway.conf';conf.write_text('output * resolution 1280x900\nfor_window [all] floating enable, resize set 1050 740\n')
 launch([a.sway,'--unsupported-gpu','-c',str(conf)],'sway.log')
 rt=Path(env['XDG_RUNTIME_DIR']);wait(lambda:list(rt.glob('sway-ipc*.sock')))
 env['SWAYSOCK']=str(next(rt.glob('sway-ipc*.sock')));env['WAYLAND_DISPLAY']=next(p.name for p in rt.glob('wayland-*') if not p.name.endswith('.lock'))
 app=launch([a.app],'app.log')
 def leaves(n):return ([n] if n.get('app_id') else [])+sum((leaves(c) for c in n.get('nodes',[])+n.get('floating_nodes',[])),[])
 def windows():return leaves(json.loads(call('swaymsg','-t','get_tree','-r')))
 wait(lambda:any('localsend' in str(n.get('app_id')).lower() for n in windows()));time.sleep(3)
 call('grim',str(out/'receive.png'))
 n=next(n for n in windows() if 'localsend' in str(n.get('app_id')).lower());r=n['rect'];(out/'window.json').write_text(json.dumps(n,indent=2))
 for name,y in [('send',116),('settings',160)]:
  call('swaymsg',f'seat seat0 cursor set {r["x"]+80} {r["y"]+y}')
  call(a.click_pointer,'pointer','click','left')
  time.sleep(.5);call('grim',str(out/(name+'.png')))
  colour=Image.open(out/(name+'.png')).convert('RGB').getpixel((r['x']+155,r['y']+y));assert colour[2]>180 and colour[0]<60, (name,colour)
 assert app.poll() is None
 (out/'result.json').write_text(json.dumps({'passed':True,'networkNamespace':True,'syntheticProfile':True,'pages':['receive','send','settings'],'theme':'dark' if a.dark else 'light','noTransfers':True},indent=2))
 print(out/'result.json')
finally:
 for p in reversed(procs):
  if p.poll() is None:os.killpg(p.pid,signal.SIGTERM)
