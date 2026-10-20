"""Painted-content proof for owned surfaces in an isolated headless session."""
import argparse
import json
import os
from pathlib import Path
import re
import shutil
import signal
import subprocess as sp
import sys
import tempfile
import time
p=argparse.ArgumentParser();p.add_argument('--output',type=Path,required=True);p.add_argument('--launcher',type=Path,required=True);p.add_argument('--sway',default='sway');p.add_argument('--theme',choices=['light','dark'],required=True);p.add_argument('--native-frame',choices=['on','off'],default='off');p.add_argument('--inside',action='store_true');a=p.parse_args()
if not a.inside:
    with tempfile.TemporaryDirectory(prefix='lumen-surfaces-',ignore_cleanup_errors=True) as private:
        env=dict(os.environ,HOME=private,XDG_RUNTIME_DIR=private,QT_ACCESSIBILITY='0',QT_LINUX_ACCESSIBILITY_ALWAYS_ON='0',NO_AT_BRIDGE='1',GTK_A11Y='none')
        for key in ('CONFIG','STATE','CACHE','DATA'):env['XDG_'+key+'_HOME']=str(Path(private)/key.lower())
        for key in ('SWAYSOCK','WAYLAND_DISPLAY','DISPLAY','DBUS_SESSION_BUS_ADDRESS','AT_SPI_BUS_ADDRESS'):env.pop(key,None)
        raise SystemExit(sp.call(['dbus-run-session','--',sys.executable,__file__,*sys.argv[1:],'--inside'],env=env))
