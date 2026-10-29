"""Genuine nested pointer/keyboard ownership for Files tabs and sidebar layout."""
from fixture_resources import close_reaped_pipes
from private_native_bus import start_private_bus
from application_fixture.isolation import run
from application_fixture.provenance import FrozenSource
from application_fixture.keyboard import PhysicalKeyboard
from application_fixture.audio import start_null_sink
import argparse,json,os,re,shutil,signal,subprocess as sp,sys,tempfile,time
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--output',type=Path,required=True);p.add_argument('--launcher',type=Path,required=True);p.add_argument('--sway',type=Path,required=True);p.add_argument('--pointer-tool',type=Path,required=True);p.add_argument('--keyboard',type=Path,required=True);p.add_argument('--font-config',type=Path,required=True);p.add_argument('--software-driver',type=Path,required=True);p.add_argument('--audio-provider',type=Path,required=True);p.add_argument('--text-scale',type=float,default=1);p.add_argument('--split-panes',action='store_true');p.add_argument('--effects',action='store_true');p.add_argument('--inside',action='store_true');a=p.parse_args()
for name in ('launcher','sway','pointer_tool','keyboard','font_config','software_driver','audio_provider'):
 value=getattr(a,name).resolve();assert str(value).startswith('/nix/store/') and value.exists();setattr(a,name,value)
repo=Path(__file__).resolve().parents[2]
if not a.inside:
    a.output=a.output.resolve();a.output.mkdir(mode=0o700,parents=True,exist_ok=False);frozen=FrozenSource(repo,__file__,[a.launcher,a.sway,a.pointer_tool,a.keyboard,a.font_config,a.audio_provider/"bin/pulseaudio",a.audio_provider/"bin/pactl",a.software_driver/'share/glvnd/egl_vendor.d/50_mesa.json',Path(sys.executable).resolve()]);prefix=re.split(r'\n[ \t]*exec quickshell ',a.launcher.read_text(),maxsplit=1)[0];env=dict(value.decode().split('=',1)for value in sp.check_output(['bash','-c',prefix+'\nexec env -0']).split(b'\0')if b'='in value)
    args=[str(Path(sys.executable).resolve()),'/source/tests/desktop/files_layout_input.py','--inside','--output','/artifacts/result','--text-scale',str(a.text_scale)]+sum((['--'+name.replace('_','-'),str(getattr(a,name))]for name in ('launcher','sway','pointer_tool','keyboard','font_config','software_driver','audio_provider')),[])+(['--effects']if a.effects else [])+(['--split-panes']if a.split_panes else [])
    code=1
    try:code=run(repo,a.output,args,env,software_graphics=a.effects)
    finally:frozen.finish(a.output,code)
    raise SystemExit(code)
assert repo==Path('/source') and os.environ['HOME']=='/home/fixture' and not Path('/.old-root').exists()
repo=Path(__file__).resolve().parents[2];out=a.output.resolve();out.mkdir(parents=True,exist_ok=False)
prefix=re.split(r'\n[ \t]*exec quickshell ',a.launcher.read_text(),maxsplit=1)[0]
exported=sp.check_output(['bash','-c',prefix+'\nexec env -0']);env=dict(value.decode().split('=',1) for value in exported.split(b'\0') if b'=' in value)
env.update(WLR_BACKENDS='headless',WLR_RENDERER='gles2' if a.effects else 'pixman',WLR_RENDERER_FORCE_SOFTWARE='1' if a.effects else '0',WLR_RENDERER_ALLOW_SOFTWARE='1',LIBGL_ALWAYS_SOFTWARE='1',MESA_LOADER_DRIVER_OVERRIDE='llvmpipe',LIBGL_DRIVERS_PATH=str(a.software_driver/'lib/dri'),__EGL_VENDOR_LIBRARY_FILENAMES=str(a.software_driver/'share/glvnd/egl_vendor.d/50_mesa.json'),FONTCONFIG_FILE=str(a.font_config),WLR_LIBINPUT_NO_DEVICES='1',QT_QPA_PLATFORM='wayland',QT_QPA_PLATFORMTHEME='generic',QT_QUICK_BACKEND='software',QT_ACCESSIBILITY='0',QT_LINUX_ACCESSIBILITY_ALWAYS_ON='0',NO_AT_BRIDGE='1',GTK_A11Y='none',LUMEN_FILE_SERVICE='0',LUMEN_LAUNCH_SCOPES='0',LUMEN_NATIVE_WINDOW_CONTROLS='0',LUMEN_NATIVE_WINDOW_ACTIONS='0',LUMEN_UNIFIED_OWNED_HEADERS='1')
for key in ('DISPLAY','WAYLAND_DISPLAY','SWAYSOCK','AT_SPI_BUS_ADDRESS'):env.pop(key,None)
if a.effects:env.pop('LIBGL_ALWAYS_SOFTWARE',None)
folders={}
for name in ('First','Second','Third'):
    folder=Path(env['HOME'])/name;folder.mkdir();(folder/'selected.txt').write_text('unchanged fixture '+name);folders[name]=str(folder)
