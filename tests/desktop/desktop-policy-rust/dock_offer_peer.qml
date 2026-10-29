import QtQuick
import Quickshell
import Quickshell.Io
ShellRoot {id:root
 property var uris:[]
 property int finished:0
 property int action:0
 function uriBytes(){const delimiter=String.fromCharCode(13,10),text=uris.join(delimiter)+delimiter,bytes=new Uint8Array(text.length);for(let i=0;i<text.length;i++){const value=text.charCodeAt(i);if(value>127)throw Error("URI encoding required");bytes[i]=value;}return bytes.buffer;}
 FloatingWindow {id:window;visible:true;title:"Private URI offer peer";implicitWidth:500;implicitHeight:320;color:"#dce4ee"
  Rectangle {id:offer;x:50;y:50;width:200;height:100;color:"#4080cc"
   Text {anchors.centerIn:parent;text:"Private URI offer";color:"white"}
   Drag.active:mouse.drag.active;Drag.dragType:Drag.Automatic;Drag.supportedActions:Qt.CopyAction;Drag.proposedAction:Qt.CopyAction
   Drag.mimeData:({"text/uri-list":root.uriBytes()})
   Drag.onDragFinished:dropAction=>{root.finished++;root.action=dropAction;offer.x=50;offer.y=50;}
   MouseArea {id:mouse;anchors.fill:parent;hoverEnabled:true;drag.target:offer;drag.threshold:8}
  }
 }
 IpcHandler {target:"offerPeer"
  function configureUris(first:string,second:string):bool{root.uris=[first,second];return root.uris.length===2 && first.length>0 && second.length>0;}
  function status():string{const at=offer.mapToItem(window.contentItem,offer.width/2,offer.height/2);return JSON.stringify({uris:root.uris,mimeBytes:offer.Drag.mimeData["text/uri-list"].byteLength,finished:root.finished,action:root.action,active:mouse.drag.active,pressed:mouse.pressed,hovered:mouse.containsMouse,x:at.x,y:at.y,width:window.width,height:window.height});}
 }
}
