"""Offscreen actual Files refresh keeps focus independent from selected membership."""
import json,os,re,shutil,subprocess,tempfile,unittest
from pathlib import Path
ROOT=Path(__file__).resolve().parents[2]
class FilesSelectionQt(unittest.TestCase):
 def test_distinct_focus_survives_sort_and_disappears_on_replacement(self):
  launcher=Path(os.environ.get('LUMEN_TEST_SHELL_LAUNCHER') or shutil.which('lumen-desktop-shell'))
  prefix=re.split(r'\n[ \t]*exec quickshell ',launcher.read_text(),maxsplit=1)[0]
  with tempfile.TemporaryDirectory(prefix='lumen-selection-qt-') as temporary:
   root=Path(temporary);root.chmod(0o700);docs=root/'Documents';docs.mkdir()
   for name in ('one.txt','two.txt','three.txt'):(docs/name).write_text('synthetic '+name)
   full=root/'Full';full.mkdir()
   for index in range(160):(full/f'item-{index:03}.txt').write_text('synthetic')
   env=dict(os.environ,HOME=temporary,XDG_RUNTIME_DIR=temporary,DBUS_SYSTEM_BUS_ADDRESS='unix:path='+temporary+'/no-system-bus',PULSE_SERVER='unix:'+temporary+'/no-audio',PIPEWIRE_RUNTIME_DIR=temporary,PIPEWIRE_REMOTE='no-pipewire')
   for key in ('CONFIG','STATE','CACHE','DATA'):env['XDG_'+key+'_HOME']=str(root/key.lower())
   for key in ('DISPLAY','WAYLAND_DISPLAY','SWAYSOCK','DBUS_SESSION_BUS_ADDRESS','AT_SPI_BUS_ADDRESS'):env.pop(key,None)
   env.update(dict(value.decode().split('=',1) for value in subprocess.check_output(['bash','-c',prefix+'\nexec env -0'],env=env).split(b'\0') if b'=' in value))
   env.update(QT_QPA_PLATFORM='offscreen',QT_QPA_PLATFORMTHEME='generic',QT_QUICK_BACKEND='software',QT_ACCESSIBILITY='0',QT_LINUX_ACCESSIBILITY_ALWAYS_ON='0',NO_AT_BRIDGE='1',GTK_A11Y='none',LUMEN_FILE_SERVICE='0',LUMEN_LAUNCH_SCOPES='0')
   appearance=root/'appearance.json';palettes=json.loads((ROOT/'res/lumen-desktop/appearance/palette.json').read_text());appearance.write_text(json.dumps(dict(font=palettes['font'],palette=palettes['dark'],preferences={})));env['LUMEN_APPEARANCE_STATE']=str(appearance)
   assets=root/'assets';shutil.copytree(ROOT/'res/lumen-desktop',assets)
   backend=assets/'shell/files.py';original=backend.read_text();needle="    if action=='volumes':\n";self.assertEqual(original.count(needle),1);backend.chmod(0o600);backend.write_text(original.replace(needle,"    if action=='volumes':return dict(volumes=[])\n"+needle).replace("    if action=='column-list':return listing(data)","    if action=='column-list' and data.get('path') not in "+repr([str(docs),str(full)])+":return dict(rows=[])\n    if action=='column-list':return listing(data)"))
   mutate=root/'replace.py';mutate.write_text("import os,sys\nfrom pathlib import Path\np=Path(sys.argv[1]);fd=os.open(p,os.O_PATH);p.unlink();p.write_text('replacement');os.close(fd)\n")
   qml=root/'shell.qml';qml.write_text('''import QtQuick
import Quickshell
import Quickshell.Io
import "assets/shell" as D
import "assets/shell/FilesSelectionModel.js" as Selection
ShellRoot {
 id:fixture
 property int phase:0
 property int ticks:0
 property string chosen:""
 property string focused:""
 QtObject {id:exported;property var state:({appName:"",title:"",windowId:0})}
 QtObject {id:permission;property bool permitted:true}
 QtObject {id:stub;property int focusEpoch:0;property var windows:[];property var exportedMenu:exported;property var screenSaverPreferences:permission;function activeFiles(window){}function focusWindow(window){}}
 D.FilesState {id:service;persistViewPreferences:false;preferencesLoaded:true;path:'''+json.dumps(str(docs))+''';viewMode:"list"}
 D.FilesWindow {id:files;shell:stub;service:service;visible:true}
 Process {id:mutator;command:[Quickshell.env("GLOBAL_MENU_PYTHON"),'''+json.dumps(str(mutate))+''',fixture.focused];onExited:{service.refresh();fixture.phase=4;}}
 TextInput {id:focusSink;parent:files.contentItem;width:1;height:1}
 function find(name,item){if(item.objectName===name)return item;for(const child of item.children || []){const found=find(name,child);if(found)return found;}return null;}
 function arm(){const band=find("files-list-band-selection",files.contentItem);check(!!band,"Missing production band");band.permitted=true;band.transaction=Selection.begin(service.rows,files.selectedPaths,files.selected,files.selectionAnchor,band.contextKey,"replace");check(!!band.transaction,"Band capture failed");band.start={x:band.width-20,y:band.height-20};check(band.view.indexAt(band.start.x,band.start.y+band.view.contentY)<0,"Start is not blank");band.capturedBoxes=band.boxes();check(band.capturedBoxes.length===3,"Real delegate geometry unavailable");band.pending=true;band.forceActiveFocus();band.update(1,1);return band;}
 function gutter(name,horizontal){const band=find(name,files.contentItem);if(!band || !band.permitted || !band.boxes().length)return false;const boxes=band.boxes();const box=boxes.find(box=>box.x+box.width>band.view.contentX && box.y+box.height>band.view.contentY);if(!box)return false;const point=horizontal?{x:box.x+box.width/2,y:band.view.contentY+band.height-band.bottomInset-6}:{x:box.x+box.width+6,y:Math.max(box.y,band.view.contentY)+Math.min(10,box.height/2)};check(point.x>=band.view.contentX && point.x<band.view.contentX+band.width-band.rightInset && point.y>=band.view.contentY && point.y<band.view.contentY+band.height-band.bottomInset,"Gutter outside usable viewport "+name);check(!boxes.some(box=>Selection.contains(box,point)),"Gutter occupied by actual delegate "+name);check(horizontal?band.view.contentWidth>band.width:band.view.contentHeight>band.height,"Folder not fully populated "+name);return true;}
 function check(value,message){if(!value)throw Error(message+" phase="+phase+" details="+JSON.stringify({current:files.current,selected:files.selected,paths:files.selectedPaths,expectedFocus:focused,history:service.history,restoring:files.restoringPresentationState}));}
 Timer {interval:50;running:true;repeat:true;onTriggered:{try {
  if(++ticks>140)throw Error("Selection fixture timed out phase="+phase+" rows="+service.rows.length+" mode="+service.viewMode);
  if(!service.ready || service.busy)return;
  if(phase===0){service.navigate(service.path);phase=1;return;}
  if(phase===1){if(service.rows.length!==3)return;files.restorePresentation(null);check(files.restoringPresentationState,"Deferred restore setup failed");files.choose(0,0);check(!files.restoringPresentationState,"Explicit selection did not retire deferred restore");fixture.chosen=files.current.path;files.choose(1,Qt.ControlModifier);fixture.focused=files.current.path;files.choose(1,Qt.ControlModifier);check(files.current.path===focused && files.selectedPaths.length===1 && files.selectedPaths[0]===chosen,"Focus not distinct before refresh");service.descending=true;service.refresh();phase=2;return;}
  if(phase===2){check(files.current && files.current.path===focused,"Resort changed focused object");check(files.selectedPaths.length===1 && files.selectedPaths[0]===chosen,"Resort changed selected membership");check(!files.selectedPaths.includes(focused),"Focused item became selected");phase=3;mutator.running=true;return;}
  if(phase===4){check(!files.current && files.selected===-1,"Replacement focus retargeted another row");check(files.selectedPaths.length===1 && files.selectedPaths[0]===chosen,"Replacement changed other selected object");check(service.warning.includes("Select them again."),"Missing lost-focus notice");const band=arm();check(files.bandPreviewPaths && files.bandPreviewPaths.length===3,"Real geometry did not preview all rows");check(files.selectedPaths.length===1 && files.selectedPaths[0]===chosen,"Preview committed selection early");band.cancel("escape");check(files.bandPreviewPaths===null && files.selectedPaths.length===1,"Escape did not restore prior selection");phase=5;return;}
  if(phase===5){const band=arm();band.view.contentY+=1;check(!band.pending && files.bandPreviewPaths===null,"Viewport scrolling retained stale band");band.view.contentY=0;phase=6;return;}
  if(phase===6){const band=arm();focusSink.forceActiveFocus();check(!band.pending && focusSink.activeFocus,"Cancellation stole a legitimate new focus");phase=7;return;}
  if(phase===7){const band=arm();band.contextKey="retired-output";check(!band.pending && files.bandPreviewPaths===null,"Context change retained gesture");phase=8;return;}
  if(phase===8){const band=arm();band.permitted=false;check(!band.pending,"Unknown or locked policy did not cancel");band.permitted=true;files.contentItem.forceActiveFocus();service.navigate('''+json.dumps(str(full))+''');phase=9;return;}
  if(phase===9){if(service.rows.length!==160 || !gutter("files-list-band-selection",false))return;service.viewMode="grid";phase=10;return;}
  if(phase===10){if(!gutter("files-grid-band-selection",false))return;service.viewMode="gallery";phase=11;return;}
  if(phase===11){if(!gutter("files-gallery-band-selection",true))return;service.viewMode="columns";phase=12;return;}
  if(phase===12){if(!gutter("files-column-band-"+service.path,false))return;const leaf=find("files-column-band-"+service.path,files.contentItem);check(!!Selection.begin(leaf.rows,files.selectedPaths,files.selected,files.selectionAnchor,leaf.contextKey,"replace"),"Active leaf must expose a captured JS row sequence, not a QAbstractItemModel adapter");console.log("FILES_SELECTION_QT_PASS");Qt.quit();}
 }catch(error){console.error("FILES_SELECTION_QT_FAIL "+error);Qt.quit();}}}
}''')
   result=subprocess.run(['dbus-run-session','--','quickshell','-p',str(qml)],env=env,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,timeout=20)
   self.assertEqual(result.returncode,0,result.stdout);self.assertIn('FILES_SELECTION_QT_PASS',result.stdout,result.stdout)
   for marker in ('FILES_SELECTION_QT_FAIL','ReferenceError:','TypeError:','Binding loop','Failed to load configuration','Cannot assign','ResourceWarning','Exception ignored','Cannot assign to non-existent'):
    self.assertNotIn(marker,result.stdout,result.stdout)
if __name__=='__main__':unittest.main()
