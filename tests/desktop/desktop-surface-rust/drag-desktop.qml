import QtQuick
import QtQuick.Window
import Quickshell
import Quickshell.Io
import Quickshell.Wayland
import "file:@ASSETS@" as D
ShellRoot {
 id:fixtureRoot
 property int backgroundClicks:0
 PanelWindow {anchors {left:true;right:true;top:true;bottom:true}exclusiveZone:0;WlrLayershell.layer:WlrLayer.Bottom;WlrLayershell.keyboardFocus:WlrKeyboardFocus.None;color:"#20242a";MouseArea {anchors.fill:parent;onClicked:fixtureRoot.backgroundClicks++;}}
 QtObject {id:quick;function show(path,owner){}}
 D.NativeFileDragState {id:dragFacts}
 QtObject {id:stub;property var quickLookWindow:quick;property bool nativeFileDragActive:dragFacts.active;property bool nativeFileDragAvailable:dragFacts.available;function openFiles(path){}}
 D.DesktopFilesSurface {id:surface;screen:Quickshell.screens[0];outputName:"HEADLESS-1";shell:stub;showFiles:true}
 IpcHandler {target:"dragTest"
  function status():string{return JSON.stringify({backgroundClicks:fixtureRoot.backgroundClicks,mask:surface.content.inputRectangles,count:surface.content.service.rows.length,active:surface.content.Window.window.active,native:surface.content.nativeDragActive,external:surface.content.externalDragActive,layout:surface.content.desktopLayout,dropPrompt:surface.content.dropConfirmationVisible,pending:surface.content.pendingDropPaths,error:surface.content.error || surface.content.service.error});}
  function confirm():void{surface.content.confirmDrop();}
  function selectAll():void{surface.content.selectedPaths=surface.content.service.rows.map(row=>row.path);}
 }
}
