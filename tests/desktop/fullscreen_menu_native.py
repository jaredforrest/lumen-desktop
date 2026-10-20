"""Output-local fullscreen menu policy under genuine nested input."""
from fixture_resources import close_reaped_pipes
import argparse,json,os,re,shutil,signal,subprocess as sp,sys,tempfile,time
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--output',type=Path,required=True);p.add_argument('--launcher',type=Path,required=True);p.add_argument('--sway',required=True);p.add_argument('--pointer-tool',required=True);p.add_argument('--effects',action='store_true');p.add_argument('--inside',action='store_true');a=p.parse_args()
if not a.inside:
    with tempfile.TemporaryDirectory(prefix='lumen-device-input-',ignore_cleanup_errors=True) as private:
        env=dict(os.environ,HOME=private,XDG_RUNTIME_DIR=private)
        for key in ('CONFIG','STATE','CACHE','DATA'):env['XDG_'+key+'_HOME']=str(Path(private)/key.lower())
        for key in ('DISPLAY','WAYLAND_DISPLAY','SWAYSOCK','DBUS_SESSION_BUS_ADDRESS','AT_SPI_BUS_ADDRESS'):env.pop(key,None)
        config=Path(private)/'bus.conf'
        config.write_text('<busconfig><type>session</type><listen>unix:tmpdir='+private+'</listen><auth>EXTERNAL</auth><policy context="default"><allow send_destination="*"/><allow receive_sender="*"/><allow own="*"/></policy></busconfig>')
        raise SystemExit(sp.call(['dbus-run-session','--config-file='+str(config),'--',sys.executable,__file__,*sys.argv[1:],'--inside'],env=env))
repo=Path(__file__).resolve().parents[2];out=a.output.resolve();out.mkdir(parents=True,exist_ok=False)
prefix=re.split(r'\n[ \t]*exec quickshell ',a.launcher.read_text(),maxsplit=1)[0]
exported=sp.check_output(['bash','-c',prefix+'\nexec env -0']);env=dict(value.decode().split('=',1) for value in exported.split(b'\0') if b'=' in value)
env.update(WLR_BACKENDS='headless',WLR_RENDERER='gles2' if a.effects else 'pixman',WLR_RENDERER_ALLOW_SOFTWARE='1',LIBGL_ALWAYS_SOFTWARE='1',WLR_LIBINPUT_NO_DEVICES='1',QT_QPA_PLATFORM='wayland',QT_QPA_PLATFORMTHEME='generic',QT_QUICK_BACKEND='software',QT_ACCESSIBILITY='0',QT_LINUX_ACCESSIBILITY_ALWAYS_ON='0',NO_AT_BRIDGE='1',GTK_A11Y='none',LUMEN_FILE_SERVICE='0',LUMEN_LAUNCH_SCOPES='0',LUMEN_NATIVE_WINDOW_CONTROLS='0',LUMEN_NATIVE_WINDOW_ACTIONS='0',LUMEN_UNIFIED_OWNED_HEADERS='1')
for key in ('DISPLAY','WAYLAND_DISPLAY','SWAYSOCK','AT_SPI_BUS_ADDRESS'):env.pop(key,None)
palettes=json.loads((repo/'res/lumen-desktop/appearance/palette.json').read_text());appearance=out/'appearance.json';appearance.write_text(json.dumps(dict(font=palettes['font'],palette=palettes['dark'],preferences={},menuBarPreferences={"fullscreenVisibility":"automatic"})));env['LUMEN_APPEARANCE_STATE']=str(appearance)
shutil.copytree(repo/'res/lumen-desktop',out/'assets')
from PIL import Image,ImageStat
env['LUMEN_COMPOSITOR_EFFECTS']='1' if a.effects else '0'
if a.effects:env['WLR_RENDERER_FORCE_SOFTWARE']='1'
env['GLOBAL_MENU_ENABLED']='0';env['WLR_HEADLESS_OUTPUTS']='2'
fixture=out/'assets/shell';qml=fixture/'shell.qml';original=qml.read_text()
addition='''
 IpcHandler {target:"fullscreenMenuAudit"
  function state():string {return JSON.stringify({owner:root.popupOwner,available:menuVisibilityState.available,outputs:menuVisibilityState.outputs,error:menuVisibilityState.error,bars:Object.keys(root.bars).map(name=>{const bar=root.bars[name];return {pointer:bar.pointerAudit(),output:name,x:bar.screen.x,y:bar.screen.y,width:bar.width,visible:bar.visible,fullscreen:bar.outputFullscreen,automatic:bar.automaticFullscreen,autoHide:bar.automaticHide,revealed:bar.edgeRevealed,menu:bar.menu,zone:bar.exclusiveZone,layer:bar.WlrLayershell.layer};})});}
  function keyboardMenu():void {root.requestMenu("Desktop","HEADLESS-1");}
  function close():void {root.claimPopup("");}
 }
'''
qml.write_text(original[:original.rfind('}')]+addition+'}\n')
menu=(fixture/'MenuBar.qml').read_text().replace('    id: bar','    id: bar\n    function pointerAudit(){return {hovered:barHover.hovered,brand:brandButton.pointerAudit()};}')
(fixture/'MenuBar.qml').write_text(menu)
button=(fixture/'MenuButton.qml').read_text().replace('    id: button','    id: button\n    function pointerAudit(){return {inside:mouse.containsMouse,x:mouse.mouseX,y:mouse.mouseY};}')
(fixture/'MenuButton.qml').write_text(button)

