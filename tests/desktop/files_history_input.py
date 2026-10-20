"""Genuine nested pointer/keyboard ownership for Files tabs and sidebar layout."""
from fixture_resources import close_reaped_pipes
import argparse,json,os,re,shutil,signal,subprocess as sp,sys,tempfile,time
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--output',type=Path,required=True);p.add_argument('--launcher',type=Path,required=True);p.add_argument('--sway',required=True);p.add_argument('--pointer-tool',required=True);p.add_argument('--effects',action='store_true');p.add_argument('--inside',action='store_true');a=p.parse_args()
if not a.inside:
    with tempfile.TemporaryDirectory(prefix='lumen-files-input-',ignore_cleanup_errors=True) as private:
        env=dict(os.environ,HOME=private,XDG_RUNTIME_DIR=private)
        for key in ('CONFIG','STATE','CACHE','DATA'):env['XDG_'+key+'_HOME']=str(Path(private)/key.lower())
        for key in ('DISPLAY','WAYLAND_DISPLAY','SWAYSOCK','DBUS_SESSION_BUS_ADDRESS','AT_SPI_BUS_ADDRESS'):env.pop(key,None)
        raise SystemExit(sp.call(['dbus-run-session','--',sys.executable,__file__,*sys.argv[1:],'--inside'],env=env))
repo=Path(__file__).resolve().parents[2];out=a.output.resolve();out.mkdir(parents=True,exist_ok=False)
prefix=re.split(r'\n[ \t]*exec quickshell ',a.launcher.read_text(),maxsplit=1)[0]
exported=sp.check_output(['bash','-c',prefix+'\nexec env -0']);env=dict(value.decode().split('=',1) for value in exported.split(b'\0') if b'=' in value)
env.update(WLR_BACKENDS='headless',WLR_RENDERER='pixman',WLR_LIBINPUT_NO_DEVICES='1',QT_QPA_PLATFORM='wayland',QT_QPA_PLATFORMTHEME='generic',QT_QUICK_BACKEND='software',QT_ACCESSIBILITY='0',QT_LINUX_ACCESSIBILITY_ALWAYS_ON='0',NO_AT_BRIDGE='1',GTK_A11Y='none',LUMEN_FILE_SERVICE='0',LUMEN_LAUNCH_SCOPES='0',LUMEN_NATIVE_WINDOW_CONTROLS='0',LUMEN_NATIVE_WINDOW_ACTIONS='0',LUMEN_UNIFIED_OWNED_HEADERS='1')
for key in ('DISPLAY','WAYLAND_DISPLAY','SWAYSOCK','AT_SPI_BUS_ADDRESS'):env.pop(key,None)
if a.effects:env.update(WLR_RENDERER='gles2',WLR_RENDERER_FORCE_SOFTWARE='1',WLR_RENDERER_ALLOW_SOFTWARE='1',LIBGL_ALWAYS_SOFTWARE='1')
folders={}
for name in ('First','Second','Third'):
    folder=Path(env['HOME'])/name;folder.mkdir();(folder/'selected.txt').write_text('unchanged fixture '+name);[(folder/('document-%03d.txt'%i)).write_text('synthetic') for i in range(600)];folders[name]=str(folder)
