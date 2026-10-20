"""Actual installed clock toggle and outside-dismiss against private retained content."""
import argparse,hashlib,json,os,re,shutil,subprocess as sp,sys,time
from pathlib import Path
from application_fixture.isolation import run as isolated_run
from fixture_providers import isolate_providers
from private_native_bus import start_private_bus
from fixture_resources import close_reaped_pipes
p=argparse.ArgumentParser();p.add_argument('--launcher',type=Path,required=True);p.add_argument('--sway',required=True);p.add_argument('--pointer-tool',required=True);p.add_argument('--at-spi-prefix',type=Path,default=os.environ.get('LUMEN_TEST_AT_SPI'));p.add_argument('--python-runtime',default=shutil.which('desktop-test-python'));p.add_argument('--egl-vendor',type=Path,default=os.environ.get('__EGL_VENDOR_LIBRARY_FILENAMES'));p.add_argument('--effects',action='store_true');p.add_argument('--output',type=Path);p.add_argument('--inside',action='store_true');p.add_argument('--source-revision');p.add_argument('--parent-net');a=p.parse_args()
if not a.inside:
 if a.output is None or not a.at_spi_prefix or not a.python_runtime:p.error('Fresh output and pinned Python/accessibility runtimes are required')
 out=a.output.resolve();out.mkdir(mode=0o700,parents=True,exist_ok=False)
 a.launcher=a.launcher.resolve();a.sway=str(Path(a.sway).resolve());a.pointer_tool=str(Path(a.pointer_tool).resolve());a.python_runtime=str(Path(a.python_runtime).resolve());a.at_spi_prefix=a.at_spi_prefix.resolve()
 for value in (a.launcher,a.sway,a.pointer_tool,a.python_runtime,a.at_spi_prefix):
  if not str(Path(value).absolute()).startswith('/nix/store/'):raise ValueError('Pinned immutable tools required')
 revision=sp.check_output(['git','-C',str(Path(__file__).resolve().parents[2]),'rev-parse','HEAD'],text=True).strip()
 args=[a.python_runtime,'/source/tests/desktop/clock_native_input.py','--inside','--launcher',str(a.launcher),'--sway',a.sway,'--pointer-tool',a.pointer_tool,'--at-spi-prefix',str(a.at_spi_prefix),'--parent-net',os.readlink('/proc/self/ns/net'),'--source-revision',revision]
 if a.effects:
  if not a.egl_vendor or not str(a.egl_vendor.resolve()).startswith('/nix/store/') or not a.egl_vendor.is_file():raise ValueError('Effects require the pinned Mesa EGL vendor file')
  args.extend(['--effects','--egl-vendor',str(a.egl_vendor.resolve())])
 raise SystemExit(isolated_run(Path(__file__).resolve().parents[2],out,args,software_graphics=a.effects))
assert os.getuid()!=0 and os.readlink('/proc/self/ns/net')!=a.parent_net
assert not any((Path('/dev')/name).exists() for name in ('dri','snd','video0'))
assert Path('/dev/udmabuf').exists()==a.effects
assert not Path('/dev/snd').exists() and list(Path('/home').iterdir())==[Path.home()]
out=Path('/artifacts')
if a.effects:
 import stat as device_stat
 info=Path('/dev/udmabuf').lstat();assert device_stat.S_ISCHR(info.st_mode);(out/'software-device.json').write_text(json.dumps(dict(device='udmabuf',major=os.major(info.st_rdev),minor=os.minor(info.st_rdev),hardwarePathsAbsent=True,renderer='gles2',forceSoftware=True)))
runtime=Path(os.environ['XDG_RUNTIME_DIR']);env=dict(os.environ);processes=[];checks=[]
script=a.launcher.read_text();prefix=re.split(r'\n[ \t]*exec quickshell ',script,maxsplit=1)[0];raw=sp.check_output(['bash','-c',prefix+'\nexec env -0'],env=env);env.update(dict(value.decode().split('=',1) for value in raw.split(b'\0') if b'=' in value))
env.update(QT_QPA_PLATFORM='wayland',QT_QPA_PLATFORMTHEME='generic',QT_QUICK_BACKEND='software',QT_ACCESSIBILITY='1',QT_LINUX_ACCESSIBILITY_ALWAYS_ON='1',GLOBAL_MENU_ENABLED='0',LUMEN_FILE_SERVICE='0',LUMEN_LAUNCH_SCOPES='0',WLR_BACKENDS='headless',WLR_RENDERER='gles2' if a.effects else 'pixman',WLR_RENDERER_ALLOW_SOFTWARE='1',WLR_RENDERER_FORCE_SOFTWARE='1',LIBGL_ALWAYS_SOFTWARE='1',WLR_LIBINPUT_NO_DEVICES='1')
if a.effects:env['__EGL_VENDOR_LIBRARY_FILENAMES']=str(a.egl_vendor)
for name in ('DISPLAY','WAYLAND_DISPLAY','SWAYSOCK','NO_AT_BRIDGE','GTK_A11Y','AT_SPI_BUS_ADDRESS'):env.pop(name,None)
def launch(command,name,**kwargs):
 with (out/name).open('w') as stream:child=sp.Popen(command,env=env,stdout=stream,stderr=sp.STDOUT,**kwargs)
 processes.append(child);return child
