import QtQuick
import QtQuick.Controls
import QtQuick.Window
import Quickshell
import Quickshell.Io
import Quickshell.Wayland
import "assets/shell" as D
ShellRoot {
 id:audit
 property int backgroundClicks:0
 property var quickLooks:[]
 property var palettes:JSON.parse(Quickshell.env("LUMEN_FIXTURE_PALETTES"))
 PanelWindow {anchors {left:true;right:true;top:true;bottom:true}exclusiveZone:0;WlrLayershell.layer:WlrLayer.Bottom;WlrLayershell.keyboardFocus:WlrKeyboardFocus.None;color:"#30343a";MouseArea {anchors.fill:parent;onClicked:audit.backgroundClicks++;}}
 QtObject {id:quick;function show(path,owner){audit.quickLooks=audit.quickLooks.concat(path);}}
 QtObject {id:stub;property var quickLookWindow:quick;property bool nativeFileDragActive:false;property bool nativeFileDragAvailable:false;function openFiles(path){throw Error("External opening prohibited");}}
 D.DesktopFilesSurface {id:surface;screen:Quickshell.screens[0];outputName:"HEADLESS-1";shell:stub;showFiles:true}
 function walk(item){let result=[item];for(const child of item.children || [])result=result.concat(walk(child));return result;}
 function rect(item){const p=item.mapToItem(surface.contentItem,0,0);return {x:p.x,y:p.y,width:item.width,height:item.height};}
 function state(){const rows=surface.content.service.rows,items=surface.content.displayItems;return {ready:surface.content.service.listingCurrent===true,listingCurrent:surface.content.service.listingCurrent,listingComplete:surface.content.service.listingComplete,count:rows.length,rows:rows.map(row=>({path:row.path,name:row.name,identity:row.identity})),width:surface.width,height:surface.height,active:surface.content.Window.window.active,selected:surface.content.selectedPaths,currentPath:surface.content.currentPath,currentKey:surface.content.currentLayoutKey===undefined?null:surface.content.currentLayoutKey,layout:surface.content.desktopLayout,mask:surface.content.inputRectangles,stage:rect(surface.content.inputArea),quickLooks:quickLooks,backgroundClicks:backgroundClicks,error:surface.content.error || surface.content.service.error,items:items.map(item=>{const tile=walk(surface.content).find(child=>child.modelData && child.fileRow && child.modelData.key===item.key),caption=tile?walk(tile).find(child=>child.font && child.text && String(child.text).includes(item.stack?item.label:item.row.name)):null,glyph=tile?walk(tile).find(child=>child.ink!==undefined && child.name!==undefined):null;return {key:item.key,stack:item.stack,path:item.row?item.row.path:null,sourceIndex:item.sourceIndex,overflow:item.overflow,rect:tile?rect(tile):null,caption:caption?{text:caption.text,rect:rect(caption),size:caption.font.pixelSize,naturalWidth:caption.implicitWidth,color:caption.color.toString(),resolvedFamily:caption.fontInfo.family}:null,icon:glyph?{width:glyph.width,height:glyph.height}:null};})};}
 IpcHandler {target:"desktopText"
  function ready():bool {return true;}
  function configure(mode:string,scale:real):void {D.Theme.colors=audit.palettes[mode];D.Theme.preferences={textScale:scale,reduceMotion:true};}
  function navigate(path:string):void {surface.content.service.navigate(path);}
  function refresh():void {surface.content.service.refresh();}
  function group(mode:string):void {surface.content.grouping(mode);}
  function status():string {return JSON.stringify(audit.state());}
 }
}
