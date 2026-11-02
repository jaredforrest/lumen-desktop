import QtQuick
import QtQuick.Controls
import Quickshell
import Quickshell.Io
import Quickshell.Wayland
import Quickshell.Services.Mpris
import "assets/shell" as D
ShellRoot {
 id:audit
 property var deferredReplay:null
 PanelWindow {id:window;anchors {top:true;bottom:true;left:true;right:true}color:"#256048";WlrLayershell.keyboardFocus:WlrKeyboardFocus.Exclusive
  D.MediaControl {id:media;x:30;y:30;width:320;height:implicitHeight;expanded:true}
 }
 function find(name,item){if(item.objectName===name)return item;for(const child of item.children || []){const result=find(name,child);if(result)return result;}return null;}
 IpcHandler {target:"mediaAudit"
  function state():string {return JSON.stringify({deferredReplay:audit.deferredReplay,players:media.players.map(p=>({name:p.dbusName,title:p.trackTitle,identity:p.identity})),selected:media.player?media.player.dbusName:"",title:find("media-title",media).text,artist:find("media-artist",media).text,playing:media.player?media.player.isPlaying:false,volumeSupported:media.player?media.player.volumeSupported:false,volume:media.player?media.player.volume:0,seek:find("media-position",media).enabled,volumeEnabled:find("media-volume",media).enabled,nextEnabled:find("media-next",media).enabled,playEnabled:find("media-play",media).enabled,playPressed:find("media-play",media).pressed,pending:find("media-play",media).pendingIntent?{operation:find("media-play",media).pendingIntent.operation,live:media.live(find("media-play",media).pendingIntent.target)}:null,height:media.height,sourceOpen:find("media-source",media).popup.visible,choices:find("media-source",media).choices.map(row=>row.label)});}
  // Controlled same-turn model replay after actual popup close/reopen. These are
  // the existing real MPRIS objects; no synthetic player or action is introduced.
  function reopenReplacementReplay():void {
   const selector=find("media-source",media);
   let closedObserved=0;const onClosed=()=>{closedObserved++;};
   selector.popup.closed.connect(onClosed);selector.popup.close();selector.popup.closed.disconnect(onClosed);selector.popup.open();
   const captured=selector.choices.slice();
   media.players=media.players.slice().reverse();
   Qt.callLater(()=>{
    audit.deferredReplay={closedObserved:closedObserved,opened:selector.popup.visible,liveSourceCount:media.liveSourceChoices.length,liveLabels:media.liveSourceChoices.map(row=>row.label),before:captured.map(row=>row.label),after:selector.choices.map(row=>row.label),sameObjects:selector.choices.length===captured.length && selector.choices.every((row,i)=>row.player===captured[i].player && row.label===captured[i].label)};
    media.players=Qt.binding(()=>Mpris.players.values.filter(p=>p.dbusName!=="org.mpris.MediaPlayer2.playerctld"));
   });
  }
  function select(name:string):void {media.selectedPlayer=name;}
  function focus(name:string):void {find(name,media).forceActiveFocus();}
  function geometry(name:string):string {const item=find(name,media);const at=item.mapToItem(window.contentItem,0,0);return JSON.stringify({x:at.x,y:at.y,width:item.width,height:item.height});}
 }
}