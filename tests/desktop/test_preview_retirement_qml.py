"""Real Qt source bindings acknowledge each cooperating preview consumer."""
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile
import unittest

class PreviewRetirement(unittest.TestCase):
    def test_two_images_and_explicit_media_stop_retire_without_early_ack(self):
        root=Path(__file__).resolve().parents[2]
        launcher=os.environ.get('LUMEN_TEST_SHELL_LAUNCHER') or shutil.which('lumen-desktop-shell')
        self.assertIsNotNone(launcher,'Installed Qt runtime is required')
        prefix=re.split(r'\n[ \t]*exec quickshell ',Path(launcher).read_text(),maxsplit=1)[0]
        installed=dict(value.decode().split('=',1) for value in subprocess.check_output(['bash','-c',prefix+'\nexec env -0']).split(b'\0') if b'=' in value)
        with tempfile.TemporaryDirectory(prefix='lumen-preview-retirement-') as temporary:
            directory=Path(temporary);picture=directory/'private image.png'
            from PIL import Image
            Image.new('RGB',(16,16),'red').save(picture)
            qml=directory/'shell.qml'
            qml.write_text('''import QtQuick
import Quickshell
import "file:'''+str(root/'res/lumen-desktop/shell')+'''" as D
ShellRoot {
 id:root
 property int phase:0
 property var dynamicItem:null
 property var retiredIds:[]
 property string uri:"'''+picture.as_uri()+'''"
 function check(value,message){if(!value)throw new Error(message);}
 D.PreviewResourceRetirement {id:leaseRegistry;onRetired:ids=>{
  root.retiredIds=root.retiredIds.concat(ids);
  if(root.phase===1){
   root.check(JSON.stringify(ids)==="[11]","Wrong image lease acknowledgement");root.phase=2;
   media.source=root.uri;media.stopped=false;leaseRegistry.retire([12],root.uri);
   media.source="";leaseRegistry.check();root.check(leaseRegistry.busy,"Media was released before stopped acknowledgement");
   media.stopped=true;Qt.callLater(leaseRegistry.check);
  }else if(root.phase===2){
   root.check(JSON.stringify(ids)==="[12]","Wrong media lease acknowledgement");root.phase=3;
   second.source=root.uri;leaseRegistry.retire([13],root.uri);secondConsumer.manager=null;
   leaseRegistry.check();root.check(leaseRegistry.busy,"Unregistering a still-bound consumer released its resource");
   second.source="";Qt.callLater(leaseRegistry.check);
  }else if(root.phase===3){
   root.check(JSON.stringify(root.retiredIds)==="[11,12,13]","Duplicated or stale retirement acknowledgement");root.phase=4;
   second.source=root.uri;root.dynamicItem=dynamicAdapter.createObject(null);leaseRegistry.retire([14],root.uri);root.dynamicItem.destroy();adapterCheck.restart();
  }else if(root.phase===4){
   root.check(second.source.toString()==="","Adapter-only destruction released a still-bound Image");root.phase=5;
   root.dynamicItem=dynamicConsumer.createObject(null);leaseRegistry.retire([15],root.uri);root.dynamicItem.destroy();
  }else if(root.phase===5){
   root.check(JSON.stringify(root.retiredIds)==="[11,12,13,14,15]","Destroyed source owner left a stale lease or duplicated an acknowledgement");
   root.check(!leaseRegistry.consumers.some(entry=>!entry.owner),"Retired source owner registry leaked");
   console.log("PASS cooperative preview retirement");Qt.quit();
  }
 }}
 Timer {id:adapterCheck;interval:50;onTriggered:{leaseRegistry.check();root.check(leaseRegistry.busy,"Adapter-only destruction released a still-bound Image");second.source="";}}
 Component {id:dynamicAdapter;D.PreviewResourceConsumer {manager:leaseRegistry;sourceObjects:[second]}}
 Component {id:dynamicConsumer;Item {
  Image {id:dynamicImage;source:root.uri;asynchronous:true}
  D.PreviewResourceConsumer {manager:leaseRegistry;sourceObjects:[dynamicImage]}
 }}
 Item {
  Image {id:first;source:root.uri;asynchronous:true}
  Image {id:second;source:root.uri;asynchronous:true}
  D.PreviewResourceConsumer {manager:leaseRegistry;sourceObjects:[first]}
  D.PreviewResourceConsumer {id:secondConsumer;manager:leaseRegistry;sourceObjects:[second]}
  QtObject {id:media;property string source:"";property bool stopped:true;property int playbackState:stopped?0:1}
  D.PreviewResourceConsumer {manager:leaseRegistry;sourceObjects:[media];mediaObjects:[media]}
 }
 Timer {interval:20;repeat:true;running:root.phase===0;onTriggered:{
  if(first.status!==Image.Ready || second.status!==Image.Ready)return;
  root.phase=1;leaseRegistry.retire([11],root.uri);first.source="";leaseRegistry.check();
  root.check(leaseRegistry.busy,"First of two actual Image sources released the shared resource");second.source="";
 }}
 Timer {interval:5000;running:true;onTriggered:{console.log("FAIL preview retirement deadline");Qt.quit();}}
}
''')
            environment=dict(os.environ)
            environment.update({key:installed[key] for key in ('PATH','QT_PLUGIN_PATH','QML_IMPORT_PATH','GLOBAL_MENU_PYTHON') if key in installed})
            environment.update(HOME=temporary,XDG_RUNTIME_DIR=temporary,QT_QPA_PLATFORM='offscreen',QT_QPA_PLATFORMTHEME='',QT_QUICK_BACKEND='software',QT_ACCESSIBILITY='0',QT_LINUX_ACCESSIBILITY_ALWAYS_ON='0',NO_AT_BRIDGE='1',GTK_A11Y='none')
            for key in ('DBUS_SESSION_BUS_ADDRESS','AT_SPI_BUS_ADDRESS','DISPLAY','WAYLAND_DISPLAY','SWAYSOCK'):environment.pop(key,None)
            result=subprocess.run(['dbus-run-session','--','quickshell','-p',str(qml)],env=environment,capture_output=True,text=True,timeout=10)
            output=result.stdout+result.stderr
            self.assertEqual(result.returncode,0,output)
            self.assertIn('PASS cooperative preview retirement',output)
            self.assertNotIn('Error:',output)

if __name__=='__main__':unittest.main()