palettes=json.loads((repo/'res/lumen-desktop/appearance/palette.json').read_text());appearance=out/'appearance.json';appearance.write_text(json.dumps(dict(font=palettes['font'],palette=palettes['dark'],preferences={})));env['LUMEN_APPEARANCE_STATE']=str(appearance)
shutil.copytree(Path(env['LUMEN_NOTIFICATION_HELPER']).parent.parent,out/'assets')
qml=out/'shell.qml';qml.write_text('''import QtQuick
import Quickshell
import Quickshell.Io
import "assets/shell" as D
import "assets/shell/FilesTabModel.js" as Tabs
import "assets/shell/FilesHistoryModel.js" as History
ShellRoot {
 id:fixture
 property var paths:'''+json.dumps(folders)+'''
 QtObject {id:exported;property var state:({appName:"",title:"",windowId:0})}
 QtObject {id:stub;property int focusEpoch:0;property var windows:[];property var exportedMenu:exported;function activeFiles(window){}function focusWindow(window){}}
 D.FilesState {id:service;persistViewPreferences:true;preferencesLoaded:true;path:fixture.paths.First;viewMode:"list";favorites:[{name:"First",path:fixture.paths.First},{name:"Second",path:fixture.paths.Second},{name:"Third",path:fixture.paths.Third}]}
 D.FilesWindow {id:files;shell:stub;service:service;visible:true}
 Component.onCompleted:{files.tabs=["First","Second","Third"].map((name,index)=>{const tab=Tabs.create(name.toLowerCase(),paths[name]);tab.history=index===0?[paths.First]:[paths.First,paths[name]];tab.historyIndex=index===0?0:1;tab.selectedPaths=[paths[name]+"/selected.txt"];tab.selectedPath=tab.selectedPaths[0];tab.viewMode="list";return tab;});service.history=files.tabs[0].history.slice();service.historyIndex=0;service.navigate(paths.First,0);}
 function find(name,item){if(item.objectName===name)return item;for(const child of item.children || []){const found=find(name,child);if(found)return found;}return null;}
 IpcHandler {target:"filesAudit"
  function state():string {return JSON.stringify({ready:service.ready && !service.busy && service.rows.length>0,tabs:files.tabs,activeId:files.activeTabId,path:service.path,history:service.history,historyIndex:service.historyIndex,selection:files.selectedPaths.length>512?[]:files.selectedPaths,selectionCount:files.selectedPaths.length,presentation:History.presentation(files.historyPresentation()),error:service.error,warning:service.warning,backendReady:service.ready,view:service.viewMode,sidebarWidth:files.sidebarWidth,savedWidth:service.sidebarWidth,collapsed:service.collapsedSidebarGroups,width:files.width,resizeFocus:fixture.find("files-sidebar-resize",files.contentItem).activeFocus,resizeFacts:fixture.find("files-sidebar-resize",files.contentItem).children.filter(item=>item.pressed!==undefined).map(item=>({pressed:item.pressed,cancelled:item.gestureCancelled})),focused:files.contentItem.Window.window && files.contentItem.Window.window.activeFocusItem?files.contentItem.Window.window.activeFocusItem.objectName:""});}
  function navigate(path:string):void {files.navigate(path);}
  function back():void {service.back(-1);}
  function sort():void {files.sortBy("name");}
  function rowGeometry(index:int):string {const name=service.viewMode==="columns"?"files-column-"+service.path:"files-"+service.viewMode+"-view";const view=fixture.find(name,files.contentItem);if(!view)return "null";const row=view.itemAtIndex(index);if(!row)return "null";const at=row.mapToItem(files.contentItem,0,0);return JSON.stringify({x:at.x,y:at.y,width:row.width,height:row.height});}
  function select():void {files.choose(0,0);files.saveTab();}
  function geometry(name:string):string {const item=fixture.find(name,files.contentItem);if(!item)return "null";const at=item.mapToItem(files.contentItem,0,0);return JSON.stringify({x:at.x,y:at.y,width:item.width,height:item.height});}
 }
}''')
processes=[]
def launch(command,name):
    with (out/name).open('w') as log:
        process=sp.Popen(command,env=env,stdout=log,stderr=sp.STDOUT,start_new_session=True)
    processes.append(process);return process

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
def key(*keys):call('wtype',*keys);time.sleep(.15)
def drag(start,dx,dy=0,cancel=False):
    move(*start);command('p')
    for index in range(8):command('m '+str(dx/8)+' '+str(dy/8))
    if cancel:
        snapshots=[state()];key('-k','Escape');snapshots.append(state());command('m 8 0');snapshots.append(state());(out/'cancellation-state.json').write_text(json.dumps(snapshots,indent=2))
    command('r')
def backendPid():
    pid=shellProcess.pid
    children=Path('/proc/'+str(pid)+'/task/'+str(pid)+'/children').read_text().split()
    return next(int(child) for child in children if str(out/'assets/shell/files.py').encode() in Path('/proc/'+child+'/cmdline').read_bytes())
def invariant():
    value=state();assert value['path']==folders['First'] and value['selection']==[folders['First']+'/selected.txt'] and value['activeId']=='first',value
    return value
