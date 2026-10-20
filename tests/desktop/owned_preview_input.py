"""Installed owned preview delayed-resource baseline, using synthetic private media."""
import argparse,hashlib,json,os,re,shutil,signal,subprocess as sp,sys,tempfile,time
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--launcher',type=Path,required=True);p.add_argument('--sway',required=True);p.add_argument('--output',type=Path,required=True);p.add_argument('--effects',action='store_true');p.add_argument('--inside',action='store_true');a=p.parse_args()
if not a.inside:
    with tempfile.TemporaryDirectory(prefix='lumen-owned-preview-') as temporary:
        env=dict(os.environ,HOME=temporary,XDG_RUNTIME_DIR=temporary)
        for key in ('CONFIG','STATE','CACHE','DATA'):env['XDG_'+key+'_HOME']=str(Path(temporary)/key.lower())
        for key in ('DBUS_SESSION_BUS_ADDRESS','WAYLAND_DISPLAY','SWAYSOCK','DISPLAY','AT_SPI_BUS_ADDRESS'):env.pop(key,None)
        env['DBUS_SYSTEM_BUS_ADDRESS']='unix:path='+temporary+'/no-system-bus'
        bus=Path(temporary)/'bus.conf';bus.write_text('<busconfig><type>session</type><listen>unix:tmpdir='+temporary+'</listen><policy context="default"><allow own="*"/><allow send_destination="*"/><allow receive_sender="*"/></policy></busconfig>')
        raise SystemExit(sp.call(['dbus-run-session','--config-file='+str(bus),'--',sys.executable,__file__,*sys.argv[1:],'--inside'],env=env))
out=a.output.resolve();out.mkdir(parents=True,exist_ok=False)
prefix=re.split(r'\n[ \t]*exec quickshell ',a.launcher.read_text(),maxsplit=1)[0]
env=dict(v.decode().split('=',1) for v in sp.check_output(['bash','-c',prefix+'\nexec env -0']).split(b'\0') if b'=' in v)
env.update(QT_QPA_PLATFORM='wayland',QT_QPA_PLATFORMTHEME='generic',QT_QUICK_BACKEND='software',QT_ACCESSIBILITY='0',QT_LINUX_ACCESSIBILITY_ALWAYS_ON='0',NO_AT_BRIDGE='1',GTK_A11Y='none',WLR_BACKENDS='headless',WLR_RENDERER='gles2' if a.effects else 'pixman',WLR_RENDERER_ALLOW_SOFTWARE='1',WLR_RENDERER_FORCE_SOFTWARE='1' if a.effects else '0',LIBGL_ALWAYS_SOFTWARE='1',WLR_LIBINPUT_NO_DEVICES='1',GLOBAL_MENU_ENABLED='0',LUMEN_FILE_SERVICE='0',LUMEN_LAUNCH_SCOPES='0')
for key in ('WAYLAND_DISPLAY','SWAYSOCK','DISPLAY'):env.pop(key,None)
assets=out/'assets';shutil.copytree(Path(env['LUMEN_NOTIFICATION_HELPER']).parent.parent,assets)
preview=assets/'shell/FilePreview.qml';original=preview.read_text();modified=original.replace('    property var result:', '''    property bool fixtureHold:true
    property string fixturePinnedImage:""
    property string fixturePinnedMedia:""
    readonly property int fixtureImageStatus:fixtureImage.status
    readonly property int fixtureDuration:player.duration
    readonly property string fixtureSource:player.source.toString() || fixtureImage.source.toString()
    readonly property bool fixturePlaying:player.playing
    readonly property bool fixtureStopped:player.playbackState===MediaPlayer.StoppedState
    property var result:''',1)
if 'id:previewImage' in modified:
    modified=modified.replace('id:previewImage','id:fixtureImage',1).replace('previewImage.source','fixtureImage.source').replace('sourceObjects:[previewImage,player]','sourceObjects:[fixtureImage,player]')
