import QtQuick
import QtQuick.Controls
import QtQuick.Window
import Quickshell
import Quickshell.Io
import "assets/shell" as D
ShellRoot {
 id:root
 D.SearchState {id:search}
 QtObject {id:notifications;property bool dnd:false;property bool available:false}
 QtObject {id:audio;property bool muted:false;property bool available:false}
 QtObject {id:exported;property var state:({menu:null})}
 QtObject {id:stub;property var search:search;property var notifications:notifications;property var audioState:audio;property var exportedMenu:exported;property var entries:[];property date currentDate:new Date();function appLabel(e){return e.name;}}
 FloatingWindow {id:stage;visible:true;implicitWidth:1000;implicitHeight:850;title:"Clipboard Launcher Fixture";color:"#202024"
  D.AppLauncher {id:launcher;shell:stub;x:80;y:50;onCloseRequested:stage.visible=false}
 }
 FloatingWindow {id:document;visible:false;implicitWidth:800;implicitHeight:600;title:"Clipboard Document Observer";color:"white"
  TextArea {id:editor;anchors.fill:parent;text:"";selectByMouse:true}
 }
 function find(item,predicate){if(predicate(item))return item;for(const child of item.children || []){const match=find(child,predicate);if(match)return match;}return null;}
 function clipboard(){return find(launcher,item=>item.objectName==="clipboard-history-state");}
 IpcHandler {target:"fixture"
  function status():string {const c=root.clipboard();return JSON.stringify({provider:launcher.provider,enabled:c.enabled,collecting:c.collecting,busy:c.busy,refreshPending:c.refreshPending,available:c.available,error:c.error,message:c.message,entries:c.entries,results:launcher.results,visible:stage.visible,document:editor.text});}
  function open():void {stage.visible=true;launcher.visible=true;launcher.setProvider("Clipboard");launcher.focusSearch();}
  function query(text:string):void {const c=root.clipboard();c.search(text);}
  function refresh():void {root.clipboard().request("list");}
  function button(text:string):string {const b=root.find(launcher,item=>item.text===text && item.visible);if(!b)return JSON.stringify({visible:false});const point=b.mapToItem(stage.contentItem,b.width/2,b.height/2);return JSON.stringify({x:point.x,y:point.y,visible:b.visible,enabled:b.enabled});}
  function select(id:string):void {const index=launcher.results.findIndex(row=>row.id===id);if(index>=0){launcher.selectedIndex=index;launcher.focusSearch();}}
  function capturedCopy(key:string):void {launcher.launchKey(key,false);}
  function documentFocus():void {stage.visible=false;document.visible=true;editor.forceActiveFocus();}
 }
 Component.onCompleted:Qt.callLater(()=>launcher.setProvider("Clipboard"))
}