try:
    config=out/'sway.conf';config.write_text('output HEADLESS-1 resolution 1280x900\nxwayland disable\nfor_window [all] floating enable, border pixel 0\n')
    launch([a.sway,'--unsupported-gpu','-c',str(config)],'sway.log');runtime=Path(env['XDG_RUNTIME_DIR']);env['SWAYSOCK']=str(wait(lambda:next(runtime.glob('sway-ipc*.sock'),None)));env['WAYLAND_DISPLAY']=wait(lambda:next((path.name for path in runtime.glob('wayland-*') if not path.name.endswith('.lock')),None))
    shellProcess=launch(['quickshell','-p',str(qml)],'shell.log');wait(lambda:state()['ready']);ipc('select')
    call('grim',str(out/'files-selected.png'))
    launch(['wtype','-s','60000'],'keyboard.log')
    pointer=sp.Popen([a.pointer_tool,'pointer','move','0','0'],env=env,stdin=sp.PIPE,text=True,start_new_session=True);processes.append(pointer);time.sleep(.2)
    checks=[]
    for mode,number in [('list','2'),('grid','1'),('gallery','4'),('columns','3')]:
        ipc('navigate',folders['First']);wait(lambda:state()['ready'] and state()['path']==folders['First']);ipc('select');key('-M','ctrl','-k',number,'-m','ctrl')
        # Real keyboard scrolling and selection through each production view.
        time.sleep(.35);key('-k','Up');row=wait(lambda:json.loads(ipc('rowGeometry','0')));r=window()['rect'];click((r['x']+row['x']+row['width']/2,r['y']+row['y']+row['height']/2))
        key(*sum((['-k','Down'] for _ in range(45)),[]));time.sleep(.4);before=state()
        assert before['selection'] and before['view']==mode,before
        ipc('navigate',folders['Second']);wait(lambda:state()['ready'] and state()['path']==folders['Second']);key('-M','ctrl','-k','2','-m','ctrl');preference=Path(env['XDG_STATE_HOME'])/'lumen-desktop/session/files.json';wait(lambda:preference.exists() and json.loads(preference.read_text()).get('view')=='list');preferenceTime=preference.stat().st_mtime_ns;key('-M','alt','-k','Left','-m','alt');wait(lambda:state()['ready'] and state()['path']==folders['First']);time.sleep(.4);after=state()
        (out/('history-'+mode+'.json')).write_text(json.dumps(dict(before=before,after=after),indent=2))
        assert after['selection']==before['selection'] and after['view']==mode,(mode,before,after)
        assert json.loads(preference.read_text())['view']=='list' and preference.stat().st_mtime_ns==preferenceTime,'History restoration overwrote global view preference'
        def offset(v):
            scroll=v['presentation']['scroll']
            if mode=='columns':return next((c['offset'] for c in scroll['columns']['columns'] if c['path']==folders['First']),0)
            return scroll[mode]['offset']
        assert offset(before)>0,(mode,'view never scrolled',before)
        assert abs(offset(before)-offset(after))<3,(mode,'scroll not restored',offset(before),offset(after))
        # Refresh and a failed navigation retain the exact committed entry.
        key('-k','F5');wait(lambda:state()['ready']);time.sleep(.25);assert state()['selection']==before['selection']
        committed=state();ipc('navigate',folders['First']+'/not-a-directory');wait(lambda:state()['ready'] and state()['error']);time.sleep(.25);failed=state();assert failed['path']==committed['path'] and failed['historyIndex']==committed['historyIndex'] and failed['selection']==committed['selection'],failed
        if mode=='list':
            previous=state()['selection'];ipc('sort');wait(lambda:state()['ready']);time.sleep(.3);assert state()['selection']==previous;ipc('sort');wait(lambda:state()['ready']);time.sleep(.3);checks.append('Changed sort coordinates reconcile captured object identity without selecting a replacement index')
        checks.append(mode+' selection, view and scrolled viewport survive Back, refresh and failed navigation')
    # Real tab controls restore independent entry stacks and presentation.
    first=state();click(point('document-tab-second',.4));wait(lambda:state()['ready'] and state()['activeId']=='second');ipc('select');key('-k','Down');second=state();click(point('document-tab-first',.4));wait(lambda:state()['ready'] and state()['activeId']=='first');time.sleep(.3);assert state()['selection']==first['selection'] and state()['historyIndex']==first['historyIndex'];click(point('document-tab-second',.4));wait(lambda:state()['ready'] and state()['activeId']=='second');time.sleep(.3);assert state()['selection']==second['selection'];checks.append('Actual tab pointer switches retain independent history entry selection')
    ipc('select')
    # A successful branch truncates forward history; failed navigation did not.
    ipc('navigate',folders['First']);wait(lambda:state()['ready'] and state()['path']==folders['First']);ipc('navigate',folders['Second']);wait(lambda:state()['ready'] and state()['path']==folders['Second']);ipc('navigate',folders['Third']);wait(lambda:state()['ready'] and state()['path']==folders['Third']);key('-M','alt','-k','Left','-m','alt');wait(lambda:state()['ready'] and state()['path']==folders['Second']);ipc('navigate',folders['First']);wait(lambda:state()['ready'] and state()['path']==folders['First']);branch=state();key('-M','alt','-k','Right','-m','alt');time.sleep(.2);assert state()['historyIndex']==branch['historyIndex'] and state()['path']==branch['path'];checks.append('Successful branch truncates forward entries; forward shortcut stays at committed destination')
    # Query is part of the entry, not a persistent directory preference.
    key('-M','ctrl','-k','f','-m','ctrl');key('document-00');wait(lambda:state()['ready'] and state()['presentation']['query']=='document-00');filtered=state();ipc('navigate',folders['Second']);wait(lambda:state()['ready'] and state()['path']==folders['Second']);ipc('back');wait(lambda:state()['ready'] and state()['path']==filtered['path']);time.sleep(.3);assert state()['presentation']['query']=='document-00';checks.append('Actual filter typing returns with history query and matching rows')
    # Superseded real list responses cannot commit an intermediate route.
    ipc('select');beforeRace=state();backend=backendPid();os.kill(backend,signal.SIGSTOP)
    try:ipc('navigate',folders['Second']);ipc('navigate',folders['Third'])
    finally:os.kill(backend,signal.SIGCONT)
    wait(lambda:state()['ready'] and state()['path']==folders['Third']);race=state();assert len(race['history'])==beforeRace['historyIndex']+2,race;checks.append('Actual paused backend response race commits only latest requested destination')
    # Kill only this fixture backend, then reconnect using the production F5.
    committed=state();backend=backendPid();os.kill(backend,signal.SIGKILL);wait(lambda:not state()['backendReady']);key('-k','F5');wait(lambda:state()['ready']);time.sleep(.3);assert state()['historyIndex']==committed['historyIndex'] and state()['selection']==committed['selection'];checks.append('Actual backend termination and keyboard reconnect retain committed entry state')
    # Overflow must never silently restore a misleading partial Select All.
    ipc('select');key('-M','ctrl','a','-m','ctrl');assert state()['selectionCount']>512;ipc('navigate',folders['Second']);wait(lambda:state()['ready'] and state()['path']==folders['Second']);key('-M','alt','-k','Left','-m','alt');wait(lambda:state()['ready'] and state()['path']==committed['path']);time.sleep(.3);overflow=state();assert overflow['selection']==[] and overflow['presentation']['current'] is None and 'selection' in overflow['warning'];checks.append('Large Select All restores no partial selection and reports explicit cleared-selection notice')
    # A failed tab list cannot overwrite another tab's committed presentation.
    inactive=next(tab for tab in state()['tabs'] if tab['id']=='first');shutil.rmtree(folders['First']);click(point('document-tab-first',.4));wait(lambda:state()['activeId']=='first' and not state()['backendReady'] or state()['activeId']=='first' and state()['error']);failedTab=state();assert failedTab['path']==folders['First'] and next(tab for tab in failedTab['tabs'] if tab['id']=='first')['history']==inactive['history'];checks.append('Failed tab destination never substitutes prior tab rows or overwrites stored stack')
    click(point('document-tab-second',.4));wait(lambda:state()['activeId']=='second' and state()['ready'])
    # Replacing a selected path must not select the new inode on refresh.
    ipc('select');selected=state()['selection'][0];original=Path(selected);original.rename(original.with_name('retired-identity.txt'));original.write_text('synthetic replacement');key('-k','F5');wait(lambda:state()['ready']);time.sleep(.3);assert selected not in state()['selection'];checks.append('Refresh never transfers known file identity to replacement at identical path')
    log=(out/'shell.log').read_text();assert not any(marker in log for marker in ('ReferenceError:','TypeError:','Binding loop','Failed to load configuration','Cannot assign')),log
    (out/'result.json').write_text(json.dumps(dict(passed=True,checks=checks,final=state()),indent=2))

finally:
    for process in reversed(processes):
        if process.poll() is None:os.killpg(process.pid,signal.SIGTERM)
    for process in reversed(processes):
        try:process.wait(timeout=4)
        except sp.TimeoutExpired:os.killpg(process.pid,signal.SIGKILL)
    for _fixture_process in processes:
        if _fixture_process.poll() is None:_fixture_process.wait(timeout=5)
    close_reaped_pipes(processes)
