import QtQuick
import QtQuick.Controls
import QtQuick.Window
import Quickshell
import Quickshell.Io
import "assets/shell" as D
ShellRoot {
 id:audit
 QtObject {id:stub;function claimPopup(name){}property var exportedMenu:({state:{windowId:0,appName:"Private application",menu:null}})}
 D.TextToolsWindow {id:tools;shell:stub;page:"Shortcuts"}
 D.RemoteDesktopWindow {id:remote;shell:stub;connections:[{id:"private",name:"Private computer",protocol:"RDP",host:"192.0.2.1"}]}
 D.FileInfoWindow {id:info;details:({name:"Private document.txt",size:"12 bytes",kind:"Text",location:"/private/directory",permissions:"Read and write",owner:"Synthetic owner"})}

 property var owner:null
 function walk(item,seen){if(!item || seen.includes(item))return [];seen.push(item);let result=[item];for(const child of item.children || [])result=result.concat(walk(child,seen));if(item.contentItem)result=result.concat(walk(item.contentItem,seen));return result;}
 function shown(item){while(item && item!==owner.contentItem){if(!item.visible)return false;item=item.parent;}return item===owner.contentItem;}
 IpcHandler {target:"utilityTypography"
  function open(kind:string):void {tools.visible=false;remote.visible=false;info.visible=false;audit.owner=kind==="TextTools"?tools:kind==="Remote"?remote:info;owner.visible=true;owner.implicitWidth=kind==="TextTools"?520:kind==="Remote"?480:360;owner.implicitHeight=320;Qt.callLater(()=>{owner.contentItem.Window.window.requestActivate();const name=kind==="TextTools"?"Menu command path":kind==="Remote"?"Remote computer address":"General";const item=audit.walk(owner.contentItem,[]).find(item=>item.Accessible.name===name);item.forceActiveFocus();});}
  function state():string {if(!owner)return "{}";const window=owner.contentItem.Window.window,focus=window.activeFocusItem,items=audit.walk(owner.contentItem,[]),item=items.find(item=>item.Accessible.name===(owner===tools?"Menu command path":owner===remote?"Remote computer address":"General"));let p=focus?focus.mapToItem(owner.contentItem,0,0):Qt.point(-1,-1);return JSON.stringify({visible:owner.visible,active:window.active,focus:focus?focus.Accessible.name:"",focusRect:focus?{x:p.x,y:p.y,width:focus.width,height:focus.height}:null,width:owner.width,height:owner.height,value:item.text || "",expanded:item.Accessible.description || "",sections:items.filter(item=>["General","Sharing & Permissions"].includes(item.Accessible.name)).map(item=>({name:item.Accessible.name,state:item.Accessible.description})),scale:D.Theme.preferences.textScale});}
 }
}
