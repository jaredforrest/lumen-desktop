"""Actual Files menu dispatch in a private namespace; no transport or physical-input claim."""
import argparse,hashlib,json,os,re,shutil,subprocess as sp,sys
from pathlib import Path
from application_fixture import offscreen
from private_native_bus import start_private_bus
from fixture_resources import close_reaped_pipes
p=argparse.ArgumentParser();p.add_argument('--launcher',type=Path,required=True);p.add_argument('--fontconfig',type=Path,required=True);p.add_argument('--output',type=Path,required=True);p.add_argument('--inside',action='store_true');p.add_argument('--window-source',type=Path);a=p.parse_args();repo=Path(__file__).resolve().parents[2]
if not a.inside:
    extra=['--window-source',str(offscreen.immutable_file(a.window_source))] if a.window_source else []
    code=offscreen.run(repo,a.output,a.launcher,a.fontconfig,[sys.executable,'/source/tests/desktop/files_share_menu_qml.py','--inside','--launcher',str(a.launcher.resolve()),'--fontconfig',str(a.fontconfig.resolve()),'--output','/artifacts']+extra)
    (a.output/'provenance.json').write_text(json.dumps(dict(revision=sp.check_output(['git','-C',str(repo),'rev-parse','HEAD'],text=True).strip(),exitCode=code),indent=2)+'\n');raise SystemExit(code)
env=offscreen.environment()
out=a.output;assets=out/'assets';shutil.copytree(repo/'res/lumen-desktop',assets);seams=[]
def change(relative,old,new):
    path=assets/relative;before=path.read_text();assert before.count(old)==1,relative;after=before.replace(old,new);path.write_text(after);seams.append(dict(path=relative,beforeSha256=hashlib.sha256(before.encode()).hexdigest(),afterSha256=hashlib.sha256(after.encode()).hexdigest(),old=old,new=new))
if a.window_source:
    baseline=offscreen.immutable_file(a.window_source).read_text()
    marker='id:actions;onOpened:';assert baseline.count(marker)==1
    baseline=baseline.replace(marker,'id:actions;objectName:"files-actions-menu";onOpened:')
    marker='required property var modelData;width:208;';assert baseline.count(marker)==1
    baseline=baseline.replace(marker,'objectName:"files-action-"+modelData.action;'+marker)
    change('shell/FilesWindow.qml',(assets/'shell/FilesWindow.qml').read_text(),baseline)
