import QtQuick
import QtQuick.Controls
import QtQuick.Window
import Quickshell
import Quickshell.Io
import "assets/shell" as D
ShellRoot {
 id:audit
 property string wanted:""
 property var palettes:JSON.parse(Quickshell.env("LUMEN_FIXTURE_PALETTES"))
 QtObject {id:stub;property var groupMembershipDrag:null;function claimPopup(name){}}
 D.WindowOverview {id:overview;shell:stub}
 function walk(item,seen){seen=seen || [];if(!item || seen.includes(item))return [];seen.push(item);let result=[item],children=Array.from(item.children || []).concat(Array.from(item.data || []));if(item.contentItem)children.push(item.contentItem);for(const child of children)result=result.concat(walk(child,seen));return result;}
 function shown(item){if(!(item instanceof Item))return false;const same=item.Window.window===overview.contentItem.Window.window;while(item && item!==overview.contentItem){if(!item.visible)return false;item=item.parent;}return item===overview.contentItem || same;}
 function rect(item){const p=item.mapToItem(overview.contentItem,0,0);return {x:p.x,y:p.y,width:item.width,height:item.height};}
 function find(name){return walk(overview.contentItem).find(child=>child instanceof Item && shown(child) && child.Accessible && (name==="card"?child.Accessible.name.startsWith("A private document title with several words · "):child.Accessible.name===name));}
 function focusOwner(){let item=overview.contentItem.Window.window?overview.contentItem.Window.window.activeFocusItem:null;while(item && !item.Accessible.name)item=item.parent;return item;}
 function state(){const focus=focusOwner(),viewport=walk(overview.contentItem).find(child=>child.objectName==="overview-body-scroll"),text=focus?walk(focus).find(child=>child instanceof Item && shown(child) && child.text && child.font && child.color):null;let clipped=focus?rect(focus):null;if(focus)for(let p=focus.parent;p && p!==overview.contentItem;p=p.parent)if(p.clip){const r=rect(p),x=Math.max(clipped.x,r.x),y=Math.max(clipped.y,r.y);clipped={x:x,y:y,width:Math.min(clipped.x+clipped.width,r.x+r.width)-x,height:Math.min(clipped.y+clipped.height,r.y+r.height)-y};}return {visible:overview.visible,active:overview.contentItem.Window.window.active,width:overview.width,height:overview.height,query:(walk(overview.contentItem).find(child=>child.placeholderText==="Find a window") || {}).text || "",count:overview.matches.length,currentId:overview.current?overview.current.id:0,currentPid:overview.current?overview.current.pid:0,focus:focus?focus.Accessible.name:"",rect:focus?rect(focus):null,clipped:clipped,text:text?{text:text.text,color:text.color.toString(),rect:rect(text),font:text.font.pixelSize,resolvedFamily:text.fontInfo?text.fontInfo.family:null}:null,groupOpen:!!find("Close window groups"),viewport:{rect:rect(viewport),contentY:viewport.contentY,contentHeight:viewport.contentHeight}};}
 IpcHandler {target:"overviewText"
  function ready():bool {return true;}
  function open(mode:string,small:bool):void {D.Theme.colors=audit.palettes[mode];D.Theme.preferences={textScale:1.5,reduceMotion:true};overview.visible=false;overview.show("");overview.implicitWidth=small?560:880;overview.implicitHeight=small?480:560;overview.contentItem.Window.window.width=overview.implicitWidth;overview.contentItem.Window.window.height=overview.implicitHeight;}
  function focus(name:string):void {audit.wanted=name;const item=audit.find(name);if(!item)throw Error("Missing real control "+name);item.forceActiveFocus();}
  function status():string {return JSON.stringify(audit.state());}
 }
}
