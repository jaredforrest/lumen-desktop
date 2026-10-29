"""Actual shell crashes preserve dirty GTK clients and restore usable controls."""
import argparse,hashlib,json,os,re,shutil,signal,subprocess as sp,sys,time
from pathlib import Path
from application_fixture.isolation import run
from application_fixture.provenance import FrozenSource
from application_fixture.keyboard import PhysicalKeyboard
from application_fixture.shell_assets import prepare
from application_fixture.audio import start_null_sink
from private_native_bus import start_private_bus
from fixture_resources import close_reaped_pipes
p=argparse.ArgumentParser()
for name in ('launcher','sway','keyboard','font-config','software-driver','audio'):p.add_argument('--'+name,type=Path,required=True)
p.add_argument('--output',type=Path,required=True);p.add_argument('--effects',action='store_true');p.add_argument('--inside',action='store_true');a=p.parse_args();repo=Path(__file__).resolve().parents[2]
for name in ('launcher','sway','keyboard','font_config','software_driver','audio'):
    path=getattr(a,name).resolve();assert str(path).startswith('/nix/store/') and path.exists();setattr(a,name,path)
if not a.inside:
    a.output=a.output.resolve();a.output.mkdir(mode=0o700,parents=True,exist_ok=False)
    runtime=Path(sys.executable).resolve();frozen=FrozenSource(repo,__file__,[a.launcher,a.sway,a.keyboard,a.font_config,a.software_driver/'share/glvnd/egl_vendor.d/50_mesa.json',a.audio/'bin/pulseaudio',a.audio/'bin/pactl',runtime])
    prefix=re.split(r'\n[ \t]*exec quickshell ',a.launcher.read_text(),maxsplit=1)[0];environment=dict(v.decode().split('=',1)for v in sp.check_output(['bash','-c',prefix+'\nexec env -0']).split(b'\0')if b'='in v)
    command=[str(runtime),'/source/tests/desktop/session_shell_recovery_native.py','--inside','--output','/artifacts/result']+sum((['--'+name.replace('_','-'),str(getattr(a,name))]for name in ('launcher','sway','keyboard','font_config','software_driver','audio')),[])+(['--effects']if a.effects else [])
    code=1
    try:code=run(repo,a.output,command,environment,software_graphics=a.effects)
    finally:frozen.finish(a.output,code)
    raise SystemExit(code)
assert repo==Path('/source') and Path.home()==Path('/home/fixture') and not Path('/.old-root').exists()
assert not any((Path('/dev')/name).exists()for name in ('dri','snd','tty','video0'))
out=a.output;out.mkdir(mode=0o700,parents=True,exist_ok=False);processes=[]
env=dict(os.environ,WLR_BACKENDS='headless',WLR_HEADLESS_OUTPUTS='1',WLR_RENDERER='gles2'if a.effects else 'pixman',WLR_RENDERER_FORCE_SOFTWARE='1'if a.effects else '0',WLR_RENDERER_ALLOW_SOFTWARE='1',WLR_LIBINPUT_NO_DEVICES='1',LIBGL_ALWAYS_SOFTWARE='0'if a.effects else '1',MESA_LOADER_DRIVER_OVERRIDE='llvmpipe',LIBGL_DRIVERS_PATH=str(a.software_driver/'lib/dri'),__EGL_VENDOR_LIBRARY_FILENAMES=str(a.software_driver/'share/glvnd/egl_vendor.d/50_mesa.json'),FONTCONFIG_FILE=str(a.font_config),QT_QPA_PLATFORM='wayland',QT_QPA_PLATFORMTHEME='generic',QT_QUICK_BACKEND='software',QT_NO_XDG_DESKTOP_PORTAL='1',QT_ACCESSIBILITY='0',NO_AT_BRIDGE='1',GTK_A11Y='none',GDK_BACKEND='wayland',LUMEN_FILE_SERVICE='0',LUMEN_LAUNCH_SCOPES='0',GLOBAL_MENU_ENABLED='1')
for name in ('SWAYSOCK','WAYLAND_DISPLAY','DISPLAY','AT_SPI_BUS_ADDRESS'):env.pop(name,None)
assets=prepare(repo,out);shell_dir=assets/'shell';source=shell_dir/'shell.qml';original=source.read_text();head,tail=original.rsplit('}',1)
source.write_text(head+''' IpcHandler {target:"recoveryProbe"
 function status():string {return JSON.stringify({popupOwner:root.popupOwner,screens:Quickshell.screens.map(s=>s.name),bars:Object.keys(root.bars),windows:root.windows.map(w=>({app:w.appId,title:w.title})),dockItems:root.referenceDock.length});}
} }'''+tail)
(out/'probe-seam.json').write_text(json.dumps(dict(scope='Read-only recovery state inspection; no application commands replaced',originalSha256=hashlib.sha256(original.encode()).hexdigest(),fixtureSha256=hashlib.sha256(source.read_bytes()).hexdigest()),indent=2))
palette=json.loads((assets/'appearance/palette.json').read_text());appearance=Path(env['XDG_STATE_HOME'])/'lumen-desktop/appearance.json';appearance.parent.mkdir(parents=True,exist_ok=True);appearance.write_text(json.dumps(dict(font=palette['font'],palette=palette['dark'],preferences=dict(reduceMotion=True))))
settings=out/'settings.json';settings.write_text(json.dumps(dict(search=dict(historyEnabled=False),clipboard=dict(enabled=False))));env['LUMEN_NIX_SETTINGS_ACTIVE']=str(settings)
def launch(command,name):
    with (out/name).open('w')as log:process=sp.Popen(list(map(str,command)),env=env,stdout=log,stderr=sp.STDOUT,start_new_session=True)
    processes.append(process);return process

