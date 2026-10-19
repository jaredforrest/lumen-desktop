"""Deliberate AT-SPI verification in an isolated bus, runtime and compositor."""
import argparse,json,os,re,shutil,signal,subprocess as sp,sys,tempfile,time
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--output',type=Path,required=True);p.add_argument('--launcher',required=True);p.add_argument('--sway',default='sway');p.add_argument('--at-spi-prefix',type=Path,default=os.environ.get('LUMEN_TEST_AT_SPI'));p.add_argument('--quickshell');p.add_argument('--full-shell',action='store_true');p.add_argument('--inside',action='store_true');a=p.parse_args()
if not a.at_spi_prefix:p.error('Provide --at-spi-prefix or use the pinned desktop-test-python runtime')
if not a.inside:
    live=Path('/run/user/'+str(os.getuid())+'/at-spi/bus_0')
    before=live.stat().st_ino if live.exists() else None
    private=tempfile.mkdtemp(prefix='lumen-atspi-');env=dict(os.environ,HOME=private,XDG_RUNTIME_DIR=private,GSETTINGS_BACKEND='memory',QT_ACCESSIBILITY='1',QT_LINUX_ACCESSIBILITY_ALWAYS_ON='1')
    for key in ('CONFIG','STATE','CACHE','DATA'):env['XDG_'+key+'_HOME']=str(Path(private)/key.lower())
    for key in ('AT_SPI_BUS_ADDRESS','NO_AT_BRIDGE','GTK_A11Y','DBUS_SESSION_BUS_ADDRESS','SWAYSOCK','WAYLAND_DISPLAY','DISPLAY'):env.pop(key,None)
    try:
        code=sp.call(['dbus-run-session','--',sys.executable,__file__,*sys.argv[1:],'--inside'],env=env)
        after=live.stat().st_ino if live.exists() else None
        assert before==after,('Live accessibility socket changed',before,after)
        result=a.output.resolve()/'result.json'
        if code==0 and result.exists():
            value=json.loads(result.read_text());value['liveSocketUnchanged']=True;result.write_text(json.dumps(value,indent=2)+'\n')
        raise SystemExit(code)
    finally:shutil.rmtree(private,ignore_errors=True)
import dbus,dbus.mainloop.glib
dbus.mainloop.glib.DBusGMainLoop(set_as_default=True)
out=a.output.resolve();out.mkdir(parents=True,exist_ok=False);runtime=Path(os.environ['XDG_RUNTIME_DIR'])
script=Path(a.launcher).read_text();prefix=re.split(r'\n[ \t]*exec quickshell ',script,maxsplit=1)[0]
raw=sp.check_output(['bash','-c',prefix+'\nexec env -0']);env=dict(v.decode().split('=',1) for v in raw.split(b'\0') if b'=' in v)
env.update(QT_QPA_PLATFORM='wayland',QT_QPA_PLATFORMTHEME='generic',QT_QUICK_BACKEND='software',QT_ACCESSIBILITY='1',QT_LINUX_ACCESSIBILITY_ALWAYS_ON='1',GSETTINGS_BACKEND='memory',WLR_BACKENDS='headless',WLR_RENDERER='pixman',WLR_LIBINPUT_NO_DEVICES='1',GLOBAL_MENU_ENABLED='0',LUMEN_FILE_SERVICE='0',LUMEN_LAUNCH_SCOPES='0')
for key in ('NO_AT_BRIDGE','GTK_A11Y','AT_SPI_BUS_ADDRESS','WAYLAND_DISPLAY','SWAYSOCK','DISPLAY'):env.pop(key,None)
processes=[]
def launch(command,name):
    proc=sp.Popen(command,env=env,stdout=(out/name).open('w'),stderr=sp.STDOUT,start_new_session=True);processes.append(proc);return proc

def wait(callback,description):
    end=time.monotonic()+20
    while time.monotonic()<end:
        if 'GLib' in globals():
            for _ in range(100):
                if not GLib.MainContext.default().pending():break
                GLib.MainContext.default().iteration(False)
        try:
            value=callback()
            if value:return value
        except (OSError,ValueError,sp.SubprocessError,dbus.DBusException):pass
        time.sleep(.05)
    raise AssertionError('Timed out: '+description)

