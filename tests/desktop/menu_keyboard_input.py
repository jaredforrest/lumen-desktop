"""Native built-in menu capture, keyboard, pointer and AT-SPI regression."""
from fixture_resources import close_reaped_pipes
import argparse,json,os,re,shutil,signal,subprocess as sp,sys,tempfile,time
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--output',type=Path,required=True);p.add_argument('--launcher',required=True);p.add_argument('--sway',default='sway');p.add_argument('--at-spi-prefix',type=Path,default=os.environ.get('LUMEN_TEST_AT_SPI'));p.add_argument('--effects',action='store_true');p.add_argument('--pointer-tool',required=True);p.add_argument('--inside',action='store_true');a=p.parse_args()
if not a.at_spi_prefix:p.error('Provide --at-spi-prefix or use the pinned desktop-test-python runtime')
if not a.inside:
    live=Path('/run/user/'+str(os.getuid())+'/at-spi/bus_0')
    before=live.stat().st_ino if live.exists() else None
    private=tempfile.mkdtemp(prefix='lumen-atspi-');env=dict(os.environ,HOME=private,XDG_RUNTIME_DIR=private,GSETTINGS_BACKEND='memory',QT_ACCESSIBILITY='1',QT_LINUX_ACCESSIBILITY_ALWAYS_ON='1')
    for key in ('CONFIG','STATE','CACHE','DATA'):env['XDG_'+key+'_HOME']=str(Path(private)/key.lower())
    for key in ('AT_SPI_BUS_ADDRESS','NO_AT_BRIDGE','GTK_A11Y','DBUS_SESSION_BUS_ADDRESS','SWAYSOCK','WAYLAND_DISPLAY','DISPLAY'):env.pop(key,None)
    bus_config=Path(private)/'bus.conf';bus_config.write_text('<busconfig><type>session</type><listen>unix:tmpdir='+private+'</listen><policy context="default"><allow own="*"/><allow send_destination="*"/><allow receive_sender="*"/></policy></busconfig>')
    env['DBUS_SYSTEM_BUS_ADDRESS']='unix:path='+str(Path(private)/'no-system-bus')
    try:
        code=sp.call(['dbus-run-session','--config-file='+str(bus_config),'--',sys.executable,__file__,*sys.argv[1:],'--inside'],env=env)
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
env.update(QT_QPA_PLATFORM='wayland',QT_QPA_PLATFORMTHEME='generic',QT_QUICK_BACKEND='software',QT_ACCESSIBILITY='1',QT_LINUX_ACCESSIBILITY_ALWAYS_ON='1',GSETTINGS_BACKEND='memory',WLR_BACKENDS='headless',WLR_RENDERER='gles2' if a.effects else 'pixman',WLR_RENDERER_FORCE_SOFTWARE='1' if a.effects else '0',WLR_RENDERER_ALLOW_SOFTWARE='1',LIBGL_ALWAYS_SOFTWARE='1',WLR_LIBINPUT_NO_DEVICES='1',GLOBAL_MENU_ENABLED='1',LUMEN_FILE_SERVICE='0',LUMEN_LAUNCH_SCOPES='0')
for key in ('NO_AT_BRIDGE','GTK_A11Y','AT_SPI_BUS_ADDRESS','WAYLAND_DISPLAY','SWAYSOCK','DISPLAY'):env.pop(key,None)
processes=[]
def launch(command,name):
    with (out/name).open('w') as stream:
        proc=sp.Popen(command,env=env,stdout=stream,stderr=sp.STDOUT,start_new_session=True)
    processes.append(proc);return proc

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
assets=out/'assets';shutil.copytree(Path(env['LUMEN_NOTIFICATION_HELPER']).parent.parent,assets)
source=assets/'shell';fixture=source
bridge=source/'GlobalMenu.qml';bridge.chmod(0o600);bridge.write_text(bridge.read_text().replace('id: bridge','id: bridge\n        stderr:SplitParser {onRead:line=>console.warn("Bridge fixture: ",line)}',1))
(source/'shell.qml').chmod(0o600)
text=(source/'shell.qml').read_text()
text=text.replace('SettingsState {id:settingsState;active:settingsWindow.visible}','SettingsState {id:settingsState;active:false}',1)
text=text.replace('function sessionAction(action){claimPopup("");sessionOptions.show(action);focusOwnedWindow("Session Options");}', 'function sessionAction(action){throw Error("Unexpected session command in keyboard fixture");}')
head,tail=text.rsplit('}',1)
text=head+'''
 IpcHandler {target:"menuAudit"
  function open(name:string):void{const bar=root.bars["HEADLESS-1"];bar.selectedWindow=root.active;bar.toggle(name,16);}
  function clear():void{}
  function status():string {const b=root.bars["HEADLESS-1"];return JSON.stringify({ready:!!b,menu:b?b.menu:"",layout:b?b.popupLayout():null,active:root.active?root.active.appId:"",bridge:root.exportedMenu.state,latestBridge:root.exportedMenu.latestState,headingLayout:b?b.menuLayoutStatus():null,hiddenAnchor:b?b.exportX(37):0,exportPath:root.exportedMenu.path,exportTarget:root.exportedMenu.invokingContext && root.exportedMenu.invokingContext.window?root.exportedMenu.invokingContext.window.title:"",highlighted:root.exportedMenu.highlighted,selection:root.exportedMenu.selection?{id:root.exportedMenu.selection.id,revision:root.exportedMenu.selection.revision}:null,target:b && b.menuWindow?b.menuWindow.title:"",filesContext:b?b.filesFocused:false,windows:root.windows.map(w=>({app:w.appId,title:w.title}))});}
 }
}'''+tail
(source/'shell.qml').write_text(text)
palette=json.loads((assets/'appearance/palette.json').read_text());appearance=Path(env['XDG_STATE_HOME'])/'lumen-desktop/appearance.json';appearance.parent.mkdir(parents=True,exist_ok=True);appearance.write_text(json.dumps(dict(font=palette['font'],palette=palette['dark'],preferences={})))
try:
    launch([str(a.at_spi_prefix/'libexec/at-spi-bus-launcher'),'--launch-immediately','--a11y=1','--screen-reader=1'],'bus.log')
    bus=dbus.SessionBus();wait(lambda:bus.name_has_owner('org.a11y.Bus'),'private accessibility bus')
    address=str(bus.get_object('org.a11y.Bus','/org/a11y/bus').GetAddress(dbus_interface='org.a11y.Bus'))
    assert address.startswith('unix:path='+str(runtime)+'/'),address
    env['AT_SPI_BUS_ADDRESS']=address;os.environ['AT_SPI_BUS_ADDRESS']=address
    a11y_bus=dbus.bus.BusConnection(address);launch([str(a.at_spi_prefix/'libexec/at-spi2-registryd')],'registry.log')
    wait(lambda:a11y_bus.name_has_owner('org.a11y.atspi.Registry'),'private registry')
    import gi
    gi.require_version('Atspi','2.0')
    from gi.repository import Atspi,GLib
    config=out/'sway.conf';config.write_text('output HEADLESS-1 resolution 1280x1000\nseat seat0 fallback true\nxwayland enable\nfocus_follows_mouse no\nfor_window [all] floating enable\n')
    launch([a.sway,'--unsupported-gpu','-c',str(config)],'sway.log')
    env['SWAYSOCK']=str(wait(lambda:next(runtime.glob('sway-ipc*.sock'),None),'private compositor'))
    env['WAYLAND_DISPLAY']=wait(lambda:next((v.name for v in runtime.glob('wayland-*') if not v.name.endswith('.lock')),None),'private display')
    launch(['wtype','-s','60000'],'keyboard.log')
    launch(['quickshell','-p',str(fixture)],'shell.log')
    def ipc(method,*args):return call('quickshell','-p',str(fixture),'ipc','call','menuAudit',method,*args)
    def state():
        value=json.loads(ipc('status'));(out/'last-state.json').write_text(json.dumps(value,indent=2));return value
    wait(lambda:state()['ready'],'owned MenuBar')
    clients={}
    def client(app):
        process=launch(['foot','--app-id='+app,'--title='+app,'-e','sh','-c','cat > "$1"','fixture',str(out/(app+'-input.txt'))],app+'.log');clients[app]=process
        wait(lambda:any(w['app']==app for w in state()['windows']),app);return process
    def stop(app):
        os.killpg(clients[app].pid,signal.SIGTERM);clients[app].wait(timeout=5)
        wait(lambda:not any(w['app']==app for w in state()['windows']),'removed '+app)
    def key(*keys):call('wtype',*keys);time.sleep(.12)
    def open_menu(name,app):
        call('swaymsg','[app_id="'+app+'"]','focus');wait(lambda:state()['active']==app and not state()['filesContext'] and state()['bridge'].get('appName')==app and state()['bridge'].get('windowId',0)>0,'native focus and installed bridge watcher')
        ipc('open',name);wait(lambda:state()['menu']==name,'menu '+name);ipc('clear')
    client('menu-source')
    open_menu('Desktop','menu-source');key('-k','Down','-k','Right')
    wait(lambda:state()['menu']=='Application','Right moves from Desktop to Application heading')
    key('-k','Right');wait(lambda:state()['menu']=='Window','Right moves to native Window heading')
    key('-k','Left');wait(lambda:state()['menu']=='Application','Left returns to Application heading')
    key('-k','Left');wait(lambda:state()['menu']=='Desktop','Left returns to Desktop heading')
    key('-k','Escape');wait(lambda:state()['menu']=='','Escape dismisses heading keyboard session')
    checks=['Actual owned Desktop/Application/Window heading traversal with arrows and Escape']
    # Actual AT-SPI object references, not a QML model inspection.
    desktop=('org.a11y.atspi.Registry','/org/a11y/atspi/accessible/root')
    def interface(ref,name):return dbus.Interface(a11y_bus.get_object(ref[0],ref[1],introspect=False),name)
    def tree(until=None):
        pending=[desktop];seen=set();rows=[]
        while pending and len(rows)<1500:
            ref=tuple(str(x) for x in pending.pop(0))
            if ref in seen or ref[1]=='/org/a11y/atspi/null':continue
            seen.add(ref)
            try:
                item=interface(ref,'org.a11y.atspi.Accessible');props=interface(ref,'org.freedesktop.DBus.Properties');states=item.GetState(timeout=2)
                def has(flag):n=int(flag);return bool(int(states[n//32])&(1<<(n%32)))
                rows.append((ref,dict(name=str(props.Get('org.a11y.atspi.Accessible','Name',timeout=2)),role=str(item.GetRoleName(timeout=2)),showing=has(Atspi.StateType.SHOWING),enabled=has(Atspi.StateType.ENABLED),focused=has(Atspi.StateType.FOCUSED))))
                if until and rows[-1][1]['name']==until and rows[-1][1]['showing']:break
                pending.extend(item.GetChildren(timeout=2))
            except dbus.DBusException:pass
        (out/'last-tree.json').write_text(json.dumps([row for _,row in rows],indent=2));return rows
    def named(name):return next(((ref,row) for ref,row in tree(name) if row['name']==name and row['showing']),None)
    pointer=sp.Popen([a.pointer_tool,'pointer','move','0','0'],env=env,stdin=sp.PIPE,text=True,start_new_session=True);processes.append(pointer)
    def pointer_command(command):pointer.stdin.write(command+'\n');pointer.stdin.flush();time.sleep(.12)
    def point(ref):
        rectangle=interface(ref,'org.a11y.atspi.Component').GetExtents(0,timeout=2)
        x,y,width,height=map(int,rectangle);assert width>0 and height>0,rectangle
        call('swaymsg','seat','seat0','cursor','set',str(x+width//2),str(y+height//2));pointer_command('m 0.5 0.5')
    def enabled_named(name):
        found=named(name);return found if found and found[1]['enabled'] else None
    import shlex
    display_file=out/'xdisplay'
    command='from pathlib import Path;import os;Path('+repr(str(display_file))+').write_text(os.environ.get("DISPLAY",""))'
    call('swaymsg','exec',shlex.quote(sys.executable)+' -c '+shlex.quote(command))
    wait(lambda:display_file.exists() and display_file.read_text(),'private XWayland display')
    env['DISPLAY']=display_file.read_text();env['GDK_BACKEND']='x11'
    exporter=launch([sys.executable,str(Path(__file__).parent/'fixtures/menu_exporter.py'),'--output',str(out)],'exporter.log')
    wait(lambda:bus.name_has_owner('org.lumen.TestMenuExporter'),'same-process private exporter')
    control=dbus.Interface(bus.get_object('org.lumen.TestMenuExporter','/Menu'), 'org.lumen.TestMenuExporter')
    def exporter_state():return json.loads(control.Status(timeout=2))
    def native():
        def nodes(n):
            yield n
            for child in n.get('nodes',[])+n.get('floating_nodes',[]):yield from nodes(child)
        return next((n for n in nodes(json.loads(call('swaymsg','-t','get_tree'))) if n.get('name')=='Menu Exporter Fixture'),None)
    wait(native,'actual exporter window');call('swaymsg','[title="Menu Exporter Fixture"]','focus')
    wait(lambda:state()['bridge'].get('supported') and state()['bridge'].get('revision')==1,'installed registrar and exporter layout')
    identity=native();assert identity['pid']==exporter.pid==exporter_state()['pid'] and identity['window']==exporter_state()['xid'],(identity,exporter_state())
    checks.append('Actual same-PID private XWayland window, registrar identity and dynamic DBusMenu transport')
    def events():return [json.loads(line) for line in (out/'events.jsonl').read_text().splitlines()] if (out/'events.jsonl').exists() else []
    def exported_open():
        for _ in range(4):
            if not state()['exportPath']:break
            key('-k','Escape')
        if state()['menu']:key('-k','Escape')
        call('swaymsg','[title="Menu Exporter Fixture"]','focus');wait(lambda:state()['bridge'].get('supported') and state()['bridge'].get('revision')==exporter_state()['revision'],'captured exporter focus')
        call('quickshell','-p',str(fixture),'ipc','call','globalmenu','open','10');wait(lambda:state()['exportPath']==[10],'exported File root')
    exported_open();key('-k','Down');wait(lambda:state()['selection'] and state()['selection']['id']==13,'skip disabled and separator')
    checked,info=wait(lambda:named('Checked Command'),'semantic checked item');assert info['role']=='menu item' and info['enabled'],info
    checked_states=interface(checked,'org.a11y.atspi.Accessible').GetState(timeout=2);flag=int(Atspi.StateType.CHECKED);assert int(checked_states[flag//32])&(1<<(flag%32))
    description=interface(checked,'org.freedesktop.DBus.Properties').Get('org.a11y.atspi.Accessible','Description',timeout=2);assert 'K' in str(description)
    unavailable,info=wait(lambda:named('Unavailable'),'semantic unavailable row');assert not info['enabled']
    key('-k','Right');wait(lambda:state()['exportPath']==[20],'Right skips disabled heading')
    key('-k','Left');wait(lambda:state()['exportPath']==[10],'Left returns enabled heading')
    key('-k','Left');wait(lambda:state()['menu']=='Application' and not state()['exportPath'],'exported to owned application heading');assert state()['target']=='Menu Exporter Fixture',state()
    key('-k','Right');wait(lambda:state()['exportPath']==[10],'owned heading returns to captured exporter');assert events()==[]
    checks.append('Physical heading ring spans owned and exported headings, skips disabled items and retains invoking window')
    key('-k','Left');wait(lambda:state()['menu']=='Application','captured owned heading before exporter change');revision=state()['bridge']['revision'];control.Change('revision',timeout=2);wait(lambda:state()['latestBridge'].get('revision')==revision+1,'latest exporter generation while display held');key('-k','Right');wait(lambda:not state()['menu'] and not state()['exportPath'],'changed exporter heading context cancels instead of retargeting');assert events()==[]
    checks.append('Exporter revision change while an owned heading holds display cancels captured heading traversal')
    exported_open();key('-k','Down')
    # Select nested row from the current root without relying on prior focus.
    for _ in range(6):
        if state()['selection'] and state()['selection']['id']==14:break
        key('-k','Down')
    assert state()['selection'] and state()['selection']['id']==14,state()
    key('-k','Right');wait(lambda:state()['exportPath']==[10,14] and state()['selection'] and state()['selection']['id']==141,'Right enters child')
    key('-k','Left');wait(lambda:state()['exportPath']==[10] and state()['selection'] and state()['selection']['id']==14,'Left restores parent selection')
    key('-k','Right');wait(lambda:state()['exportPath']==[10,14],'re-enter child');key('-k','Escape');wait(lambda:state()['exportPath']==[10],'Escape closes one child level');key('-k','Escape');wait(lambda:not state()['exportPath'],'Escape dismisses root')
    before=len(exporter_state()['keys']);key('-k','k');wait(lambda:len(exporter_state()['keys'])==before+1,'exact original client input after nested Escape')
    assert events()==[];checks.append('Nested Right/Left and one-level Escape retain parent selection and restore original native input')
    call('swaymsg','output','HEADLESS-1','resolution','640x800');wait(lambda:state()['headingLayout']['width']==640 and state()['headingLayout']['visible']<state()['headingLayout']['headings'],'narrow output overflows headings');exported_open()
    for heading in [20]+list(range(30,38)):
        key('-k','Right');wait(lambda:state()['exportPath']==[heading],'keyboard reaches hidden heading '+str(heading))
    layout=json.loads(call('quickshell','-p',str(fixture),'ipc','call','globalmenu','layout'));assert len(layout)==1 and 0<=layout[0]['x']<=640-layout[0]['width'] and state()['hiddenAnchor']>0,(layout,state())
    call('grim',str(out/'overflow-menu.png'));key('-k','Escape')
    wait(lambda:state()['active']=='Menu_exporter.py','native invocation context before pointer overflow');more,info=wait(lambda:named('More menus and status controls'),'actual overflow heading button');point(more);pointer_command('p');pointer_command('r');wait(lambda:state()['menu']=='Overflow','pointer overflow menu')
    hidden,info=wait(lambda:named('Additional Heading 8'),'semantic overflow menu row');revision=state()['bridge']['revision'];control.Change('revision',timeout=2);wait(lambda:state()['latestBridge'].get('revision')==revision+1,'held overflow observes new exporter revision');assert interface(hidden,'org.a11y.atspi.Action').DoAction(0,timeout=2);wait(lambda:not state()['menu'] and not state()['exportPath'],'stale overflow heading activation cancels');assert events()==[]
    call('swaymsg','[title="Menu Exporter Fixture"]','focus');wait(lambda:state()['bridge'].get('revision')==exporter_state()['revision'],'fresh overflow invoking context');more,info=wait(lambda:named('More menus and status controls'),'fresh overflow button');point(more);pointer_command('p');pointer_command('r');wait(lambda:state()['menu']=='Overflow','fresh pointer overflow');hidden,info=wait(lambda:named('Additional Heading 8'),'fresh semantic overflow row');assert interface(hidden,'org.a11y.atspi.Action').DoAction(0,timeout=2);wait(lambda:state()['exportPath']==[37],'captured pointer overflow transition');assert state()['exportTarget']=='Menu Exporter Fixture',state()
    for _ in range(10):key('-k','Left')
    wait(lambda:state()['menu']=='Application','overflow child returns through heading ring');assert state()['target']=='Menu Exporter Fixture';key('-k','Escape')
    call('swaymsg','output','HEADLESS-1','resolution','1280x1000');wait(lambda:state()['headingLayout']['width']==1280,'restore private output');assert events()==[]
    checks.append('Physical arrows reach enabled overflow headings on a narrow output with an actual on-screen anchor')
    exported_open();key('-k','Down');wait(lambda:state()['selection'] and state()['selection']['id']==13,'captured same-ID row')
    stale,row=wait(lambda:named('Checked Command'),'old accessibility generation');revision=state()['bridge']['revision'];control.Change('revision',timeout=2)
    wait(lambda:state()['bridge']['revision']==revision+1 and state()['selection'] is None,'same-ID unchanged-label generation invalidates selection')
    key('-k','Return');assert events()==[]
    try:interface(stale,'org.a11y.atspi.Action').DoAction(0,timeout=2)
    except dbus.DBusException:pass
    assert events()==[];checks.append('Real same-ID unchanged-label revision invalidates keyboard and stale accessibility actions')
    checked,row=wait(lambda:named('Checked Command'),'fresh row');point(checked);pointer_command('p');revision=state()['bridge']['revision'];control.Change('disable',timeout=2);wait(lambda:state()['bridge']['revision']==revision+1,'held row disabled');pointer_command('r');assert events()==[]
    checks.append('Real pointer press followed by exporter update cannot activate a replaced disabled row')
    key('-k','Escape');exported_open();key('-k','Down');wait(lambda:state()['selection'] and state()['selection']['id']==14,'disabled command skipped');key('-k','Right');wait(lambda:state()['exportPath']==[10,14],'deep semantic menu')
    deep,info=wait(lambda:named('Deep Command'),'actual deep semantic action');assert info['role']=='menu item'
    from PIL import Image,ImageStat
    def presented():
        layout=json.loads(call('quickshell','-p',str(fixture),'ipc','call','globalmenu','layout'));call('grim',str(out/'nested-menu.png'))
        with Image.open(out/'nested-menu.png') as image:
            return len(layout)==2 and all(max(ImageStat.Stat(image.convert('RGB').crop((round(c['x']),round(c['y']),round(c['x']+c['width']),round(c['y']+c['height'])))).stddev)>8 for c in layout)
    wait(presented,'actually painted parent and nested menu cards')
    assert interface(deep,'org.a11y.atspi.Action').DoAction(0,timeout=2)
    wait(lambda:events() and events()[-1]['id']==141 and not state()['exportPath'],'captured deep command once');assert len(events())==1,events()
    checks.append('Assistive submenu command dispatches exactly once to the captured exporter')
    exported_open();call('swaymsg','seat','seat0','cursor','set','1100','700');pointer_command('m 0.5 0.5');pointer_command('p');pointer_command('r');wait(lambda:not state()['exportPath'],'outside click dismissal');assert len(events())==1
    exported_open();key('-k','Down');old_token=state()['bridge']['token'];control.HideWindow(timeout=2);wait(lambda:native() is None and not state()['exportPath'],'native window loss while exporter remains alive');assert exporter.poll() is None and bus.name_has_owner('org.lumen.TestMenuExporter');key('-k','Return');assert len(events())==1
    control.ShowWindow(timeout=2);wait(lambda:native() and native()['pid']==exporter.pid,'same process remapped native window');call('swaymsg','[title="Menu Exporter Fixture"]','focus');wait(lambda:state()['bridge'].get('supported') and state()['bridge']['token']!=old_token,'new native window generation in same exporter process')
    checks.append('Unmapping the invoking window cancels its menu even while the exporter stays alive; remapping requires a new native token')
    exported_open();key('-k','Down');stale_nested,info=wait(lambda:named('Nested'),'captured exporter generation');old_token=state()['bridge']['token'];os.killpg(exporter.pid,signal.SIGTERM);exporter.wait(timeout=5);wait(lambda:not state()['bridge'].get('supported') and not state()['exportPath'],'exporter death cancels menu');key('-k','Return');assert len(events())==1
    exporter=launch([sys.executable,str(Path(__file__).parent/'fixtures/menu_exporter.py'),'--output',str(out)],'replacement-exporter.log');wait(lambda:native() and native()['pid']==exporter.pid,'replacement actual window');call('swaymsg','[title="Menu Exporter Fixture"]','focus');wait(lambda:state()['bridge'].get('supported') and state()['bridge']['token']!=old_token,'replacement owner and native identity');control=dbus.Interface(bus.get_object('org.lumen.TestMenuExporter','/Menu'),'org.lumen.TestMenuExporter')
    try:interface(stale_nested,'org.a11y.atspi.Action').DoAction(0,timeout=2)
    except dbus.DBusException:pass
    assert len(events())==1 and not state()['exportPath']
    exported_open();fresh,info=wait(lambda:named('Checked Command'),'replacement semantic row');assert interface(fresh,'org.a11y.atspi.Component').GrabFocus(timeout=2);wait(lambda:named('Checked Command')[1]['focused'],'actual assistive row focus');key('-k','Return')
    wait(lambda:len(events())==2 and events()[-1]['id']==13 and events()[-1]['pid']==exporter.pid,'Return dispatches once to replacement captured generation')
    checks.append('Outside click and exporter/window replacement cancel stale intents; fresh assistive focus plus Return dispatches once to the exact new owner')
    call('grim',str(out/'accepted.png'))
    text=(out/'shell.log').read_text();assert not any(x in text for x in ('TypeError:','ReferenceError:','Binding loop','Failed to load configuration','Cannot assign')),text
    (out/'result.json').write_text(json.dumps(dict(passed=True,checks=checks,source='Installed shell and real private same-PID DBusMenu XWayland exporter; no unexported app parity claim',initialExporter={'pid':identity['pid'],'xid':identity['window']},replacementExporter=exporter_state()),indent=2)+'\n')

finally:
    for proc in reversed(processes):
        if proc.poll() is None:
            try:os.killpg(proc.pid,signal.SIGTERM)
            except ProcessLookupError:pass
    for proc in reversed(processes):
        try:proc.wait(timeout=3)
        except sp.TimeoutExpired:os.killpg(proc.pid,signal.SIGKILL);proc.wait(timeout=3)
    if 'pointer' in globals() and pointer.stdin:pointer.stdin.close()
    for _fixture_process in processes:
        if _fixture_process.poll() is None:_fixture_process.wait(timeout=5)
    close_reaped_pipes(processes)