def call(*command):return sp.check_output(list(map(str,command)),env=env,text=True,timeout=1).strip()
def ipc(target,method,*args):return call('quickshell','-p',shell_dir,'ipc','call',target,method,*args)
def probe():return json.loads(ipc('recoveryProbe','status'))
def tree():return json.loads(call(a.sway.parent/'swaymsg','-r','-t','get_tree'))
def nodes(node):
    yield node
    for child in node.get('nodes',[])+node.get('floating_nodes',[]):yield from nodes(child)
def wait(callback,description,seconds=15):
    deadline=time.monotonic()+seconds
    while time.monotonic()<deadline:
        try:value=callback()
        except (OSError,ValueError,sp.SubprocessError):value=None
        if value:return value
        time.sleep(.03)
    raise AssertionError('Recovery observation deadline: '+description)
def client_windows():return {n['app_id']:n for n in nodes(tree())if str(n.get('app_id','')).startswith('org.lumen.Recovery')}
def identity(process):
    data=Path('/proc',str(process.pid),'stat').read_text().rsplit(')',1)[1].split()
    return dict(pid=process.pid,startTimeTicks=int(data[19]),executable=os.readlink('/proc/'+str(process.pid)+'/exe'))

def document(name):
    import dbus
    bus=dbus.SessionBus();return json.loads(bus.get_object('org.lumen.Recovery'+name,'/Document').Status(dbus_interface='org.lumen.RecoveryDocument',timeout=1))