def call(*args):return sp.check_output(args,env=env,text=True,timeout=8).strip()
source=Path(__file__).resolve().parents[2]/'res/lumen-desktop/shell'
fixture=out/'fixture';fixture.mkdir()
(fixture/'shell.qml').write_text('''import QtQuick
import QtQuick.Window
import Quickshell
import Quickshell.Io
import "'''+source.as_uri()+'''" as D
ShellRoot {id:mock;property int disabledPresses:0
 QtObject {id:service;property bool available:true;property string error:"";property var focusState:({profiles:[{id:"work",name:"Work",allowedApps:[],allowCritical:false,schedules:[]}]});property var applications:[{key:"chat",name:"Chat"}];function command(data){focusState={profiles:[data.profile]};}}
 FloatingWindow {id:window;visible:true;title:"Accessibility Fixture";implicitWidth:600;implicitHeight:850
  Column {width:500;spacing:8;Accessible.role:Accessible.Pane;Accessible.name:"Fixture content"
   D.ControlTile {id:focus;width:300;height:62;label:"Focus";subtitle:active?"Work":"Off";toggle:true;active:false;unavailable:false;onClicked:active=!active}
   D.ControlRow {width:300;text:"For one hour"}
   D.MenuButton {text:"Unavailable command";enabled:false;onClicked:mock.disabledPresses++}
   D.SettingsToggle {width:400;text:"Show Calendar";checked:false}
   D.FocusProfileEditor {service:service;profileId:"work"}
  }
 }
 IpcHandler {target:"a11yFixture";function status():string {return JSON.stringify({visible:window.visible,focus:focus.active,profile:service.focusState.profiles[0],disabledPresses:mock.disabledPresses});}function close():void {window.visible=false;}}
}''')
try:
    launch([str(a.at_spi_prefix/'libexec/at-spi-bus-launcher'),'--launch-immediately','--a11y=1','--screen-reader=1'],'bus.log')
    bus=dbus.SessionBus()
    wait(lambda:bus.name_has_owner('org.a11y.Bus'),'private bus launcher ownership')
    address=wait(lambda:str(bus.get_object('org.a11y.Bus','/org/a11y/bus').GetAddress(dbus_interface='org.a11y.Bus')),'private AT-SPI bus')
    assert address.startswith('unix:path='+str(runtime)+'/'),address
    env['AT_SPI_BUS_ADDRESS']=address;os.environ['AT_SPI_BUS_ADDRESS']=address
    status=dict(dbus.Interface(bus.get_object('org.a11y.Bus','/org/a11y/bus'),'org.freedesktop.DBus.Properties').GetAll('org.a11y.Status'))
    assert status.get('IsEnabled') and status.get('ScreenReaderEnabled'),status
    a11y_bus=dbus.bus.BusConnection(address)
    launch([str(a.at_spi_prefix/'libexec/at-spi2-registryd')],'registry.log')
    wait(lambda:a11y_bus.name_has_owner('org.a11y.atspi.Registry'),'private accessibility registry')
    os.environ['GI_TYPELIB_PATH']=str(a.at_spi_prefix/'lib/girepository-1.0')+':'+os.environ.get('GI_TYPELIB_PATH','')
    os.environ['LD_LIBRARY_PATH']=str(a.at_spi_prefix/'lib')+':'+os.environ.get('LD_LIBRARY_PATH','')
    import gi
    gi.require_version('Atspi','2.0')
    from gi.repository import Atspi,GLib
    config=out/'sway.conf';config.write_text('output HEADLESS-1 resolution 1000x1000\nseat seat0 fallback true\n')
    launch([a.sway,'--unsupported-gpu','-c',str(config)],'sway.log')
    env['SWAYSOCK']=str(wait(lambda:next(runtime.glob('sway-ipc.*.sock'),None),'private compositor'))
    env['WAYLAND_DISPLAY']=wait(lambda:next((v.name for v in runtime.glob('wayland-*') if not v.name.endswith('.lock')),None),'private display')
    launch(['wtype','-s','30000'],'keyboard.log');time.sleep(.1)
    qs=a.quickshell or 'quickshell'
    launch([qs,'-p',str(fixture)],'fixture.log')
    def ipc(method):return call(qs,'-p',str(fixture),'ipc','call','a11yFixture',method)
    wait(lambda:json.loads(ipc('status'))['visible'],'fixture window')
    call('wtype','-k','Tab')
    registry=dbus.Interface(a11y_bus.get_object('org.a11y.atspi.Registry','/org/a11y/atspi/registry'),'org.a11y.atspi.Registry')
    from xml.etree import ElementTree
    xml=a11y_bus.get_object('org.a11y.atspi.Registry','/org/a11y/atspi/registry').Introspect(dbus_interface='org.freedesktop.DBus.Introspectable')
    definition=ElementTree.fromstring(str(xml)).find("interface[@name='org.a11y.atspi.Registry']/method[@name='RegisterEvent']")
    inputs=[arg.get('type') for arg in definition.findall('arg') if arg.get('direction','in')=='in']
    values=['object:state-changed']+[[] if kind=='as' else '' for kind in inputs[1:]]
    registry.RegisterEvent(*values,signature=''.join(inputs),timeout=2)
    events=[]
    a11y_bus.add_signal_receiver(lambda *args:events.append(dict(detail=str(args[0]),value=int(args[1]))),signal_name='StateChanged',dbus_interface='org.a11y.atspi.Event.Object')
    desktop=('org.a11y.atspi.Registry','/org/a11y/atspi/accessible/root')
    def interface(ref,name):return dbus.Interface(a11y_bus.get_object(*ref,introspect=False),name)
    def tree():
        rows=[];pending=[desktop];seen=set()
        while pending and len(rows)<500:
            ref=tuple(str(v) for v in pending.pop(0))
            if ref in seen or ref[1]=='/org/a11y/atspi/null':continue
            seen.add(ref)
            try:
                item=interface(ref,'org.a11y.atspi.Accessible')
                name=interface(ref,'org.freedesktop.DBus.Properties').Get('org.a11y.atspi.Accessible','Name',timeout=2)
                states=item.GetState(timeout=2)
                def has(state):
                    bit=int(state);return bool(int(states[bit//32]) & (1<<(bit%32)))
                rows.append((ref,dict(name=str(name),role=str(item.GetRoleName(timeout=2)),focused=has(Atspi.StateType.FOCUSED),checked=has(Atspi.StateType.CHECKED),focusable=has(Atspi.StateType.FOCUSABLE),showing=has(Atspi.StateType.SHOWING),selected=has(Atspi.StateType.SELECTED))))
                pending.extend(item.GetChildren(timeout=2))
            except dbus.DBusException as exc:
                with (out/'protocol-errors.log').open('a') as log:log.write(str(ref)+' '+str(exc)+'\n')
                continue
        return rows
    def named(name):
        rows=tree();(out/'last-tree.json').write_text(json.dumps([row for _,row in rows],indent=2));return next(((item,row) for item,row in rows if row['name']==name),None)
    wait(lambda:named('Focus, Off'),'Focus accessibility object')
    (out/'initial-tree.json').write_text(json.dumps([row for _,row in tree()],indent=2))
    focus,row=named('Focus, Off');assert row['role']=='check box' and not row['checked'],row
    assert interface(focus,'org.a11y.atspi.Component').GrabFocus(timeout=2)
    wait(lambda:named('Focus, Off')[1]['focused'],'Focus keyboard focus')
    call('wtype','-k','space')
    wait(lambda:json.loads(ipc('status'))['focus'],'Space activates Focus')
    wait(lambda:named('Focus, Work')[1]['checked'],'checked accessibility state')
    toggle,row=wait(lambda:named('Show Calendar'),'settings toggle')
    assert row['role']=='check box' and not row['checked'],row
    assert interface(toggle,'org.a11y.atspi.Component').GrabFocus(timeout=2)
    call('wtype','-k','space')
    wait(lambda:named('Show Calendar')[1]['checked'],'settings toggle checked state')
    for label,key in [('Allow critical notifications','allowCritical'),('Chat','allowedApps')]:
        item,row=named(label);assert row['role']=='check box' and not row['checked'],row
        assert interface(item,'org.a11y.atspi.Component').GrabFocus(timeout=2)
        call('wtype','-k','space')
        wait(lambda:named(label)[1]['checked'],'Focus policy checkbox '+label)
    assert json.loads(ipc('status'))['profile']['allowCritical']
    assert json.loads(ipc('status'))['profile']['allowedApps']==['chat']
    wait(lambda:any(v['detail']=='checked' and v['value']==1 for v in events),'checked state event')
    wait(lambda:any(v['detail']=='focused' and v['value']==1 for v in events),'focus state event')
    unavailable,_=named('Unavailable command')
    interface(unavailable,'org.a11y.atspi.Action').DoAction(0,timeout=2)
    assert json.loads(ipc('status'))['disabledPresses']==0
    assert named('For one hour')[1]['role']=='push button'
    assert named('Focus profile name')[1]['role'] in ('text','entry'),named('Focus profile name')[1]
    full_checks=[]
    if a.full_shell:
        ipc('close')
        documents=out/'Documents';documents.mkdir();(documents/'example.txt').write_text('Accessibility fixture document')
        env['LUMEN_SEARCH_ROOTS']=json.dumps([str(documents)])
        helpers=out/'bin';helpers.mkdir();calendar=helpers/'lumen-calendar'
        calendar.write_text('#!'+sys.executable+'\nimport json,sys\nr=json.loads(sys.stdin.readline());print(json.dumps(dict(configured=False,events=[],monthEvents=[],days=[],total=0,providers=[])))\n');calendar.chmod(0o700)
        env['PATH']=str(helpers)+':'+env['PATH']
        launch([qs,'-p',str(source)],'desktop.log')
        def desktop_ipc(method,*values):return call(qs,'-p',str(source),'ipc','call','desktop',method,*values)
        wait(lambda:json.loads(desktop_ipc('status'))['screens'],'full owned desktop')
        desktop_ipc('settings','Appearance')
        search,row=wait(lambda:named('Search settings'),'Settings accessible search')
        assert row['role'] in ('text','entry'),row
        assert interface(search,'org.a11y.atspi.Component').GrabFocus(timeout=2)
        interface(search,'org.a11y.atspi.EditableText').SetTextContents('appearance',timeout=2)
        assert str(interface(search,'org.a11y.atspi.Text').GetText(0,-1,timeout=2))=='appearance'
        full_checks.append('actual Settings search named, editable and focusable through AT-SPI')
        interface(search,'org.a11y.atspi.EditableText').SetTextContents('',timeout=2)
        desktop_ipc('files',str(documents))
        wait(lambda:json.loads(desktop_ipc('filesStatus'))['path']==str(documents),'owned Files fixture')
        view,row=wait(lambda:named('Files view'),'Files accessible view selector')
        assert row['role']=='combo box',row
        assert interface(view,'org.a11y.atspi.Component').GrabFocus(timeout=2)
        call('wtype','-k','Tab')
        wait(lambda:any(row['focused'] for _,row in tree()),'Files real keyboard traversal')
        full_checks.append('actual Files view selector exposes combo-box role and keyboard traversal')
        (out/'owned-windows-tree.json').write_text(json.dumps([row for _,row in tree()],indent=2))
    for log in ['fixture.log']+(['desktop.log'] if a.full_shell else []):
        text=(out/log).read_text()
        assert 'qt.accessibility.atspi:' not in text,text
        assert not any(message in text for message in ('Failed to load configuration','ReferenceError:','TypeError:')),text
    (out/'final-tree.json').write_text(json.dumps([row for _,row in tree()],indent=2))
    (out/'result.json').write_text(json.dumps(dict(passed=True,bus=address,status={str(k):bool(v) for k,v in status.items()},events=events,checks=['private AT-SPI bus','actual roles and names','protocol focus reflected in state','physical Space activates controls','checked state propagated']+full_checks),indent=2))
    print(out/'result.json')
finally:
    for proc in reversed(processes):
        if proc.poll() is None:
            os.killpg(proc.pid,signal.SIGTERM)
            try:proc.wait(timeout=4)
            except sp.TimeoutExpired:os.killpg(proc.pid,signal.SIGKILL)
