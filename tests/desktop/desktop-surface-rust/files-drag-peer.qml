import QtQuick
import Quickshell
import Quickshell.Io
ShellRoot {
 id:root
 property var received:[]
 property string rawReceived:""
 property var consumed:[]
 property string consumerError:""
 property int consumeSerial:0
 property string offerMime:"text/uri-list"
 property bool nativeActive:false
 property int receiveAction:0
 property int finished:0
 property int action:0
 property int offerActions:Qt.CopyAction
 property string offerUri:""
 FloatingWindow {visible:true;title:"FilesDropPeer";implicitWidth:650;implicitHeight:900;color:"#dde4ee"
  DropArea {anchors.fill:parent;onEntered:drag=>{drag.accepted=drag.hasUrls && (drag.supportedActions & Qt.CopyAction)!==0;};onDropped:drop=>{if(drop.hasUrls && (drop.supportedActions & Qt.CopyAction)!==0){root.rawReceived=drop.getDataAsString("text/uri-list");root.received=drop.urls.map(url=>url.toString());root.consumeSerial++;root.consumed=[];for(const uri of root.rawReceived.split(/\r?\n/).filter(value=>value.length>0)){if(!uri.startsWith("file:///")){root.consumerError="Nonlocal URI";continue;}reader.createObject(root,{uri:uri,generation:root.consumeSerial});}drop.accept(Qt.CopyAction);root.receiveAction=drop.action;}}}
  Rectangle {id:offer;x:60;y:60;width:220;height:100;color:"#4080cc"
   Text {anchors.centerIn:parent;text:"Disposable file offer";color:"white"}
   Drag.active:mouse.drag.active;Drag.dragType:Drag.Automatic;Drag.supportedActions:root.offerActions;Drag.proposedAction:root.offerActions
   Drag.mimeData:{const value={};value[root.offerMime]=root.offerUri+(root.offerMime==="text/uri-list"?"\r\n":"");return value;}
   Drag.onDragStarted:root.nativeActive=true
   Drag.onDragFinished:dropAction=>{root.nativeActive=false;root.finished++;root.action=dropAction;offer.x=60;offer.y=60;}
   MouseArea {id:mouse;anchors.fill:parent;drag.target:offer;drag.threshold:8}
  }
 }
 Component {id:reader;FileView {id:input;property string uri:"";property int generation:0;path:uri.startsWith("file:///")?decodeURIComponent(uri.slice(7)):"";printErrors:false
  onLoaded:{if(generation===root.consumeSerial)root.consumed=root.consumed.concat([{uri:uri,text:text()}]);Qt.callLater(()=>input.destroy());}
  onLoadFailed:{if(generation===root.consumeSerial)root.consumerError="Offered file could not be read";Qt.callLater(()=>input.destroy());}
 }}
 IpcHandler {target:"filesPeerTest"
  function status():string{return JSON.stringify({received:root.received,rawReceived:root.rawReceived,consumed:root.consumed,consumerError:root.consumerError,receiveAction:root.receiveAction,finished:root.finished,action:root.action,offerActions:root.offerActions,dragActive:root.nativeActive});}
  function offer(uri:string,actions:int,mime:string):void{root.offerUri=uri;root.offerActions=actions;root.offerMime=mime;}
  function clear():void{root.consumeSerial++;root.received=[];root.rawReceived="";root.consumed=[];root.consumerError="";root.receiveAction=0;}
 }
}