else:modified=modified.replace('        Image {\n','        Image {\n            id:fixtureImage\n',1)
modified=modified.replace('root.active && picture.visible','root.active && !root.fixtureHold && picture.visible').replace('root.active && root.media','root.active && !root.fixtureHold && root.media')
for name,expression in [('Image','root.active && !root.fixtureHold && picture.visible ? root.result.uri || "" : ""'),('Media','root.active && !root.fixtureHold && root.media ? root.result.uri || "" : ""')]:
    assert modified.count('source: '+expression)==1
    modified=modified.replace('source: '+expression,'source: root.fixturePinned'+name+' || ('+expression+')',1)
assert modified.count('!root.fixtureHold')==2
roundtrip=modified
for name,expression in [('Image','root.active && !root.fixtureHold && picture.visible ? root.result.uri || "" : ""'),('Media','root.active && !root.fixtureHold && root.media ? root.result.uri || "" : ""')]:roundtrip=roundtrip.replace('source: root.fixturePinned'+name+' || ('+expression+')','source: '+expression,1)
roundtrip=roundtrip.replace('root.active && !root.fixtureHold && picture.visible','root.active && picture.visible').replace('root.active && !root.fixtureHold && root.media','root.active && root.media')
start=roundtrip.index('    property bool fixtureHold:true');end=roundtrip.index('    property var result:',start);roundtrip=roundtrip[:start]+roundtrip[end:]
if 'id:previewImage' in original:roundtrip=roundtrip.replace('id:fixtureImage','id:previewImage',1).replace('fixtureImage.source','previewImage.source').replace('sourceObjects:[fixtureImage,player]','sourceObjects:[previewImage,player]')
else:roundtrip=roundtrip.replace('            id:fixtureImage\n','',1)
assert roundtrip==original
preview.chmod(0o600);preview.write_text(modified)
stateFile=assets/'shell/OwnedPreviewState.qml'
stateOriginal=stateFile.read_text() if stateFile.exists() else None
if stateOriginal:
    stateModified=stateOriginal.replace('    objectName:"owned-preview-state"','    objectName:"owned-preview-state"\n    property var fixtureAccepted:[]').replace('root.result=data;root.received(data);','root.fixtureAccepted=root.fixtureAccepted.concat([{id:data.id,path:data.sourcePath,token:data.sourceToken,duration:data.duration}]);root.result=data;root.received(data);')
    stateFile.chmod(0o600);stateFile.write_text(stateModified)