client=out/'client.qml';client.write_text('''import QtQuick
import Quickshell
import Quickshell.Io
ShellRoot {id:root;property int clicks:0;property int keys:0
 FloatingWindow {id:window;visible:true;title:"Fullscreen Input Probe";implicitWidth:500;implicitHeight:360;color:"#204060"
  Item {id:clientInput;anchors.fill:parent;focus:true;Keys.onPressed:event=>{root.keys++;event.accepted=true;};MouseArea {anchors.fill:parent;onClicked:root.clicks++}}
 }
 IpcHandler {target:"client";function focus():void {clientInput.forceActiveFocus();}function state():string {return JSON.stringify({clicks:root.clicks,keys:root.keys,focused:clientInput.activeFocus});}}
}''')
processes=[]
def launch(command,name):
    with (out/name).open('w') as log:process=sp.Popen(command,env=env,stdout=log,stderr=sp.STDOUT,start_new_session=True)
    processes.append(process);return process

def call(*command):return sp.check_output(command,env=env,text=True,timeout=10).strip()
def wait(fn):
    end=time.monotonic()+20
    while time.monotonic()<end:
        try:
            value=fn()
            if value:return value
        except (OSError,ValueError,StopIteration,sp.SubprocessError):pass
        time.sleep(.1)
    if 'pointer' in globals():
        (out/'failed-state.json').write_text(json.dumps(state(),indent=2))
    raise AssertionError('Fullscreen menu fixture timed out')
def ipc(method,*args):return call('quickshell','-p',str(qml),'ipc','call','fullscreenMenuAudit',method,*args)
def state():return json.loads(ipc('state'))
def row(name):return next(row for row in state()['bars'] if row['output']==name)
def observed():return json.loads(call('quickshell','-p',str(client),'ipc','call','client','state'))
def nodes(node):
    yield node
    for child in node.get('nodes',[])+node.get('floating_nodes',[]):yield from nodes(child)
def native():return next(node for node in nodes(json.loads(call('swaymsg','-r','-t','get_tree'))) if node.get('name')=='Fullscreen Input Probe')
def command(value):pointer.stdin.write(value+'\n');pointer.stdin.flush();time.sleep(.12)
def move(x,y):
    # The cursor IPC command takes layout-bounds-relative, not global, pixels.
    outputs=[value['rect'] for value in json.loads(call('swaymsg','-r','-t','get_outputs')) if value.get('active')]
    call('swaymsg','seat','seat0','cursor','set',str(round(x-min(value['x'] for value in outputs))),str(round(y-min(value['y'] for value in outputs))));command('m 0.5 0.5')
