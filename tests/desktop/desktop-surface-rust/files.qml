import QtQuick
import Quickshell
import Quickshell.Io
import Quickshell.Wayland
import "file:@ASSETS@" as D
ShellRoot {
 property int backgroundClicks:0
 PanelWindow {anchors {left:true;right:true;top:true;bottom:true}color:"#303030";exclusiveZone:0;WlrLayershell.layer:WlrLayer.Background;MouseArea {anchors.fill:parent;onClicked:backgroundClicks++}}
 QtObject {id:quick;function show(path,owner){}}
 QtObject {id:stub;property var quickLookWindow:quick;function openFiles(path){}}
 D.DesktopFilesSurface {id:surface;screen:Quickshell.screens[0];outputName:"HEADLESS-1";shell:stub;showFiles:true;WlrLayershell.layer:WlrLayer.@LAYER@}
 IpcHandler {target:"desktopFilesTest"
  function status():string {return JSON.stringify({count:surface.content.service.rows.length,backgroundClicks:backgroundClicks,layout:surface.content.desktopLayout,items:surface.content.displayItems,path:surface.content.directory,selected:surface.content.selectedPaths,confirm:surface.content.confirmationVisible,error:surface.content.error || surface.content.service.error,keyboard:surface.content.keyboardActive,allowKeyboard:surface.allowKeyboard,geometry:{x:surface.content.inputArea.x,y:surface.content.inputArea.y,width:surface.content.inputArea.width,height:surface.content.inputArea.height},size:{width:surface.width,height:surface.height}});}
  function select(index:int):void {surface.content.choose(index,0);}
  function askTrash():void {surface.content.askTrash();}
  function confirmTrash():void {surface.content.confirmTrash();}
  function selectAll():void {surface.content.selectedPaths=surface.content.service.rows.map(row=>row.path);surface.content.currentPath=surface.content.service.rows[1].path;surface.content.focusContents();}
  function group(mode:string):void {surface.content.grouping(mode);}
  function rename(name:string):void {surface.content.service.act("rename",{path:surface.content.currentPath,name:name});}
 }
}