def focus(name):call(a.sway.parent/'swaymsg','[app_id="org.lumen.Recovery'+name+'"] focus');wait(lambda:document(name)['focused'],'native client focus '+name)
try:
    start_private_bus(env,out,processes);os.environ['DBUS_SESSION_BUS_ADDRESS']=env['DBUS_SESSION_BUS_ADDRESS'];env['DBUS_SYSTEM_BUS_ADDRESS']=env['DBUS_SESSION_BUS_ADDRESS']
    cookie=Path(env['XDG_CONFIG_HOME'])/'pulse/cookie';cookie.parent.mkdir(parents=True);cookie.write_bytes(os.urandom(256));cookie.chmod(0o600);start_null_sink(a.audio,env,out,processes)
    config=out/'sway.conf';config.write_text('output HEADLESS-1 resolution 1200x900\nseat seat0 fallback true\nxwayland disable\nfocus_follows_mouse no\nfor_window [all] floating enable\n')
    compositor=launch([a.sway,'--unsupported-gpu','-c',config],'sway.log');runtime=Path(env['XDG_RUNTIME_DIR']);env['SWAYSOCK']=str(wait(lambda:next(runtime.glob('sway-ipc*.sock'),None),'private compositor'));env['WAYLAND_DISPLAY']=wait(lambda:next((path.name for path in runtime.glob('wayland-*')if not path.name.endswith('.lock')),None),'private display')
    with (out/'keyboard.log').open('w')as log:keyboard=PhysicalKeyboard(a.keyboard,env,log)
    processes.append(keyboard.process)
    clients={name:launch([sys.executable,repo/'tests/desktop/fixtures/recovery_document.py','--document',name],'client-'+name+'.log')for name in ['A','B']}
    wait(lambda:len(client_windows())==2,'two actual GTK documents')
    expected={name:'Synthetic unsaved document '+name for name in clients}
    for name in clients:focus(name);keyboard.type(expected[name]);wait(lambda:document(name)['text']==expected[name]and document(name)['modified'],'actual dirty document '+name)
    before={name:dict(process=identity(process),container=client_windows()['org.lumen.Recovery'+name]['id'],document=document(name))for name,process in clients.items()}
    shell=launch(['quickshell','-p',shell_dir],'shell-0.log');wait(lambda:probe()['bars']==['HEADLESS-1']and probe()['dockItems']>0,'initial actual shell')
    receipts=[]
    for cycle,held in enumerate(['menu','dock'],1):
        focus('A')
        if held=='menu':ipc('desktop','menu','Desktop');wait(lambda:any(item['menu']=='Desktop'for item in json.loads(ipc('desktop','layout'))),'actual captured menu')
        else:ipc('desktop','focusDock');wait(lambda:probe()['popupOwner'].startswith('dock:'),'actual dock keyboard capture')
        assert shell.poll()is None;old_pid=shell.pid;os.kill(shell.pid,signal.SIGKILL);assert shell.wait(timeout=3)==-signal.SIGKILL
        expected['A']+=' gap'+str(cycle);keyboard.type(' gap'+str(cycle));wait(lambda:document('A')['text']==expected['A'],'keyboard grab released by actual shell death')
        started=time.monotonic();shell=launch(['quickshell','-p',shell_dir],'shell-'+str(cycle)+'.log')
        wait(lambda:probe()['bars']==['HEADLESS-1']and probe()['dockItems']>0,'useful restarted shell',seconds=5)
        ipc('desktop','menu','Desktop');wait(lambda:any(item['menu']=='Desktop'for item in json.loads(ipc('desktop','layout'))),'recovered menu',seconds=max(.01,5-(time.monotonic()-started)))
        elapsed=time.monotonic()-started;assert elapsed<=5,elapsed;keyboard.key('-k','Escape');wait(lambda:probe()['popupOwner']=='','recovered menu releases capture')
        focus('B');expected['B']+=' restored'+str(cycle);keyboard.type(' restored'+str(cycle));wait(lambda:document('B')['text']==expected['B'],'ordinary native input after restart')
        ipc('desktop','focusDock');wait(lambda:probe()['popupOwner'].startswith('dock:'),'recovered dock capture');keyboard.key('-k','Escape');wait(lambda:probe()['popupOwner']=='','recovered dock releases capture')
        expected['B']+=' dock'+str(cycle);keyboard.type(' dock'+str(cycle));wait(lambda:document('B')['text']==expected['B'],'native input after dock Escape')
        current=client_windows();assert len(current)==2 and all(process.poll()is None for process in clients.values())
        for name,process in clients.items():
            assert identity(process)==before[name]['process'];assert current['org.lumen.Recovery'+name]['id']==before[name]['container'];value=document(name);assert value['modified']and value['text']==expected[name],value
        shell_clients=[window for window in probe()['windows']if window['app'].startswith('org.lumen.Recovery')];assert len(shell_clients)==2,shell_clients
        receipts.append(dict(held=held,killedShellPid=old_pid,restartedShellPid=shell.pid,recoverySeconds=elapsed,clients={name:dict(identity=identity(process),container=current['org.lumen.Recovery'+name]['id'],document=document(name))for name,process in clients.items()},shellWindows=shell_clients))
    call('grim',out/'recovered.png');(out/'result.json').write_text(json.dumps(dict(passed=True,cycles=receipts,checks=['Actual SIGKILL while menu and Dock own keyboard','Dirty GTK text survives both crashes','Exact client process start time and compositor container identities retained','No duplicate restored client windows','Recovered menu usable within5seconds after explicit supervisor restart','Native keyboard returns during outage and after recovered menu/Dock Escape'],scope='Private explicit supervisor restart only; does not claim actual systemd user supervision or crash-loop fallback execution'),indent=2)+'\n')
finally:
    # A shell killed without cleanup may leave its own provider descendants.
    # Client documents belong to distinct process groups and remain until here.
    for process in reversed(processes):
        try:os.killpg(process.pid,signal.SIGTERM)
        except ProcessLookupError:pass
    for process in reversed(processes):
        try:process.wait(timeout=3)
        except sp.TimeoutExpired:os.killpg(process.pid,signal.SIGKILL);process.wait(timeout=3)
    close_reaped_pipes(processes)
    while True:
        try:
            child,_=os.waitpid(-1,os.WNOHANG)
            if not child:break
        except ChildProcessError:break
    warnings={str(path.relative_to(out)):[line for line in path.read_text().splitlines()if line.startswith(('W:','E:'))or any(marker in line.lower()for marker in ('warn','referenceerror:','typeerror:','binding loop','cannot assign','error:'))]for path in out.rglob('*.log')}
    warnings={name:rows for name,rows in warnings.items()if rows};(out/'warning-scan.json').write_text(json.dumps(warnings,indent=2));assert not warnings,warnings
