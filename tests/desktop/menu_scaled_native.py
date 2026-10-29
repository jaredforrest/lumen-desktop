"""Native built-in menu capture, keyboard, pointer and AT-SPI regression."""
from fixture_resources import close_reaped_pipes
import argparse,json,os,re,shutil,signal,subprocess as sp,sys,tempfile,time
from pathlib import Path
from fixture_providers import isolate_providers
p=argparse.ArgumentParser();p.add_argument('--output',type=Path,required=True);p.add_argument('--launcher',required=True);p.add_argument('--sway',default='sway');p.add_argument('--at-spi-prefix',type=Path,default=os.environ.get('LUMEN_TEST_AT_SPI'));p.add_argument('--effects',action='store_true');p.add_argument('--pointer-tool',required=True);p.add_argument('--inside',action='store_true');p.add_argument('--xwayland',type=Path,required=True);p.add_argument('--keyboard',type=Path,required=True);p.add_argument('--audio',type=Path,required=True);p.add_argument('--font-config',type=Path,required=True);p.add_argument('--software-driver',type=Path,required=True);a=p.parse_args()
if not a.at_spi_prefix:p.error('Provide --at-spi-prefix or use the pinned desktop-test-python runtime')
repo=Path(__file__).resolve().parents[2]
from application_fixture.isolation import run
from application_fixture.provenance import FrozenSource
from private_native_bus import start_private_bus
if not a.inside:
    a.output=a.output.resolve();a.output.mkdir(mode=0o700,parents=True,exist_ok=False)
    launcher=Path(a.launcher).resolve();sway=Path(a.sway).resolve();pointer=Path(a.pointer_tool).resolve();atspi=a.at_spi_prefix.resolve()
    for name in ('font_config','software_driver','audio','keyboard','xwayland'):setattr(a,name,getattr(a,name).resolve())
    assert all(str(v).startswith('/nix/store/') and v.exists() for v in (launcher,sway,pointer,atspi,a.font_config,a.software_driver,a.audio,a.keyboard,a.xwayland))
    prefix=re.split(r'\n[ \t]*exec quickshell ',launcher.read_text(),maxsplit=1)[0]
    env=dict(v.decode().split('=',1)for v in sp.check_output(['bash','-c',prefix+'\nexec env -0']).split(b'\0')if b'='in v)
    frozen=FrozenSource(repo,__file__,[launcher,sway,pointer,atspi/"libexec/at-spi-bus-launcher",Path(sys.executable).resolve(),a.font_config,a.software_driver/"share/glvnd/egl_vendor.d/50_mesa.json",a.audio/"bin/pulseaudio",a.audio/"bin/pactl",a.keyboard,a.xwayland])
    args=[str(Path(sys.executable).resolve()),'/source/tests/desktop/menu_scaled_native.py','--inside','--xwayland',str(a.xwayland),'--keyboard',str(a.keyboard),'--audio',str(a.audio),'--launcher',str(launcher),'--sway',str(sway),'--pointer-tool',str(pointer),'--at-spi-prefix',str(atspi),'--font-config',str(a.font_config),'--software-driver',str(a.software_driver),'--output','/artifacts/result']+(['--effects']if a.effects else [])
    code=1
    try:code=run(repo,a.output,args,env,software_graphics=a.effects,private_pty=True)
    finally:frozen.finish(a.output,code)
    raise SystemExit(code)
assert repo==Path('/source') and os.environ['HOME']=='/home/fixture' and not Path('/.old-root').exists()
import dbus,dbus.mainloop.glib
dbus.mainloop.glib.DBusGMainLoop(set_as_default=True)
out=a.output.resolve();out.mkdir(parents=True,exist_ok=False);runtime=Path(os.environ['XDG_RUNTIME_DIR'])
script=Path(a.launcher).read_text();prefix=re.split(r'\n[ \t]*exec quickshell ',script,maxsplit=1)[0]
raw=sp.check_output(['bash','-c',prefix+'\nexec env -0']);env=dict(v.decode().split('=',1) for v in raw.split(b'\0') if b'=' in v)
env.update(QT_QPA_PLATFORM='wayland',QT_QPA_PLATFORMTHEME='generic',QT_QUICK_BACKEND='software',QT_ACCESSIBILITY='1',QT_LINUX_ACCESSIBILITY_ALWAYS_ON='1',GSETTINGS_BACKEND='memory',WLR_BACKENDS='headless',WLR_RENDERER='gles2' if a.effects else 'pixman',WLR_RENDERER_FORCE_SOFTWARE='1' if a.effects else '0',WLR_RENDERER_ALLOW_SOFTWARE='1',LIBGL_ALWAYS_SOFTWARE='0'if a.effects else '1',WLR_LIBINPUT_NO_DEVICES='1',GLOBAL_MENU_ENABLED='1',LUMEN_FILE_SERVICE='0',LUMEN_LAUNCH_SCOPES='0')
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
env.update(FONTCONFIG_FILE=str(a.font_config),LIBGL_DRIVERS_PATH=str(a.software_driver/'lib/dri'),MESA_LOADER_DRIVER_OVERRIDE='llvmpipe',__EGL_VENDOR_LIBRARY_FILENAMES=str(a.software_driver/'share/glvnd/egl_vendor.d/50_mesa.json'),QT_NO_XDG_DESKTOP_PORTAL='1')
assets=out/'assets';shutil.copytree(repo/'res/lumen-desktop',assets)
for directory in [assets,*[v for v in assets.rglob('*')if v.is_dir()]]:directory.chmod(0o700)
for file in assets.rglob('*'):
    if file.is_file():file.chmod(0o600)
