"""Canonical Action calls and genuine deprecated aliases against a private Qt peer."""
import argparse,json,os,re,subprocess as sp,tempfile,time,warnings
from pathlib import Path
from private_native_bus import start_private_bus
from fixture_resources import close_reaped_pipes
p=argparse.ArgumentParser();p.add_argument('--atspi',type=Path,required=True);p.add_argument('--launcher',type=Path,required=True);p.add_argument('--sway',required=True);p.add_argument('--output',type=Path,required=True);a=p.parse_args();a.output.mkdir(parents=True,exist_ok=False);processes=[]
with tempfile.TemporaryDirectory(prefix='lumen-action-metadata-') as private:
 env=dict(os.environ,HOME=private,XDG_RUNTIME_DIR=private,QT_QPA_PLATFORM='offscreen',QT_QUICK_BACKEND='software',GSETTINGS_BACKEND='memory',DBUS_SYSTEM_BUS_ADDRESS='unix:path='+private+'/absent-system')
 for key in ('SWAYSOCK','WAYLAND_DISPLAY','DISPLAY','AT_SPI_BUS_ADDRESS','NO_AT_BRIDGE','GTK_A11Y'):env.pop(key,None)
 for key in ('CONFIG','STATE','DATA','CACHE'):env['XDG_'+key+'_HOME']=private+'/'+key.lower()
 def launch(command,name):
  with (a.output/name).open('w') as log:process=sp.Popen(command,env=env,stdout=log,stderr=sp.STDOUT)
  processes.append(process);return process
 def wait(predicate,label):
  deadline=time.monotonic()+8
  while time.monotonic()<deadline:
   if 'GLib' in globals():
    while GLib.MainContext.default().pending():GLib.MainContext.default().iteration(False)
   result=predicate()
   if result:return result
   time.sleep(.02)
  raise AssertionError('Timed out '+label)
 try:
  start_private_bus(env,a.output,processes);prefix=re.split(r'\n[ \t]*exec quickshell ',a.launcher.read_text(),maxsplit=1)[0];raw=sp.check_output(['bash','-c',prefix+'\nexec env -0'],env=env);env=dict(value.decode().split('=',1) for value in raw.split(b'\0') if b'=' in value)
  env.update(QT_QPA_PLATFORM='offscreen',QT_QPA_PLATFORMTHEME='generic',QT_QUICK_BACKEND='software',QT_ACCESSIBILITY='1',QT_LINUX_ACCESSIBILITY_ALWAYS_ON='1');env.pop('NO_AT_BRIDGE',None);env.pop('GTK_A11Y',None)
  launch([str(a.atspi/'libexec/at-spi-bus-launcher'),'--launch-immediately','--a11y=1','--screen-reader=1'],'bus-launcher.log')
  import dbus,dbus.mainloop.glib
  dbus.mainloop.glib.DBusGMainLoop(set_as_default=True);bus=dbus.bus.BusConnection(env['DBUS_SESSION_BUS_ADDRESS']);wait(lambda:bus.name_has_owner('org.a11y.Bus'),'bus owner');address=str(bus.get_object('org.a11y.Bus','/org/a11y/bus').GetAddress(dbus_interface='org.a11y.Bus'));assert address.startswith('unix:path='+private+'/');env['AT_SPI_BUS_ADDRESS']=address;launch([str(a.atspi/'libexec/at-spi2-registryd')],'registry.log');a11y=dbus.bus.BusConnection(address);wait(lambda:a11y.name_has_owner('org.a11y.atspi.Registry'),'registry')
  os.environ.update({key:env[key] for key in ('DBUS_SESSION_BUS_ADDRESS','DBUS_SYSTEM_BUS_ADDRESS','AT_SPI_BUS_ADDRESS')});os.environ['GI_TYPELIB_PATH']=str(a.atspi/'lib/girepository-1.0')+':'+os.environ.get('GI_TYPELIB_PATH','')
  import gi
  gi.require_version('Atspi','2.0')
  from gi.repository import Atspi,GLib
  source=Path(re.search(r'export LUMEN_NOTIFICATION_HELPER=(\S+)',a.launcher.read_text()).group(1)).parent.parent/'shell'
  palette=json.loads((source.parent/'appearance/palette.json').read_text());appearance=Path(env['XDG_STATE_HOME'])/'lumen-desktop/appearance.json';appearance.parent.mkdir(parents=True);appearance.write_text(json.dumps(dict(font=palette['font'],palette=palette['light'],preferences={})))
  config=a.output/'sway.conf';config.write_text('output HEADLESS-1 resolution 800x600\nseat seat0 fallback true\n');env.update(WLR_BACKENDS='headless',WLR_RENDERER='pixman',WLR_LIBINPUT_NO_DEVICES='1');launch([a.sway,'--unsupported-gpu','-c',str(config)],'sway.log');env['SWAYSOCK']=str(wait(lambda:next(Path(private).glob('sway-ipc.*.sock'),None),'private compositor'));env['WAYLAND_DISPLAY']=wait(lambda:next((value.name for value in Path(private).glob('wayland-*') if not value.name.endswith('.lock')),None),'private display');env['QT_QPA_PLATFORM']='wayland'
  fixture=a.output/'shell.qml';fixture.write_text('''import QtQuick
import Quickshell
import Quickshell.Io
import "file:'''+str(source)+'''" as D
ShellRoot {property int presses:0
 FloatingWindow {visible:true;implicitWidth:320;implicitHeight:200;D.DialogButton {text:"Action sentinel";onClicked:presses++}}
 IpcHandler {target:"metadata";function count():int{return presses;}}
}''');launch(['quickshell','-p',str(fixture)],'shell.log')
  def find():
   pending=[Atspi.get_desktop(0)]
   while pending:
    item=pending.pop(0)
    if Atspi.Accessible.get_name(item)=='Action sentinel':return item
    pending.extend(Atspi.Accessible.get_child_at_index(item,index) for index in range(Atspi.Accessible.get_child_count(item)))
  target=wait(find,'private production action')
  name=Atspi.Action.get_action_name(target,0);description=Atspi.Action.get_action_description(target,0);assert str(name).lower() in ('press','click'),name
  for method in (Atspi.Action.get_name,Atspi.Action.get_description):
   with warnings.catch_warnings():
    warnings.simplefilter('error')
    try:method(target,0)
    except (DeprecationWarning,SystemError) as error:
     assert isinstance(error,DeprecationWarning) or isinstance(error.__cause__,DeprecationWarning),repr(error)
    else:raise AssertionError('Genuine deprecated alias lost its deprecation')
  assert Atspi.Action.do_action(target,0)
  def count():return sp.check_output(['quickshell','-p',str(fixture),'ipc','call','metadata','count'],env=env,text=True,stderr=sp.PIPE,timeout=2).strip()=='1'
  wait(count,'exact private action once')
  proof=json.loads((a.atspi/'share/action-alias-proof.json').read_text());(a.output/'result.json').write_text(json.dumps(dict(passed=True,name=name,description=description,proof=proof,checks=['canonical actual Action name and description warning-strict','genuine old aliases still raise deprecation','actual production button action once']),indent=2));print('PASS canonical Action API and retained deprecated aliases')
 finally:
  for process in reversed(processes):
   if process.poll() is None:
    process.terminate()
    try:process.wait(timeout=5)
    except sp.TimeoutExpired:process.kill();process.wait(timeout=5)
  close_reaped_pipes(processes)
