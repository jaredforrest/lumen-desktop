import QtQuick
import QtQuick.Controls
import Quickshell
import Quickshell.Io
import Quickshell.Wayland
import "assets/shell" as D
ShellRoot {
 PanelWindow {id:window;anchors {top:true;bottom:true;left:true;right:true}color:"#256048";WlrLayershell.keyboardFocus:WlrKeyboardFocus.Exclusive
  D.MediaControl {id:media;x:30;y:30;width:320;height:implicitHeight;expanded:true}
 }
 function find(name,item){if(item.objectName===name)return item;for(const child of item.children || []){const result=find(name,child);if(result)return result;}return null;}
 IpcHandler {target:"mediaAudit"
  function state():string {return JSON.stringify({players:media.players.map(p=>({name:p.dbusName,title:p.trackTitle,identity:p.identity})),selected:media.player?media.player.dbusName:"",title:find("media-title",media).text,artist:find("media-artist",media).text,playing:media.player?media.player.isPlaying:false,volumeSupported:media.player?media.player.volumeSupported:false,volume:media.player?media.player.volume:0,seek:find("media-position",media).enabled,volumeEnabled:find("media-volume",media).enabled,nextEnabled:find("media-next",media).enabled,playEnabled:find("media-play",media).enabled,playPressed:find("media-play",media).pressed,pending:find("media-play",media).pendingIntent?{operation:find("media-play",media).pendingIntent.operation,live:media.live(find("media-play",media).pendingIntent.target)}:null,height:media.height,sourceOpen:find("media-source",media).popup.visible,choices:find("media-source",media).choices.map(row=>row.label)});}
  function select(name:string):void {media.selectedPlayer=name;}
  function focus(name:string):void {find(name,media).forceActiveFocus();}
  function geometry(name:string):string {const item=find(name,media);const at=item.mapToItem(window.contentItem,0,0);return JSON.stringify({x:at.x,y:at.y,width:item.width,height:item.height});}
 }
}