isolate_providers(assets/'shell',out)
# Optional hardware services are absent in the detached fixture. Keep their
# production menu consumers, with explicit unavailable producer state.
changes=[]
def fixture_edit(name,transform,scope):
    import hashlib
    path=assets/'shell'/name;before=path.read_text();after=transform(before);assert before!=after
    path.write_text(after);changes.append(dict(path=name,scope=scope,before=hashlib.sha256(before.encode()).hexdigest(),after=hashlib.sha256(after.encode()).hexdigest()))
fixture_edit('AudioState.qml',lambda text:text.replace('import Quickshell.Services.Pipewire','').replace('id: audio','id: audio\n QtObject {id:privateAudio;property var nodes:({values:[]});property var defaultAudioSource:null;property var defaultAudioSink:null;property bool ready:false;property var preferredDefaultAudioSource:null;property var preferredDefaultAudioSink:null}').replace('Pipewire.','privateAudio.').replace('PwObjectTracker { objects:','QtObject {property var objects:'),'Unavailable audio state; no audio connection')
fixture_edit('ControlCentre.qml',lambda text:text.replace('import Quickshell.Bluetooth','').replace('Bluetooth.defaultAdapter','null'),'Unavailable Bluetooth control tile; no system bus connection')
fixture_edit('SystemPanels.qml',lambda text:text.replace('import Quickshell.Bluetooth',''),'No Bluetooth module initialization for unused system panels')
fixture_edit('ConnectivityPanel.qml',lambda text:text.replace('import Quickshell.Bluetooth','').replace('Bluetooth.defaultAdapter','null'),'Unavailable Bluetooth adapter; no system bus connection')
helper=out/'network-unavailable.py';helper.write_text('import json\nprint(json.dumps(dict(status="unknown",reason="unavailable",message="Private fixture")))\n')
fixture_edit('SystemState.qml',lambda text:text.replace('"desktop-network-diagnose"',json.dumps(sys.executable)+','+json.dumps(str(helper))),'Unavailable internet diagnostic; no host command')
fixture_edit('MenuBar.qml',lambda text:text.replace('id: bar','id: bar\n function fixtureMenuText(){let rows=[];function walk(item){if(item.text && item.font && item.visible)rows.push({text:item.text,size:item.font.pixelSize,width:item.width,naturalWidth:item.implicitWidth,truncated:!!item.truncated});for(const child of item.children || [])walk(child);}walk(popupContents);return {scale:Theme.textScale,rows:rows};}',1),'Read-only native text geometry inspection')
fixture_edit('MenuCard.qml',lambda text:text.replace('hover:r.pointerInside','hover:r.pointerInside,text:r.children.filter(c=>c.text && c.font).map(c=>({text:c.text,size:c.font.pixelSize,width:c.width,naturalWidth:c.implicitWidth,truncated:!!c.truncated}))'),'Read-only exported row text geometry inspection')
(out/'optional-provider-isolation.json').write_text(json.dumps(changes,indent=2))

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
  function status():string {const b=root.bars["HEADLESS-1"];return JSON.stringify({text:b?b.fixtureMenuText():null,ready:!!b,menu:b?b.menu:"",layout:b?b.popupLayout():null,active:root.active?root.active.appId:"",bridge:root.exportedMenu.state,latestBridge:root.exportedMenu.latestState,headingLayout:b?b.menuLayoutStatus():null,hiddenAnchor:b?b.exportX(37):0,exportPath:root.exportedMenu.path,exportTarget:root.exportedMenu.invokingContext && root.exportedMenu.invokingContext.window?root.exportedMenu.invokingContext.window.title:"",highlighted:root.exportedMenu.highlighted,hoverBlock:root.exportedMenu.hoverBlock?{id:root.exportedMenu.hoverBlock.intent.id,depth:root.exportedMenu.hoverBlock.intent.depth}:null,selection:root.exportedMenu.selection?{id:root.exportedMenu.selection.id,revision:root.exportedMenu.selection.revision}:null,target:b && b.menuWindow?b.menuWindow.title:"",filesContext:b?b.filesFocused:false,windows:root.windows.map(w=>({app:w.appId,title:w.title}))});}
 }
}'''+tail
(source/'shell.qml').write_text(text)
palette=json.loads((assets/'appearance/palette.json').read_text());appearance=Path(env['XDG_STATE_HOME'])/'lumen-desktop/appearance.json';appearance.parent.mkdir(parents=True,exist_ok=True);appearance.write_text(json.dumps(dict(font=palette['font'],palette=palette['dark'],preferences=dict(textScale=1.5,reduceMotion=True))))
try:
    from application_fixture.audio import start_null_sink
    start_private_bus(env,out,processes);os.environ['DBUS_SESSION_BUS_ADDRESS']=env['DBUS_SESSION_BUS_ADDRESS'];env['DBUS_SYSTEM_BUS_ADDRESS']=env['DBUS_SESSION_BUS_ADDRESS']
    cookie=Path(env['XDG_CONFIG_HOME'])/'pulse/cookie';cookie.parent.mkdir(parents=True);cookie.write_bytes(os.urandom(256));cookie.chmod(0o600)
    start_null_sink(a.audio,env,out,processes)
    launch([str(a.at_spi_prefix/'libexec/at-spi-bus-launcher'),'--launch-immediately','--a11y=1','--screen-reader=1'],'a11y-bus.log')
    bus=dbus.SessionBus();wait(lambda:bus.name_has_owner('org.a11y.Bus'),'private accessibility bus')
    address=str(bus.get_object('org.a11y.Bus','/org/a11y/bus').GetAddress(dbus_interface='org.a11y.Bus'))
    assert address.startswith('unix:path='+str(runtime)+'/'),address
    env['AT_SPI_BUS_ADDRESS']=address;os.environ['AT_SPI_BUS_ADDRESS']=address
    a11y_bus=dbus.bus.BusConnection(address);launch([str(a.at_spi_prefix/'libexec/at-spi2-registryd')],'registry.log')
    wait(lambda:a11y_bus.name_has_owner('org.a11y.atspi.Registry'),'private registry')
    import gi
    gi.require_version('Atspi','2.0')
    from gi.repository import Atspi,GLib
    keymap=out/'x11-us.xkb';keymap.write_text((repo/'tests/desktop/fixtures/x11-us.xkb').read_text());env['LUMEN_TEST_KEYMAP']=str(keymap)
    # XWayland's initial map is compiled before a seat map arrives. Supply the
    # same bounded US fixture vocabulary at both stages, without log switches.
    xkb=out/'xkb';(xkb/'rules').mkdir(parents=True);(xkb/'compiled').mkdir();(xkb/'geometry').mkdir();(xkb/'geometry/fixture').write_text('default xkb_geometry "fixture" {description="Private US input fixture";};\n')
    sections=[('keycodes','xkb_keycodes'),('types','xkb_types'),('compat','xkb_compatibility'),('symbols','xkb_symbols')]
    text=keymap.read_text()
    for index,(directory,marker)in enumerate(sections):
        start=text.index(marker);end=text.index(sections[index+1][1],start)if index+1<len(sections)else text.rfind('};')
        (xkb/directory).mkdir();(xkb/directory/'fixture').write_text('default '+text[start:end].strip()+'\n')
    rules=''
    for lhs,rhs in [('model','keycodes'),('model','types'),('model','compat'),('layout','symbols'),('model','geometry')]:rules+='! '+lhs+' = '+rhs+'\n  * = fixture\n'
    for name in ('base','evdev'):(xkb/'rules'/name).write_text(rules)
    wrapper=out/'Xwayland';wrapper.write_text(a.launcher and Path(a.launcher).read_text().splitlines()[0]+'\nexec '+str(a.xwayland)+' -xkbdir '+str(xkb)+' "$@"\n');wrapper.chmod(0o700);env['WLR_XWAYLAND']=str(wrapper)

    config=out/'sway.conf';config.write_text('input * xkb_file '+str(keymap)+'\n'+'output HEADLESS-1 resolution 1280x1000\nseat seat0 fallback true\nxwayland enable\nfocus_follows_mouse no\nfor_window [all] floating enable\n')
    launch([a.sway,'--unsupported-gpu','-c',str(config)],'sway.log')
    env['SWAYSOCK']=str(wait(lambda:next(runtime.glob('sway-ipc*.sock'),None),'private compositor'))
    env['WAYLAND_DISPLAY']=wait(lambda:next((v.name for v in runtime.glob('wayland-*') if not v.name.endswith('.lock')),None),'private display')
    from application_fixture.keyboard import PhysicalKeyboard
    with (out/'keyboard.log').open('w')as log:keyboard=PhysicalKeyboard(a.keyboard,env,log)
    processes.append(keyboard.process)
    launch(['quickshell','-p',str(fixture)],'shell.log')
    def ipc(method,*args):return call('quickshell','-p',str(fixture),'ipc','call','menuAudit',method,*args)
    def state():
        value=json.loads(ipc('status'));(out/'last-state.json').write_text(json.dumps(value,indent=2));return value
    wait(lambda:state()['ready'],'owned MenuBar')
    clients={}
    def client(app):
        code='import gi;gi.require_version("Gtk","3.0");from gi.repository import Gtk,GLib;GLib.set_prgname('+repr(app)+');w=Gtk.Window(title='+repr(app)+');w.set_default_size(500,300);w.add(Gtk.TextView());w.connect("destroy",Gtk.main_quit);w.show_all();Gtk.main()'
        process=launch([sys.executable,'-c',code],app+'.log');clients[app]=process
        wait(lambda:any(w['app']==app for w in state()['windows']),app);return process
    def stop(app):
        os.killpg(clients[app].pid,signal.SIGTERM);clients[app].wait(timeout=5)
        wait(lambda:not any(w['app']==app for w in state()['windows']),'removed '+app)
    def key(*keys):keyboard.key(*keys)
    def open_menu(name,app):
        call('swaymsg','[app_id="'+app+'"]','focus');wait(lambda:state()['active']==app and not state()['filesContext'] and state()['bridge'].get('appName')==app and state()['bridge'].get('windowId',0)>0,'native focus and installed bridge watcher')
        ipc('open',name);wait(lambda:state()['menu']==name,'menu '+name);ipc('clear')
    client('menu-source')
    open_menu('Desktop','menu-source')
    builtin_text=state()['text'];assert builtin_text['scale']==1.5 and any(row['size']==20 for row in builtin_text['rows']),builtin_text
    (out/'builtin-text.json').write_text(json.dumps(builtin_text,indent=2));key('-k','Down','-k','Right')
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
    exported_open()
    exported_text=json.loads(call('quickshell','-p',str(fixture),'ipc','call','globalmenu','layout'));assert exported_text
    samples=[text for card in exported_text for row in card['rows'] for text in row['text'] if text['text'] in ('Checked Command','⌃K')]
    assert any(row['text']=='Checked Command' and row['size']==20 and not row['truncated'] for row in samples),samples
    assert any(row['text']=='⌃K' and row['size']==18 and not row['truncated'] for row in samples),samples
    (out/'exported-text.json').write_text(json.dumps(exported_text,indent=2));key('-k','Down');wait(lambda:state()['selection'] and state()['selection']['id']==13,'skip disabled and separator')
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
    checks.append('Actual 640 pixel heading overflow remains bounded at150% and keyboard reaches every enabled exported heading')
    exported_open();key('-k','Down');wait(lambda:state()['selection'] and state()['selection']['id']==13,'scaled exact captured command');key('-k','Return')
    wait(lambda:len(events())==1 and events()[0]['id']==13 and events()[0]['pid']==exporter.pid,'scaled Return dispatches once to exact exporter')
    before=len(exporter_state()['keys']);key('-k','k');wait(lambda:len(exporter_state()['keys'])==before+1,'original native input after accepted command')
    checks.append('150% Return dispatches exactly once to captured native exporter and releases native keyboard input')
    call('grim',str(out/'accepted.png'))
    (out/'result.json').write_text(json.dumps(dict(passed=True,checks=checks,textScale=1.5,builtinText=builtin_text,exportedText=exported_text,source='Production menu text and generation-safe native keyboard routing on a private exporter; optional hardware providers explicitly unavailable; no unexported application or full reference parity claim'),indent=2)+'\n')

finally:
    for proc in reversed(processes):
        if proc.poll() is None:
            try:os.killpg(proc.pid,signal.SIGTERM)
            except ProcessLookupError:pass
    for proc in reversed(processes):
        try:proc.wait(timeout=3)
        except sp.TimeoutExpired:os.killpg(proc.pid,signal.SIGKILL);proc.wait(timeout=3)
    for _fixture_process in processes:
        if _fixture_process.poll() is None:_fixture_process.wait(timeout=5)
    close_reaped_pipes(processes)

    logs={str(path.relative_to(out)):path.read_text()for path in out.rglob('*.log')}
    warnings={name:[line for line in text.splitlines()if (line.startswith(('W:','E:')) or any(marker in line.lower()for marker in ('warn','referenceerror:','typeerror:','binding loop','cannot assign','error:')))]for name,text in logs.items()}
    warnings={name:rows for name,rows in warnings.items()if rows};(out/'warning-scan.json').write_text(json.dumps(warnings,indent=2));assert not warnings,warnings
