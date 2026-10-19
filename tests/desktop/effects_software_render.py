"""Actual Scenefx shader/framebuffer/pixel proof with explicitly software EGL."""
import argparse,json,os,signal,subprocess as sp,tempfile,time
from pathlib import Path
from PIL import Image
from private_native_bus import start_private_bus
p=argparse.ArgumentParser();p.add_argument('--sway',required=True);p.add_argument('--output',type=Path,required=True);p.add_argument('--require-no-drm',action='store_true');a=p.parse_args();a.output.mkdir(parents=True,exist_ok=False)
assert os.access('/dev/udmabuf',os.R_OK|os.W_OK),'Software FX requires a readable/writable /dev/udmabuf allocator; see CONTRIBUTING.md'
if a.require_no_drm:assert not list(Path('/dev/dri').glob('renderD*')),'Render devices must be hidden before starting this fixture'
processes=[]
with tempfile.TemporaryDirectory(prefix='lumen-fx-software-') as private:
 env=dict(os.environ,HOME=private,XDG_RUNTIME_DIR=private,WLR_BACKENDS='headless',WLR_RENDERER='gles2',WLR_RENDERER_FORCE_SOFTWARE='1',WLR_RENDERER_ALLOW_SOFTWARE='1',LIBGL_ALWAYS_SOFTWARE='1',WLR_LIBINPUT_NO_DEVICES='1')
 for key in ('WAYLAND_DISPLAY','DISPLAY','SWAYSOCK','DBUS_SESSION_BUS_ADDRESS'):env.pop(key,None)
 def launch(cmd,name):
  with (a.output/name).open('w') as log:proc=sp.Popen(cmd,env=env,stdout=log,stderr=sp.STDOUT,start_new_session=True)
  processes.append(proc);return proc
 def wait(fn):
  deadline=time.monotonic()+12
  while time.monotonic()<deadline:
   if fn():return
   time.sleep(.04)
  raise AssertionError('Software FX fixture timed out; inspect '+str(a.output))
 def call(*cmd):return sp.check_output(cmd,env=env,text=True,timeout=5)
 def ipc(command):
  rows=json.loads(call(str(Path(a.sway).parent/'swaymsg'),'-r',command));assert all(row['success'] for row in rows),rows
 def nodes(node):
  yield node
  for child in node.get('nodes',[])+node.get('floating_nodes',[]):yield from nodes(child)
 def window():return next((n for n in nodes(json.loads(call(str(Path(a.sway).parent/'swaymsg'),'-r','-t','get_tree'))) if n.get('name')=='FX Pixel Fixture'),None)
 try:
  start_private_bus(env,a.output,processes)
  conf=a.output/'sway.conf';conf.write_text('output HEADLESS-1 resolution 800x600\noutput HEADLESS-1 bg #000000 solid_color\nxwayland disable\ndefault_border none\ncorner_radius 0\nshadows disable\nanimation_duration_ms 0\nfor_window [title="^FX Pixel Fixture$"] floating enable, resize set 400 300, move position 200 150\n')
  launch([a.sway,'--unsupported-gpu','-d','-c',str(conf)],'sway.log');wait(lambda:list(Path(private).glob('sway-ipc*.sock')))
  env['SWAYSOCK']=str(next(Path(private).glob('sway-ipc*.sock')));env['WAYLAND_DISPLAY']=next(f.name for f in Path(private).glob('wayland-*') if not f.name.endswith('.lock'))
  launch(['foot','--title=FX Pixel Fixture','--override=colors-dark.background=ff0000','sh','-c','sleep 600'],'foot.log');wait(window)
  ipc('seat seat0 cursor set 10 10');time.sleep(.3)
  rect=window()['rect'];assert rect['width']>=350 and rect['height']>=250,rect
  call('grim',str(a.output/'square.png'))
  conf.write_text(conf.read_text().replace('corner_radius 0','corner_radius 32'));ipc('reload');time.sleep(.3);call('grim',str(a.output/'rounded.png'))
  x,y=rect['x'],rect['y'];square=Image.open(a.output/'square.png').convert('RGB');rounded=Image.open(a.output/'rounded.png').convert('RGB')
  # The lower corner avoids terminal text/cursor and tests actual corner shader output.
  corner=(x+1,y+rect['height']-2);interior=(x+80,y+rect['height']-30)
  assert square.getpixel(corner)==(255,0,0),(corner,square.getpixel(corner))
  assert rounded.getpixel(corner)==(0,0,0),(corner,rounded.getpixel(corner))
  assert rounded.getpixel(interior)==square.getpixel(interior)==(255,0,0)
  log=(a.output/'sway.log').read_text()
  proofs=('Using software rendering','Creating scenefx FX renderer','FX RENDERER: Shaders Initialized Successfully','Created GL FBO for buffer','Trying udmabuf allocator')
  for proof in proofs:assert proof in log,proof
  assert 'GL renderer: llvmpipe' in log or 'GL renderer: softpipe' in log
  result=dict(passed=True,allocator='udmabuf',noRenderDevices=not list(Path('/dev/dri').glob('renderD*')),checks=list(proofs)+['Actual corner effect changes corner pixels and preserves interior pixels'])
  (a.output/'result.json').write_text(json.dumps(result,indent=2));print(a.output/'result.json')
 finally:
  for proc in reversed(processes):
   if proc.poll() is None:
    os.killpg(proc.pid,signal.SIGTERM)
    try:proc.wait(timeout=5)
    except sp.TimeoutExpired:os.killpg(proc.pid,signal.SIGKILL);proc.wait(timeout=5)
   if proc.stdout is not None:proc.stdout.close()
