import QtQuick
import QtQuick.Window
import Quickshell
import Quickshell.Io
ShellRoot {id:fixture
 property string response:""
 Process {id:registration;stdout:StdioCollector {onStreamFinished:fixture.response=text}}
 IpcHandler {target:"zoomFixture"
  function register(container:int,title:string):void {fixture.response="";registration.command=["lumen-window-geometry","--app","org.quickshell","--title",title,"--container",String(container),"--outer-width","1200","--outer-height","820","--minimum-width","1","--minimum-height","1","--preferred-width","640","--preferred-height","440"];registration.running=true;}
  function state():string {return JSON.stringify({response:fixture.response,busy:registration.running,width:window.width,height:window.height,clientWidth:resizable.width,clientHeight:resizable.height});}
  function minSize(width:int,height:int):void {window.minimumSize=Qt.size(width,height);}
  function title(value:string):void {window.title=value;}
  function showClient(value:bool):void {resizable.visible=value;}
  function resizeClient(width:int,height:int):void {resizable.width=width;resizable.height=height;}
 }
 Window {id:resizable;visible:false;title:"Client Resize Fixture";width:420;height:300;Rectangle {anchors.fill:parent;color:"#a2e3f3"}}
 FloatingWindow {id:window; onWidthChanged:console.log("client-size",Date.now(),width,height);onHeightChanged:console.log("client-size",Date.now(),width,height); visible:true; title:"Preferred Zoom Fixture"; implicitWidth:420; implicitHeight:300
 Rectangle {anchors.fill:parent;color:"#f3e3a2";Text {anchors.centerIn:parent;text:"Preferred client content";color:"#202020";font.pixelSize:22}}
}}
