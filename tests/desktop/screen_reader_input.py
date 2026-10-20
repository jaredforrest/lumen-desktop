"""Real Orca speech/events over production owned controls in a disposable session."""
import argparse,hashlib,json,os,re,shutil,subprocess as sp,sys,tempfile,time
from pathlib import Path
from screen_reader_speech import configure,utterances,source_proof,transport_messages,plain_text
from private_native_bus import start_private_bus
from fixture_resources import close_reaped_pipes
p=argparse.ArgumentParser();p.add_argument('--launcher',type=Path,required=True);p.add_argument('--sway',required=True);p.add_argument('--orca',type=Path,required=True);p.add_argument('--dispatcher',type=Path,required=True);p.add_argument('--dispatcher-source',type=Path,required=True);p.add_argument('--at-spi-prefix',type=Path,default=os.environ.get('LUMEN_TEST_AT_SPI'));p.add_argument('--output',type=Path,required=True);p.add_argument('--schema-compiler',required=True);p.add_argument('--isolation-tool',required=True);p.add_argument('--inside',action='store_true');p.add_argument('--effects',action='store_true');a=p.parse_args();
if not a.inside:
 live=Path('/run/user/'+str(os.getuid())+'/at-spi/bus_0');before=live.stat().st_ino if live.exists() else None
 result=sp.run([a.isolation_tool,'--user','--map-current-user','--pid','--fork','--mount-proc','--kill-child','--net',sys.executable,str(Path(__file__).resolve()),*sys.argv[1:],'--inside'],timeout=180)
 assert before==(live.stat().st_ino if live.exists() else None),'Live accessibility socket changed'
 raise SystemExit(result.returncode)
