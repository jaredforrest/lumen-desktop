import QtQuick
import QtQuick.Controls
import QtQuick.Window
import Quickshell
import Quickshell.Io
import "assets/shell" as D
ShellRoot {
 id:root
 D.SearchState {id:search;active:stage.visible && launcher.visible}
 QtObject {id:notifications;property bool dnd:false;property bool available:false}
 QtObject {id:audio;property bool muted:false;property bool available:false}
 QtObject {id:exported;property var state:({menu:null})}
 QtObject {id:stub;property var search:search;property var notifications:notifications;property var audioState:audio;property var exportedMenu:exported;property var entries:[];property date currentDate:new Date();function appLabel(e){return e.name;}function openFiles(path){root.browsed=path;}}
 property string browsed:""
 FloatingWindow {id:stage;visible:true;implicitWidth:1100;implicitHeight:850;title:"Search Lifecycle Fixture";color:"#202024"
  D.AppLauncher {id:launcher;visible:stage.visible;shell:stub;x:70;y:50;onCloseRequested:stage.visible=false}
 }
 function find(item,predicate){if(predicate(item))return item;for(const child of item.children || []){const match=find(child,predicate);if(match)return match;}return null;}
 IpcHandler {target:"fixture"
  function status():string{return JSON.stringify({provider:launcher.provider,results:launcher.results,selected:launcher.selectedIndex,key:launcher.selectedKey,query:launcher.queryText,available:search.available,searching:search.searching,opening:search.opening,error:search.error,preview:search.preview,previewId:search.previewId,searchId:search.searchId,previewOpen:launcher.previewOpen,visible:stage.visible,browsed:root.browsed});}
  function image():string{const b=root.find(launcher,item=>item.source!==undefined && item.fillMode!==undefined && String(item.source).indexOf("lumen-preview-snapshots-")>=0);if(!b)return "{}";const point=b.mapToItem(stage.contentItem,b.width/2,b.height/2);return JSON.stringify({x:point.x,y:point.y,status:b.status,source:String(b.source)});}
  function reopen():void{stage.visible=true;launcher.focusSearch();}
  function button(text:string):string{const b=root.find(launcher,item=>item.text===text && item.visible);if(!b)return "{}";const point=b.mapToItem(stage.contentItem,b.width/2,b.height/2);return JSON.stringify({x:point.x,y:point.y,enabled:b.enabled});}
 }
}
