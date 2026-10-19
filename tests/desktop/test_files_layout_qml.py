"""Real Files backend/tab restoration with disposable content and preferences."""
import json,os,re,shutil,subprocess,tempfile,unittest
from pathlib import Path
ROOT=Path(__file__).resolve().parents[2]
class FilesLayoutQt(unittest.TestCase):
    def test_exact_tab_history_selection_close_and_sidebar_layout(self):
        launcher=Path(os.environ.get('LUMEN_TEST_SHELL_LAUNCHER') or shutil.which('lumen-desktop-shell'))
        prefix=re.split(r'\n[ \t]*exec quickshell ',launcher.read_text(),maxsplit=1)[0]
        with tempfile.TemporaryDirectory(prefix='lumen-files-layout-') as temporary:
            root=Path(temporary)
            for name in ['First','Second','Third']:
                (root/name).mkdir();(root/name/'selected.txt').write_text('fixture '+name)
            env=dict(os.environ,HOME=temporary,XDG_RUNTIME_DIR=temporary)
            for key in ('CONFIG','STATE','CACHE','DATA'):env['XDG_'+key+'_HOME']=str(root/key.lower())
            for key in ('DISPLAY','WAYLAND_DISPLAY','SWAYSOCK','DBUS_SESSION_BUS_ADDRESS','AT_SPI_BUS_ADDRESS'):env.pop(key,None)
            exported=subprocess.check_output(['bash','-c',prefix+'\nexec env -0'],env=env);env.update(dict(value.decode().split('=',1) for value in exported.split(b'\0') if b'=' in value))
            env.update(QT_QPA_PLATFORM='offscreen',QT_QPA_PLATFORMTHEME='generic',QT_QUICK_BACKEND='software',QT_ACCESSIBILITY='0',QT_LINUX_ACCESSIBILITY_ALWAYS_ON='0',NO_AT_BRIDGE='1',GTK_A11Y='none',LUMEN_FILE_SERVICE='0',LUMEN_LAUNCH_SCOPES='0')
            appearance=root/'appearance.json';palettes=json.loads((ROOT/'res/lumen-desktop/appearance/palette.json').read_text());appearance.write_text(json.dumps(dict(font=palettes['font'],palette=palettes['dark'],preferences={})));env['LUMEN_APPEARANCE_STATE']=str(appearance)
            shutil.copytree(ROOT/'res/lumen-desktop',root/'assets')
            qml=root/'shell.qml';qml.write_text('''import QtQuick
import Quickshell
import "assets/shell" as D
ShellRoot {
 id:fixture
 property int phase:0
 property int ticks:0
 property string firstId:""
 property string secondId:""
 property string thirdId:""
 property var thirdHistory:[]
 property int thirdHistoryIndex:-1
 property string base:'''+json.dumps(str(root))+'''
 QtObject {id:exported;property var state:({appName:"",title:"",windowId:0})}
 QtObject {id:stub;property int focusEpoch:0;property var windows:[];property var exportedMenu:exported;function activeFiles(window){}function focusWindow(window){}}
 D.FilesState {id:service;persistViewPreferences:false;path:fixture.base+"/First"}
 D.FilesWindow {id:files;shell:stub;service:service;visible:true}
 function check(value,message){if(!value)throw Error(message+" phase="+phase);}
 function select(){files.choose(0,0);check(files.selectedPaths.length===1,"Selection missing");}
 Timer {interval:60;running:true;repeat:true;onTriggered:{try {
  if(++ticks>240)throw Error("Files layout timeout phase="+phase+" busy="+service.busy+" error="+service.error);
  if(!service.ready || service.busy)return;
  if(phase===0){service.navigate(base+"/First");phase=1;return;}
  if(!service.rows.length)return;
  if(phase===1){select();firstId=files.activeTabId;files.newTab();secondId=files.activeTabId;phase=2;return;}
  if(phase===2){service.navigate(base+"/Second");phase=3;return;}
  if(phase===3){select();service.viewMode="list";files.newTab();thirdId=files.activeTabId;phase=4;return;}
  if(phase===4){service.navigate(base+"/Third");phase=5;return;}
  if(phase===5){
   select();thirdHistory=service.history.slice();thirdHistoryIndex=service.historyIndex;
   files.reorderTab(firstId,2);check(files.activeTabId===thirdId,"Reorder changed active identity");check(service.path===base+"/Third","Reorder navigated");
   files.closeTabId(firstId);check(files.activeTabId===thirdId && service.path===base+"/Third","Inactive close navigated");
   files.switchTab(files.tabs.findIndex(tab=>tab.id===secondId));phase=6;return;
  }
  if(phase===6){
   check(service.path===base+"/Second","Wrong selected tab location");check(files.selectedPaths[0]===base+"/Second/selected.txt","Tab selection not restored");check(service.viewMode==="list","Tab view not restored");
   files.closeTabId(secondId);phase=7;return;
  }
  if(phase===7){
   check(files.activeTabId===thirdId && service.path===base+"/Third","Active close chose wrong tab");check(JSON.stringify(service.history)===JSON.stringify(thirdHistory) && service.historyIndex===thirdHistoryIndex,"Close modified destination history");check(files.selectedPaths[0]===base+"/Third/selected.txt","Close lost selected object");
   files.newTab();const temporaryId=files.activeTabId;files.switchTab(files.tabs.findIndex(tab=>tab.id===thirdId));files.closeTabId(temporaryId);phase=75;return;
  }
  if(phase===75){
   check(files.tabs.length===1 && files.activeTabId===thirdId,"Pending close changed tab identity");check(service.path===base+"/Third" && JSON.stringify(service.history)===JSON.stringify(thirdHistory),"Rapid switch saved another tab state");check(files.selectedPaths[0]===base+"/Third/selected.txt","Rapid switch lost selection");
   files.collapseSidebar("favourites",true);files.collapseSidebar("locations",true);service.sidebarWidth=340;phase=8;return;
  }
  if(phase===8){
   check(files.groupCollapsed("favourites") && files.groupCollapsed("locations"),"Groups did not collapse");check(service.path===base+"/Third" && files.selectedPaths[0]===base+"/Third/selected.txt","Sidebar operation navigated/lost selection");
   phase=9;return;
  }
  if(phase===9){check(service.sidebarWidth===340,"Narrow clamp overwrote saved width");files.collapseSidebar("favourites",false);files.collapseSidebar("locations",false);check(files.selectedPaths.length===1,"Expand lost selection");console.log("FILES_LAYOUT_PASS");Qt.quit();}
 }catch(error){console.error("FILES_LAYOUT_FAIL "+error);Qt.quit();}}}
}''')
            result=subprocess.run(['quickshell','-p',str(qml)],env=env,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,timeout=30)
            self.assertEqual(result.returncode,0,result.stdout)
            self.assertIn('FILES_LAYOUT_PASS',result.stdout,result.stdout)
            for marker in ['FILES_LAYOUT_FAIL','ReferenceError:','TypeError:','Binding loop','Failed to load configuration','Cannot assign']:
                self.assertNotIn(marker,result.stdout,result.stdout)
if __name__=='__main__':unittest.main()