def click(x,y):move(x,y);command('p');command('r')
def key(*keys):call('wtype',*keys);time.sleep(.12)
def preference(value,legacy=False):
    data=json.loads(appearance.read_text());data['menuBarPreferences']=dict(**({'fullscreenVisibility':value} if value=='automatic' or legacy else {'visibility':value}),order=['battery','sound','focus','search','controls'],hourCycle='24',showDate=True);pending=appearance.with_suffix('.new');pending.write_text(json.dumps(data));pending.replace(appearance)
def painted(name,first,second=True):
    def capture():
        call('grim',str(out/name))
        with Image.open(out/name) as source:image=source.convert('RGB')
        mismatches=[]
        for x,shown in ((300,first),(1100,second)):
            pixel=image.getpixel((x,10));matches=min(pixel)>20 and max(pixel)<120 and max(pixel)-min(pixel)<25 if shown else pixel[2]>pixel[0]+40
            if not matches:mismatches.append(dict(x=x,shown=shown,pixel=pixel))
        (out/(name+'-pixels.json')).write_text(json.dumps(mismatches));return not mismatches
    wait(capture)
try:
    import shlex
    config=out/'sway.conf';config.write_text('output HEADLESS-1 resolution 800x600 position 0 0\noutput HEADLESS-2 resolution 640x480 position 800 0\nxwayland disable\nfor_window [title="Fullscreen Input Probe"] floating enable, move container to output HEADLESS-1\nbindsym Mod4+m exec quickshell -p '+shlex.quote(str(qml))+' ipc call fullscreenMenuAudit keyboardMenu\n')
    launch([a.sway,'--unsupported-gpu','-c',str(config)],'sway.log');runtime=Path(env['XDG_RUNTIME_DIR']);env['SWAYSOCK']=str(wait(lambda:next(runtime.glob('sway-ipc*.sock'),None)));env['WAYLAND_DISPLAY']=wait(lambda:next((path.name for path in runtime.glob('wayland-*') if not path.name.endswith('.lock')),None))
    launch(['wtype','-s','120000'],'keyboard.log');pointer=sp.Popen([a.pointer_tool,'pointer','move','0','0'],env=env,stdin=sp.PIPE,text=True,start_new_session=True);processes.append(pointer);command('m 0.5 0.5');launch(['quickshell','-p',str(fixture)],'shell.log');wait(lambda:state()['available'] and len(state()['bars'])==2);launch(['quickshell','-p',str(client)],'client.log');wait(native)
    move(500,300);(out/'desktop-state.json').write_text(json.dumps(dict(state=state(),tree=json.loads(call('swaymsg','-r','-t','get_tree')),outputs=json.loads(call('swaymsg','-r','-t','get_outputs'))),indent=2));assert all(row['visible'] and row['zone']==28 for row in state()['bars']);painted('desktop.png',True);checks=['Desktop retains its menu and reserved work area on both outputs']
    preference('always',legacy=True);wait(lambda:all(value['visible'] and value['zone']==28 for value in state()['bars']));checks.append('Legacy automatic startup and always live snapshots migrate without warnings or competing fields')
    preference('fullscreen');move(500,300);wait(lambda:all(not value['visible'] and value['zone']==0 for value in state()['bars']))
    def desktop_painted(name,first,second):
        def capture():
            call('grim',str(out/name))
            with Image.open(out/name) as source:image=source.convert('RGB')
            return all((min(image.getpixel((x,10)))>20 if shown else max(image.getpixel((x,10)))<10) for x,shown in [(300,first),(1100,second)])
        wait(capture)
    desktop_painted('desktop-hidden.png',False,False)
    move(300,0);wait(lambda:row('HEADLESS-1')['visible'] and not row('HEADLESS-2')['visible']);desktop_painted('desktop-local-reveal.png',True,False)
    click(34,14);wait(lambda:row('HEADLESS-1')['menu']=='Desktop');key('-k','Escape');move(500,300);wait(lambda:all(not value['visible'] for value in state()['bars']))
    key('-M','logo','-k','m','-m','logo');wait(lambda:row('HEADLESS-1')['menu']=='Desktop');key('-k','Escape');wait(lambda:not row('HEADLESS-1')['visible'])
    call('swaymsg','[con_id='+str(native()['id'])+']','focus');call('quickshell','-p',str(client),'ipc','call','client','focus');rect=native()['rect'];before=observed();click(rect['x']+rect['width']/2,rect['y']+rect['height']/2);key('-k','k');wait(lambda:observed()['keys']==before['keys']+1);assert observed()['clicks']==before['clicks']+1
    checks.append('Desktop autohide removes reserved space; output-local pointer and keyboard menus return exact client input')
    preference('auto_hide');move(500,300);wait(lambda:all(value['autoHide'] and not value['visible'] for value in state()['bars']))
    preference('always');wait(lambda:all(value['visible'] and value['zone']==28 for value in state()['bars']));desktop_painted('desktop-live-always.png',True,True)
    preference('desktop');wait(lambda:all(value['visible'] and value['zone']==28 for value in state()['bars']));checks.append('Live canonical desktop/always/auto_hide choices have one visibility authority on both outputs')
    identity=native()['id'];call('swaymsg','[con_id='+str(identity)+']','focus');call('quickshell','-p',str(client),'ipc','call','client','focus');call('swaymsg','[con_id='+str(identity)+']','fullscreen','enable');wait(lambda:row('HEADLESS-1')['automatic'] and not row('HEADLESS-1')['visible']);assert row('HEADLESS-2')['visible'] and not row('HEADLESS-2')['fullscreen'];assert native()['fullscreen_mode']==1;painted('automatic-hidden.png',False)
    before=observed();key('-k','k');wait(lambda:observed()['keys']==before['keys']+1);click(400,200);assert observed()['clicks']==before['clicks']+1
    move(300,0);wait(lambda:row('HEADLESS-1')['visible']);painted('automatic-revealed.png',True);assert native()['fullscreen_mode']==1
    click(34,14);wait(lambda:row('HEADLESS-1')['menu']=='Desktop');key('-k','Escape');wait(lambda:row('HEADLESS-1')['menu']=='');move(400,200);wait(lambda:not row('HEADLESS-1')['visible']);painted('automatic-left.png',False);assert native()['fullscreen_mode']==1
    before=observed();click(400,200);key('-k','k');wait(lambda:observed()['keys']==before['keys']+1);assert observed()['clicks']==before['clicks']+1;checks.append('Automatic top-edge reveal, menu click, Escape and leave preserve fullscreen and restore exact app input')
    key('-M','logo','-k','m','-m','logo');wait(lambda:row('HEADLESS-1')['menu']=='Desktop' and row('HEADLESS-1')['visible']);key('-k','Escape');wait(lambda:not row('HEADLESS-1')['visible']);assert native()['fullscreen_mode']==1;checks.append('Physical keyboard route opens a hidden menu and Escape returns to the fullscreen client')
    preference('fullscreen');move(400,200);wait(lambda:row('HEADLESS-1')['visible'] and not row('HEADLESS-1')['autoHide'] and not row('HEADLESS-2')['visible']);desktop_painted('fullscreen-only-visible.png',True,False);assert native()['fullscreen_mode']==1
    preference('auto_hide');wait(lambda:all(value['autoHide'] and not value['visible'] for value in state()['bars']));assert native()['fullscreen_mode']==1
    preference('desktop');wait(lambda:row('HEADLESS-1')['automatic'] and not row('HEADLESS-1')['visible'] and row('HEADLESS-2')['visible']);checks.append('Fullscreen-only and everywhere-autohide modes independently respect the other desktop output')
    preference('always');wait(lambda:not row('HEADLESS-1')['automatic'] and row('HEADLESS-1')['visible']);assert row('HEADLESS-1')['zone']==0;painted('always.png',True);move(400,200);time.sleep(.7);assert row('HEADLESS-1')['visible'];assert native()['fullscreen_mode']==1
    preference('automatic');wait(lambda:row('HEADLESS-1')['automatic'] and not row('HEADLESS-1')['visible']);painted('live-automatic.png',False);checks.append('Live always/automatic presentation changes preserve fullscreen; fullscreen reserves no desktop work area')
    call('swaymsg','[con_id='+str(identity)+']','fullscreen','enable','global');wait(lambda:native()['fullscreen_mode']==2 and all(value is True for value in state()['outputs'].values()));move(500,200);wait(lambda:all(not value['visible'] for value in state()['bars']));painted('global-hidden.png',False,False)
    move(1100,0);wait(lambda:row('HEADLESS-2')['visible']);painted('global-secondary-revealed.png',False,True);click(834,14);wait(lambda:row('HEADLESS-2')['menu']=='Desktop');key('-k','Escape');move(1100,200);wait(lambda:not row('HEADLESS-2')['visible']);assert native()['fullscreen_mode']==2;checks.append('Actual global fullscreen covers both outputs while secondary menu reveal/click/Escape preserves mode2')
    call('swaymsg','[con_id='+str(identity)+']','fullscreen','disable');call('swaymsg','[con_id='+str(identity)+']','fullscreen','enable');wait(lambda:row('HEADLESS-1')['fullscreen'] and not row('HEADLESS-2')['fullscreen']);move(400,200)
    call('swaymsg','output','HEADLESS-1','disable');wait(lambda:len(state()['bars'])==1 and 'HEADLESS-1' not in state()['outputs'] and row('HEADLESS-2')['fullscreen']);assert native()['fullscreen_mode']==1 and native()['id']==identity
    remaining=next(output for output in json.loads(call('swaymsg','-r','-t','get_outputs')) if output['name']=='HEADLESS-2')['rect'];move(remaining['x']+300,remaining['y']);wait(lambda:row('HEADLESS-2')['visible'])
    def remaining_painted():
        call('grim','-o','HEADLESS-2',str(out/'migrated-menu.png'))
        with Image.open(out/'migrated-menu.png') as source:pixel=source.convert('RGB').getpixel((300,10))
        return min(pixel)>20 and max(pixel)<120 and max(pixel)-min(pixel)<25
    wait(remaining_painted);click(remaining['x']+34,remaining['y']+14);(out/'removal-click.json').write_text(json.dumps(dict(state=state(),client=observed(),outputs=json.loads(call('swaymsg','-r','-t','get_outputs')),seats=json.loads(call('swaymsg','-r','-t','get_seats'))),indent=2));wait(lambda:row('HEADLESS-2')['menu']=='Desktop');key('-k','Escape');move(remaining['x']+300,remaining['y']+200);wait(lambda:not row('HEADLESS-2')['visible']);checks.append('Removing the fullscreen output migrates the same client and its local reveal/menu behavior without orphaned chrome')
    call('swaymsg','[con_id='+str(identity)+']','fullscreen','disable');wait(lambda:not row('HEADLESS-2')['fullscreen'] and row('HEADLESS-2')['visible']);assert row('HEADLESS-2')['zone']==28
    log=(out/'shell.log').read_text();assert not any(marker in log for marker in ('WARN scene:','ReferenceError:','TypeError:','Binding loop','Failed to load configuration','Cannot assign')),log
    (out/'result.json').write_text(json.dumps(dict(passed=True,checks=checks,final=state(),client=observed(),scope='Actual private fullscreen native client, production menu/provider, two outputs and physical pointer/keyboard'),indent=2));print('PASS native fullscreen menu policy')
finally:
    for process in reversed(processes):
        try:os.killpg(process.pid,signal.SIGTERM)
        except ProcessLookupError:pass
    for process in reversed(processes):
        try:process.wait(timeout=3)
        except sp.TimeoutExpired:os.killpg(process.pid,signal.SIGKILL);process.wait(timeout=3)
    if 'pointer' in globals() and pointer.stdin:pointer.stdin.close()
    for _fixture_process in processes:
        if _fixture_process.poll() is None:_fixture_process.wait(timeout=5)
    close_reaped_pipes(processes)