out=a.output.resolve();out.mkdir(parents=True,exist_ok=False);(out/'source-proof.json').write_text(json.dumps(source_proof(a.dispatcher_source),indent=2));processes=[];checks=[];events=[]
live=Path('/run/user/'+str(os.getuid())+'/at-spi/bus_0');live_before=live.stat().st_ino if live.exists() else None
with tempfile.TemporaryDirectory(prefix='lumen-reader-') as private:
 runtime=Path(private);env=dict(os.environ,HOME=private,XDG_RUNTIME_DIR=private,GSETTINGS_BACKEND='memory',GDK_BACKEND='wayland',GST_REGISTRY_FORK='no',XDG_SESSION_TYPE='wayland',BRLAPI_HOST=str(runtime/'absent-braille'),PULSE_SERVER='unix:'+str(runtime/'absent-audio'),ALSA_CONFIG_PATH=str(runtime/'absent-alsa'))
 for key in ('CONFIG','STATE','CACHE','DATA'):env['XDG_'+key+'_HOME']=str(runtime/key.lower())
 for key in ('DISPLAY','WAYLAND_DISPLAY','SWAYSOCK','AT_SPI_BUS_ADDRESS','DBUS_SESSION_BUS_ADDRESS','NO_AT_BRIDGE','GTK_A11Y'):env.pop(key,None)
 def launch(command,name,environment=None):
  with (out/name).open('w') as stream:proc=sp.Popen(command,env=environment or env,stdout=stream,stderr=sp.STDOUT,start_new_session=True)
  processes.append(proc);return proc
 def wait(predicate,label):
  deadline=time.monotonic()+15
  while time.monotonic()<deadline:
   if 'GLib' in globals():
    for _ in range(100):
     if not GLib.MainContext.default().pending():break
     GLib.MainContext.default().iteration(False)
   value=predicate()
   if value:return value
   time.sleep(.02)
  raise AssertionError('Timed out: '+label)
 def call(*args):return sp.check_output(args,env=env,text=True,stderr=sp.PIPE,timeout=5).strip()
 try:
  start_private_bus(env,out,processes)
  script=a.launcher.read_text();prefix=re.split(r'\n[ \t]*exec quickshell ',script,maxsplit=1)[0];raw=sp.check_output(['bash','-c',prefix+'\nexec env -0'],env=env);env=dict(v.decode().split('=',1) for v in raw.split(b'\0') if b'=' in v)
  source=Path(re.search(r'export LUMEN_NOTIFICATION_HELPER=(\S+)',script).group(1)).parent.parent/'shell'
  palette=json.loads((source.parent/'appearance/palette.json').read_text());appearance=Path(env['XDG_STATE_HOME'])/'lumen-desktop/appearance.json';appearance.parent.mkdir(parents=True,exist_ok=True);appearance.write_text(json.dumps(dict(font=palette['font'],palette=palette['light'],preferences={})))
  env.update(QT_QPA_PLATFORM='wayland',QT_QPA_PLATFORMTHEME='generic',QT_ACCESSIBILITY='1',QT_LINUX_ACCESSIBILITY_ALWAYS_ON='1',GLOBAL_MENU_ENABLED='0',LUMEN_FILE_SERVICE='0',LUMEN_LAUNCH_SCOPES='0',WLR_BACKENDS='headless',WLR_LIBINPUT_NO_DEVICES='1',WLR_RENDERER='gles2' if a.effects else 'pixman',WLR_RENDERER_ALLOW_SOFTWARE='1',WLR_RENDERER_FORCE_SOFTWARE='1',QT_QUICK_BACKEND='software')
  for key in ('NO_AT_BRIDGE','GTK_A11Y'):env.pop(key,None)
  launch([str(a.at_spi_prefix/'libexec/at-spi-bus-launcher'),'--launch-immediately','--a11y=1','--screen-reader=1'],'a11y-bus.log')
  import dbus,dbus.mainloop.glib
  dbus.mainloop.glib.DBusGMainLoop(set_as_default=True);bus=dbus.bus.BusConnection(env['DBUS_SESSION_BUS_ADDRESS']);wait(lambda:bus.name_has_owner('org.a11y.Bus'),'accessibility bus')
  address=str(bus.get_object('org.a11y.Bus','/org/a11y/bus').GetAddress(dbus_interface='org.a11y.Bus'));assert address.startswith('unix:path='+private+'/');env['AT_SPI_BUS_ADDRESS']=address
  launch([str(a.at_spi_prefix/'libexec/at-spi2-registryd')],'registry.log');a11y=dbus.bus.BusConnection(address);wait(lambda:a11y.name_has_owner('org.a11y.atspi.Registry'),'registry')
  from gi.repository import GLib
  for interface in ('org.a11y.atspi.Event.Object','org.a11y.atspi.Event.Window','org.a11y.atspi.Event.Focus'):
   a11y.add_signal_receiver(lambda *args,**kwargs:events.append(dict(time=time.monotonic(),arguments=str(args),sender=kwargs.get('sender'),interface=kwargs.get('interface'),member=kwargs.get('member'),path=kwargs.get('path'))),dbus_interface=interface,interface_keyword='interface',member_keyword='member',path_keyword='path',sender_keyword='sender')
  registry=dbus.Interface(a11y.get_object('org.a11y.atspi.Registry','/org/a11y/atspi/registry'),'org.a11y.atspi.Registry')
  from xml.etree import ElementTree
  xml=a11y.get_object('org.a11y.atspi.Registry','/org/a11y/atspi/registry').Introspect(dbus_interface='org.freedesktop.DBus.Introspectable');definition=ElementTree.fromstring(str(xml)).find("interface[@name='org.a11y.atspi.Registry']/method[@name='RegisterEvent']");inputs=[arg.get('type') for arg in definition.findall('arg') if arg.get('direction','in')=='in']
  for event in ('object:','window:','focus:'):registry.RegisterEvent(*([event]+[[] if kind=='as' else '' for kind in inputs[1:]]),signature=''.join(inputs),timeout=2)
  speech=configure(out,a.dispatcher,shutil.which('true'));env['SPEECHD_ADDRESS']=speech['address'];launch(speech['command'],'speech.log',dict(env,PATH=str(speech['emptyBin'])));wait(lambda:speech['socket'].exists(),'private speech socket')
  schemas=out/'schemas';schemas.mkdir();original=next((a.orca/'share/gsettings-schemas').glob('*/glib-2.0/schemas/*.xml'));shutil.copy(original,schemas/original.name);(schemas/'fixture.gschema.override').write_text("[org.gnome.Orca.Braille]\nenabled=false\n[org.gnome.Orca.Sound]\nenabled=false\n[org.gnome.Orca.Speech]\nsynthesizer='observer'\n");call(a.schema_compiler,str(schemas));env['GSETTINGS_SCHEMA_DIR']=str(schemas)
  config=out/'sway.conf';config.write_text('output HEADLESS-1 resolution 1100x1000\nseat seat0 fallback true\n');launch([a.sway,'--unsupported-gpu','-c',str(config)],'sway.log');env['SWAYSOCK']=str(wait(lambda:next(runtime.glob('sway-ipc.*.sock'),None),'private compositor'));env['WAYLAND_DISPLAY']=wait(lambda:next((v.name for v in runtime.glob('wayland-*') if not v.name.endswith('.lock')),None),'private display');launch(['wtype','-s','30000'],'keyboard.log')
  fixture=out/'shell.qml';fixture.write_text('''import QtQuick
import Quickshell
import Quickshell.Io
import "file:'''+str(source)+'''" as D
ShellRoot {id:root;property var launched:[];property var finalActions:[]
 QtObject {id:searchService;property var files:[];property var preview:({});property bool searching:false;property bool opening:false;property bool scanning:false;property int indexed:0;property bool limited:false;property bool unreadable:false;property bool more:false;property string error:'';signal opened();signal browseRequested(string path);function search(value){}function requestPreview(value){}}
 QtObject {id:stub;property var search:searchService;property var entries:[{id:'fixture.alpha',name:'Sentinel Alpha',genericName:'First result',noDisplay:false},{id:'fixture.beta',name:'Sentinel Beta',genericName:'Second result',noDisplay:false}];property var notifications:({dnd:false,available:false});property var audioState:({muted:false,available:false});property var exportedMenu:({state:{menu:null},label:function(value){return value;}});property date currentDate:new Date();function activate(entry,newWindow){root.launched=root.launched.concat([entry.id]);}function appLabel(entry){return entry.name;}function appIcon(entry){return '';}}
 QtObject {id:notices;property bool available:true;property bool dnd:false;property string error:'';property var history:[];property var groups:[];property var expandedCards:({});function command(value){}function invoke(notification,action){}function toggleExpanded(id){}}
 QtObject {id:lifecycle;property bool available:false;property var session:({phase:'idle',token:'',error:''});signal sessionAuthorized(var value);function sessionCommand(action,value){}}
 FloatingWindow {id:main;title:'Reader fixture';visible:true;implicitWidth:950;implicitHeight:850
  D.AppLauncher {id:launcher;anchors.fill:parent;shell:stub;onCloseRequested:main.visible=false}
  D.NotificationCentre {id:centre;visible:false;width:600;service:notices}
 }
 D.SessionOptions {id:modal;executeActions:false;lifecycle:lifecycle;onConfirmed:action=>root.finalActions=root.finalActions.concat([action])}
 function note(id,title){return {id:id,app:'Sentinel Inbox',summary:title,body:id===1?'Arrival details: Sentinel Orchard.':'Delivery details: Sentinel Harbour.',icon:'',image:'',desktop:'',timestamp:Date.now(),actions:[]};}
 IpcHandler {target:'readerFixture'
  function state():string{return JSON.stringify({visible:main.visible,query:launcher.queryText,selectionQuery:launcher.selectionQuery,selected:launcher.selectedKey,results:launcher.results.map(row=>row.id),launched:root.launched,finalActions:root.finalActions,modal:modal.visible,error:modal.error});}
  function search():void{main.visible=true;centre.visible=false;launcher.visible=true;launcher.setProvider('Applications');launcher.focusSearch();}
  function update():void{stub.entries=stub.entries.concat([{id:'fixture.gamma',name:'Sentinel Gamma',genericName:'Late result',noDisplay:false}]);}
  function remove():void{stub.entries=stub.entries.filter(entry=>entry.id!=='fixture.alpha');}
  function notifications():void{launcher.visible=false;centre.visible=true;const rows=[root.note(1,'Sentinel First'),root.note(2,'Sentinel Second')];notices.history=rows;notices.groups=[{key:'inbox',rows:rows}];}
  function modalShow():void{modal.show('reboot');}
  function restore():void{main.visible=true;launcher.visible=true;centre.visible=false;launcher.focusSearch();}
 }
}''')
  (out/'runtime-manifest.json').write_text(json.dumps(dict(launcher=str(a.launcher),launcherSha256=hashlib.sha256(a.launcher.read_bytes()).hexdigest(),fixtureSha256=hashlib.sha256(fixture.read_bytes()).hexdigest(),installedComponents={name:dict(path=str(source/name),sha256=hashlib.sha256((source/name).read_bytes()).hexdigest()) for name in ('AppLauncher.qml','NotificationCentre.qml','NotificationCard.qml','SessionOptions.qml','StatusMessage.qml','DialogButton.qml')},providers='Synthetic application/notification/lifecycle boundary only; production controls imported unchanged',reader=str(a.orca),atspi=str(a.at_spi_prefix),dispatcher=str(a.dispatcher),namespace='Private PID, mount/proc, user and network namespaces'),indent=2))
  reader=launch([str(a.orca/'bin/orca'),'--speech-system=speechdispatcherfactory','--debug-file='+str(out/'orca-debug.log')],'orca.log')
  def reader_ready():
   assert reader.poll() is None,'Actual Orca exited before readiness'
   if not bus.name_has_owner('org.gnome.Orca.Service'):return False
   return str(bus.get_object('org.gnome.Orca.Service','/org/gnome/Orca/Service',introspect=False).GetVersion(dbus_interface='org.gnome.Orca.Service',timeout=2))=='50.2'
  wait(reader_ready,'real Orca event loop')
  shell=launch(['quickshell','-p',str(fixture)],'shell.log')
  def ipc(method):return call('quickshell','-p',str(fixture),'ipc','call','readerFixture',method)
  def state():
   try:return json.loads(ipc('state'))
   except (sp.CalledProcessError,ValueError):return {}
  def nodes(node):
   yield node
   for child in node.get('nodes',[])+node.get('floating_nodes',[]):yield from nodes(child)
  def mapped():return next((node for node in nodes(json.loads(call('swaymsg','-r','-t','get_tree'))) if node.get('pid')==shell.pid and node.get('name')=='Reader fixture' and node.get('rect',{}).get('width',0)>0 and node.get('rect',{}).get('height',0)>0),None)
  owner=wait(mapped,'exact owned mapped window');call('swaymsg','[con_id='+str(owner['id'])+'] focus');wait(lambda:mapped().get('focused'),'exact owned focus')
  def interface(ref,name):return dbus.Interface(a11y.get_object(*ref,introspect=False),name)
  def tree():
   pending=[('org.a11y.atspi.Registry','/org/a11y/atspi/accessible/root')];seen=set();rows=[]
   while pending and len(rows)<500:
    ref=tuple(str(value) for value in pending.pop(0))
    if ref in seen or ref[1]=='/org/a11y/atspi/null':continue
    seen.add(ref);item=interface(ref,'org.a11y.atspi.Accessible')
    try:
     name=str(interface(ref,'org.freedesktop.DBus.Properties').Get('org.a11y.atspi.Accessible','Name',timeout=2));states=item.GetState(timeout=2);rows.append((ref,dict(name=name,states=list(map(int,states)),role=str(item.GetRoleName(timeout=2)))));pending.extend(item.GetChildren(timeout=2))
    except dbus.DBusException as error:
     if error.get_dbus_name() not in ('org.freedesktop.DBus.Error.UnknownObject','org.freedesktop.DBus.Error.UnknownMethod'):raise
   (out/'last-tree.json').write_text(json.dumps(rows,indent=2));return rows
  def named(value):return next(((ref,row) for ref,row in tree() if row['name']==value),None)
  def messages():return [message for message in transport_messages(speech['protocol'].read_text()) if ':Orca:' in message['client']]
  def spoken(value,after=0):return any(value in plain_text(message['text']) for message in messages()[after:])
  def observe_idle(seconds):
   deadline=time.monotonic()+seconds
   while time.monotonic()<deadline:
    while GLib.MainContext.default().pending():GLib.MainContext.default().iteration(False)
    time.sleep(.02)
  wait(lambda:state().get('visible'),'actual owned window');ipc('search');call('wtype','Sentinel');wait(lambda:state()['query']=='Sentinel' and state()['selectionQuery']=='Sentinel','acknowledged physical query results')
  alpha,row=wait(lambda:named('Sentinel Alpha'),'production search result');assert interface(alpha,'org.a11y.atspi.Component').GrabFocus(timeout=2)
  wait(lambda:spoken('Sentinel Alpha'),'real reader first result speech')
  call('wtype','-k','Tab');wait(lambda:spoken('Sentinel Beta'),'real keyboard next result speech')
  checks.append('physical search focus speaks results in traversal order')
  query=wait(lambda:named('Search applications, files, actions and clipboard history'),'query accessible')[0]
  text=interface(query,'org.a11y.atspi.Text');caret=int(interface(query,'org.freedesktop.DBus.Properties').Get('org.a11y.atspi.Text','CaretOffset',timeout=2));selected=state()['selected']
  ipc('update');wait(lambda:'fixture.gamma' in state()['results'],'actual provider update')
  assert state()['query']=='Sentinel' and state()['selected']==selected,(selected,state())
  assert int(interface(query,'org.freedesktop.DBus.Properties').Get('org.a11y.atspi.Text','CaretOffset',timeout=2))==caret
  checks.append('provider update preserves query caret and captured selected identity')
  ipc('remove');wait(lambda:'fixture.alpha' not in state()['results'],'removed captured result')
  try:interface(alpha,'org.a11y.atspi.Action').DoAction(0,timeout=2)
  except dbus.DBusException as error:
   assert error.get_dbus_name() in ('org.freedesktop.DBus.Error.UnknownObject','org.freedesktop.DBus.Error.UnknownMethod')
  assert state()['launched']==[]
  checks.append('retired accessible result cannot launch a replacement')
  idle_start=len(messages());observe_idle(2)
  idle_messages=messages()[idle_start:]
  assert sum('Sentinel' in plain_text(message['text']) for message in idle_messages)<=1,'Idle results repeat endlessly'
  checks.append('settled search produces no repeated idle result stream')
  ipc('notifications')
  first=wait(lambda:named('Sentinel Inbox, Sentinel First, 2 notifications. Expand stack'),'collapsed production notification')[0]
  assert interface(first,'org.a11y.atspi.Component').GrabFocus(timeout=2)
  wait(lambda:spoken('Expand stack'),'collapsed stack speech')
  call('wtype','-k','Return')
  first=wait(lambda:named('Sentinel Inbox, Sentinel First'),'expanded first notification')[0]
  assert interface(first,'org.a11y.atspi.Component').GrabFocus(timeout=2)
  wait(lambda:spoken('Sentinel First'),'first notification speech')
  wait(lambda:spoken('Sentinel Orchard'),'notification body read by real reader')
  second=wait(lambda:named('Sentinel Inbox, Sentinel Second'),'expanded second notification')[0]
  assert interface(second,'org.a11y.atspi.Component').GrabFocus(timeout=2)
  wait(lambda:spoken('Sentinel Second') and spoken('Sentinel Harbour'),'second notification speech and body')
  checks.append('production notification stack expands by keyboard and reader speaks ordered cards')
  ipc('restore');wait(lambda:state()['query']=='','fresh query on launcher return');ipc('modalShow')
  wait(lambda:state()['modal'],'production confirmation visible')
  wait(lambda:spoken('Restart confirmation') and spoken('Cancel'),'modal context before initial cancel speech')
  call('wtype','-k','Tab');wait(lambda:spoken('Continue'),'modal keyboard continue speech')
  call('wtype','-k','Return');wait(lambda:state()['error']!='','real actionable modal error')
  wait(lambda:spoken('Application observation is unavailable'),'modal actionable error speech')
  call('wtype','-k','Return');observe_idle(2)
  error_messages=[message for message in messages() if 'Application observation is unavailable' in plain_text(message['text'])]
  assert len(error_messages)==1,'Unchanged error is repeatedly announced'
  continuation=wait(lambda:named('Continue'),'original continuation control')[0]
  assert int(interface(continuation,'org.a11y.atspi.Accessible').GetState(timeout=2)[0]) & (1<<12),'Announcement stole keyboard focus'
  checks.append('actionable error is announced once without focus steal or idle repeats')
  return_checkpoint=len(messages());call('wtype','-k','Escape');wait(lambda:not state()['modal'],'physical modal cancel')
  wait(lambda:bool(int(interface(query,'org.a11y.atspi.Accessible').GetState(timeout=2)[0]) & (1<<12)),'actual opener focus return')
  wait(lambda:spoken('Search applications, files, actions and clipboard history',return_checkpoint),'reader speaks restored opener')
  assert state()['launched']==[] and state()['finalActions']==[]
  checks.append('production modal context error and keyboard cancellation are spoken without final action')
  reader.terminate();reader.wait(timeout=5)
  debug=(out/'orca-debug.log').read_text();(out/'initial-speech.json').write_text(json.dumps(dict(state=state(),utterances=utterances(debug),events=events),indent=2));assert any('Sentinel Alpha' in value for value in utterances(debug)) and any('Sentinel Beta' in value for value in utterances(debug)),'No owned result speech';checks.append('real Orca speech from production owned search')
  accepted=messages();ordered=['Sentinel Alpha','Sentinel Beta','Expand stack','Sentinel Orchard','Sentinel Harbour','Restart confirmation','Cancel','Continue','Application observation is unavailable'];positions=[next(index for index,message in enumerate(accepted) if value in plain_text(message['text'])) for value in ordered];assert positions==sorted(positions),(ordered,positions)
  wait(lambda:all('END' in message['events'] or 'CANCELED' in message['events'] for message in messages() if message['id'] in [accepted[index]['id'] for index in positions]),'reader transport completed selected utterances')
  assert all('BEGIN' in messages()[index]['events'] for index in positions),'Selected speech never began'
  assert events and any(event['member']=='Announcement' and 'Application observation is unavailable' in event['arguments'] for event in events)
  checks.append('ordered reader transcript has real accepted begin/end speech and timestamped announcement event')
  (out/'transcript.json').write_text(json.dumps(messages(),indent=2))
  (out/'result.json').write_text(json.dumps(dict(passed=True,checks=checks,utterances=utterances(debug),orderedSentinels=ordered,positions=positions,events=events),indent=2));print('PASS real Orca',len(checks),'checks')
 finally:
  if 'state' in locals():
   try:(out/'final-state.json').write_text(json.dumps(state(),indent=2))
   except (sp.SubprocessError,ValueError):pass
  (out/'events.json').write_text(json.dumps(events,indent=2))
  for process in reversed(processes):
   if process.poll() is None:
    process.terminate()
    try:process.wait(timeout=5)
    except sp.TimeoutExpired:process.kill();process.wait(timeout=5)
  close_reaped_pipes(processes)
assert live_before==(live.stat().st_ino if live.exists() else None),'Live accessibility socket changed'
