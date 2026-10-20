"""Full installed menu/report against a real, private, hardware-free PipeWire graph."""
import argparse,hashlib,json,os,re,shutil,signal,subprocess as sp,sys,time
from pathlib import Path
from application_fixture.isolation import run as isolated_run
from application_fixture.capture_graph import SyntheticCaptureGraph
from fixture_providers import isolate_providers
from private_native_bus import start_private_bus
from fixture_resources import close_reaped_pipes
p=argparse.ArgumentParser();p.add_argument('--launcher',type=Path,required=True);p.add_argument('--sway',required=True);p.add_argument('--pointer-tool',required=True);p.add_argument('--pipewire',type=Path,required=True);p.add_argument('--at-spi-prefix',type=Path,default=os.environ.get('LUMEN_TEST_AT_SPI'));p.add_argument('--python-runtime',default=shutil.which('desktop-test-python'));p.add_argument('--egl-vendor',type=Path,default=os.environ.get('__EGL_VENDOR_LIBRARY_FILENAMES'));p.add_argument('--effects',action='store_true');p.add_argument('--output',type=Path);p.add_argument('--inside',action='store_true');p.add_argument('--source-revision');p.add_argument('--parent-net');a=p.parse_args()
if not a.inside:
 if a.output is None or not a.at_spi_prefix or not a.python_runtime:p.error('Fresh output and pinned Python/accessibility runtimes are required')
 out=a.output.resolve();out.mkdir(mode=0o700,parents=True,exist_ok=False)
 a.launcher=a.launcher.resolve();a.sway=str(Path(a.sway).resolve());a.pointer_tool=str(Path(a.pointer_tool).resolve());a.python_runtime=str(Path(a.python_runtime).resolve());a.at_spi_prefix=a.at_spi_prefix.resolve();a.pipewire=a.pipewire.resolve()
 for value in (a.launcher,a.sway,a.pointer_tool,a.python_runtime,a.at_spi_prefix):
  if not str(Path(value).absolute()).startswith('/nix/store/'):raise ValueError('Pinned immutable tools required')
 if not str(a.pipewire.resolve()).startswith('/nix/store/') or not all((a.pipewire/name).is_file() for name in ('pipewire','pw-dump','pw-cli','pw-link')):raise ValueError('Pinned PipeWire tools required')
 revision=sp.check_output(['git','-C',str(Path(__file__).resolve().parents[2]),'rev-parse','HEAD'],text=True).strip()
 args=[a.python_runtime,'/source/tests/desktop/capture_native_input.py','--inside','--launcher',str(a.launcher),'--sway',a.sway,'--pointer-tool',a.pointer_tool,'--pipewire',str(a.pipewire),'--at-spi-prefix',str(a.at_spi_prefix),'--parent-net',os.readlink('/proc/self/ns/net'),'--source-revision',revision]
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
runtime=Path(os.environ['XDG_RUNTIME_DIR']);env=dict(os.environ);processes=[];checks=[];graph=None
script=a.launcher.read_text();prefix=re.split(r'\n[ \t]*exec quickshell ',script,maxsplit=1)[0];raw=sp.check_output(['bash','-c',prefix+'\nexec env -0'],env=env);env.update(dict(value.decode().split('=',1) for value in raw.split(b'\0') if b'=' in value))
env.update(QT_QPA_PLATFORM='wayland',QT_QPA_PLATFORMTHEME='generic',QT_QUICK_BACKEND='software',QT_ACCESSIBILITY='1',QT_LINUX_ACCESSIBILITY_ALWAYS_ON='1',GLOBAL_MENU_ENABLED='0',LUMEN_FILE_SERVICE='0',LUMEN_LAUNCH_SCOPES='0',WLR_BACKENDS='headless',WLR_RENDERER='gles2' if a.effects else 'pixman',WLR_RENDERER_ALLOW_SOFTWARE='1',WLR_RENDERER_FORCE_SOFTWARE='1',LIBGL_ALWAYS_SOFTWARE='1',WLR_LIBINPUT_NO_DEVICES='1',PIPEWIRE_REMOTE='capture-fixture',PIPEWIRE_RUNTIME_DIR=str(runtime))
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
 graph=SyntheticCaptureGraph(a.pipewire,runtime,out,env);graph.start();graph.prepare()
 launch([str(a.at_spi_prefix/'libexec/at-spi-bus-launcher'),'--launch-immediately','--a11y=1'],'a11y.log')
 import dbus
 bus=dbus.bus.BusConnection(env['DBUS_SESSION_BUS_ADDRESS']);wait(lambda:bus.name_has_owner('org.a11y.Bus'),'private accessibility bus')
 env['AT_SPI_BUS_ADDRESS']=str(bus.get_object('org.a11y.Bus','/org/a11y/bus').GetAddress(dbus_interface='org.a11y.Bus'));assert env['AT_SPI_BUS_ADDRESS'].startswith('unix:path='+str(runtime)+'/')
 launch([str(a.at_spi_prefix/'libexec/at-spi2-registryd')],'registry.log');atspi=dbus.bus.BusConnection(env['AT_SPI_BUS_ADDRESS']);wait(lambda:atspi.name_has_owner('org.a11y.atspi.Registry'),'private registry')
 assets=out/'assets';shutil.copytree(Path(env['LUMEN_NOTIFICATION_HELPER']).parent.parent,assets);source=assets/'shell'
 for name in ('CaptureState.qml','CaptureReport.qml','capture_observation.py'):
  assert (source/name).read_bytes()==(Path('/source/res/lumen-desktop/shell')/name).read_bytes(),('Installed capture source mismatch',name)
 isolate_providers(source,out)
 # Read-only geometry/hover inspection; no replacement control or activation path.
 target=source/'MenuBar.qml';original=target.read_text();needle='id: bar';assert original.count(needle)==1
 probe='''
 function captureProbe(){
  function scan(item,name,text){if((name && item.objectName===name) || (text && item.text===text && item.hovered!==undefined && item.visible))return item;for(const child of item.children || []){const found=scan(child,name,text);if(found)return found;}return null;}
  function rect(item,owner){if(!item)return null;const point=item.mapToItem(owner,0,0);return {x:point.x,y:point.y,w:item.width,h:item.height,visible:item.visible,hovered:item.hovered===undefined?false:item.hovered,focused:item.activeFocus};}
  const controls=scan(bar.contentItem,"status-item-controls","");const row=scan(dismiss.contentItem,"","Capture observations…");
  return {menu:menu,kind:systemMenu,indicator:rect(captureIndicator,bar.contentItem),controls:rect(controls,bar.contentItem),row:rect(row,dismiss.contentItem),layout:popupLayout(),headings:menuLayoutStatus(),popupFocused:popupContents.activeFocus};
 }
'''
 modified=original.replace(needle,needle+probe,1);target.chmod(0o600);target.write_text(modified);assert modified.replace(probe,'',1)==original
 (out/'menu-probe-manifest.json').write_text(json.dumps(dict(originalSha256=hashlib.sha256(original.encode()).hexdigest(),modifiedSha256=hashlib.sha256(modified.encode()).hexdigest(),transformation='Read-only captureProbe geometry/hover introspection after id:bar',roundtrip=True),indent=2))
 target=source/'shell.qml';original=target.read_text();head,tail=original.rsplit('}',1)
 addition='''
 IpcHandler {target:"captureAudit"
  function state():string{const bar=root.bars["HEADLESS-1"];return JSON.stringify({ready:!!bar,capture:root.capture.status,lastPublished:root.capture.lastPublished,bar:bar?bar.captureProbe():null});}
 }
''';target.chmod(0o600);target.write_text(head+addition+'}'+tail)
 (out/'shell-probe-manifest.json').write_text(json.dumps(dict(originalSha256=hashlib.sha256(original.encode()).hexdigest(),modifiedSha256=hashlib.sha256(target.read_bytes()).hexdigest(),transformation='Append read-only Ipc capture/status observer',roundtrip=(head+'}'+tail)==original),indent=2))
 # The interpreter is unchanged except a self-stop gate for one exact owned capture child.
 actual_python=env['GLOBAL_MENU_PYTHON'];shim=out/'capture-python';shim.write_text('#!'+sys.executable+'\n'+'''import json,os,signal,sys
from pathlib import Path
out=Path('/artifacts')
if len(sys.argv)>1 and Path(sys.argv[1]).resolve()==out/'assets/shell/capture_observation.py':
 if (out/'hold-capture').exists():
  facts=dict(pid=os.getpid(),parent=os.getppid(),start=Path('/proc/self/stat').read_text().rsplit(') ',1)[1].split()[19])
  (out/'held-child.json').write_text(json.dumps(facts));os.kill(os.getpid(),signal.SIGSTOP)
os.execv('''+repr(actual_python)+''',['''+repr(actual_python)+''',*sys.argv[1:]])
''');shim.chmod(0o700);env['GLOBAL_MENU_PYTHON']=str(shim)
 palette=json.loads((assets/'appearance/palette.json').read_text());appearance=Path(env['XDG_STATE_HOME'])/'lumen-desktop/appearance.json';appearance.parent.mkdir(parents=True,exist_ok=True);appearance.write_text(json.dumps(dict(font=palette['font'],palette=palette['light'],preferences=dict(reduceMotion=True))))
 config=out/'sway.conf';config.write_text('output HEADLESS-1 resolution 1280x800\nseat seat0 fallback true\nfocus_follows_mouse no\nfor_window [all] floating enable\n');launch([a.sway,'-d','--unsupported-gpu','-c',str(config)],'sway.log');env['SWAYSOCK']=str(wait(lambda:next(runtime.glob('sway-ipc.*.sock'),None),'private compositor'));env['WAYLAND_DISPLAY']=wait(lambda:next((value.name for value in runtime.glob('wayland-*') if not value.name.endswith('.lock')),None),'private display')
 launch(['wtype','-s','60000'],'keyboard.log');pointer=launch([a.pointer_tool,'pointer','move','0','0'],'pointer.log',stdin=sp.PIPE,text=True)
 pointer.stdin.write('m 0 0\n');pointer.stdin.flush()
 wait(lambda:"adding virtual pointer: '0:0:wlr_virtual_pointer_v1'" in (out/'sway.log').read_text(),'actual private compositor virtual pointer registration')
 shell=launch(['quickshell','-p',str(source)],'shell.log')
 client_file=out/'client.qml';client_file.write_text('import Quickshell\nimport QtQuick\nShellRoot {FloatingWindow {visible:true;title:"Capture return target";implicitWidth:500;implicitHeight:320;Rectangle {anchors.fill:parent;color:"#334455";TextInput {anchors.centerIn:parent;focus:true;text:"Synthetic return target"}}}}');client=launch(['quickshell','-p',str(client_file)],'client.log')
 def ipc():return json.loads(call('quickshell','-p',str(source),'ipc','call','captureAudit','state'))
 def state():
  value=ipc();(out/'last-state.json').write_text(json.dumps(value,indent=2));return value
 def client_node():return next((node for node in nodes(json.loads(call('swaymsg','-r','-t','get_tree'))) if node.get('pid')==client.pid and node.get('name')=='Capture return target'),None)
 owned=wait(client_node,'exact private return window');con_id=owned['id'];call('swaymsg','[con_id='+str(con_id)+'] focus');wait(lambda:client_node().get('focused'),'native return focus')
 wait(lambda:state()['ready'] and state()['capture']['available'] and not state()['capture']['rows'],'real empty graph UI');assert not state()['bar']['indicator']['visible'];checks.append('empty private real graph has no capture dot and no global idle assertion')
 graph.link();wait(lambda:state()['capture']['available'] and len(state()['capture']['rows'])==1 and state()['bar']['indicator']['visible'],'actual linked graph dot');assert state()['capture']['rows'][0]['kind']=='audio-input';checks.append('real synthetic PipeWire link drives production capture indicator')
 def pointer_command(value):pointer.stdin.write(value+'\n');pointer.stdin.flush()
 def click(rect,hover=None):
  assert rect and rect['w']>0 and rect['h']>0,rect
  call('swaymsg','seat','seat0','cursor','set',str(int(rect['x']+rect['w']/2)),str(int(rect['y']+rect['h']/2)));pointer_command('m 0.5 0.5')
  if hover:
   try:wait(hover,'physical pointer hover acknowledgement')
   except AssertionError:
    call('grim',str(out/'pointer-failure.png'));(out/'pointer-failure-seats.json').write_text(call('swaymsg','-r','-t','get_seats'));raise
  pointer_command('p');pointer_command('r')
 def interface(ref,name):return dbus.Interface(atspi.get_object(*ref,introspect=False),name)
 def named(name):
  pending=[('org.a11y.atspi.Registry','/org/a11y/atspi/accessible/root')];seen=set();observed=[]
  while pending and len(seen)<4000:
   ref=tuple(map(str,pending.pop(0)))
   if ref in seen or ref[1]=='/org/a11y/atspi/null':continue
   seen.add(ref)
   try:
    accessible=interface(ref,'org.a11y.atspi.Accessible');value=str(interface(ref,'org.freedesktop.DBus.Properties').Get('org.a11y.atspi.Accessible','Name',timeout=2));showing=int(accessible.GetState(timeout=2)[0])&(1<<25)
    observed.append(dict(name=value,showing=bool(showing),ref=ref));(out/'last-accessibility-tree.json').write_text(json.dumps(observed,indent=2))
    if value==name and showing:return ref
    pending.extend(accessible.GetChildren(timeout=2))
   except dbus.DBusException:pass
  return None
 def report(label):
  wait(lambda:state()['bar']['kind']=='Capture' and state()['bar']['menu']=='Capture','actual capture panel routing')
  call('grim',str(out/(label+'-before-accessibility.png')))
  wait(lambda:named('Capture observations'),'production report title')
  wait(lambda:named('Active PipeWire capture links'),'production report actual active-link text')
  layout=state()['bar']['layout'];assert layout['panelWidth']>0 and layout['panelHeight']>0 and layout['x']>=0 and layout['y']>=0 and layout['x']+layout['panelWidth']<=layout['width']+2 and layout['y']+layout['panelHeight']<=layout['height']+2,layout
  call('grim',str(out/(label+'.png')));(out/(label+'-layout.json')).write_text(json.dumps(layout,indent=2));return layout
 def escape():
  call('wtype','-k','Escape');wait(lambda:state()['bar']['menu']=='','physical report dismissal');wait(lambda:client_node().get('focused') and client_node()['id']==con_id,'exact return conId focus')
 click(state()['bar']['indicator'],lambda:state()['bar']['indicator']['hovered']);report('indicator-report');checks.append('physical indicator opens actual measured capture report');escape();checks.append('Escape returns exact pre-popup native window focus')
 click(state()['bar']['controls']);wait(lambda:state()['bar']['menu']=='Controls','physical Control Centre open');row=wait(lambda:named('Capture observations…, PipeWire-visible activity'),'actual Control Centre report button');assert interface(row,'org.a11y.atspi.Component').GrabFocus(timeout=2);wait(lambda:state()['bar']['row'] and state()['bar']['row']['focused'],'exact Control Centre row focus');call('wtype','-k','space');report('control-centre-report');checks.append('production Control Centre keyboard button opens the same report');escape()
 call('swaymsg','output','HEADLESS-1','resolution','640x480');wait(lambda:state()['bar']['headings']['width']==640,'native narrow bar configure');layout=state()['bar'];assert layout['headings']['menuEnd']<=layout['headings']['statusStart']+2,layout;assert layout['indicator']['x']>=0 and layout['indicator']['x']+layout['indicator']['w']<=640
 click(layout['indicator'],lambda:state()['bar']['indicator']['hovered']);report('narrow-report');checks.append('crowded narrow output retains separate indicator/menu bounds and clipped report viewport');escape()
 graph.unlink();wait(lambda:state()['capture']['available'] and not state()['capture']['rows'] and not state()['bar']['indicator']['visible'],'actual unlink removes dot');checks.append('actual graph unlink retires observation and dot')
 graph.link();wait(lambda:state()['bar']['indicator']['visible'],'fresh graph observation before held child');(out/'hold-capture').touch();facts=wait(lambda:json.loads((out/'held-child.json').read_text()) if (out/'held-child.json').exists() else None,'exact held observation child')
 stat=Path('/proc/'+str(facts['pid'])+'/stat').read_text().rsplit(') ',1)[1].split();assert stat[19]==facts['start'] and int(stat[1])==shell.pid and facts['parent']==shell.pid
 wait(lambda:not state()['capture']['available'] and not state()['bar']['indicator']['visible'],'natural7.5-second observation expiry');checks.append('held exact provider child causes natural stale expiry and dot removal')
 (out/'hold-capture').unlink();assert Path('/proc/'+str(facts['pid'])+'/stat').read_text().rsplit(') ',1)[1].split()[19]==facts['start'];os.kill(facts['pid'],signal.SIGKILL);wait(lambda:state()['bar']['indicator']['visible'],'fresh generation recovers real graph')
 graph.close();wait(lambda:not state()['capture']['available'] and not state()['bar']['indicator']['visible'],'private provider disappearance clears dot');checks.append('provider disappearance fails unavailable without retaining a positive dot')
 if a.effects:
  renderer=(out/'sway.log').read_text();assert all(value in renderer for value in ('Creating scenefx FX renderer','Using OpenGL ES','GL renderer: llvmpipe','FX RENDERER: Shaders Initialized Successfully','Trying udmabuf allocator','Created GL FBO for buffer')),renderer
 (out/'result.json').write_text(json.dumps(dict(passed=True,checks=checks,effects=a.effects,sourceRevision=a.source_revision,fixtureSha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),packages={key:dict(path=str(value),sha256=hashlib.sha256(Path(value).read_bytes()).hexdigest()) for key,value in dict(launcher=a.launcher,compositor=a.sway,pointer=a.pointer_tool).items()},scope='Actual full shell, private real generated audio graph; no hardware, host graph, permission or global idle claim'),indent=2));print('PASS actual full capture menu',len(checks),'checks')
finally:
 (out/'child-status-before-cleanup.json').write_text(json.dumps([dict(pid=child.pid,status=child.poll(),args=child.args) for child in processes],indent=2))
 if graph:graph.close()
 for child in reversed(processes):
  if child.poll() is None:
   child.terminate()
   try:child.wait(timeout=5)
   except sp.TimeoutExpired:child.kill();child.wait(timeout=5)
 close_reaped_pipes(processes)
