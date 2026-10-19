"""Native built-in menu capture, keyboard, pointer and AT-SPI regression."""
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
for expected in ('SettingsState {id:settingsState;active:settingsWindow.visible}', 'function sessionAction(action){claimPopup("");sessionOptions.show(action);focusOwnedWindow("Session Options");}', 'function filesAction(action){', 'function focusWindow(window) {'):
    assert text.count(expected)==1,('Fixture safety/instrumentation boundary changed',expected)
text=text.replace('SettingsState {id:settingsState;active:settingsWindow.visible}','SettingsState {id:settingsState;active:false}',1)
text=text.replace('function sessionAction(action){claimPopup("");sessionOptions.show(action);focusOwnedWindow("Session Options");}', 'function sessionAction(action){menuFixtureSessions=menuFixtureSessions.concat([action]);}')
text=text.replace('function filesAction(action){', 'function filesAction(action){menuFixtureFileActions=menuFixtureFileActions.concat([{action:action,target:root.filesWindow===root.menuFixtureFirstFiles?"first":"second"}]);',1)
text=text.replace('function focusWindow(window) {','function focusWindow(window) {\n        if(window)menuFixtureFocus=menuFixtureFocus.concat([window.appId]);',1)
head,tail=text.rsplit('}',1)
text=head+'''
 property var menuFixtureFocus:[]
 property var menuFixtureSessions:[]
 property var menuFixtureFileActions:[]
 property var menuFixtureFirstFiles:null
 property var menuFixtureSecondFiles:null
 IpcHandler {target:"menuAudit"
  function open(name:string):void{const bar=root.bars["HEADLESS-1"];bar.selectedWindow=root.active;bar.toggle(name,16);}
  function retarget(app:string):void{root.bars["HEADLESS-1"].selectedWindow=root.windows.find(w=>w.appId===app);}
  function status():string {const b=root.bars["HEADLESS-1"];return JSON.stringify({ready:!!b,menu:b?b.menu:"",layout:b?b.popupLayout():null,focuses:root.menuFixtureFocus,sessions:root.menuFixtureSessions,active:root.active?root.active.appId:"",bridge:root.exportedMenu.state,filesContext:b?b.filesFocused:false,fileActions:root.menuFixtureFileActions,windows:root.windows.map(w=>({app:w.appId,title:w.title}))});}
  function clear():void{root.menuFixtureFocus=[];root.menuFixtureSessions=[];root.menuFixtureFileActions=[];}
  function filesStart():void{root.openFiles(Quickshell.env("HOME"));root.menuFixtureFirstFiles=root.filesWindow;}
  function filesSecond():void{root.newFilesWindow(Quickshell.env("HOME"));root.menuFixtureSecondFiles=root.filesWindow;}
  function filesRetarget():void{root.filesWindow=root.menuFixtureFirstFiles;}
  function filesFocusFirst():void{root.filesWindow=root.menuFixtureFirstFiles;root.filesWindow.openPath(Quickshell.env("HOME"));}
  function settingsStart():void{settingsWindow.visible=true;}
  function replaceAssociationEpoch():void{root.ownedToplevelAssociations=root.ownedToplevelAssociations.map(record=>Object.assign({},record));}
  function filesStatus():string{return JSON.stringify({settingsVisible:settingsWindow.visible,firstVisible:root.menuFixtureFirstFiles?root.menuFixtureFirstFiles.visible:false,secondVisible:root.menuFixtureSecondFiles?root.menuFixtureSecondFiles.visible:false,actions:root.menuFixtureFileActions,epoch:root.focusEpoch,menuEpoch:root.bars["HEADLESS-1"].menuFocusEpoch,same:root.bars["HEADLESS-1"].menuFilesWindow===root.filesWindow,visible:root.filesWindow.visible,backendReady:root.filesState.ready,busy:root.filesState.busy,exported:root.exportedMenu.state,expectedTitle:root.filesWindow.title,key:root.ownedContext.keyRole,ready:root.filesWindow.visible && root.filesState.ready && !root.filesState.busy && root.exportedMenu.state.title===root.filesWindow.title && root.exportedMenu.state.appName==="org.quickshell",first:root.menuFixtureFirstFiles?root.menuFixtureFirstFiles.listMode:false,second:root.menuFixtureSecondFiles?root.menuFixtureSecondFiles.listMode:false,focused:root.bars["HEADLESS-1"].filesFocused});}
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
    config=out/'sway.conf';config.write_text('output HEADLESS-1 resolution 1280x1000\nseat seat0 fallback true\nxwayland disable\nfocus_follows_mouse no\nfor_window [all] floating enable\n')
    launch([a.sway,'--unsupported-gpu','-c',str(config)],'sway.log')
    env['SWAYSOCK']=str(wait(lambda:next(runtime.glob('sway-ipc*.sock'),None),'private compositor'))
    env['WAYLAND_DISPLAY']=wait(lambda:next((v.name for v in runtime.glob('wayland-*') if not v.name.endswith('.lock')),None),'private display')
    launch(['wtype','-s','60000'],'keyboard.log')
    launch(['quickshell','-p',str(fixture)],'shell.log')
    def ipc(method,*args):return call('quickshell','-p',str(fixture),'ipc','call','menuAudit',method,*args)
    def state():
        value=json.loads(ipc('status'));(out/'last-state.json').write_text(json.dumps(value,indent=2));return value
    def files_state():
        value=json.loads(ipc('filesStatus'));(out/'last-files-state.json').write_text(json.dumps(value,indent=2));return value
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
    client('menu-source');client('menu-bystander')
    open_menu('Application','menu-source');key('-k','Down','-k','Down')
    ipc('retarget','menu-bystander');key('-k','Return')
    wait(lambda:state()['menu']=='','command dismissal')
    value=state();assert any(w['app']=='menu-bystander' for w in value['windows']),value
    wait(lambda:not any(w['app']=='menu-source' for w in state()['windows']),'captured close source')
    checks=['Installed bridge imports the shared runtime and watches actual native focus', 'Physical Return closes the captured original window despite mutable selectedWindow replacement; bystander remains']
    # Actual AT-SPI object references, not a QML model inspection.
    desktop=(':1.0','/org/a11y/atspi/accessible/root')
    registry=dbus.Interface(a11y_bus.get_object('org.a11y.atspi.Registry','/org/a11y/atspi/registry'),'org.a11y.atspi.Registry')
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
    client('menu-held');open_menu('Application','menu-held')
    close,row=wait(lambda:named('Close Window'),'pointer close row');point(close);pointer_command('p')
    ipc('retarget','menu-bystander');pointer_command('r')
    wait(lambda:not any(w['app']=='menu-held' for w in state()['windows']),'pointer captured close')
    assert any(w['app']=='menu-bystander' for w in state()['windows'])
    checks.append('Actual pointer press and release retain the original window rather than mutable selection')
    client('menu-held');open_menu('Application','menu-held')
    close,row=wait(lambda:named('Close Window'),'held doomed close row');point(close);pointer_command('p')
    stop('menu-held');client('menu-held');pointer_command('r')
    assert any(w['app']=='menu-held' for w in state()['windows']) and state()['focuses']==[]
    key('-k','Escape');wait(lambda:state()['menu']=='','held cancellation Escape')
    checks.append('Destroyed pressed row cancels and cannot close a newly created same-app replacement')
    client('menu-doomed');client('menu-other')
    open_menu('Window','menu-bystander')
    doomed,role=wait(lambda:named('menu-doomed'),'semantic native window row');assert role['role']=='menu item' and role['enabled'],role
    # Explicit key traversal to a captured row, then remove that row before Return.
    for _ in range(12):
        key('-k','Down')
        if named('menu-doomed')[1]['focused']:break
    assert named('menu-doomed')[1]['focused']
    stop('menu-doomed');key('-k','Return');assert state()['focuses']==[],state()
    try:interface(doomed,'org.a11y.atspi.Action').DoAction(0,timeout=2)
    except dbus.DBusException:pass
    assert state()['focuses']==[] and any(w['app']=='menu-other' for w in state()['windows'])
    checks.append('Removed keyboard row and destroyed AT-SPI reference cannot retarget the replacement index')
    key('-k','Escape');wait(lambda:state()['menu']=='','Escape dismissal')
    open_menu('Window','menu-bystander');other,row=wait(lambda:named('menu-other'),'remaining native row')
    assert interface(other,'org.a11y.atspi.Component').GrabFocus(timeout=2)
    wait(lambda:named('menu-other')[1]['focused'],'assistive focus selection')
    key('-k','Return');wait(lambda:state()['focuses']==['menu-other'] and state()['active']=='menu-other' and state()['bridge'].get('appName')=='menu-other' and state()['menu']=='','Return after actual assistive focus once')
    open_menu('Window','menu-bystander');other,row=wait(lambda:named('menu-other'),'fresh assistive row')
    assert interface(other,'org.a11y.atspi.Action').DoAction(0,timeout=2)
    wait(lambda:state()['focuses']==['menu-other'] and state()['active']=='menu-other' and state()['bridge'].get('appName')=='menu-other','assistive exact native focus once');assert state()['menu']==''
    checks.append('Actual assistive menu-item action focuses its captured live window once')
    open_menu('Desktop','menu-bystander');key('-k','Home');assert state()['layout']['selectedIndex']==0
    key('-k','Tab');assert state()['layout']['selectedIndex']==1
    key('-k','Tab');assert state()['layout']['selectedIndex']==3
    key('-k','ISO_Left_Tab');assert state()['layout']['selectedIndex']==1
    key('-k','End');key('-k','Up')
    logout,row=wait(lambda:named('Log Out…'),'session stub row');assert row['role']=='menu item'
    assert interface(logout,'org.a11y.atspi.Action').DoAction(0,timeout=2)
    wait(lambda:state()['sessions']==['logout'],'session fixture action once')
    checks.append('Home End Tab Backtab and arrows skip separators; session action reaches fixture stub exactly once')
    open_menu('Desktop','menu-bystander');logout,row=wait(lambda:named('Log Out…'),'physical session stub row')
    point(logout);pointer_command('p');pointer_command('r');wait(lambda:state()['sessions']==['logout'],'pointer session stub once')
    try:interface(logout,'org.a11y.atspi.Action').DoAction(0,timeout=2)
    except dbus.DBusException:pass
    assert state()['sessions']==['logout']
    checks.append('Physical session activation dispatches once and closed/destroyed menu references cannot repeat it')
    # Owned Files actions retain the exact main window and focus epoch.
    ipc('filesStart');wait(lambda:files_state()['ready'],'first private Files')
    ipc('open','File');ipc('open','View');wait(lambda:files_state()['focused'],'Files menu context after heading change')
    listing,row=wait(lambda:enabled_named('as List'),'owned Files view command');assert interface(listing,'org.a11y.atspi.Action').DoAction(0,timeout=2)
    wait(lambda:files_state()['first'],'owned Files list view')
    ipc('filesSecond');wait(lambda:files_state()['ready'],'second private Files')
    ipc('open','View');icons,row=wait(lambda:enabled_named('as Icons'),'captured second Files command')
    # Use a fresh keyboard intent, then replace the owned target while open.
    before_views=files_state();ipc('clear');key('-k','Home','-k','Down');ipc('filesRetarget');key('-k','Return')
    after_views=files_state();assert (after_views['first'],after_views['second'])==(before_views['first'],before_views['second']) and state()['fileActions']==[]
    key('-k','Escape')
    checks.append('Owned Files legitimate view command works and a changed main-window context cancels captured menu input')
    ipc('settingsStart');wait(lambda:any(w['title']=='System Settings' for w in state()['windows']),'actual Settings bystander')
    call('swaymsg','[title="System Settings"]','focus');wait(lambda:state()['bridge'].get('title')=='System Settings','Settings bridge context')
    ipc('open','Application');unknown,row=wait(lambda:named('Close All Windows'),'unknown owned close-all row');assert not row['enabled']
    try:interface(unknown,'org.a11y.atspi.Action').DoAction(0,timeout=2)
    except dbus.DBusException:pass
    assert files_state()['settingsVisible'] and files_state()['firstVisible'] and files_state()['secondVisible']
    key('-k','Escape')
    ipc('filesFocusFirst');wait(lambda:files_state()['ready'],'exact owned Files context')
    ipc('open','Application');close_all,row=wait(lambda:enabled_named('Close All Windows'),'explicit owned close-all group')
    point(close_all);pointer_command('p');ipc('replaceAssociationEpoch');pointer_command('r')
    assert files_state()['firstVisible'] and files_state()['secondVisible'] and files_state()['settingsVisible']
    close_all,row=wait(lambda:enabled_named('Close All Windows'),'fresh owned group epoch')
    point(close_all);pointer_command('p');pointer_command('r')
    wait(lambda:not files_state()['firstVisible'] and not files_state()['secondVisible'],'captured Files group close')
    assert files_state()['settingsVisible'] and any(w['title']=='System Settings' for w in state()['windows'])
    checks.append('Explicit Files association scopes Close All, replacement association epoch cancels held input, unknown Settings grouping stays disabled and Settings survives')
    open_menu('Edit','menu-bystander');key('-k','Down','-k','Return');assert state()['layout']['selectedIndex']==-1
    key('-k','Escape');wait(lambda:state()['menu']=='','disabled menu Escape');assert state()['focuses']==[]
    key('menu-focus-restored','-k','Return');wait(lambda:(out/'menu-bystander-input.txt').exists() and 'menu-focus-restored' in (out/'menu-bystander-input.txt').read_text(),'physical ordinary client input after Escape')
    checks.append('Disabled commands have no keyboard activation and Escape restores ordinary native focus')
    text=(out/'shell.log').read_text();assert not any(x in text for x in ('TypeError:','ReferenceError:','Binding loop','Failed to load configuration','Cannot assign')),text
    (out/'result.json').write_text(json.dumps(dict(passed=True,checks=checks,source='Installed owned shell components with fixture-only session stub'),indent=2)+'\n')
    print('PASS native built-in menu capture, semantics and keyboard')
finally:
    for proc in reversed(processes):
        if proc.poll() is None:
            try:os.killpg(proc.pid,signal.SIGTERM)
            except ProcessLookupError:pass
    for proc in reversed(processes):
        try:proc.wait(timeout=3)
        except sp.TimeoutExpired:os.killpg(proc.pid,signal.SIGKILL);proc.wait(timeout=3)
