"""Real private MPRIS transport and native media-control intent acceptance."""
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
env.update(WLR_BACKENDS='headless',WLR_RENDERER='gles2' if a.effects else 'pixman',WLR_RENDERER_FORCE_SOFTWARE='1' if a.effects else '0',WLR_RENDERER_ALLOW_SOFTWARE='1',LIBGL_ALWAYS_SOFTWARE='1',WLR_LIBINPUT_NO_DEVICES='1',QT_QPA_PLATFORM='wayland',QT_QPA_PLATFORMTHEME='generic',QT_QUICK_BACKEND='software',QT_ACCESSIBILITY='0',QT_LINUX_ACCESSIBILITY_ALWAYS_ON='0',NO_AT_BRIDGE='1',GTK_A11Y='none',LUMEN_FILE_SERVICE='0',LUMEN_LAUNCH_SCOPES='0',LUMEN_NATIVE_WINDOW_CONTROLS='0',LUMEN_NATIVE_WINDOW_ACTIONS='0',LUMEN_UNIFIED_OWNED_HEADERS='1')
for key in ('DISPLAY','WAYLAND_DISPLAY','SWAYSOCK','AT_SPI_BUS_ADDRESS'):env.pop(key,None)
palettes=json.loads((repo/'res/lumen-desktop/appearance/palette.json').read_text());appearance=out/'appearance.json';appearance.write_text(json.dumps(dict(font=palettes['font'],palette=palettes['dark'],preferences={})));env['LUMEN_APPEARANCE_STATE']=str(appearance)
shutil.copytree(repo/'res/lumen-desktop',out/'assets')
from PIL import Image,ImageStat
env['LUMEN_COMPOSITOR_EFFECTS']='1' if a.effects else '0'
qml=out/'shell.qml';qml.write_text('''import QtQuick
import QtQuick.Controls
import Quickshell
import Quickshell.Io
import Quickshell.Wayland
import "assets/shell" as D
ShellRoot {
 PanelWindow {id:window;anchors {top:true;bottom:true;left:true;right:true}color:"#256048";WlrLayershell.keyboardFocus:WlrKeyboardFocus.Exclusive
  D.MediaControl {id:media;x:30;y:30;width:320;height:implicitHeight;expanded:true}
 }
 function find(name,item){if(item.objectName===name)return item;for(const child of item.children || []){const result=find(name,child);if(result)return result;}return null;}
 IpcHandler {target:"mediaAudit"
  function state():string {return JSON.stringify({players:media.players.map(p=>({name:p.dbusName,title:p.trackTitle,identity:p.identity})),selected:media.player?media.player.dbusName:"",title:find("media-title",media).text,artist:find("media-artist",media).text,playing:media.player?media.player.isPlaying:false,volumeSupported:media.player?media.player.volumeSupported:false,volume:media.player?media.player.volume:0,seek:find("media-position",media).enabled,volumeEnabled:find("media-volume",media).enabled,nextEnabled:find("media-next",media).enabled,playEnabled:find("media-play",media).enabled,playPressed:find("media-play",media).pressed,pending:find("media-play",media).pendingIntent?{operation:find("media-play",media).pendingIntent.operation,live:media.live(find("media-play",media).pendingIntent.target)}:null,height:media.height,sourceOpen:find("media-source",media).popup.visible,choices:find("media-source",media).choices.map(row=>row.label)});}
  function select(name:string):void {media.selectedPlayer=name;}
  function focus(name:string):void {find(name,media).forceActiveFocus();}
  function geometry(name:string):string {const item=find(name,media);const at=item.mapToItem(window.contentItem,0,0);return JSON.stringify({x:at.x,y:at.y,width:item.width,height:item.height});}
 }
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
        except (OSError,ValueError,StopIteration,sp.SubprocessError,dbus.DBusException):pass
        time.sleep(.1)
    (out/'failure-state.json').write_text(json.dumps(state(),indent=2));raise AssertionError('Media fixture timed out')
def ipc(method,*values):return call('quickshell','-p',str(qml),'ipc','call','mediaAudit',method,*values)
def state():return json.loads(ipc('state'))
def point(name,rx=.5):
    r=json.loads(ipc('geometry',name));return r['x']+r['width']*rx,r['y']+r['height']/2

def command(value):pointer.stdin.write(value+'\n');pointer.stdin.flush();time.sleep(.12)
def move(position):
    x,y=position;call('swaymsg','seat','seat0','cursor','set',str(round(x)),str(round(y)));command('m 0.5 0.5')
def click(name,rx=.5):move(point(name,rx));command('p');command('r')
def key(*keys):call('wtype',*keys);time.sleep(.12)
env['LUMEN_MPRIS_CALLS']=str(out/'calls.jsonl')
import dbus
# The accessibility transport is private to this fixture as well.
env.update(QT_ACCESSIBILITY='1',QT_LINUX_ACCESSIBILITY_ALWAYS_ON='1',GSETTINGS_BACKEND='memory')
for envkey in ('NO_AT_BRIDGE','GTK_A11Y'):env.pop(envkey,None)
try:
    atspi=Path(os.environ['LUMEN_TEST_AT_SPI']);launch([str(atspi/'libexec/at-spi-bus-launcher'),'--launch-immediately','--a11y=1','--screen-reader=1'],'a11y-bus.log');session=dbus.SessionBus();wait(lambda:session.name_has_owner('org.a11y.Bus'));address=str(session.get_object('org.a11y.Bus','/org/a11y/bus').GetAddress(dbus_interface='org.a11y.Bus'));env['AT_SPI_BUS_ADDRESS']=address
    a11y=dbus.bus.BusConnection(address);launch([str(atspi/'libexec/at-spi2-registryd')],'registry.log');wait(lambda:a11y.name_has_owner('org.a11y.atspi.Registry'))
    def interface(ref,name):return dbus.Interface(a11y.get_object(*ref,introspect=False),name)
    def accessible(name):
        pending=[('org.a11y.atspi.Registry','/org/a11y/atspi/accessible/root')];seen=set();rows=[];found=None
        while pending and len(rows)<150:
            ref=tuple(str(v) for v in pending.pop(0))
            if ref in seen or ref[1]=='/org/a11y/atspi/null':continue
            seen.add(ref);item=interface(ref,'org.a11y.atspi.Accessible');label=str(interface(ref,'org.freedesktop.DBus.Properties').Get('org.a11y.atspi.Accessible','Name'));rows.append(dict(name=label,role=str(item.GetRoleName())))
            if label==name:found=ref
            pending.extend(item.GetChildren())
        (out/'accessibility-tree.json').write_text(json.dumps(rows,indent=2));return found

    launch([sys.executable,str(repo/'tests/desktop/fixtures/mpris_mock.py')],'provider.log');bus=dbus.SessionBus();wait(lambda:bus.name_has_owner('org.lumen.MediaFixture'));provider=dbus.Interface(bus.get_object('org.lumen.MediaFixture','/org/lumen/MediaFixture'),'org.lumen.MediaFixture')
    def add(name,**properties):provider.Add(json.dumps(dict(name=name,identity='Player '+name,properties=properties)))
    def configure(name,**properties):provider.Configure(name,json.dumps(properties))
    def calls():return json.loads(str(provider.Calls()))
    config=out/'sway.conf';config.write_text('output HEADLESS-1 resolution 800x600\nxwayland disable\n')
    launch([a.sway,'--unsupported-gpu','-c',str(config)],'sway.log');runtime=Path(env['XDG_RUNTIME_DIR']);env['SWAYSOCK']=str(wait(lambda:next(runtime.glob('sway-ipc*.sock'),None)));env['WAYLAND_DISPLAY']=wait(lambda:next((path.name for path in runtime.glob('wayland-*') if not path.name.endswith('.lock')),None))
    launch(['wtype','-s','120000'],'keyboard.log');pointer=sp.Popen([a.pointer_tool,'pointer','move','0','0'],env=env,stdin=sp.PIPE,text=True,start_new_session=True);processes.append(pointer);command('m 0.5 0.5');launch(['quickshell','-p',str(qml)],'shell.log')
    wait(lambda:state()['title']=='Not Playing');assert not state()['playEnabled'];checks=['No service leaves playback unavailable']
    add('alpha',PlaybackStatus='Playing');wait(lambda:state()['playing'] and state()['title']=='Synthetic title');assert state()['artist']=='Synthetic artist' and state()['volumeSupported'] and state()['seek']
    add('beta',PlaybackStatus='Paused',Metadata={'mpris:trackid':'/fixture/beta','mpris:length':240000000,'xesam:title':'Second title','xesam:artist':['Second artist']});wait(lambda:len(state()['players'])==2)
    ipc('focus','media-source');key('-k','End');wait(lambda:state()['selected'].endswith('.beta'));assert state()['title']=='Second title'
    before=len(calls());ipc('focus','media-play');key('-k','space');wait(lambda:len(calls())==before+1);assert calls()[-1]['name'].endswith('.beta') and calls()[-1]['method'] in ('Play','PlayPause')
    def painted():
        call('grim',str(out/'supported.png'))
        with Image.open(out/'supported.png') as image:return max(ImageStat.Stat(image.convert('RGB').crop((30,30,350,round(30+state()['height'])))).stddev)>10
    wait(painted)
    checks.append('Actual MPRIS properties supply title/artist/status and keyboard source selection/play target the second player')
    add('gamma');wait(lambda:len(state()['players'])==3);ipc('focus','media-source');key('-k','space');wait(lambda:state()['sourceOpen']);provider.Remove('gamma');add('delta');wait(lambda:len(state()['players'])==3);assert 'Player gamma' in state()['choices'] and 'Player delta' not in state()['choices'];key('-k','End','-k','Return');wait(lambda:not state()['sourceOpen']);assert state()['selected'].endswith('.beta'),state();provider.Remove('delta');wait(lambda:len(state()['players'])==2);checks.append('An open source chooser retains its labels and identities; a removed choice cannot select a replacement row')
    before=len(calls());move(point('media-next'));command('p');ipc('select','org.mpris.MediaPlayer2.alpha');command('r');assert len(calls())==before,calls();checks.append('Pointer press/release cannot redirect a pending action to a newly selected player')
    ipc('select','org.mpris.MediaPlayer2.beta');wait(lambda:state()['selected'].endswith('.beta'));move(point('media-next'));command('p');provider.Remove('beta');wait(lambda:len(state()['players'])==1);add('beta',Metadata={'mpris:trackid':'/fixture/new','mpris:length':100000000,'xesam:title':'Replacement title'});wait(lambda:state()['title']=='Replacement title');command('r');assert len(calls())==before,calls();checks.append('Replacement owner with the same service name receives no action held on its predecessor')
    before=len(calls());ipc('focus','media-next');key('-k','space');wait(lambda:len(calls())==before+1);assert calls()[-1]['method']=='Next' and calls()[-1]['generation']==5
    before=len(calls());target=wait(lambda:accessible('Next track'));assert interface(target,'org.a11y.atspi.Action').DoAction(0);wait(lambda:len(calls())==before+1);assert calls()[-1]['method']=='Next' and calls()[-1]['generation']==5
    assert accessible('Media source') and accessible('Player volume') and accessible('Now Playing');checks.append('Actual private AT-SPI exposes media context/source/volume and dispatches the enabled Next action to its exact owner')
    configure('beta',PlaybackStatus='Paused');wait(lambda:not state()['playing']);before=len(calls());move(point('media-play'));command('p');(out/'held-play-before.json').write_text(json.dumps(state(),indent=2));configure('beta',PlaybackStatus='Playing');wait(lambda:state()['playing']);(out/'held-play-after.json').write_text(json.dumps(state(),indent=2));command('r');wait(lambda:state()['pending'] is None);assert state()['playing'] and all(row['method']=='Play' for row in calls()[before:]),calls();checks.append('A held Play intent stays Play when another client starts the same player before release')
    configure('beta',CanGoNext=False,CanSeek=False,CanControl=False);wait(lambda:not state()['nextEnabled'] and not state()['playEnabled'] and not state()['seek'] and not state()['volumeEnabled']);before=len(calls());click('media-next');assert len(calls())==before
    checks.append('Live capability loss disables transport, position and player-volume controls without sending commands')
    configure('beta',CanControl=True,CanSeek=True,CanGoNext=True);wait(lambda:state()['volumeEnabled'] and state()['seek']);ipc('focus','media-volume');key('-k','Right');wait(lambda:len(calls())>before);assert calls()[-1]['method']=='Set' and calls()[-1]['args'][0]=='Volume';assert state()['volume']>.5
    before=len(calls());click('media-position',.7);wait(lambda:len(calls())>before);assert calls()[-1]['method']=='SetPosition'
    checks.append('Real keyboard player-volume and pointer position controls write typed MPRIS properties/methods')
    provider.Remove('beta');wait(lambda:len(state()['players'])==1);provider.Remove('alpha');wait(lambda:state()['title']=='Not Playing');add('unsupported',Volume=None,CanSeek=False);wait(lambda:len(state()['players'])==1);assert not state()['volumeSupported'] and not state()['volumeEnabled'] and not state()['seek']
    call('grim',str(out/'unsupported.png'));provider.Remove('unsupported');wait(lambda:not state()['players']);call('grim',str(out/'unavailable.png'));checks.append('Missing optional volume/seek capabilities and disappearing services remain honest and unavailable')
    log=(out/'shell.log').read_text();assert not any(marker in log for marker in ('WARN scene:','ReferenceError:','TypeError:','Binding loop','Failed to load configuration','Cannot assign')),log
    (out/'result.json').write_text(json.dumps(dict(passed=True,checks=checks,calls=calls(),scope='Private real D-Bus MPRIS services, native compositor and actual pointer/keyboard'),indent=2));print('PASS native media transport and captured player identity')
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
