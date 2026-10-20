"""Real private D-Bus and native-authority wire, without a desktop or reader UI."""
import argparse,json,os,select,socket,struct,subprocess as sp,sys,tempfile,threading,time
from pathlib import Path
from private_native_bus import start_private_bus
from fixture_resources import close_reaped_pipes
if '--serve' in sys.argv:
 import gi
 gi.require_version('Gio','2.0')
 from gi.repository import Gio,GLib
 root=Path(sys.argv[-1]);pending=[];bus=Gio.bus_get_sync(Gio.BusType.SESSION,None)
 bus.call_sync('org.freedesktop.DBus','/org/freedesktop/DBus','org.freedesktop.DBus','RequestName',GLib.Variant('(su)',('org.gnome.Orca.Service',0)),None,Gio.DBusCallFlags.NONE,2000,None)
 xml='<node><interface name="org.gnome.Orca.Module"><method name="ListCommands"><arg type="a(ss)" direction="out"/></method><method name="ExecuteCommand"><arg type="s" direction="in"/><arg type="b" direction="in"/><arg type="b" direction="out"/></method></interface></node>';info=Gio.DBusNodeInfo.new_for_xml(xml).interfaces[0]
 def handle(connection,sender,path,interface,method,parameters,invocation):
  if method=='ListCommands':
   if (root/'lock-on-list').exists():(root/'lock.json').write_text(json.dumps(dict(available=True,locked=True,revision=2)))
   if (root/'drop-name').exists():bus.call_sync('org.freedesktop.DBus','/org/freedesktop/DBus','org.freedesktop.DBus','ReleaseName',GLib.Variant('(s)',('org.gnome.Orca.Service',)),None,Gio.DBusCallFlags.NONE,2000,None)
   if (root/'hang-list').exists():pending.append(invocation);return
   action={'WhereAmIPresenter':'WhereAmIBasic','SayAllPresenter':'SayAll','SpeechManager':'InterruptSpeech'}[path.rsplit('/',1)[1]];invocation.return_value(GLib.Variant('(a(ss))',([(action,'Private supported command')],)))
  else:
   with (root/'commands.jsonl').open('a') as stream:stream.write(json.dumps(dict(path=path,args=list(parameters.unpack())))+'\n')
   if (root/'hang-execute').exists():pending.append(invocation);return
   invocation.return_value(GLib.Variant('(b)',(True,)))
 for name in ('WhereAmIPresenter','SayAllPresenter','SpeechManager'):bus.register_object_with_closures2('/org/gnome/Orca/Service/'+name,info,handle,None,None)
 print('ready',flush=True);GLib.MainLoop().run();raise SystemExit
