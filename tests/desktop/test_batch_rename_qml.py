"""Offscreen production dialog protocol: captured previews, stale replies and consent."""
import json,os,re,shutil,subprocess,tempfile,unittest
from pathlib import Path
ROOT=Path(__file__).resolve().parents[2]

class BatchRenameQt(unittest.TestCase):
 def test_captured_dialog_protocol_and_late_reply_retirement(self):
  launcher=Path(os.environ.get('LUMEN_TEST_SHELL_LAUNCHER') or shutil.which('lumen-desktop-shell'))
  prefix=re.split(r'\n[ \t]*exec quickshell ',launcher.read_text(),maxsplit=1)[0]
  with tempfile.TemporaryDirectory(prefix='lumen-batch-rename-qt-') as temporary:
   root=Path(temporary);root.chmod(0o700);env=dict(os.environ,HOME=temporary,XDG_RUNTIME_DIR=temporary,DBUS_SYSTEM_BUS_ADDRESS='unix:path='+temporary+'/no-system-bus')
   for key in ('CONFIG','STATE','CACHE','DATA'):env['XDG_'+key+'_HOME']=str(root/key.lower())
   for key in ('DISPLAY','WAYLAND_DISPLAY','SWAYSOCK','DBUS_SESSION_BUS_ADDRESS','AT_SPI_BUS_ADDRESS'):env.pop(key,None)
   env.update(dict(value.decode().split('=',1) for value in subprocess.check_output(['bash','-c',prefix+'\nexec env -0'],env=env).split(b'\0') if b'=' in value))
   env.update(QT_QPA_PLATFORM='offscreen',QT_QPA_PLATFORMTHEME='generic',QT_QUICK_BACKEND='software',QT_ACCESSIBILITY='0',QT_LINUX_ACCESSIBILITY_ALWAYS_ON='0',NO_AT_BRIDGE='1',GTK_A11Y='none')
   appearance=root/'appearance.json';palettes=json.loads((ROOT/'res/lumen-desktop/appearance/palette.json').read_text());appearance.write_text(json.dumps(dict(font=palettes['font'],palette=palettes['dark'],preferences={})));env['LUMEN_APPEARANCE_STATE']=str(appearance)
   shutil.copytree(ROOT/'res/lumen-desktop',root/'assets');qml=root/'shell.qml'
   qml.write_text('''import QtQuick
import Quickshell
import "assets/shell" as D
ShellRoot {
 id:fixture
 property int phase:0
 property int ticks:0
 property var selected:[{path:"/synthetic/one",identity:{device:"1",inode:"1152921504606846977"}},{path:"/synthetic/two",identity:{device:"1",inode:"1152921504606846978"}}]
 QtObject {id:service
  property bool ready:true
  property int sequence:0
  property var requests:[]
  signal batchRenameReady(var reply)
  function send(action,data){const id=++sequence;requests=requests.concat([{id:id,action:action,data:data}]);return id;}
 }
 FloatingWindow {id:window;visible:true;implicitWidth:800;implicitHeight:700
  D.BatchRenameDialog {id:dialog;service:service;ownerWidth:window.width;ownerHeight:window.height}
 }
 function check(value,message){if(!value)throw Error(message+" phase="+phase);}
 function find(name,item){if(item.objectName===name)return item;for(const child of item.children || []){const found=find(name,child);if(found)return found;}return null;}
 function previewReply(id,token){service.batchRenameReady({id:id,action:"rename-preview",token:token,rows:[{before:"one",after:"new-one",status:"ready",error:""},{before:"two",after:"new-two",status:"ready",error:""}],executable:true,count:2});}
 Timer {interval:50;running:true;repeat:true;onTriggered:{try {
  if(++ticks>100)throw Error("Dialog protocol timeout");
  if(phase===0){dialog.begin(selected);dialog.preview();previewReply(service.requests[service.requests.length-1].id,"entering-plan");check(dialog.token==="entering-plan" && dialog.executable,"Legitimate reply during popup entry discarded");dialog.changed();phase=1;return;}
  if(phase===1){check(dialog.opened,"Dialog did not open");find("batch-rename-first",dialog.contentItem).text="new-";dialog.preview();const request=service.requests[service.requests.length-1];check(request.action==="rename-preview","Missing preview");check(request.data.items[0].identity.inode==="1152921504606846977","Identity precision lost");selected[0].identity.inode="99";selected[0].path="/synthetic/bystander";check(dialog.captured[0].identity.inode==="1152921504606846977" && dialog.captured[0].path==="/synthetic/one","Selection was not captured");previewReply(request.id,"captured-plan");phase=2;return;}
  if(phase===2){check(dialog.executable,"Preview cannot be confirmed");dialog.confirm();dialog.confirm();const confirms=service.requests.filter(request=>request.action==="rename-confirm");check(confirms.length===1 && confirms[0].data.token==="captured-plan" && confirms[0].data.confirmed===true,"Confirmation duplicated or retargeted");service.batchRenameReady({id:confirms[0].id,action:"rename-confirm",ok:false,rows:[{before:"one",after:"new-one",status:"renamed",error:""},{before:"two",after:"new-two",status:"failed",error:"Permission denied"}]});phase=3;return;}
  if(phase===3){check(dialog.completed && !dialog.executing && dialog.rows[1].status==="failed","Partial result hidden");dialog.close();dialog.begin(selected);find("batch-rename-first",dialog.contentItem).text="new-";dialog.preview();const request=service.requests[service.requests.length-1];dialog.changed();previewReply(request.id,"stale-plan");check(!dialog.executable && dialog.token==="","Changed form accepted old preview");check(service.requests.some(request=>request.action==="rename-cancel" && request.data.token==="stale-plan"),"Late preview token leaked");phase=4;return;}
  if(phase===4){dialog.preview();const request=service.requests[service.requests.length-1];dialog.close();previewReply(request.id,"closed-plan");check(service.requests.some(request=>request.action==="rename-cancel" && request.data.token==="closed-plan"),"Closed preview token leaked");service.ready=false;check(dialog.token==="" && !dialog.executable,"Disconnect retained authority");console.log("BATCH_RENAME_QT_PASS");Qt.quit();}
 }catch(error){console.error("BATCH_RENAME_QT_FAIL "+error);Qt.quit();}}}
}''')
   result=subprocess.run(['quickshell','-p',str(qml)],env=env,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,timeout=20)
   self.assertEqual(result.returncode,0,result.stdout);self.assertIn('BATCH_RENAME_QT_PASS',result.stdout,result.stdout)
   for marker in ('BATCH_RENAME_QT_FAIL','ReferenceError:','TypeError:','Binding loop','Failed to load configuration','Cannot assign','ResourceWarning','Exception ignored','Accessible attached property must'):
    self.assertNotIn(marker,result.stdout,result.stdout)

if __name__=='__main__':unittest.main()
