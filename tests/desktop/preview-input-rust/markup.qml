import QtQuick
import QtQuick.Window
import Quickshell
import Quickshell.Io
import "assets/shell" as D
ShellRoot {
 id:root
 D.QuickLookWindow {id:quick}
 function find(item,name,seen){if(!item || seen.includes(item))return null;seen.push(item);if((item.text===name || item.objectName===name) && item.visible && item.enabled)return item;for(const list of [item.children,item.data,item.contentData])if(list && typeof list.length==="number")for(let i=0;i<list.length;i++){const found=find(list[i],name,seen);if(found)return found;}if(item.contentItem && item.contentItem!==item)return find(item.contentItem,name,seen);return null;}
 IpcHandler {target:"fixture"
  function startPreview(path:string):void{quick.show(path,null);}
  function imageGeometry():string{const image=quick.fixtureMarkup.fixtureImage,window=quick.contentItem.Window.window;const point=image.mapToItem(window.contentItem,0,0);return JSON.stringify({x:point.x,y:point.y,width:image.width,height:image.height,paintedWidth:image.paintedWidth,paintedHeight:image.paintedHeight,ready:image.status===Image.Ready,autoTransform:image.autoTransform});}
  function status():string{const mark=quick.fixtureMarkup;return JSON.stringify({result:quick.result,message:quick.actionMessage,markup:mark.visible,strokes:mark.strokes,current:mark.current,saving:mark.saving,drawing:mark.drawing,draft:mark.draft,error:mark.message,uri:mark.sourceUri});}
  function button(name:string):string{let item=root.find(quick.contentItem,name,[]);if(!item && quick.fixtureMarkup.visible)item=root.find(quick.fixtureMarkup.contentItem,name,[]);if(!item && quick.fixtureMenu.visible)for(let i=0;i<quick.fixtureMenu.count;i++){const candidate=quick.fixtureMenu.itemAt(i);if(candidate && candidate.text===name && candidate.enabled)item=candidate;}if(!item)return "{}";const window=quick.contentItem.Window.window;const point=item.mapToItem(window.contentItem,item.width/2,item.height/2);return JSON.stringify({x:point.x,y:point.y,width:item.width,height:item.height,title:window.title,windowWidth:window.width,windowHeight:window.height,active:window.active,hovered:item.hovered===undefined?item.containsMouse:item.hovered,down:item.down===undefined?item.pressed:item.down});}
 }
}