def call(*arguments):return sp.check_output(arguments,env=env,text=True,stderr=sp.PIPE,timeout=5).strip()
def wait(predicate,label):
 end=time.monotonic()+15
 while time.monotonic()<end:
  try:
   value=predicate()
   if value:return value
  except (OSError,ValueError,sp.SubprocessError):pass
  time.sleep(.03)
 raise AssertionError('Timed out: '+label)
def nodes(node):
 yield node
 for child in node.get('nodes',[])+node.get('floating_nodes',[]):yield from nodes(child)
try:
 start_private_bus(env,out,processes)
 launch([str(a.at_spi_prefix/'libexec/at-spi-bus-launcher'),'--launch-immediately','--a11y=1'],'a11y.log')
 import dbus
 bus=dbus.bus.BusConnection(env['DBUS_SESSION_BUS_ADDRESS']);wait(lambda:bus.name_has_owner('org.a11y.Bus'),'private accessibility bus')
 env['AT_SPI_BUS_ADDRESS']=str(bus.get_object('org.a11y.Bus','/org/a11y/bus').GetAddress(dbus_interface='org.a11y.Bus'));assert env['AT_SPI_BUS_ADDRESS'].startswith('unix:path='+str(runtime)+'/')
 launch([str(a.at_spi_prefix/'libexec/at-spi2-registryd')],'registry.log');atspi=dbus.bus.BusConnection(env['AT_SPI_BUS_ADDRESS']);wait(lambda:atspi.name_has_owner('org.a11y.atspi.Registry'),'private registry')
 assets=out/'assets';shutil.copytree(Path(env['LUMEN_NOTIFICATION_HELPER']).parent.parent,assets);source=assets/'shell'
 for name in ('MenuBar.qml','NotificationPanel.qml','CalendarCard.qml','shell.qml'):
  assert (source/name).read_bytes()==(Path('/source/res/lumen-desktop/shell')/name).read_bytes(),('Installed clock source mismatch',name)
 isolate_providers(source,out)
 # No consumer sharing provider is configured in this private fixture.
 env["LUMEN_NEARBY_SHARING_PROVIDER"]=""
 # Read-only geometry/hover inspection; no replacement control or activation path.
 target=source/'MenuBar.qml';original=target.read_text();needle='id: bar';assert original.count(needle)==1
 probe='''
 function clockProbe(){
  function hover(item){if(item.containsMouse!==undefined && item.containsMouse)return true;return (item.children || []).some(child=>hover(child));}
  const point=clockButton.mapToItem(bar.contentItem,0,0);
  return {menu:menu,clock:{x:point.x,y:point.y,w:clockButton.width,h:clockButton.height,hovered:hover(clockButton)},layout:popupLayout(),popupFocused:popupContents.activeFocus};
 }
'''
 modified=original.replace(needle,needle+probe,1);target.chmod(0o600);target.write_text(modified);assert modified.replace(probe,'',1)==original
 (out/'menu-probe-manifest.json').write_text(json.dumps(dict(originalSha256=hashlib.sha256(original.encode()).hexdigest(),modifiedSha256=hashlib.sha256(modified.encode()).hexdigest(),transformation='Read-only clockProbe geometry/hover introspection after id:bar',roundtrip=True),indent=2))
 target=source/'shell.qml';original=target.read_text();head,tail=original.rsplit('}',1)
 addition='''
 IpcHandler {target:"clockAudit"
  function state():string{const bar=root.bars["HEADLESS-1"];return JSON.stringify({ready:!!bar,history:root.notifications.history.map(row=>({id:row.id,summary:row.summary,body:row.body})),bar:bar?bar.clockProbe():null});}
 }
''';target.chmod(0o600);target.write_text(head+addition+'}'+tail)
 (out/'shell-probe-manifest.json').write_text(json.dumps(dict(originalSha256=hashlib.sha256(original.encode()).hexdigest(),modifiedSha256=hashlib.sha256(target.read_bytes()).hexdigest(),transformation='Append read-only clock/history status observer',roundtrip=(head+'}'+tail)==original),indent=2))
 # A fixed synthetic calendar provider, never an account/network adapter.
 calendar=out/'calendar-provider';calendar.write_text('#!'+sys.executable+'\n'+"import json,sys\nr=json.loads(sys.stdin.readline());assert r.get('action') in ('list','load','sync')\nd=r.get('selectedDate','2026-10-20');e=[dict(id='synthetic-event',title='Synthetic clock appointment',date=d,endDate=d,startTime='12:00',endTime='13:00',allDay=False,provider='Fixture',calendar='Local')];print(json.dumps(dict(configured=True,events=e,monthEvents=e,days=[d],total=1,providers=[],month=r.get('month'),date=d)))\n");calendar.chmod(0o700)
 for filename in ('CalendarCard.qml',):
  target=source/filename;original=target.read_text();needle='["lumen-calendar"]';assert original.count(needle)==2;modified=original.replace(needle,json.dumps([str(calendar)]));target.chmod(0o600);target.write_text(modified);(out/'calendar-provider-manifest.json').write_text(json.dumps(dict(originalSha256=hashlib.sha256(original.encode()).hexdigest(),modifiedSha256=hashlib.sha256(modified.encode()).hexdigest(),matches=2,scope='Fixed local synthetic calendar command only')))
 env['GLOBAL_MENU_PYTHON']=sys.executable;env['LUMEN_NOTIFICATION_STATE']=str(runtime/'history');settings=runtime/'settings.json';settings.write_text(json.dumps(dict(notifications=dict(preview='when_unlocked',policies={}))));env['LUMEN_NIX_SETTINGS_ACTIVE']=str(settings)
 palette=json.loads((assets/'appearance/palette.json').read_text());appearance=Path(env['XDG_STATE_HOME'])/'lumen-desktop/appearance.json';appearance.parent.mkdir(parents=True,exist_ok=True);appearance.write_text(json.dumps(dict(font=palette['font'],palette=palette['light'],preferences=dict(reduceMotion=True))))
 config=out/'sway.conf';config.write_text('output HEADLESS-1 resolution 1280x800\nseat seat0 fallback true\nfocus_follows_mouse no\nfor_window [all] floating enable\n');launch([a.sway,'-d','--unsupported-gpu','-c',str(config)],'sway.log');env['SWAYSOCK']=str(wait(lambda:next(runtime.glob('sway-ipc.*.sock'),None),'private compositor'));env['WAYLAND_DISPLAY']=wait(lambda:next((value.name for value in runtime.glob('wayland-*') if not value.name.endswith('.lock')),None),'private display')
 launch(['wtype','-s','60000'],'keyboard.log');pointer=launch([a.pointer_tool,'pointer','move','0','0'],'pointer.log',stdin=sp.PIPE,text=True)
 pointer.stdin.write('m 0 0\n');pointer.stdin.flush()
 wait(lambda:"adding virtual pointer: '0:0:wlr_virtual_pointer_v1'" in (out/'sway.log').read_text(),'actual private compositor virtual pointer registration')
 shell=launch(['quickshell','-p',str(source)],'shell.log')
 client_file=out/'client.qml';client_file.write_text('import Quickshell\nimport QtQuick\nShellRoot {FloatingWindow {visible:true;title:"Clock return target";implicitWidth:500;implicitHeight:320;Rectangle {anchors.fill:parent;color:"#334455";TextInput {anchors.centerIn:parent;focus:true;text:"Synthetic return target"}}}}');client=launch(['quickshell','-p',str(client_file)],'client.log')
 def ipc():return json.loads(call('quickshell','-p',str(source),'ipc','call','clockAudit','state'))
 def state():
  value=ipc();(out/'last-state.json').write_text(json.dumps(value,indent=2));return value
 def client_node():return next((node for node in nodes(json.loads(call('swaymsg','-r','-t','get_tree'))) if node.get('pid')==client.pid and node.get('name')=='Clock return target'),None)
 owned=wait(client_node,'exact private return window');con_id=owned['id'];call('swaymsg','[con_id='+str(con_id)+'] focus');wait(lambda:client_node().get('focused'),'native return focus')
 wait(lambda:state()['ready'],'actual installed clock owner')
 server=launch([sys.executable,env['LUMEN_NOTIFICATION_HELPER'],'serve'],'notifications.log')
 wait(lambda:bus.name_has_owner('org.freedesktop.Notifications'),'private notification service')
 api=dbus.Interface(bus.get_object('org.freedesktop.Notifications','/org/freedesktop/Notifications'),'org.freedesktop.Notifications')
 notification=int(api.Notify('Clock fixture',0,'','Synthetic retained notification','Synthetic private body',[],{},0))
 wait(lambda:any(row['id']==notification and row['summary']=='Synthetic retained notification' for row in state()['history']),'real retained notification transported')
 checks.append('Production notification transport retains exact synthetic content on private bus')
 def pointer_command(value):pointer.stdin.write(value+'\n');pointer.stdin.flush()
 def click(rect,hover=None):
  assert rect and rect['w']>0 and rect['h']>0,rect
  call('swaymsg','seat','seat0','cursor','set',str(int(rect['x']+rect['w']/2)),str(int(rect['y']+rect['h']/2)));pointer_command('m 0.5 0.5')
  if hover:
   try:wait(hover,'physical pointer hover acknowledgement')
   except AssertionError:
    call('grim',str(out/'pointer-failure.png'));(out/'pointer-failure-seats.json').write_text(call('swaymsg','-r','-t','get_seats'));raise
  pointer_command('p');pointer_command('r')
 def clock():return state()['bar']['clock']
 def opened():
  value=state();return value if value['bar']['menu']=='Date' and value['bar']['layout']['calendar'] and value['bar']['layout']['calendar']['events']==1 and not value['bar']['layout']['calendar']['busy'] else None
 def closed():return state()['bar']['menu']==''
 click(clock(),lambda:clock()['hovered']);value=wait(opened,'first physical clock click opens populated surface');layout=value['bar']['layout'];assert layout['panelWidth']>0 and layout['panelHeight']>0 and layout['x']>=0 and layout['y']>=0 and layout['x']+layout['panelWidth']<=layout['width']+2 and layout['y']+layout['panelHeight']<=layout['height']+2,layout
 call('grim',str(out/'clock-open.png'));(out/'opened-state.json').write_text(json.dumps(value,indent=2));checks.append('First actual clock press/release opens measured notification and calendar surface')
 click(clock());wait(closed,'second physical clock click closes');call('grim',str(out/'clock-second-click.png'));checks.append('Second actual clock press/release dismisses the existing surface')
 click(clock(),lambda:clock()['hovered']);wait(opened,'physical reopen');call('swaymsg','seat','seat0','cursor','set','100','600');pointer_command('m 0.5 0.5');pointer_command('p');pointer_command('r');wait(closed,'actual outside press dismissal');call('grim',str(out/'clock-outside.png'));wait(lambda:client_node()['id']==con_id and client_node().get('focused'),'exact pre-popup window focus after outside');checks.append('Outside physical click dismisses and restores exact native window focus')
 click(clock(),lambda:clock()['hovered']);wait(opened,'physical reopen for Escape');call('wtype','-k','Escape');wait(closed,'physical Escape');wait(lambda:client_node()['id']==con_id and client_node().get('focused'),'exact native focus after Escape');checks.append('Physical Escape dismisses populated surface and restores captured native focus')
 assert any(row['id']==notification and row['summary']=='Synthetic retained notification' for row in state()['history']);checks.append('Toggle and dismissal preserve retained notification identity and content')
 if a.effects:
  renderer=(out/'sway.log').read_text();assert all(value in renderer for value in ('Creating scenefx FX renderer','Using OpenGL ES','GL renderer: llvmpipe','FX RENDERER: Shaders Initialized Successfully','Trying udmabuf allocator','Created GL FBO for buffer')),renderer
 (out/'result.json').write_text(json.dumps(dict(passed=True,checks=checks,effects=a.effects,sourceRevision=a.source_revision,fixtureSha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),packages={key:dict(path=str(value),sha256=hashlib.sha256(Path(value).read_bytes()).hexdigest()) for key,value in dict(launcher=a.launcher,compositor=a.sway,pointer=a.pointer_tool).items()},scope='Actual full shell clock and physical pointer/keyboard; synthetic private notifications/calendar, no account or reference measurement claim'),indent=2));print('PASS actual installed clock',len(checks),'checks')
finally:
 (out/'child-status-before-cleanup.json').write_text(json.dumps([dict(pid=child.pid,status=child.poll(),args=child.args) for child in processes],indent=2))
 for child in reversed(processes):
  if child.poll() is None:
   child.terminate()
   try:child.wait(timeout=5)
   except sp.TimeoutExpired:child.kill();child.wait(timeout=5)
 close_reaped_pipes(processes)