out=a.output.resolve();out.mkdir(parents=True,exist_ok=False);runtime=Path(os.environ['XDG_RUNTIME_DIR'])
prefix,tail=re.split(r'\n[ \t]*exec quickshell ',a.launcher.read_text(),maxsplit=1)
import shlex
assets=Path(shlex.split(tail)[-1]);source=Path(__file__).resolve().parents[2]/'res/lumen-desktop/shell'
env=dict(v.decode().split('=',1) for v in sp.check_output(['bash','-c',prefix+'\nexec env -0']).split(b'\0') if b'=' in v)
env.update(WLR_BACKENDS='headless',WLR_RENDERER='pixman',WLR_LIBINPUT_NO_DEVICES='1',QT_QPA_PLATFORM='wayland',QT_QPA_PLATFORMTHEME='generic',QT_QUICK_BACKEND='software',QT_ACCESSIBILITY='0',QT_LINUX_ACCESSIBILITY_ALWAYS_ON='0',NO_AT_BRIDGE='1',GTK_A11Y='none',GLOBAL_MENU_ENABLED='0',LUMEN_FILE_SERVICE='0',LUMEN_LAUNCH_SCOPES='0',LUMEN_NATIVE_WINDOW_CONTROLS='1',LUMEN_UNIFIED_OWNED_HEADERS='0' if a.native_frame=='on' else '1')
for key in ('WAYLAND_DISPLAY','SWAYSOCK','DISPLAY','AT_SPI_BUS_ADDRESS'):env.pop(key,None)
palette=json.loads((source.parent/'appearance/palette.json').read_text());appearance=out/'appearance.json';appearance.write_text(json.dumps({'font':palette['font'],'palette':palette[a.theme],'preferences':{'mode':a.theme,'reduceMotion':True}}));env['LUMEN_APPEARANCE_STATE']=str(appearance)
fakebin=out/'bin';fakebin.mkdir()
calendar=fakebin/'lumen-calendar'
calendar.write_text('#!'+sys.executable+'\nimport json,sys\nr=json.loads(sys.stdin.readline());print(json.dumps(dict(configured=False,events=[],monthEvents=[],days=[],total=0,providers=[],month=r.get("month"),date=r.get("selectedDate"))))\n')
calendar.chmod(0o700);env['PATH']=str(fakebin)+':'+env['PATH']
processes=[]
def launch(command,name):
    with (out/name).open('w') as log:
        proc=sp.Popen(command,env=env,stdout=log,stderr=sp.STDOUT,start_new_session=True)
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
original=(source/'shell.qml').read_text()
addition='''
    FileInfoWindow {id:auditInfo}
    PanelWindow {id:auditFocus;visible:false;anchors {top:true;right:true}margins {top:38;right:12}implicitWidth:300;implicitHeight:auditFocusPanel.height;color:"transparent";exclusionMode:ExclusionMode.Ignore
        SystemPanels {id:auditFocusPanel;width:300;kind:"Focus";shell:root;today:new Date();editingFocus:"work"}
    }
    IpcHandler {target:"surfaceAudit"
        function open(name:string):void {
            root.claimPopup("");auditFocus.visible=false;root.hudVisible=false;notificationState.rows=[];applicationSwitcher.visible=false;windowLayoutMenu.visible=false;
            for(const window of [helpWindow,sessionOptions,remoteDesktopWindow,overview,textTools,quickLook,auditInfo,forceQuitDialog,downloadsStack])window.visible=false;
            if(name==="help")root.openHelp();
            else if(name==="session"){sessionOptions.executeActions=false;root.sessionAction("suspend");}
            else if(name==="remote")root.openRemoteDesktop();
            else if(name==="overview")root.openOverview(false);
            else if(name==="text")root.openTextTools();
            else if(name==="quick")quickLook.show("/tmp/Example.txt",null);
            else if(name==="info")auditInfo.show({name:"Example document.txt",path:"/fixture/Example document.txt",size:"123 bytes",type:"Text document",modified:"Today",permissions:"Read and write"});
            else if(name==="focus-editor"){notificationState.focusState={profiles:[{id:"work",name:"Work",allowedApps:[],allowCritical:false,schedules:[{days:[0,1,2,3,4],start:540,end:1020}]}],active:"",name:"",until:0,scheduled:false};auditFocus.visible=true;}
            else if(name==="forcequit")forceQuitDialog.show("Fixture Application",[]);
            else if(name==="folder")downloadsStack.show("/tmp",root.preferredOutput(),{presentation:"list"});
            else if(name==="switcher"){applicationSwitcher.items=[{key:"fixture",window:null,label:"Fixture Application",icon:Qt.resolvedUrl("icons/application.svg")}];applicationSwitcher.selected=0;applicationSwitcher.visible=true;}
            else if(name==="layout"){windowLayoutMenu.event(JSON.stringify({lumen_layout:1,id:42,x:300,y:100}));windowLayoutMenu.open();}
            else if(name==="hud"){root.hudOutput=root.preferredOutput();root.hudVisible=true;}
            else if(name==="banner"){notificationState.available=true;notificationState.bannerOutput=root.preferredOutput();notificationState.rows=[{id:1,app:"Fixture",desktop:"",summary:"Saved successfully",body:"A visible notification body.",icon:"",image:"",timestamp:Date.now(),actions:[],banner:true,unread:true,urgency:1,transient:false}];}
            else root.showMenu(name);
        }
        function geometry(name:string):string {
            const window=({forcequit:forceQuitDialog,folder:downloadsStack,switcher:applicationSwitcher,layout:windowLayoutMenu,"focus-editor":auditFocus})[name];
            if(!window || !window.visible)return "null";
            if(name==="folder" && downloadsStack.rows.length===0)return "null";
            if(name==="focus-editor")return JSON.stringify({x:0,y:0,width:300,height:auditFocusPanel.height});
            let result=null;
            function visit(item){if(!item.visible)return;if(item.color!==undefined && item.color.a>0.1 && item.width>100 && item.height>70 && !(item.width>=window.width-2 && item.height>=window.height-2)){const at=item.mapToItem(window.contentItem,0,0);if(!result || item.width*item.height>result.width*result.height)result={x:at.x,y:at.y,width:item.width,height:item.height};}for(const child of item.children || [])visit(child);}
            visit(window.contentItem);return JSON.stringify(result);
        }
        function ready():bool{return true;}
    }
'''
documents=out/'Documents';documents.mkdir();(documents/'Example.txt').write_text('Fixture content')
addition=addition.replace('"/tmp"',json.dumps(str(documents))).replace('"/tmp/Example.txt"',json.dumps(str(documents/'Example.txt')))
(fixture/'shell.qml').write_text(original[:original.rfind('}')]+addition+'}\n')