palettes=json.loads((repo/'res/lumen-desktop/appearance/palette.json').read_text());appearance=out/'appearance.json';appearance.write_text(json.dumps(dict(font=palettes['font'],palette=palettes['dark'],preferences=dict(textScale=a.text_scale,reduceMotion=True))));env['LUMEN_APPEARANCE_STATE']=str(appearance)
shutil.copytree(repo/'res/lumen-desktop',out/'assets')
qml=out/'shell.qml';qml.write_text('''import QtQuick
import Quickshell
import Quickshell.Io
import "assets/shell" as D
import "assets/shell/FilesTabModel.js" as Tabs
ShellRoot {
 id:fixture
 property var paths:'''+json.dumps(folders)+'''
 QtObject {id:exported;property var state:({appName:"",title:"",windowId:0})}
 QtObject {id:stub;property int focusEpoch:0;property var windows:[];property var exportedMenu:exported;function activeFiles(window){}function focusWindow(window){}}
 D.FilesState {id:service;persistViewPreferences:false;path:fixture.paths.First;viewMode:"list";favorites:[{name:"First",path:fixture.paths.First},{name:"Second",path:fixture.paths.Second},{name:"Third",path:fixture.paths.Third}]}
 D.FilesWindow {id:files;shell:stub;service:service;visible:true;implicitWidth:1200}
 Component.onCompleted:{files.tabs=["First","Second","Third"].map((name,index)=>{const tab=Tabs.create(name.toLowerCase(),paths[name]);tab.history=index===0?[paths.First]:[paths.First,paths[name]];tab.historyIndex=index===0?0:1;tab.selectedPaths=[paths[name]+"/selected.txt"];tab.selectedPath=tab.selectedPaths[0];tab.viewMode="list";return tab;});service.history=files.tabs[0].history.slice();service.historyIndex=0;service.navigate(paths.First,0);}
 function find(name,item){if(item.objectName===name)return item;for(const child of item.children || []){const found=find(name,child);if(found)return found;}return null;}
 IpcHandler {target:"filesAudit"
  function state():string {return JSON.stringify({ready:service.ready && !service.busy && service.rows.length>0,tabs:files.tabs,activeId:files.activeTabId,path:service.path,history:service.history,historyIndex:service.historyIndex,selection:files.selectedPaths,sidebarWidth:files.sidebarWidth,savedWidth:service.sidebarWidth,collapsed:service.collapsedSidebarGroups,width:files.width,textScale:D.Theme.textScale,tabGeometry:files.tabs.map(tab=>{const item=fixture.find("document-tab-"+tab.id,files.contentItem),at=item.mapToItem(files.contentItem,0,0),close=fixture.find("document-tab-close-"+tab.id,files.contentItem),cp=close.mapToItem(files.contentItem,0,0);return {id:tab.id,x:at.x,width:item.width,closeX:cp.x,closeWidth:close.width};}),resizeFocus:fixture.find("files-sidebar-resize",files.contentItem).activeFocus,resizeFacts:fixture.find("files-sidebar-resize",files.contentItem).children.filter(item=>item.pressed!==undefined).map(item=>({pressed:item.pressed,cancelled:item.gestureCancelled})),focused:files.contentItem.Window.window && files.contentItem.Window.window.activeFocusItem?files.contentItem.Window.window.activeFocusItem.objectName:""});}
  function scale(value:real):void {D.Theme.preferences={textScale:value,reduceMotion:true};}
  function select():void {files.choose(0,0);files.saveTab();}
  function geometry(name:string):string {const item=fixture.find(name,files.contentItem);if(!item)return "null";const at=item.mapToItem(files.contentItem,0,0);return JSON.stringify({x:at.x,y:at.y,width:item.width,height:item.height});}
 }
}''')
if a.split_panes:
    actual_assets=Path(re.search(r'exec quickshell .*--path (\S+)/shell',a.launcher.read_text()).group(1))
    for name in ('FilesWindow.qml','FilesState.qml','ColumnView.qml','SplitResizeHandle.qml','SidebarResizeHandle.qml','FileLayoutModel.js','files.py'):
        assert (actual_assets/'shell'/name).read_bytes()==(repo/'res/lumen-desktop/shell'/name).read_bytes(),name
    text=qml.read_text().replace('persistViewPreferences:false','persistViewPreferences:true')
    text=text.replace('  function scale(value:real):void','''  function splitState():string {const view=fixture.find("files-column-view",files.contentItem);return JSON.stringify({ready:service.ready && !service.busy && service.rows.length>0,previewOpen:files.previewOpen,previewVisible:files.previewVisible,previewWidth:files.previewWidth,preferredPreview:service.previewWidth,previewMessage:files.previewMessage,preferredColumns:service.columnWidths,columnsLoaded:service.columns.length>1 && service.columns.every(column=>column.loaded!==false),columnX:view?view.contentX:0,columnWidth:view?view.width:0,scale:D.Theme.textScale});}
  function splitSetup(view:string,preview:bool):void {service.viewMode=view;files.previewOpen=preview;}
  function columnStart():void {const view=fixture.find("files-column-view",files.contentItem);view.contentX=0;}
  function columnOffset(value:real):void {const view=fixture.find("files-column-view",files.contentItem);view.contentX=view.clampOffset(value);}
  function scale(value:real):void''')
    qml.write_text(text)
