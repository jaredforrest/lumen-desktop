"""Enlarged owned system panels on a private small-output native session."""
import argparse
import json
import os
from pathlib import Path
from fixture_providers import isolate_providers
import re
import shutil
import signal
import subprocess as sp
import sys
import tempfile
import time
import dbus
p=argparse.ArgumentParser();p.add_argument('--output',type=Path,required=True);p.add_argument('--launcher',type=Path,required=True);p.add_argument('--sway',default='sway');p.add_argument('--theme',choices=['light','dark'],default='dark');p.add_argument('--native-frame',choices=['on','off'],default='off');p.add_argument('--effects',action='store_true');p.add_argument('--inside',action='store_true');a=p.parse_args()
if not a.inside:
    with tempfile.TemporaryDirectory(prefix='lumen-surfaces-',ignore_cleanup_errors=True) as private:
        env=dict(os.environ,HOME=private,XDG_RUNTIME_DIR=private,QT_ACCESSIBILITY='0',QT_LINUX_ACCESSIBILITY_ALWAYS_ON='0',NO_AT_BRIDGE='1',GTK_A11Y='none')
        for key in ('CONFIG','STATE','CACHE','DATA'):env['XDG_'+key+'_HOME']=str(Path(private)/key.lower())
        for key in ('SWAYSOCK','WAYLAND_DISPLAY','DISPLAY','DBUS_SESSION_BUS_ADDRESS','AT_SPI_BUS_ADDRESS'):env.pop(key,None)
        config=Path(private)/'bus.conf';config.write_text('<busconfig><type>session</type><listen>unix:tmpdir='+private+'</listen><auth>EXTERNAL</auth><policy context="default"><allow own="*"/><allow send_destination="*"/><allow receive_sender="*"/></policy></busconfig>');raise SystemExit(sp.call(['dbus-run-session','--config-file='+str(config),'--',sys.executable,__file__,*sys.argv[1:],'--inside'],env=env))
out=a.output.resolve();out.mkdir(parents=True,exist_ok=False);runtime=Path(os.environ['XDG_RUNTIME_DIR'])
prefix,tail=re.split(r'\n[ \t]*exec quickshell ',a.launcher.read_text(),maxsplit=1)
import shlex
assets=Path(shlex.split(tail)[-1]);source=Path(__file__).resolve().parents[2]/'res/lumen-desktop/shell'
env=dict(v.decode().split('=',1) for v in sp.check_output(['bash','-c',prefix+'\nexec env -0']).split(b'\0') if b'=' in v)
env.update(WLR_BACKENDS='headless',WLR_RENDERER='gles2' if a.effects else 'pixman',WLR_RENDERER_FORCE_SOFTWARE='1' if a.effects else '0',WLR_RENDERER_ALLOW_SOFTWARE='1',LIBGL_ALWAYS_SOFTWARE='1',WLR_LIBINPUT_NO_DEVICES='1',QT_QPA_PLATFORM='wayland',QT_QPA_PLATFORMTHEME='generic',QT_QUICK_BACKEND='software',QT_ACCESSIBILITY='0',QT_LINUX_ACCESSIBILITY_ALWAYS_ON='0',NO_AT_BRIDGE='1',GTK_A11Y='none',GLOBAL_MENU_ENABLED='0',LUMEN_FILE_SERVICE='0',LUMEN_LAUNCH_SCOPES='0',LUMEN_NATIVE_WINDOW_CONTROLS='1',LUMEN_UNIFIED_OWNED_HEADERS='0' if a.native_frame=='on' else '1',LUMEN_COMPOSITOR_EFFECTS='1' if a.effects else '0')
for key in ('WAYLAND_DISPLAY','SWAYSOCK','DISPLAY','AT_SPI_BUS_ADDRESS'):env.pop(key,None)
palette=json.loads((source.parent/'appearance/palette.json').read_text());appearance=out/'appearance.json';appearance.write_text(json.dumps({'font':palette['font'],'palette':palette[a.theme],'preferences':{'mode':a.theme,'reduceMotion':True,'textScale':1.5}}));env['LUMEN_APPEARANCE_STATE']=str(appearance)
fakebin=out/'bin';fakebin.mkdir()
calendar=fakebin/'lumen-calendar'
calendar.write_text('#!'+sys.executable+'\nimport json,sys\nr=json.loads(sys.stdin.readline());assert r["action"] in ("list","load");print(json.dumps(dict(configured=False,events=[],monthEvents=[],days=[],total=0,providers=[],month=r.get("month"),date=r.get("selectedDate"))))\n')
calendar.chmod(0o700);env['PATH']=str(fakebin)+':'+env['PATH']
notification=fakebin/'notifications.py'
notification.write_text('#!'+sys.executable+'\n'+"""import json,sys
row=dict(id=1,app='Synthetic application',desktop='fixture',summary='A long notification heading remains available with enlarged interface text',body='A private synthetic notification body with several lines of explanatory detail.',timestamp=0,actions=[dict(id='default',label='Open synthetic notification')],icon='',image='',transient=False,banner=False,unread=False,urgency=1)
print(json.dumps(dict(transport=sys.argv[-1],instance='a'*32,revision=1,ready=True,rows=[row],dnd=False,previewsVisible=True,focus=dict(profiles=[],active='',name='',until=0,scheduled=False))),flush=True)
for line in sys.stdin:pass
""")
env['LUMEN_NOTIFICATION_HELPER']=str(notification)
processes=[]
def launch(command,name):
    with (out/name).open('w') as log:proc=sp.Popen(command,env=env,stdout=log,stderr=sp.STDOUT,start_new_session=True)
    processes.append(proc);return proc