(out/'instrumentation.json').write_text(json.dumps(dict(launcher=str(a.launcher),originalSha256=hashlib.sha256(original.encode()).hexdigest(),modifiedSha256=hashlib.sha256(modified.encode()).hexdigest(),stateOriginalSha256=hashlib.sha256(stateOriginal.encode()).hexdigest() if stateOriginal else None,stateModifiedSha256=hashlib.sha256(stateModified.encode()).hexdigest() if stateOriginal else None,roundtrip=True,scope='Hash-verified reversible actual Image/MediaPlayer activation and source-retirement gates; read-only status/accepted reply log. Provider, request, result and URI unchanged.'),indent=2))
palettes=json.loads((assets/'appearance/palette.json').read_text());appearance=Path(env['XDG_STATE_HOME'])/'lumen-desktop/appearance.json';appearance.parent.mkdir(parents=True,exist_ok=True);appearance.write_text(json.dumps(dict(font=palettes['font'],palette=palettes['dark'],preferences={})))
docs=Path(env['HOME'])/'Synthetic';docs.mkdir()
from PIL import Image
for name,color in [('image.png','red'),('replacement.png','blue')]:Image.new('RGB',(320,240),color).save(docs/name)
def video(name,color,duration):sp.run(['ffmpeg','-loglevel','error','-f','lavfi','-i','color=c='+color+':s=320x240:r=10','-t',str(duration),'-an','-c:v','ffv1',str(docs/name)],env=env,check=True,timeout=20)
video('video.mkv','red',2);video('replacement.mkv','blue',6)
qml=out/'shell.qml';qml.write_text('''import QtQuick
import QtQuick.Window
import Quickshell
import Quickshell.Io
import "assets/shell" as D
ShellRoot {
 id:fixture
 property string folder:'''+json.dumps(str(docs))+'''
 QtObject {id:exported;property var state:({appName:"",title:"",windowId:0})}
 QtObject {id:stub;property int focusEpoch:0;property var windows:[];property var exportedMenu:exported;property var quickLookWindow:quick;function activeFiles(w){}function focusWindow(w){}function focusOwnedWindow(title){}function claimPopup(owner){}}
 D.FilesState {id:service;persistViewPreferences:false;path:fixture.folder;viewMode:"gallery"}
 D.FilesWindow {id:files;shell:stub;service:service;visible:true;previewOpen:false}
 D.QuickLookWindow {id:quick}
 function previews(item){let found=[];if(item.fixtureHold!==undefined)found.push(item);for(const child of item.children || [])found=found.concat(previews(child));return found;}
 function controller(item){if(item.objectName==="owned-preview-state")return item;for(const child of item.children || []){const found=controller(child);if(found)return found;}return null;}
 function visiblePreview(item){return previews(item).find(p=>p.visible && p.active);}
 IpcHandler {target:"previewAudit"
  function refreshRows():void {service.refresh();}
  function startPreview(surface:string,path:string):void {for(const p of fixture.previews(files.contentItem).concat(fixture.previews(quick.contentItem)))p.fixtureHold=true;if(surface==="quick")quick.show(path,files);else {quick.visible=false;const i=service.rows.findIndex(r=>r.path===path);files.choose(i,0);}}
  function restartFilesPreview(path:string):void {const i=service.rows.findIndex(r=>r.path===path);files.choose(i,0);files.schedulePreview();}
  function closeQuick():void {quick.visible=false;}
  function focusFiles():void {files.focusContents();}
  function release(surface:string):void {const p=surface==="quick"?quick.preview:fixture.visiblePreview(files.contentItem);if(p)p.fixtureHold=false;}
  function pinRetirement(surface:string):void {const p=surface==="quick"?quick.preview:fixture.visiblePreview(files.contentItem);if(!p)return;if(p.result.type==="media")p.fixturePinnedMedia=p.fixtureSource;else p.fixturePinnedImage=p.fixtureSource;}
  function unpinRetirement(surface:string):void {const p=surface==="quick"?quick.preview:fixture.visiblePreview(files.contentItem);if(p){p.fixturePinnedImage="";p.fixturePinnedMedia="";}}
  function page(number:int):void {quick.preview.fixtureHold=true;quick.page=number;quick.refresh();}
  function play():void {quick.preview.togglePlayback();}
  function status(surface:string):string {const p=surface==="quick"?quick.preview:fixture.visiblePreview(files.contentItem),result=surface==="quick"?quick.result:service.preview;return JSON.stringify({ready:service.ready && !service.busy,viewMode:service.viewMode,rows:service.rows.map(r=>({path:r.path,identity:r.identity})),result:result,present:!!p,held:p?p.fixtureHold:false,imageStatus:p?p.fixtureImageStatus:-1,duration:p?p.fixtureDuration:0,playing:p?p.fixturePlaying:false,stopped:p?p.fixtureStopped:true,source:p?p.fixtureSource:"",geometry:p?({title:p.Window.window.title,x:p.mapToItem(p.Window.window.contentItem,0,0).x,y:p.mapToItem(p.Window.window.contentItem,0,0).y,width:p.width,height:p.height}):null,controller:(()=>{const c=fixture.controller(surface==="quick"?quick.contentItem:service);return c?({id:c.currentId,path:c.currentPath,busy:c.busy,retiring:c.retiring,resourceRetirement:c.resourceRetirement.busy,accepted:c.fixtureAccepted}):null;})(),visible:surface==="quick"?quick.visible:files.visible});}
 }
 Component.onCompleted:service.navigate(folder)
}
''')
processes=[]
def launch(command,name):
    with (out/name).open('w') as log:proc=sp.Popen(command,env=env,stdout=log,stderr=sp.STDOUT,start_new_session=True)
    processes.append(proc);return proc