processes=[]
def launch(command,name):
    with (out/name).open('w') as _fixture_log0:
        process=sp.Popen(command,env=env,stdout=_fixture_log0,stderr=sp.STDOUT,start_new_session=True);processes.append(process);return process

def call(*command):return sp.check_output(command,env=env,text=True,timeout=10).strip()
def wait(fn):
    end=time.monotonic()+20
    while time.monotonic()<end:
        try:
            value=fn()
            if value:return value
        except (OSError,ValueError,sp.SubprocessError):pass
        time.sleep(.1)
    if (out/'shell.log').exists():
        try:(out/'failed-state.json').write_text(json.dumps(state(),indent=2))
        except Exception:pass
    raise AssertionError('Files input fixture timed out; inspect logs')
def nodes(node):
    yield node
    for child in node.get('nodes',[])+node.get('floating_nodes',[]):yield from nodes(child)
def window():return next(node for node in nodes(json.loads(call('swaymsg','-r','-t','get_tree'))) if node.get('app_id')=='org.quickshell' and node.get('name','').startswith('Files'))
def ipc(method,*values):return call('quickshell','-p',str(qml),'ipc','call','filesAudit',method,*values)
def state():return json.loads(ipc('state'))
def point(name,rx=.5,ry=.5):
    value=json.loads(ipc('geometry',name));assert value,name;r=window()['rect'];return (r['x']+value['x']+value['width']*rx,r['y']+value['y']+min(value['height'],25)*ry)
def move(x,y):
    call('swaymsg','seat','seat0','cursor','set',str(round(x)),str(round(y)));pointer.stdin.write('m 0.5 0.5\n');pointer.stdin.flush();time.sleep(.12)
def command(value):pointer.stdin.write(value+'\n');pointer.stdin.flush();time.sleep(.12)
def click(position):move(*position);command('p');command('r')
def key(*keys):keyboard.key(*keys)
def drag(start,dx,dy=0,cancel=False):
    move(*start);command('p')
    for index in range(8):command('m '+str(dx/8)+' '+str(dy/8))
    if cancel:
        snapshots=[state()];key('-k','Escape');snapshots.append(state());command('m 8 0');snapshots.append(state());(out/'cancellation-state.json').write_text(json.dumps(snapshots,indent=2))
    command('r')
def invariant():
    value=state();assert value['path']==folders['First'] and value['selection']==[folders['First']+'/selected.txt'] and value['activeId']=='first',value
    return value
