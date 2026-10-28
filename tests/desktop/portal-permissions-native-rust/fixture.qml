import QtQuick
import Quickshell
import Quickshell.Io
import "assets/shell" as D
ShellRoot {
 id:fixture
 D.PortalPermissionState {id:service;active:true}
 FloatingWindow {id:window;title:"Permission fixture";visible:true;implicitWidth:420;implicitHeight:820;color:D.Theme.colors.background
  Flickable {id:scroll;anchors.fill:parent;contentWidth:width;contentHeight:pane.implicitHeight+32;clip:true
   D.PortalPermissionsPane {id:pane;x:16;y:16;width:parent.width-32;service:service}
  }
 }
 function find(name,item){if(item.objectName===name)return item;for(const child of item.children || []){const result=find(name,child);if(result)return result;}return null;}
 IpcHandler {target:"filesAudit"
  function state():string {return JSON.stringify({ready:service.facts.available && !service.busy,rows:service.facts.rows,pending:pane.pending,error:service.error,message:service.message,width:window.width,dark:D.Theme.dark});}
  function geometry(name:string):string {const item=fixture.find(name,window.contentItem);if(!item)return "null";const content=item.mapToItem(pane,0,0);scroll.contentY=Math.max(0,Math.min(scroll.contentHeight-scroll.height,content.y-100));const at=item.mapToItem(window.contentItem,0,0);return JSON.stringify({x:at.x,y:at.y,width:item.width,height:item.height});}
 }
}