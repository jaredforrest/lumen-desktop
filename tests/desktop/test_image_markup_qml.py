"""Actual Qt markup dialog captures editing intent and retires its image source."""
import json,os,re,shutil,subprocess,tempfile,unittest
from pathlib import Path
from PIL import Image
ROOT=Path(__file__).resolve().parents[2]
class ImageMarkupQt(unittest.TestCase):
 def test_captured_save_cancel_late_reply_and_selection_change(self):
  launcher=Path(os.environ.get('LUMEN_TEST_SHELL_LAUNCHER') or shutil.which('lumen-desktop-shell'));prefix=re.split(r'\n[ \t]*exec quickshell ',launcher.read_text(),maxsplit=1)[0]
  with tempfile.TemporaryDirectory(prefix='lumen-markup-qt-') as temporary:
   root=Path(temporary);root.chmod(0o700);env=dict(os.environ,HOME=temporary,XDG_RUNTIME_DIR=temporary,DBUS_SYSTEM_BUS_ADDRESS='unix:path='+temporary+'/no-system-bus')
   for key in ('CONFIG','STATE','CACHE','DATA'):env['XDG_'+key+'_HOME']=str(root/key.lower())
   for key in ('DISPLAY','WAYLAND_DISPLAY','SWAYSOCK','DBUS_SESSION_BUS_ADDRESS','AT_SPI_BUS_ADDRESS'):env.pop(key,None)
   env.update(dict(value.decode().split('=',1) for value in subprocess.check_output(['bash','-c',prefix+'\nexec env -0'],env=env).split(b'\0') if b'=' in value));env.update(QT_QPA_PLATFORM='offscreen',QT_QPA_PLATFORMTHEME='generic',QT_QUICK_BACKEND='software',QT_ACCESSIBILITY='0',NO_AT_BRIDGE='1',GTK_A11Y='none')
   shutil.copytree(ROOT/'res/lumen-desktop/shell',root/'assets');image=root/'original.png';Image.new('RGB',(80,60),'white').save(image);palette=json.loads((ROOT/'res/lumen-desktop/appearance/palette.json').read_text());appearance=root/'appearance.json';appearance.write_text(json.dumps(dict(font=palette['font'],palette=palette['dark'],preferences={})));env['LUMEN_APPEARANCE_STATE']=str(appearance)
   qml=root/'shell.qml';qml.write_text('''import QtQuick
import Quickshell
import "assets" as D
ShellRoot {
 property int phase:0
 property int ticks:0
 property var staleIntent:null
 property var original:({capability:"first",session:"epoch"})
 property var imageResult:({type:"image",mime:"image/png",uri:'''+json.dumps(image.as_uri())+''',path:'''+json.dumps(str(image))+'''})
 QtObject {id:service;property var actionSelection:original;property bool actionBusy:false;property string actionOperation:"";property var resourceRetirement:null;property var requests:[];signal actionReceived(var value)
  function invoke(captured,operation,parameters){if(operation==="image-markup")requests=requests.concat([{captured:captured,operation:operation,parameters:parameters}]);actionBusy=true;actionOperation=operation;return true;}
 }
 FloatingWindow {id:window;visible:true;implicitWidth:680;implicitHeight:520
  D.ImageMarkupDialog {id:dialog;service:service;ownerWidth:window.width;ownerHeight:window.height}
 }
 function metadata(){service.actionBusy=false;service.actionReceived({operation:"image-markup-info",capability:"first",supported:true,width:80,height:60});}
 function check(value,message){if(!value)throw Error(message);}
 Timer {interval:30;running:true;repeat:true;onTriggered:{try{
  if(++ticks>100)throw Error("Markup protocol timeout");
  if(phase===0){check(dialog.begin(imageResult,original),"Image rejected");metadata();phase=1;return;}
  if(phase===1){if(!dialog.opened || !dialog.imageReady)return;dialog.addRectangle();staleIntent=dialog.captureSave();check(dialog.strokes.length===1,"No keyboard-equivalent rectangle");dialog.cancel();check(service.requests.length===0,"Cancel wrote a file");phase=2;return;}
  if(phase===2){if(dialog.visible)return;check(dialog.sourceUri==="" && dialog.captured===null,"Closed image lease retained");dialog.begin(imageResult,original);metadata();phase=3;return;}
  if(phase===3){if(!dialog.opened || !dialog.imageReady)return;dialog.addRectangle();check(!dialog.saveTo("file:///synthetic/stale.png",staleIntent),"Old editor Save intent retargeted reopened editor");const intent=dialog.captureSave();dialog.saveTo("file:///synthetic/new.png",intent);dialog.saveTo("file:///synthetic/duplicate.png",intent);check(service.requests.length===1,"Duplicate save dispatch");const request=service.requests[0];check(request.operation==="image-markup" && request.captured.capability==="first" && request.parameters.strokes[0].tool==="rectangle","Save retargeted");service.actionBusy=false;service.actionReceived({operation:"image-markup",capability:"other",saved:true});check(dialog.visible,"Unrelated acknowledgement closed editor");service.actionReceived({operation:"image-markup",capability:"first",error:"Destination exists"});check(dialog.visible && dialog.message==="Destination exists" && dialog.strokes.length===1,"Failed Save lost edits");service.actionSelection={capability:"replacement",session:"epoch"};phase=4;return;}
  if(phase===4){if(dialog.visible)return;check(dialog.sourceUri==="","Selection replacement retained old image");check(service.requests.length===1,"Selection change dispatched");check(!dialog.begin({type:"image",mime:"application/pdf"},original),"PDF editor falsely advertised");console.log("IMAGE_MARKUP_QT_PASS");Qt.quit();}
 }catch(error){console.error("IMAGE_MARKUP_QT_FAIL "+error);Qt.quit();}}}
}''')
   result=subprocess.run(['quickshell','-p',str(qml)],env=env,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,timeout=15)
   self.assertEqual(result.returncode,0,result.stdout);self.assertIn('IMAGE_MARKUP_QT_PASS',result.stdout,result.stdout)
   for marker in ('IMAGE_MARKUP_QT_FAIL','TypeError:','ReferenceError:','Binding loop','Cannot assign','Failed to load configuration','ResourceWarning','Exception ignored','Could not attach Keys'):
    self.assertNotIn(marker,result.stdout,result.stdout)
if __name__=='__main__':unittest.main()
