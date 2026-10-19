"""Real empty/live-dialog compositor shutdown and restart in a private session."""
import argparse,json,os,resource,select,signal,subprocess as sp,tempfile,time
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--sway',required=True);p.add_argument('--client',required=True);p.add_argument('--output',type=Path,required=True);p.add_argument('--effects',action='store_true');a=p.parse_args();a.output.mkdir(parents=True,exist_ok=False)
processes=[]
with tempfile.TemporaryDirectory(prefix='lumen-dialog-lifetime-') as temporary:
 private=Path(temporary);runtime=private/'runtime';runtime.mkdir(mode=0o700)
 env=dict(os.environ,HOME=str(private),XDG_RUNTIME_DIR=str(runtime),WLR_BACKENDS='headless',WLR_HEADLESS_OUTPUTS='1',WLR_RENDERER='gles2' if a.effects else 'pixman',WLR_RENDERER_ALLOW_SOFTWARE='1',LIBGL_ALWAYS_SOFTWARE='1',WLR_LIBINPUT_NO_DEVICES='1')
 for key in ('SWAYSOCK','WAYLAND_DISPLAY','DISPLAY','DBUS_SESSION_BUS_ADDRESS','AT_SPI_BUS_ADDRESS'):env.pop(key,None)
 bus_config=private/'bus.conf';bus_config.write_text('<busconfig><type>session</type><listen>unix:tmpdir='+str(runtime)+'</listen><policy context="default"><allow own="*"/><allow send_destination="*"/><allow receive_sender="*"/></policy></busconfig>')
 with (a.output/'bus.log').open('w') as stream:bus=sp.Popen(['dbus-daemon','--nofork','--print-address=1','--config-file='+str(bus_config)],env=env,stdout=sp.PIPE,stderr=stream,text=True,start_new_session=True)
 processes.append(bus);assert select.select([bus.stdout],[],[],5)[0];env['DBUS_SESSION_BUS_ADDRESS']=bus.stdout.readline().strip();assert env['DBUS_SESSION_BUS_ADDRESS'].startswith('unix:')
 swaymsg=str(Path(a.sway).parent/'swaymsg')
 config=private/'sway.conf';config.write_text('output HEADLESS-1 resolution 800x600\nxwayland disable\nseat seat0 fallback true\n')
 def no_core():resource.setrlimit(resource.RLIMIT_CORE,(0,0))
 def launch(command,name,**kwargs):
  with (a.output/name).open('w') as stream:proc=sp.Popen(command,env=env,stdout=stream,stderr=sp.STDOUT,start_new_session=True,preexec_fn=no_core,**kwargs)
  processes.append(proc);return proc
 def wait(predicate):
  deadline=time.monotonic()+12
  while time.monotonic()<deadline:
   value=predicate()
   if value:return value
   time.sleep(.03)
  raise AssertionError('Dialog lifetime timeout: '+str(a.output)+'; compositor logs: '+ '\n'.join(path.name+': '+path.read_text() for path in a.output.glob('*sway*.log')))
 def query(kind):return json.loads(sp.check_output([swaymsg,'-r','-t',kind],env=env,text=True,timeout=4))
 def nodes(node):
  yield node
  for child in node.get('nodes',[])+node.get('floating_nodes',[]):yield from nodes(child)
 checks=[]
 try:
  for cycle in ('empty','live-dialog','restart'):
   for key in ('SWAYSOCK','WAYLAND_DISPLAY'):env.pop(key,None)
   compositor=launch([a.sway,'--unsupported-gpu','-c',str(config)],cycle+'-sway.log')
   socket=wait(lambda:next(runtime.glob('sway-ipc*.sock'),None));wayland=wait(lambda:next((path for path in runtime.glob('wayland-*') if not path.name.endswith('.lock')),None))
   env['SWAYSOCK']=str(socket);env['WAYLAND_DISPLAY']=wayland.name
   assert query('get_version')['native_window_relationships'] is True
   client=None
   if cycle=='live-dialog':
    client=launch([a.client],cycle+'-client.log',stdin=sp.PIPE,text=True)
    wait(lambda:sum(1 for node in nodes(query('get_tree')) if (node.get('name') or '').startswith('Relationship '))==4)
    client.stdin.write('modal\n');client.stdin.flush()
    wait(lambda:any((node.get('lumen_relationships') or {}).get('modal') is True for node in nodes(query('get_tree'))))
   compositor.send_signal(signal.SIGTERM);code=compositor.wait(timeout=8)
   (a.output/(cycle+'-exit.json')).write_text(json.dumps(dict(returncode=code,ipcSocketRemains=socket.exists(),waylandSocketRemains=wayland.exists()),indent=2)+'\n')
   assert code==0,cycle+' compositor shutdown failed: '+str(code)
   assert not socket.exists() and not wayland.exists() and not Path(str(wayland)+'.lock').exists(),cycle+' left native sockets after clean exit'
   if client is not None:
    if client.stdin:client.stdin.close()
    client.wait(timeout=4)
   checks.append(cycle+': clean exit, both native endpoints removed')
  (a.output/'result.json').write_text(json.dumps(dict(passed=True,checks=checks,scope='Private headless compositor; live native dialog; real SIGTERM shutdown and restart; no host session'),indent=2)+'\n')
 finally:
  for proc in reversed(processes):
   if proc.poll() is None:os.killpg(proc.pid,signal.SIGTERM)
  for proc in reversed(processes):
   try:proc.wait(timeout=3)
   except sp.TimeoutExpired:os.killpg(proc.pid,signal.SIGKILL);proc.wait(timeout=3)
   if proc.stdin and not proc.stdin.closed:proc.stdin.close()
  if bus.stdout:bus.stdout.close()
print('PASS clean native dialog-manager shutdown and restart')
