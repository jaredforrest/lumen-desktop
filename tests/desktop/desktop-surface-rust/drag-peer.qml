import QtQuick
import Quickshell
import Quickshell.Io
ShellRoot {
 id:root
 property var received:[]
 property int finished:0
 property int action:0
 FloatingWindow {visible:true;title:"NativeFilePeer";implicitWidth:600;implicitHeight:500;color:"#dde4ee"
  DropArea {anchors.fill:parent;onEntered:drag=>{drag.accepted=drag.hasUrls;};onDropped:drop=>{if(drop.hasUrls){root.received=drop.urls.map(url=>url.toString());drop.accept(Qt.CopyAction);}}}
  Rectangle {id:offer;x:50;y:50;width:200;height:100;color:"#4080cc"
   Text {anchors.centerIn:parent;text:"Drag two incoming files";color:"white"}
   Drag.active:mouse.drag.active;Drag.dragType:Drag.Automatic;Drag.supportedActions:Qt.CopyAction;Drag.proposedAction:Qt.CopyAction
   Drag.mimeData:({"text/uri-list":"@COLLISION_URI@\r\n@FRESH_URI@\r\n"})
   Drag.onDragFinished:dropAction=>{root.finished++;root.action=dropAction;offer.x=50;offer.y=50;}
   MouseArea {id:mouse;anchors.fill:parent;drag.target:offer;drag.threshold:8}
  }
 }
 IpcHandler {target:"dragTest";function status():string{return JSON.stringify({received:root.received,finished:root.finished,action:root.action});}function clear():void{root.received=[];}}
}