try:
    start_private_bus(env,out,processes)
    start_null_sink(a.audio_provider,env,out,processes)
    config=out/'sway.conf';config.write_text('output HEADLESS-1 resolution 1280x900\nxwayland disable\nfor_window [all] floating enable, border pixel 0\n')
    launch([str(a.sway),'--unsupported-gpu','-c',str(config)],'sway.log');runtime=Path(env['XDG_RUNTIME_DIR']);env['SWAYSOCK']=str(wait(lambda:next(runtime.glob('sway-ipc*.sock'),None)));env['WAYLAND_DISPLAY']=wait(lambda:next((path.name for path in runtime.glob('wayland-*') if not path.name.endswith('.lock')),None))
    shell_process=launch(['quickshell','-p',str(qml)],'shell.log');wait(lambda:state()['ready']);ipc('select');invariant()
    call('grim',str(out/'files-selected.png'))
    with (out/'keyboard.log').open('w')as log:keyboard=PhysicalKeyboard(a.keyboard,env,log)
    processes.append(keyboard.process)
    pointer=sp.Popen([str(a.pointer_tool),'pointer','move','0','0'],env=env,stdin=sp.PIPE,text=True,start_new_session=True);processes.append(pointer);time.sleep(.2)
    checks=[]
    if a.split_panes:
        def split():return json.loads(ipc('splitState'))
        pref=Path(env['XDG_STATE_HOME'])/'lumen-desktop/session/files.json'
        def persisted():return json.loads(pref.read_text())
        ipc('splitSetup','list','true');wait(lambda:split()['previewVisible'])
        initial=split()['preferredPreview'];drag(point('files-preview-resize'),-70);wait(lambda:split()['preferredPreview']>=initial+65)
        saved_preview=split()['preferredPreview'];key('-k','Right');wait(lambda:split()['preferredPreview']==saved_preview-10);saved_preview=split()['preferredPreview'];wait(lambda:persisted()['previewWidth']==saved_preview)
        invariant()
        checks.append('Trailing preview divider follows actual pointer and reversed physical arrow direction without changing selection')
        original=pref.read_bytes();drag(point('files-preview-resize'),-40,cancel=True);assert split()['preferredPreview']==saved_preview and pref.read_bytes()==original
        invariant()
        checks.append('Preview Escape with additional held motion preserves previous preference bytes')
        node=window();call('swaymsg','[con_id='+str(node['id'])+'] resize set width 480 px height 600 px');wait(lambda:state()['width']==480 and not split()['previewVisible']);assert split()['previewOpen'] and split()['previewMessage'] and split()['preferredPreview']==saved_preview
        call('swaymsg','[con_id='+str(node['id'])+'] resize set width 1200 px height 650 px');wait(lambda:state()['width']==1200 and split()['previewVisible'] and split()['previewWidth']==saved_preview)
        invariant()
        checks.append('Narrow output hides preview with explanation and widening restores unclobbered preferred width')
        ipc('scale','1.5');wait(lambda:split()['scale']==1.5);assert split()['previewWidth']>=240
        ipc('splitSetup','columns','false');wait(lambda:split()['columnsLoaded']);node=window();call('swaymsg','[con_id='+str(node['id'])+'] resize set width 720 px height 650 px');wait(lambda:state()['width']==720);ipc('columnStart');wait(lambda:split()['columnX']<1)
        before=json.loads(ipc('geometry','files-column-resize-0'));drag(point('files-column-resize-0'),50);wait(lambda:len(split()['preferredColumns'])>0 and split()['preferredColumns'][0]>=330);assert split()['columnX']<1,split()
        saved_column=split()['preferredColumns'][0];key('-k','Right');wait(lambda:split()['preferredColumns'][0]==saved_column+10);saved_column=split()['preferredColumns'][0];wait(lambda:persisted()['columnWidths'][0]==saved_column)
        invariant()
        checks.append('At150% a leading column divider resizes with real pointer and keyboard without jumping to the last column')
        idle_original=pref.read_bytes();ipc('columnOffset','60');wait(lambda:abs(split()['columnX']-60)<1);key('-k','Escape');assert abs(split()['columnX']-60)<1 and pref.read_bytes()==idle_original,split();ipc('columnStart');wait(lambda:split()['columnX']<1)
        checks.append('Escape without an active drag preserves the current column viewport and preference bytes')
        original=pref.read_bytes();drag(point('files-column-resize-0'),-45,cancel=True);assert split()['preferredColumns'][0]==saved_column and split()['columnX']<1 and pref.read_bytes()==original
        invariant()
        checks.append('Column Escape restores width and viewport while preserving persisted bytes')
        key('-k','Home');wait(lambda:split()['preferredColumns'][0]==300);key('-k','End');wait(lambda:split()['preferredColumns'][0]>=400);saved_column=split()['preferredColumns'][0];wait(lambda:persisted()['columnWidths'][0]==saved_column)
        checks.append('Physical Home and End respect enlarged column minimum and available viewport maximum')
        original=pref.read_bytes();call('swaymsg','output','HEADLESS-1','scale','1.5');wait(lambda:any(output.get('name')=='HEADLESS-1' and output.get('scale')==1.5 for output in json.loads(call('swaymsg','-r','-t','get_outputs'))))
        call('swaymsg','[con_id='+str(node['id'])+'] resize set width 600 px height 600 px');wait(lambda:state()['width']==600);assert split()['preferredColumns'][0]==saved_column and pref.read_bytes()==original
        handle=json.loads(ipc('geometry','files-column-resize-0'));assert handle['x']+handle['width']<=state()['width']-8 and split()['columnX']<1,handle
        call('swaymsg','output','HEADLESS-1','scale','1');call('swaymsg','[con_id='+str(node['id'])+'] resize set width 720 px height 650 px');wait(lambda:state()['width']==720);assert split()['preferredColumns'][0]==saved_column and pref.read_bytes()==original;invariant()
        checks.append('Actual fractional output scale and narrow/wide configure clamp panes without overwriting saved sizes')
        call('grim',str(out/'files-split-enlarged.png'))
        os.killpg(shell_process.pid,signal.SIGTERM);shell_process.wait(timeout=5);close_reaped_pipes([shell_process]);wait(lambda:not any(n.get('app_id')=='org.quickshell' for n in nodes(json.loads(call('swaymsg','-r','-t','get_tree')))))
        shell_process=launch(['quickshell','-p',str(qml)],'shell-reopened.log');wait(lambda:split()['ready'] and split()['preferredPreview']==saved_preview and len(split()['preferredColumns'])>0 and split()['preferredColumns'][0]==saved_column)
        checks.append('Fresh actual Files service and window restore both persisted split preferences after process restart')
        for name,path in folders.items():assert (Path(path)/'selected.txt').read_text()=='unchanged fixture '+name
        for name in ('shell.log','shell-reopened.log'):
            log=(out/name).read_text();assert not any(marker in log for marker in ('ReferenceError:','TypeError:','Binding loop','Failed to load configuration','Cannot assign')),log
        (out/'result.json').write_text(json.dumps(dict(passed=True,checks=checks,final=split(),scope='Actual pointer/keyboard and real isolated Files preference backend; no reference measurement claim'),indent=2)+'\n')
    else:
        click(point('files-sidebar-favourites',.4,.5));wait(lambda:'favourites' in state()['collapsed']);invariant()
        key('-k','space');wait(lambda:'favourites' not in state()['collapsed']);invariant();checks.append('Pointer disclosure and keyboard expansion preserve current route and selected file')
        width=state()['sidebarWidth'];drag(point('files-sidebar-resize'),90);wait(lambda:state()['savedWidth']>=width+85);invariant()
        saved=state()['savedWidth'];key('-k','Right');wait(lambda:state()['savedWidth']==saved+10);invariant();checks.append('Pointer and keyboard resize are distinct from file/sidebar selection')
        saved=state()['savedWidth'];drag(point('files-sidebar-resize'),40,cancel=True);assert state()['savedWidth']==saved,(saved,state());invariant();checks.append('Escape cancels a held sidebar resize without committing')
        first=point('document-tab-first');third=point('document-tab-third');drag(third,first[0]-third[0]);wait(lambda:[tab['id'] for tab in state()['tabs']]==['third','first','second']);invariant();checks.append('Real tab drag reorders stable identity without changing active content')
        before=[tab['id'] for tab in state()['tabs']];drag(point('document-tab-third'),90,cancel=True);assert [tab['id'] for tab in state()['tabs']]==before;invariant();checks.append('Escape cancels tab drag even with further motion before release')
        drag(point('document-tab-third'),0,80);assert [tab['id'] for tab in state()['tabs']]==before;invariant();checks.append('Release outside tab drop region cancels without navigation')
        click(point('document-tab-first',.4));key('-M','ctrl','-M','shift','-k','Right','-m','shift','-m','ctrl');wait(lambda:[tab['id'] for tab in state()['tabs']]==['third','second','first']);invariant();checks.append('Keyboard tab reorder retains active identity, history and selection')
        node=window();call('swaymsg','[con_id='+str(node['id'])+'] resize set width 480 px height 600 px');wait(lambda:state()['width']==480)
        for scale in ['1','1.5']:
            ipc('scale',scale)
            wait(lambda:(lambda r:r['x']>=state()['sidebarWidth'] and r['x']+r['width']<=state()['width']-30)(json.loads(ipc('geometry','document-tab-close-first'))))
            invariant()
        checks.append('Live text scale changes retain narrow selected tab and visible close control')
        tab_close=wait(lambda:json.loads(ipc('geometry','document-tab-close-first')));assert tab_close['x']>=state()['sidebarWidth'] and tab_close['x']+tab_close['width']<=state()['width']-30,tab_close
        call('grim',str(out/'files-narrow-tabs.png'));invariant();checks.append('Narrow tab strip keeps active tab close target visible without shrinking targets')
        call('swaymsg','[con_id='+str(node['id'])+'] resize set width 900 px height 600 px');wait(lambda:state()['width']==900)
        click(point('document-tab-close-second'));wait(lambda:len(state()['tabs'])==2);invariant();checks.append('Closing an inactive tab does not navigate or lose active selection')
        key('-M','ctrl','-k','w','-m','ctrl');wait(lambda:state()['activeId']=='third' and state()['ready']);value=state();assert value['path']==folders['Third'] and [entry if isinstance(entry,str) else entry['path'] for entry in value['history']]==[folders['First'],folders['Third']] and value['historyIndex']==1 and value['selection']==[folders['Third']+'/selected.txt'],value;checks.append('Closing active tab restores exact destination history and selected identity')
        node=window();call('swaymsg','[con_id='+str(node['id'])+'] resize set width 480 px height 600 px');wait(lambda:state()['width']==480);value=state();assert value['sidebarWidth']<=180 and value['width']-value['sidebarWidth']>=300,value;assert value['savedWidth']==saved,value;checks.append('Actual 480px window bounds sidebar without overwriting wider saved preference')
        call('grim',str(out/'files-narrow.png'))
        from PIL import Image,ImageStat
        image=Image.open(out/'files-narrow.png').convert('RGB');r=window()['rect'];crop=image.crop((r['x']+10,r['y']+70,r['x']+r['width']-10,r['y']+r['height']-10));assert max(ImageStat.Stat(crop).mean)>10 and max(ImageStat.Stat(crop).stddev)>2,'Blank file interior'
        log=(out/'shell.log').read_text();assert not any(marker in log for marker in ('ReferenceError:','TypeError:','Binding loop','Failed to load configuration','Cannot assign')),log
        for name,path in folders.items():assert (Path(path)/'selected.txt').read_text()=='unchanged fixture '+name
        (out/'result.json').write_text(json.dumps(dict(passed=True,checks=checks,final=value,scope='Genuine virtual pointer/keyboard in isolated compositor; real disposable Files backend; no live user input or files'),indent=2)+'\n')
finally:
    for process in reversed(processes):
        if process.poll() is None:os.killpg(process.pid,signal.SIGTERM)
    for process in reversed(processes):
        try:process.wait(timeout=4)
        except sp.TimeoutExpired:os.killpg(process.pid,signal.SIGKILL)
    for _fixture_process in processes:
        if _fixture_process.poll() is None:_fixture_process.wait(timeout=5)
    close_reaped_pipes(processes)
    if (out/'shell.log').exists():
        warnings=[line for path in out.glob('*.log') for line in path.read_text().splitlines() if 'WARN'in line]
        if (out/'result.json').exists():
            result=json.loads((out/'result.json').read_text());result['runtimeWarnings']=warnings;(out/'result.json').write_text(json.dumps(result,indent=2)+'\n')
        assert not warnings,warnings
print('PASS native Files tab/sidebar pointer and keyboard regression')