change('shell/files.py',"    if action=='volumes':\n","    if action=='volumes':\n        return dict(volumes=[])\n")
change('shell/NearbySharingState.qml','readonly property bool configured:!!(Quickshell.env("LUMEN_NEARBY_SHARING_PROVIDER") || "")','readonly property bool configured:true')
change('shell/NearbySharingState.qml','function refresh(){if(configured && !sharingReader.running)sharingReader.running=true;}','function refresh(){}')
change('shell/AppLaunch.qml','QtObject {','QtObject {\n    property var recordedCommands:[]')
change('shell/AppLaunch.qml','function run(command, workingDirectory) {','function run(command, workingDirectory) {\n        recordedCommands=recordedCommands.concat([command]);return;')
(out/'fixture-seams.json').write_text(json.dumps(seams,indent=2)+'\n')
folder=Path('/home/fixture/Documents');folder.mkdir();(folder/'a $(literal).txt').write_text('first unchanged');(folder/'b.txt').write_text('second unchanged')
palette=json.loads((assets/'appearance/palette.json').read_text());appearance=out/'appearance.json';appearance.write_text(json.dumps(dict(font=palette['font'],palette=palette['dark'],preferences={})))
qml=out/'fixture.qml';qml.write_text('''import QtQuick
import Quickshell
import "assets/shell" as D
ShellRoot {
 id:test
 property int phase:0
 property int ticks:0
 property var savedRows:[]
 property var menu:null
 QtObject {id:exported;property var state:({appName:"",title:"",windowId:0})}
 QtObject {id:stub;property int focusEpoch:0;property var windows:[];property var exportedMenu:exported;function activeFiles(window){}function focusWindow(window){}}
 D.FilesState {id:service;persistViewPreferences:false;path:"/home/fixture/Documents"}
 D.FilesWindow {id:files;shell:stub;service:service;visible:true}
 function check(value,message){if(!value)throw Error(message+" phase="+phase);}
 function find(name,item,seen){if(!item || seen.includes(item))return null;seen.push(item);if(item.objectName===name)return item;const next=Array.from(item.children || []).concat(Array.from(item.resources || []));if(item.contentItem)next.push(item.contentItem);for(const child of next){const result=find(name,child,seen);if(result)return result;}return null;}
 function select(index){files.choose(index,0);}
 function open(){service.error="";menu.open();}
 function activate(){const row=find("files-action-nearby-send",files.contentItem,[]);check(row && row.enabled,"actual sharing row must be enabled");row.clicked();}
 Timer {interval:50;running:true;repeat:true;onTriggered:{try{
  if(++test.ticks>120)throw Error("Fixture readiness deadline");
  if(test.phase===0){if(!service.ready || service.busy || service.rows.length!==2)return;test.menu=find("files-actions-menu",files.contentItem,[]);check(test.menu,"actual menu found");test.savedRows=service.rows;D.NearbySharingState.state={available:true,active:false,canSend:true,detail:"Off"};D.NearbySharingState.observedAt=Date.now();D.NearbySharingState.now=Date.now();select(0);open();}
  else if(test.phase===1){activate();check(D.AppLaunch.recordedCommands.length===1,"unchanged selection dispatched exactly once");check(D.AppLaunch.recordedCommands[0][2]==="/home/fixture/Documents/a $(literal).txt","literal selected argument");select(0);open();select(1);}
  else if(test.phase===2){activate();check(D.AppLaunch.recordedCommands.length===1 && service.error.includes("changed"),"later selection must not dispatch");select(0);open();const rows=service.rows.slice();rows[0]=Object.assign({},rows[0],{identity:{device:rows[0].identity.device,inode:rows[0].identity.inode+1}});service.rows=rows;}
  else if(test.phase===3){activate();check(D.AppLaunch.recordedCommands.length===1 && service.error.includes("changed"),"replaced row must not dispatch");service.rows=test.savedRows;select(1);open();}
  else if(test.phase===4){activate();check(D.AppLaunch.recordedCommands.length===2 && D.AppLaunch.recordedCommands[1][2]==="/home/fixture/Documents/b.txt","fresh explicit menu dispatch");console.log("FILES_SHARE_MENU_PASS "+JSON.stringify({commands:D.AppLaunch.recordedCommands,checks:4}));Qt.quit();}
  ++test.phase;
 }catch(error){console.error("FILES_SHARE_MENU_FAIL "+error);Qt.exit(1);}}}
 Component.onCompleted:service.navigate("/home/fixture/Documents")
}''')
env['LUMEN_APPEARANCE_STATE']=str(appearance)
processes=[]
try:
    start_private_bus(env,out,processes)
    result=sp.run(['quickshell','-p',str(qml)],env=env,text=True,stdout=sp.PIPE,stderr=sp.STDOUT,timeout=15);(out/'qml.log').write_text(result.stdout)
finally:
    for process in reversed(processes):
        process.terminate();process.wait(timeout=5)
    close_reaped_pipes(processes)
match=re.search(r'FILES_SHARE_MENU_PASS (\{.*\})',result.stdout);assert result.returncode==0 and match,result.stdout
assert not re.search(r'(?i)(ReferenceError|TypeError|binding loop|FILES_SHARE_MENU_FAIL)',result.stdout),result.stdout
record=json.loads(match[1]);assert (folder/'a $(literal).txt').read_text()=='first unchanged' and (folder/'b.txt').read_text()=='second unchanged';record.update(passed=True,scope='Actual production Files menu signal dispatch; no physical pointer, transport or delivery claim',namespace=dict(status=Path('/proc/self/status').read_text(),mounts=Path('/proc/self/mountinfo').read_text()));(out/'result.json').write_text(json.dumps(record,indent=2)+'\n')