try:
    config=out/'sway.conf';config.write_text('output HEADLESS-1 resolution 1280x900\nxwayland disable\ndefault_border none\nfor_window [app_id="org.quickshell"] floating enable\n')
    if a.native_frame=='on':
        with config.open('a') as stream:stream.write('default_border normal 1\ndefault_floating_border normal 1\nfor_window [app_id="org.quickshell"] border normal 1\n')
    launch([a.sway,'--unsupported-gpu','-c',str(config)],'sway.log')
    env['SWAYSOCK']=str(wait(lambda:next(runtime.glob('sway-ipc*.sock'),None)));env['WAYLAND_DISPLAY']=wait(lambda:next((f.name for f in runtime.glob('wayland-*') if not f.name.endswith('.lock')),None))
    launch(['quickshell','-p',str(fixture)],'shell.log')
    def ipc(method,*values):return call('quickshell','-p',str(fixture),'ipc','call','surfaceAudit',method,*values)
    wait(lambda:ipc('ready')=='true')
    from PIL import Image,ImageStat
    results=[]
    for name,title in [('help','Desktop Help'),('session','Session Options'),('remote','Remote Desktop'),('overview','Window Overview'),('text','Text Tools'),('info','File Info'),('quick','Quick Look')]:
        ipc('open',name)
        def painted():
            window=next((n for n in nodes(json.loads(call('swaymsg','-r','-t','get_tree'))) if n.get('app_id')=='org.quickshell' and n.get('name')==title and n.get('visible')),None)
            if not window:return None
            if a.native_frame=='on' and window.get('deco_rect',{}).get('height',0)<=0:
                response=json.loads(call('swaymsg','[con_id='+str(window['id'])+'] border normal 1'))
                assert all(row.get('success') for row in response),response
                return None
            call('grim',str(out/(name+'.png')))
            image=Image.open(out/(name+'.png')).convert('RGB');r=window['rect'];box=(r['x']+15,r['y']+55,r['x']+r['width']-15,r['y']+r['height']-15);crop=image.crop(box);stats=ImageStat.Stat(crop)
            if max(stats.mean)<=10 or max(stats.stddev)<=2:return None
            crop.save(out/(name+'-content.png'))
            return dict(surface=name,mean=stats.mean,deviation=stats.stddev,rect=r,decoration=window.get('deco_rect'))
        results.append(wait(painted))

    # Real shell popups use published panel rectangles; sample interiors only.
    for name in ['Desktop','Application','Controls','Wi-Fi','Bluetooth','VPN','Sound','Focus','Date','Now Playing','Launcher']:
        ipc('open',name)
        def popup_painted():
            data=json.loads(call('quickshell','-p',str(fixture),'ipc','call','desktop','layout'))
            panel=next((v for v in data if v.get('menu')==name and v.get('panelWidth',0)>0),None)
            if not panel:return None
            call('grim',str(out/('popup-'+name.replace(' ','-')+'.png')))
            image=Image.open(out/('popup-'+name.replace(' ','-')+'.png')).convert('RGB')
            x=int(panel['x']);y=int(panel['y']);w=int(panel['panelWidth']);h=int(panel['panelHeight']);stats=ImageStat.Stat(image.crop((x+10,y+10,min(image.width,x+w-10),min(image.height,y+h-10))))
            if max(stats.mean)<=10 or max(stats.stddev)<=2:return None
            header=None
            if name in ['Controls','Date','Now Playing','VPN']:
                header=image.getpixel((x+w//2,y+5))
                if max(header)<=10:return None
            return dict(surface=name,mean=stats.mean,deviation=stats.stddev,layout=panel,headerMaterialPixel=header)
        results.append(wait(popup_painted))
    for name in ['forcequit','folder','switcher','layout','hud','banner','focus-editor']:
        ipc('open',name)
        def overlay_painted():
            if name=='hud':rect=dict(x=545,y=634,width=190,height=146)
            elif name=='banner':rect=dict(x=916,y=35,width=350,height=100)
            else:rect=json.loads(ipc('geometry',name))
            if name=='focus-editor' and rect:rect.update(x=968+rect['x'],y=38+rect['y'])
            if not rect:return None
            call('grim',str(out/('overlay-'+name+'.png')))
            image=Image.open(out/('overlay-'+name+'.png')).convert('RGB');x=int(rect['x']);y=int(rect['y']);w=int(rect['width']);h=int(rect['height']);crop=image.crop((x+12,y+12,x+w-12,y+h-12));stats=ImageStat.Stat(crop)
            if max(stats.mean)<=10 or max(stats.stddev)<=2:return None
            crop.save(out/('overlay-'+name+'-content.png'))
            return dict(surface=name,mean=stats.mean,deviation=stats.stddev,rect=rect)
        results.append(wait(overlay_painted))
    log=(out/'shell.log').read_text()
    assert not any(marker in log for marker in ('WARN scene:', 'ReferenceError:', 'TypeError:', 'Failed to load configuration', 'Binding loop detected', 'Cannot assign', 'Cannot open:', 'Error decoding:', 'QML Image:', 'Could not load icon')), log
    import hashlib
    digest=hashlib.sha256()
    for path in sorted((out/'assets').rglob('*')):
        if path.is_file() and path.suffix in ('.qml','.svg'):digest.update(str(path.relative_to(out/'assets')).encode());digest.update(path.read_bytes())
    (out/'result.json').write_text(json.dumps(dict(passed=True,theme=a.theme,renderer='software',nativeFrame=a.native_frame,sourceDigest=digest.hexdigest(),surfaces=results),indent=2)+'\n')
finally:
    for proc in reversed(processes):
        if proc.poll() is None:os.killpg(proc.pid,signal.SIGTERM)
    for proc in reversed(processes):
        try:proc.wait(timeout=5)
        except sp.TimeoutExpired:os.killpg(proc.pid,signal.SIGKILL)