def call(*args):return sp.check_output(args,env=env,text=True,timeout=10).strip()
def wait(fn):
    end=time.monotonic()+20
    while time.monotonic()<end:
        try:
            value=fn()
            if value:return value
        except (OSError,ValueError,sp.SubprocessError):pass
        time.sleep(.1)
    raise AssertionError('Timed out waiting for painted surface')

def nodes(node):
    yield node
    for child in node.get('nodes',[])+node.get('floating_nodes',[]):yield from nodes(child)
fixture=out/'assets'/'shell'
shutil.copytree(source.parent,out/'assets',symlinks=True)
isolate_providers(fixture,out)
original=(source/'shell.qml').read_text()
addition='''
 property var auditWindows:[]
 IpcHandler {target:"popoverAudit"
  function open(name:string):void {root.claimPopup("");sessionOptions.visible=false;if(name==="session"){sessionOptions.executeActions=false;sessionOptions.show("reboot");sessionOptions.error="A deliberately long translated diagnostic describes why the application observation is unavailable. Save your documents and cancel to keep this session open.";}else root.showMenu(name);}
  function longLabels():void {for(const window of root.auditWindows){function scan(item){if(item.label!==undefined && item.label==="Displays")item.label="Display configuration and arrangement preferences";for(const child of item.children || [])scan(child);}scan(window.contentItem);}}
  function ready():bool{return true;}
  function state():string {
   let focused=null,texts=[];
   for(const window of root.auditWindows.concat([sessionOptions])){if(!window.visible || !window.contentItem)continue;
    function scan(item){if(!item.visible)return;
     if(item.text!==undefined && item.font!==undefined && item.width>0 && item.height>0)texts.push({text:String(item.text),size:item.font.pixelSize,width:item.width,height:item.height,truncated:!!item.truncated});
     if(item.activeFocus && item.activeFocusOnTab){const pos=item.mapToItem(window.contentItem,0,0);let reachable=pos.x>=0 && pos.y>=0 && pos.x+item.width<=window.width+1 && pos.y+item.height<=window.height+1;let ancestor=item.parent;while(ancestor){if(ancestor.clip){const at=item.mapToItem(ancestor,0,0);reachable=reachable && at.y>=-1 && at.y+item.height<=ancestor.height+1;}ancestor=ancestor.parent;}focused={text:item.text || item.accessibleLabel || item.objectName || String(item),x:pos.x,y:pos.y,width:item.width,height:item.height,reachable:reachable};}
     for(const child of item.children || [])scan(child);
    }scan(window.contentItem);
   }return JSON.stringify({focused:focused,texts:texts});
  }
 }
'''
menu=(fixture/'MenuBar.qml').read_text();menu=menu.replace('id: dismiss','id: dismiss;Component.onCompleted:bar.shell.auditWindows=bar.shell.auditWindows.concat([dismiss])');(fixture/'MenuBar.qml').write_text(menu)
(fixture/'shell.qml').write_text(original[:original.rfind('}')]+addition+'}\n')
try:
    env['LUMEN_MPRIS_CALLS']=str(out/'media-calls.jsonl')
    launch([sys.executable,str(Path(__file__).parent/'fixtures/mpris_mock.py')],'media-provider.log');bus=dbus.SessionBus();wait(lambda:bus.name_has_owner('org.lumen.MediaFixture'));mediaProvider=dbus.Interface(bus.get_object('org.lumen.MediaFixture','/org/lumen/MediaFixture'),'org.lumen.MediaFixture')
    for player in ('alpha','beta'):mediaProvider.Add(json.dumps(dict(name=player,identity='Synthetic '+player,properties=dict(PlaybackStatus='Playing' if player=='alpha' else 'Paused',Metadata={'mpris:trackid':'/fixture/'+player,'mpris:length':180000000,'xesam:title':'An extended synthetic track title for enlarged media controls','xesam:artist':['An extended synthetic artist name']}))))
    config=out/'sway.conf';config.write_text('output HEADLESS-1 resolution 640x480\nxwayland disable\ndefault_border none\nfor_window [app_id="org.quickshell"] floating enable, resize set 420 300\n')
    if a.native_frame=='on':
        with config.open('a') as stream:stream.write('default_border normal 1\ndefault_floating_border normal 1\nfor_window [app_id="org.quickshell"] border normal 1\n')
    launch([a.sway,'--unsupported-gpu','-c',str(config)],'sway.log')
    env['SWAYSOCK']=str(wait(lambda:next(runtime.glob('sway-ipc*.sock'),None)));env['WAYLAND_DISPLAY']=wait(lambda:next((f.name for f in runtime.glob('wayland-*') if not f.name.endswith('.lock')),None))
    launch(['wtype','-s','120000'],'keyboard.log')
    launch(['quickshell','-p',str(fixture)],'shell.log')
    def ipc(method,*args):return call('quickshell','-p',str(fixture),'ipc','call','popoverAudit',method,*args)
    wait(lambda:ipc('ready')=='true');checks=[]
    for name in ('Controls','Now Playing','Date','Calendar','session'):
        ipc('open',name);time.sleep(.6)
        if name=='Controls':ipc('longLabels')
        snapshots=[]
        for index in range(35):
            call('wtype','-k','Tab');time.sleep(.08);value=json.loads(ipc('state'));snapshots.append(value)
            if value['focused']:assert value['focused']['reachable'],(name,index,value['focused'])
        (out/(name+'-focus.json')).write_text(json.dumps(snapshots,indent=2))
        call('grim',str(out/(name+'.png')))
        from PIL import Image,ImageStat
        with Image.open(out/(name+'.png')) as image:assert max(ImageStat.Stat(image.convert('RGB')).stddev)>10
        commands=sorted(set(str(s['focused']['text']) for s in snapshots if s['focused']));assert len(commands)>=2,(name,commands)
        if name=='Controls':
            labels=[text for snap in snapshots for text in snap['texts'] if text['text']=='Display configuration and arrangement preferences'];assert labels and all(not row['truncated'] and row['size']==18 for row in labels),labels
            assert 'System Settings…' in commands,commands
        if name=='Now Playing':assert {'media-source','media-position','media-volume','media-play','media-next','media-previous'}.issubset(commands),commands
        if name=='Date':assert 'Edit Widgets' in commands and 'Open Calendar' in commands,commands
        if name=='session':assert 'Cancel' in commands and 'Continue' in commands,commands
        checks.append(dict(surface=name,focusedCommands=commands))
    log=(out/'shell.log').read_text();assert not any(marker in log for marker in ('WARN scene:','ReferenceError:','TypeError:','Binding loop','Failed to load configuration','Cannot assign')),log
    (out/'result.json').write_text(json.dumps(dict(passed=True,checks=checks,textScale=1.5,output=[640,480],providers='Private synthetic notification/calendar and real private MPRIS services'),indent=2));print('PASS small-output system popover text/reachability')
finally:
    for process in reversed(processes):
        try:os.killpg(process.pid,signal.SIGTERM)
        except ProcessLookupError:pass
    for process in reversed(processes):
        try:process.wait(timeout=3)
        except sp.TimeoutExpired:os.killpg(process.pid,signal.SIGKILL);process.wait(timeout=3)
