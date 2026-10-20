"""Real PipeWire protocol acceptance, only inside the disposable app boundary."""
import argparse,json,os,signal,subprocess,sys,time
from pathlib import Path
sys.path.insert(0,str(Path(__file__).resolve().parents[2]/'res/lumen-desktop/shell'))
import capture_observation as capture
p=argparse.ArgumentParser();p.add_argument('--pipewire',type=Path,required=True);p.add_argument('--parent-net',required=True);a=p.parse_args()
assert os.getuid()!=0 and os.readlink('/proc/self/ns/net')!=a.parent_net
assert not Path('/dev/snd').exists() and not Path('/home/jalio').exists()
out=Path('/artifacts');runtime=Path(os.environ['XDG_RUNTIME_DIR']);env=dict(os.environ,PIPEWIRE_REMOTE='capture-fixture',PIPEWIRE_RUNTIME_DIR=str(runtime))
config=out/'pipewire.conf';config.write_text('''context.properties = { core.daemon = true core.name = capture-fixture support.dbus = false }
context.spa-libs = { audio.convert.* = audioconvert/libspa-audioconvert audio.adapt = audioconvert/libspa-audioconvert support.* = support/libspa-support audiotestsrc = audiotestsrc/libspa-audiotestsrc }
context.modules = [ { name = libpipewire-module-protocol-native } { name = libpipewire-module-metadata } { name = libpipewire-module-spa-node-factory } { name = libpipewire-module-client-node } { name = libpipewire-module-access } { name = libpipewire-module-adapter } { name = libpipewire-module-link-factory } ]
context.objects = [ { factory = spa-node-factory args = { factory.name = support.node.driver node.name = Synthetic-Driver node.group = pipewire.dummy priority.driver = 20000 } } ]
''')
processes=[]
def run(*args):return subprocess.check_output([str(a.pipewire/args[0]),*args[1:]],env=env,text=True,timeout=4).strip()
def wait(fn,description):
 end=time.monotonic()+8
 while time.monotonic()<end:
  value=fn()
  if value:return value
  time.sleep(.05)
 raise AssertionError(description)
def graph():
 old=os.environ.copy();os.environ.update(env)
 try:return capture.read_graph([str(a.pipewire/'pw-dump'),'--no-colors'])
 finally:os.environ.clear();os.environ.update(old)
try:
 with (out/'server.log').open('w') as log:server=subprocess.Popen([str(a.pipewire/'pipewire'),'-c',str(config)],env=env,stdout=log,stderr=subprocess.STDOUT,start_new_session=True)
 processes.append(server);wait(lambda:(runtime/'capture-fixture').is_socket(),'Private PipeWire socket')
 baseline=capture.describe(graph());assert baseline['available'] and not baseline['rows']
 run('pw-cli','create-node','adapter','{ factory.name = audiotestsrc node.name = synthetic-source media.class = Audio/Source audio.position = [ MONO ] adapter.auto-port-config = { mode = dsp monitor = false position = preserve } object.linger = true }')
 run('pw-cli','create-node','adapter','{ factory.name = support.null-audio-sink node.name = synthetic-capture media.class = Stream/Input/Audio audio.position = [ MONO ] adapter.auto-port-config = { mode = dsp monitor = false position = preserve } object.linger = true }')
 nodes=graph();(out/'created.json').write_text(json.dumps(nodes,indent=2)+'\n')
 outputs=run('pw-link','-o').splitlines();inputs=run('pw-link','-i').splitlines()
 source=next(v for v in outputs if v.startswith('synthetic-source:'));target=next(v for v in inputs if v.startswith('synthetic-capture:'))
 run('pw-link',source,target)
 active=wait(lambda:(value if (value:=capture.describe(graph()))['rows'] else None),'Actual active synthetic capture link')
 assert len(active['rows'])==1 and active['rows'][0]['kind']=='audio-input'
 run('pw-link','-d',source,target);cleared=wait(lambda:(value if not (value:=capture.describe(graph()))['rows'] else None),'Unlink clears capture observation')
 (out/'result.json').write_text(json.dumps(dict(passed=True,checks=['Private real PipeWire graph starts without capture','Synthetic linked capture is observed through actual pw-dump','Actual unlink retires observation'],active=active,cleared=cleared,scope='Synthetic graph in isolated non-root namespaces with no hardware or host PipeWire; not camera/microphone/portal/hardware acceptance'),indent=2)+'\n')
finally:
 for child in reversed(processes):
  if child.poll() is None:os.killpg(child.pid,signal.SIGTERM)
 for child in reversed(processes):
  try:child.wait(timeout=3)
  except subprocess.TimeoutExpired:child.kill();child.wait()