p=argparse.ArgumentParser();p.add_argument('--disabled',type=Path,required=True);p.add_argument('--enabled',type=Path,required=True);p.add_argument('--output',type=Path,required=True);a=p.parse_args();out=a.output.resolve();out.mkdir(parents=True,exist_ok=False);processes=[];checks=[]
with tempfile.TemporaryDirectory(prefix='lumen-reader-provider-') as directory:
 root=Path(directory);env=dict(os.environ,HOME=directory,XDG_RUNTIME_DIR=directory);start_private_bus(env,out,processes);state=root/'lock.json';state.write_text(json.dumps(dict(available=True,locked=False,revision=1)));env['SWAYSOCK']=str(root/'ipc.sock');server=socket.socket(socket.AF_UNIX,socket.SOCK_STREAM);server.bind(env['SWAYSOCK']);server.listen();server.settimeout(.1);stopping=threading.Event();connections=[];threads=[];header=struct.Struct('<6sII')
 def exact(connection,size):
  data=b''
  while len(data)<size:
   chunk=connection.recv(size-len(data))
   if not chunk:raise EOFError
   data+=chunk
  return data
 def peer(connection):
  try:
   magic,length,kind=header.unpack(exact(connection,header.size));assert magic==b'i3-ipc';exact(connection,length)
   value={'success':True} if kind==2 else {'lumen_session_lock':dict(version=1,**json.loads(state.read_text()))};body=json.dumps(value).encode();connection.sendall(header.pack(magic,len(body),kind)+body)
   if kind==2:
    while not stopping.is_set():
     try:
      if not connection.recv(1):break
     except socket.timeout:pass
  except (OSError,EOFError,ValueError):pass
  finally:connection.close()
 def accept():
  while not stopping.is_set():
   try:connection,_=server.accept()
   except socket.timeout:continue
   except OSError:break
   connection.settimeout(.1);connections.append(connection);thread=threading.Thread(target=peer,args=(connection,));threads.append(thread);thread.start()
 worker=threading.Thread(target=accept);worker.start()
 def run(package,*arguments):
  result=sp.run([str(package/'bin/lumen-reader-command'),*arguments],env=env,text=True,capture_output=True,timeout=5);return result.returncode,json.loads(result.stdout)
 def commands():return (root/'commands.jsonl').read_text().splitlines() if (root/'commands.jsonl').exists() else []
 try:
  code,result=run(a.disabled,'where-am-i');assert code==1 and result['reason']=='disabled';checks.append('packaged default disabled without reader service')
  code,result=run(a.enabled,'--enabled','where-am-i');assert code==1 and result['reason']=='unsupported_command';checks.append('public command cannot forward options or arbitrary arguments')
  with (out/'provider.log').open('w') as log:provider=sp.Popen([sys.executable,__file__,'--serve',str(root)],env=env,stdout=sp.PIPE,stderr=log,text=True)
  processes.append(provider);assert select.select([provider.stdout],[],[],5)[0] and provider.stdout.readline().strip()=='ready'
  code,result=run(a.enabled,'where-am-i');assert code==0 and result['accepted'];assert len(commands())==1;checks.append('unique owned provider actual typed command')
  state.write_text(json.dumps(dict(available=True,locked=True,revision=2)));code,result=run(a.enabled,'read-all');assert code==1 and result['reason']=='session_unavailable';assert len(commands())==1;checks.append('authoritative locked session rejects reading')
  state.write_text(json.dumps(dict(available=False,locked=True,revision=3)));code,result=run(a.enabled,'where-am-i');assert code==1 and result['reason']=='session_unavailable';assert len(commands())==1;checks.append('unknown native capability rejects reading')
  code,result=run(a.enabled,'stop');assert code==0 and result['accepted'];assert len(commands())==2;checks.append('safe owned stop remains available with unknown lock authority')
  state.write_text(json.dumps(dict(available=True,locked=False,revision=1)));(root/'lock-on-list').touch();code,result=run(a.enabled,'where-am-i');assert code==1 and not result['accepted'];assert len(commands())==2;(root/'lock-on-list').unlink();checks.append('lock transition during discovery produces zero read dispatch')
  state.write_text(json.dumps(dict(available=True,locked=False,revision=1)));(root/'hang-list').touch();started=time.monotonic();code,result=run(a.enabled,'where-am-i');assert code==1 and result['reason']=='provider_timeout';assert 2<=time.monotonic()-started<4 and len(commands())==2;(root/'hang-list').unlink();checks.append('actual withheld provider reply expires within fixed budget without dispatch')
  (root/'hang-execute').touch();code,result=run(a.enabled,'where-am-i');assert code==1 and result['reason']=='dispatch_unknown' and result['dispatched'];assert len(commands())==3;(root/'hang-execute').unlink();checks.append('missing command acknowledgement remains explicitly ambiguous and never retries')
  state.write_text(json.dumps(dict(available=True,locked=False,revision=1)));(root/'drop-name').touch();code,result=run(a.enabled,'where-am-i');assert code==1 and not result['accepted'];assert len(commands())==3;checks.append('reader owner retirement during discovery produces zero dispatch and no retry')
  (out/'result.json').write_text(json.dumps(dict(passed=True,checks=checks,commands=[json.loads(row) for row in commands()]),indent=2));print('PASS private reader provider',len(checks),'checks')
 finally:
  stopping.set();server.close();worker.join(timeout=2)
  for connection in connections:
   try:connection.shutdown(socket.SHUT_RDWR)
   except OSError:pass
  for thread in threads:thread.join(timeout=2)
  for process in reversed(processes):
   if process.poll() is None:
    process.terminate()
    try:process.wait(timeout=5)
    except sp.TimeoutExpired:process.kill();process.wait(timeout=5)
  close_reaped_pipes(processes)
