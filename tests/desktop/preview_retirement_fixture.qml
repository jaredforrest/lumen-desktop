import QtQuick
import Quickshell
import "LUMEN_ASSETS" as D
ShellRoot {
 id:root
 property int phase:0
 property var dynamicItem:null
 property var retiredIds:[]
 property string uri:"LUMEN_PREVIEW_URI"
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