def call(*parts):return sp.check_output(parts,env=env,text=True,timeout=8).strip()
def ipc(method,*values):return call('quickshell','-p',str(qml),'ipc','call','previewAudit',method,*values)
def state(surface):return json.loads(ipc('status',surface))
def wait(fn,label):
    deadline=time.monotonic()+20
    while time.monotonic()<deadline:
        if 'shell' in globals() and shell.poll() is not None:raise AssertionError('Installed preview shell exited; see shell.log')
        try:
            value=fn()
            if value:return value
        except (OSError,ValueError,sp.SubprocessError):pass
        time.sleep(.05)
    try:(out/'failure-state.json').write_text(json.dumps({surface:state(surface) for surface in ('files','quick')},indent=2))
    except (OSError,ValueError,sp.SubprocessError):pass
    raise AssertionError('Preview fixture timeout '+label)
results=[];pausedProviders={}
try:
    conf=out/'sway.conf';conf.write_text('output HEADLESS-1 resolution 1280x1000\nseat seat0 fallback true\nxwayland disable\nfocus_follows_mouse no\nfor_window [all] floating enable, border pixel 0\n')
    launch([a.sway,'--unsupported-gpu','-c',str(conf)],'sway.log');runtime=Path(env['XDG_RUNTIME_DIR']);env['SWAYSOCK']=str(wait(lambda:next(runtime.glob('sway-ipc*.sock'),None),'compositor'));env['WAYLAND_DISPLAY']=wait(lambda:next((v.name for v in runtime.glob('wayland-*') if not v.name.endswith('.lock')),None),'display')
    launch(['wtype','-s','60000'],'keyboard.log');shell=launch(['quickshell','-p',str(qml)],'shell.log');wait(lambda:state('files')['ready'],'Files ready')
    # Restore original assets before each independent surface/resource case.
    for surface in ('files','quick'):
        for kind in ('image','media'):
            target=docs/('image.png' if kind=='image' else 'video.mkv')
            if kind=='image':Image.new('RGB',(320,240),'red').save(target)
            else:
                target.unlink();video(target.name,'red',2)
            ipc('refreshRows');wait(lambda:state('files')['ready'] and any(r['path']==str(target) and r['identity']['inode']==str(target.stat().st_ino) for r in state('files')['rows']),'current directory identity');time.sleep(.4)
            ipc('startPreview',surface,str(target));before=wait(lambda:(s if (s:=state(surface))['result'].get('type')==kind and s['held'] else None),surface+' '+kind+' original reply');assert before['source']=='',before
            oldIdentity=[target.stat().st_dev,target.stat().st_ino]
            paused=None
            if surface=='files':
                children=Path('/proc/'+str(shell.pid)+'/task/'+str(shell.pid)+'/children').read_text().split();paused=next(int(pid) for pid in children if str(assets/'shell/files.py').encode() in Path('/proc/'+pid+'/cmdline').read_bytes());pausedProviders[paused]=Path('/proc/'+str(paused)+'/stat').read_text().rsplit(') ',1)[1].split()[19];os.kill(paused,signal.SIGSTOP)
            replacement=docs/('replacement.png' if kind=='image' else 'replacement.mkv')
            if kind=='image':Image.new('RGB',(320,240),'blue').save(replacement)
            else:
                replacement.unlink(missing_ok=True);video(replacement.name,'blue',6)
            replacement.replace(target);newIdentity=[target.stat().st_dev,target.stat().st_ino];assert oldIdentity!=newIdentity
            ipc('release',surface)
            after=wait(lambda:(s if (s:=state(surface))['source'] and (s['imageStatus']==1 if kind=='image' else s['duration']>0) else None),surface+' '+kind+' delayed load')
            time.sleep(.15);call('grim',str(out/(surface+'-'+kind+'.png')))
            pixels=None
            if kind=='image':
                def leaves(n):return [n]+sum((leaves(c) for c in n.get('nodes',[])+n.get('floating_nodes',[])),[])
                tree=json.loads(call('swaymsg','-r','-t','get_tree'));node=next(n for n in leaves(tree) if n.get('name')==after['geometry']['title'] and n.get('app_id')=='org.quickshell');g=after['geometry'];r=node['rect'];image=Image.open(out/(surface+'-'+kind+'.png')).convert('RGB');crop=image.crop((r['x']+g['x']+8,r['y']+g['y']+8,r['x']+g['x']+g['width']-8,r['y']+g['y']+g['height']-42));pixels={'red':sum(red>200 and green<40 and blue<40 for red,green,blue in crop.get_flattened_data()),'blue':sum(blue>200 and red<40 and green<40 for red,green,blue in crop.get_flattened_data())}
            results.append(dict(surface=surface,kind=kind,oldIdentity=oldIdentity,newIdentity=newIdentity,before=before,after=after,pixels=pixels))
            if paused:os.kill(paused,signal.SIGCONT);pausedProviders.pop(paused,None)
    (out/'baseline.json').write_text(json.dumps(results,indent=2)+'\n')
    # Baseline is expected to fail: the actual deferred media decoder observes
    # six seconds while the captured provider response described two seconds.
    failures=[r for r in results if (r['kind']=='media' and abs(r['after']['duration']-r['before']['result']['duration']*1000)>200) or (r['kind']=='image' and r['pixels']['blue']>r['pixels']['red'])]
    assert not failures,('Owned preview reopened replacement resources',[(r['surface'],r['kind'],r['pixels'],r['after']['duration']) for r in failures])
    checks=['Files and Quick Look image pixels retain original red after delayed load of replaced blue path','Files and Quick Look actual media decoder retains original two-second resource after six-second replacement']
    # Closing the actual utility retires its source before terminating its owner.
    quickResource=Path(results[-1]['before']['result']['resourcePath']);obsoleteFilesResource=Path(results[1]['before']['result']['resourcePath'])
    # The replaced old Files selection is now legitimately retired. Establish
    # an exact CURRENT unchanged lease for the independent-owner assertion.
    target=docs/'image.png';ipc('refreshRows');wait(lambda:state('files')['ready'],'current Files rows')
    ipc('restartFilesPreview',str(target));currentFiles=wait(lambda:(s if (s:=state('files'))['result'].get('type')=='image' and s['result'].get('sourcePath')==str(target) else None),'current Files independent lease')
    ipc('release','files');currentFiles=wait(lambda:(s if (s:=state('files'))['imageStatus']==1 and s['source']==s['result'].get('uri') else None),'current Files source loaded')
    filesResource=Path(currentFiles['result']['resourcePath']);assert filesResource!=obsoleteFilesResource
    wait(lambda:not obsoleteFilesResource.exists(),'old unselected Files lease retired')
    tree=json.loads(call('swaymsg','-r','-t','get_tree'))
    def allnodes(n):return [n]+sum((allnodes(c) for c in n.get('nodes',[])+n.get('floating_nodes',[])),[])
    node=next(n for n in allnodes(tree) if n.get('name')=='Quick Look' and n.get('app_id')=='org.quickshell');call('swaymsg','[con_id='+str(node['id'])+'] focus');call('wtype','-k','Escape');wait(lambda:not state('quick')['visible'] and not state('quick')['source'],'utility keyboard close');wait(lambda:not quickResource.exists(),'utility resource retirement');afterFiles=state('files');(out/'owner-debug.json').write_text(json.dumps(dict(before=currentFiles,after=afterFiles,expected=str(filesResource)),indent=2));assert filesResource.exists(),'Utility cleanup removed another live provider resource';assert afterFiles['source']==currentFiles['source'] and afterFiles['result']['sourceToken']==currentFiles['result']['sourceToken'];(out/'independent-owner.json').write_text(json.dumps(dict(before=currentFiles,after=afterFiles,obsoleteRetired=str(obsoleteFilesResource)),indent=2));checks.append('Physical Escape retires utility sources/resources while preserving exact current Files source and token; old unselected lease retires')
    # Fresh explicit show starts a new provider and never revives a retired URI.
    target=docs/'image.png';Image.new('RGB',(320,240),'red').save(target);ipc('refreshRows');wait(lambda:state('files')['ready'],'directory refresh');ipc('startPreview','quick',str(target));fresh=wait(lambda:(s if (s:=state('quick'))['result'].get('type')=='image' else None),'fresh utility');freshResource=Path(fresh['result']['resourcePath']);assert freshResource!=quickResource and freshResource.exists();ipc('release','quick');wait(lambda:state('quick')['imageStatus']==1,'fresh utility image');checks.append('Reopened utility receives a fresh observed snapshot and provider lifetime')
    # Retire exactly the preview subprocess, not the directory/operation backend.
    previewPid=int(filesResource.parent.name.split('-',1)[0]);previewStart=filesResource.parent.name.split('-',2)[1];assert Path('/proc/'+str(previewPid)+'/stat').read_text().rsplit(') ',1)[1].split()[19]==previewStart;os.kill(previewPid,signal.SIGKILL);wait(lambda:state('files')['result'].get('error') or not state('files')['result'],'preview provider exit');assert state('files')['ready'];ipc('restartFilesPreview',str(target));recovered=wait(lambda:(s if (s:=state('files'))['result'].get('type')=='image' else None),'Files preview restart');ipc('release','files');wait(lambda:state('files')['imageStatus']==1,'Files recovered image');wait(lambda:not filesResource.exists(),'orphaned resource cleanup');assert freshResource.exists(),'New Files provider removed a live utility resource';checks.append('Preview crash leaves directory backend usable and reconnect cleans only dead-owner resources')
    # A stopped preview cannot stall folder controls or replay after its deadline.
    quickPid=int(freshResource.parent.name.split('-',1)[0]);quickStart=freshResource.parent.name.split('-',2)[1];assert Path('/proc/'+str(quickPid)+'/stat').read_text().rsplit(') ',1)[1].split()[19]==quickStart;pausedProviders[quickPid]=quickStart;os.kill(quickPid,signal.SIGSTOP);ipc('startPreview','quick',str(docs/'video.mkv'));wait(lambda:state('quick')['controller']['busy'],'stopped preview request');ipc('focusFiles');call('wtype','-M','ctrl','-k','2','-m','ctrl');ipc('refreshRows');wait(lambda:state('files')['ready'],'folder controls while preview stopped');wait(lambda:state('quick')['result'].get('error') and not state('quick')['controller']['busy'],'preview deadline');wait(lambda:not Path('/proc/'+str(quickPid)).exists(),'owned stalled preview termination');pausedProviders.pop(quickPid,None);assert state('files')['ready'];assert not state('quick')['source'];checks.append('Actual SIGSTOP preview reaches bounded unavailable state without stopping folder controls or retrying stale reads')
    # Oversized resource requests remain unavailable instead of using the source URI.
    large=docs/'large.mkv'
    with large.open('wb') as stream:stream.truncate(64*1024*1024+1)
    ipc('startPreview','quick',str(large));wait(lambda:state('quick')['result'].get('error'),'oversize refusal');assert not state('quick')['source'] and not state('quick')['result'].get('uri');checks.append('Oversized resources fail without mutable original-URI fallback')
    # A real rendered PDF page remains readable after its shared cache is pruned.
    pdf=docs/'document.pdf';objects=[b'<< /Type /Catalog /Pages 2 0 R >>',b'<< /Type /Pages /Kids [3 0 R 5 0 R 7 0 R] /Count 3 >>']
    for page_index,color in enumerate((b'1 0 0',b'0 1 0',b'0 0 1')):
        drawing=color+b' rg 0 0 320 240 re f';objects.append(b'<< /Type /Page /Parent 2 0 R /MediaBox [0 0 320 240] /Contents '+str(4+page_index*2).encode()+b' 0 R /Resources << >> >>');objects.append(b'<< /Length '+str(len(drawing)).encode()+b' >>\nstream\n'+drawing+b'\nendstream')
    content=b'%PDF-1.4\n';offsets=[0]
    for index,obj in enumerate(objects,1):offsets.append(len(content));content+=str(index).encode()+b' 0 obj\n'+obj+b'\nendobj\n'
    xref=len(content);count=len(objects)+1;content+=b'xref\n0 '+str(count).encode()+b'\n0000000000 65535 f \n'+b''.join(('%010d 00000 n \n'%offset).encode() for offset in offsets[1:])+b'trailer\n<< /Size '+str(count).encode()+b' /Root 1 0 R >>\nstartxref\n'+str(xref).encode()+b'\n%%EOF\n';pdf.write_bytes(content)
    ipc('startPreview','quick',str(pdf));page=wait(lambda:(s if (s:=state('quick'))['result'].get('type')=='image' else None),'rendered PDF reply');pageResource=Path(page['result']['uri'].removeprefix('file://'));assert 'lumen-preview-snapshots-' in str(pageResource)
    for cached in (Path(env['XDG_CACHE_HOME'])/'lumen-desktop/previews').glob('*.png'):cached.unlink()
    assert pageResource.exists();ipc('release','quick');wait(lambda:state('quick')['imageStatus']==1,'retained PDF page after cache prune');checks.append('Actual PDF raster remains available after shared renderer cache pruning')
    # Real physical PDF paging retains the original immutable input but only
    # the current raster; no already-retired page URI is reused.
    inputResource=Path(page['result']['resourcePath']);pdfToken=page['result']['sourceToken'];pdfUris=[pageResource]
    node=next(n for n in allnodes(json.loads(call('swaymsg','-r','-t','get_tree'))) if n.get('name')=='Quick Look' and n.get('app_id')=='org.quickshell');call('swaymsg','[con_id='+str(node['id'])+'] focus')
    pageProof=[]
    for number in (2,3):
        call('wtype','-k','Down');nextPage=wait(lambda:(value if (value:=state('quick'))['result'].get('requestedPage')==number and value['imageStatus']==1 else None),'physical PDF page '+str(number));currentRaster=Path(nextPage['result']['uri'].removeprefix('file://'))
        assert currentRaster not in pdfUris and currentRaster.exists() and inputResource.exists();assert nextPage['result']['resourcePath']==str(inputResource) and nextPage['result']['sourceToken']==pdfToken
        wait(lambda:not any(path.exists() for path in pdfUris),'previous PDF rasters retired');pdfUris.append(currentRaster);pageProof.append(nextPage)
    (out/'pdf-leases.json').write_text(json.dumps(dict(input=str(inputResource),pages=pageProof),indent=2));checks.append('Physical PDF paging reuses exact immutable input and retires previous page rasters without URI reuse')
    replacementPdf=docs/'replacement-document.pdf';replacementPdf.write_bytes(content);replacementPdf.replace(pdf);call('wtype','-k','Up');wait(lambda:state('quick')['result'].get('error') and not state('quick')['source'],'changed original PDF token refused');checks.append('Paging after original PDF replacement remains explicitly unavailable rather than adopting a replacement')
    # More than one lifetime budget of actual decoded gallery images succeeds
    # by retiring each previous working set, while the current URI stays live.
    largeImages=[]
    for index,color in enumerate(('red','green','blue','yellow','magenta')):
        path=docs/('working-'+str(index)+'.bmp');Image.new('RGB',(4096,2560),color).save(path);largeImages.append((path,color))
    assert sum(path.stat().st_size for path,color in largeImages)>128*1024*1024
    ipc('refreshRows');wait(lambda:state('files')['ready'] and all(any(row['path']==str(path) for row in state('files')['rows']) for path,color in largeImages),'large gallery rows');ipc('focusFiles');filesNode=next(node for node in allnodes(json.loads(call('swaymsg','-r','-t','get_tree'))) if node.get('name')==currentFiles['geometry']['title'] and node.get('app_id')=='org.quickshell');call('swaymsg','[con_id='+str(filesNode['id'])+'] focus');call('wtype','-M','ctrl','-k','4','-m','ctrl');wait(lambda:state('files')['viewMode']=='gallery','actual Files gallery shortcut')
    galleryProof=[]
    for surface in ('files','quick'):
        previous=[]
        for path,color in largeImages:
            ipc('startPreview',surface,str(path));value=wait(lambda:(value if (value:=state(surface))['result'].get('sourcePath')==str(path) and value['result'].get('type')=='image' else None),surface+' gallery resource '+color);resource=Path(value['result']['uri'].removeprefix('file://'));assert resource not in previous and resource.exists()
            ipc('release',surface);loaded=wait(lambda:(value if (value:=state(surface))['imageStatus']==1 and value['source']==resource.as_uri() else None),surface+' decoded gallery '+color)
            wait(lambda:not any(old.exists() for old in previous),surface+' previous gallery leases retired');previous.append(resource)
            with Image.open(resource) as decoded:pixel=decoded.getpixel((0,0))
            assert pixel==Image.new('RGB',(1,1),color).getpixel((0,0));live=list(resource.parent.iterdir());assert len(live)==1 and sum(file.stat().st_size for file in live)<=64*1024*1024
            galleryProof.append(dict(surface=surface,path=str(path),source=str(resource),pixel=pixel,workingBytes=sum(file.stat().st_size for file in live),state=loaded))
    (out/'gallery-leases.json').write_text(json.dumps(galleryProof,indent=2));checks.append('Files and Quick Look decode more than128 MiB of successive gallery inputs with one bounded current resource')
    # Hold a real Image source across the result-clear boundary: the old URI
    # remains leased and no new response is accepted before cooperative release.
    pinned=state('quick');oldImage=Path(pinned['result']['uri'].removeprefix('file://'));accepted=len(pinned['controller']['accepted']);ipc('pinRetirement','quick');ipc('startPreview','quick',str(largeImages[0][0]));wait(lambda:state('quick')['controller']['resourceRetirement'],'held actual Image retirement');time.sleep(.2);held=state('quick');assert oldImage.exists() and held['source']==oldImage.as_uri() and len(held['controller']['accepted'])==accepted
    ipc('unpinRetirement','quick');wait(lambda:state('quick')['result'].get('sourcePath')==str(largeImages[0][0]),'retired Image replacement reply');wait(lambda:not oldImage.exists(),'held Image lease released');ipc('release','quick');wait(lambda:state('quick')['imageStatus']==1,'replacement image decoded');imageRetirement=dict(before=pinned,held=held,after=state('quick'),retiredUri=str(oldImage),retiredFileExists=oldImage.exists());checks.append('Held actual Image source keeps old URI pinned and blocks replacement allocation until acknowledged retirement')
    video('playing.mkv','red',12);ipc('startPreview','quick',str(docs/'playing.mkv'));wait(lambda:state('quick')['result'].get('type')=='media','playing media response');ipc('release','quick');wait(lambda:state('quick')['duration']>10000,'actual media decoder ready');ipc('play');wait(lambda:state('quick')['playing'],'actual MediaPlayer playback');playing=state('quick');oldMedia=Path(playing['result']['uri'].removeprefix('file://'));accepted=len(playing['controller']['accepted']);ipc('pinRetirement','quick');ipc('startPreview','quick',str(largeImages[1][0]));wait(lambda:state('quick')['controller']['resourceRetirement'],'held actual MediaPlayer retirement');time.sleep(.2);held=state('quick');assert oldMedia.exists() and held['source']==oldMedia.as_uri() and len(held['controller']['accepted'])==accepted
    ipc('unpinRetirement','quick');wait(lambda:state('quick')['result'].get('sourcePath')==str(largeImages[1][0]),'retired media replacement reply');wait(lambda:not oldMedia.exists() and state('quick')['stopped'],'held MediaPlayer source stopped and lease released');mediaRetirement=dict(before=playing,held=held,after=state('quick'),retiredUri=str(oldMedia),retiredFileExists=oldMedia.exists());(out/'retirement-leases.json').write_text(json.dumps(dict(image=imageRetirement,media=mediaRetirement),indent=2));checks.append('Actual playing MediaPlayer source is retained until source-cleared and stopped acknowledgement before replacement')
    log=(out/'shell.log').read_text();assert not any(marker in log for marker in ('ReferenceError:','TypeError:','Binding loop','Failed to load configuration','Cannot assign')),log
    (out/'result.json').write_text(json.dumps(dict(passed=True,checks=checks,resources=results,final=state('quick')),indent=2)+'\n')
finally:
    for pid,start in pausedProviders.items():
        try:
            if Path('/proc/'+str(pid)+'/stat').read_text().rsplit(') ',1)[1].split()[19]==start:os.kill(pid,signal.SIGCONT)
        except (FileNotFoundError,ProcessLookupError):pass
    for proc in reversed(processes):
        if proc.poll() is None:os.killpg(proc.pid,signal.SIGTERM)
    for proc in processes:
        try:proc.wait(timeout=4)
        except sp.TimeoutExpired:os.killpg(proc.pid,signal.SIGKILL);proc.wait(timeout=4)
