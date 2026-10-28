import QtQuick
import Quickshell
import Quickshell.Io
import Quickshell.Wayland
import "file:@ASSETS@" as D
ShellRoot {
 id:root
 property bool displayVisible:false
 property bool mediaVisible:false
 component FakePlayer:QtObject {
  property string dbusName:"org.mpris.fixture.one"
  property int uniqueId:1
  property string identity:"Fixture Player"
  property bool isPlaying:false
  property bool canSeek:true
  property bool volumeSupported:true
  property real volume:0.5
  property bool positionSupported:true
  property bool lengthSupported:true
  property bool canControl:true
  property bool canGoPrevious:true
  property bool canGoNext:true
  property bool canTogglePlaying:true
  property real position:20
  property real length:180
  property string trackTitle:"A synthetic track"
  property string trackArtist:"Fixture Artist"
  property string trackArtUrl:""
  function previous(){}
  function next(){}
  function togglePlaying(){isPlaying=!isPlaying;}
 }
 FakePlayer {id:firstPlayer}
 FakePlayer {id:secondPlayer;dbusName:"org.mpris.fixture.two";uniqueId:2;identity:"Second Fixture Player"}
 property int clicks:0
 property string confirmedAction:""
 QtObject {id:stub;property string popupOwner:"";signal dismissPopups(string owner);function claimPopup(name){popupOwner=name;dismissPopups(name);}function openFiles(path){} }
 D.SessionOptions {id:session;executeActions:false;onConfirmed:action=>root.confirmedAction=action}
 D.HelpWindow {id:help;shell:stub}
 D.QuickLookWindow {id:preview}
 D.TextToolsWindow {id:tools;shell:stub}
 D.RemoteDesktopWindow {id:remote;shell:stub}
 D.FileInfoWindow {id:info}
 D.FolderStack {id:stack;shell:stub}
 PanelWindow {id:panel;visible:!session.visible && !help.visible && !stack.visible && !preview.visible && !info.visible && !tools.visible && !remote.visible;anchors {top:true;bottom:true;left:true;right:true} color:D.Theme.colors.background;WlrLayershell.keyboardFocus:WlrKeyboardFocus.Exclusive
  D.MediaControl {id:media;x:30;y:30;width:300;height:270;visible:root.mediaVisible;expanded:true;players:[firstPlayer,secondPlayer]}
  D.DisplaySettingsPane {id:display;x:30;y:30;width:600;visible:root.displayVisible}
  Column {visible:!root.displayVisible && !root.mediaVisible;x:30;y:40;width:500;spacing:15
   D.DialogButton {id:button;text:"Delete Fixture";destructive:true;onClicked:root.clicks++}
   D.DesktopComboBox {id:combo;width:280;model:["One","Two","Three"];currentIndex:1}
   D.DesktopScrollBar {id:scroll;size:0.2;height:100;orientation:Qt.Vertical}
   D.CreativeTools {id:creative;width:500;page:"Colours"}
  }
 }
 IpcHandler {target:"test"
  function remoteShow():void {remote.connections=[{id:"fixture",name:"Office Computer",host:"office.example.test",protocol:"RDP",printer:true},{id:"vnc",name:"Studio Computer",host:"studio.example.test",protocol:"VNC",printer:false}];remote.visible=true;}
  function remoteSelect():void {remote.choose(0);}
  function remoteClose():void {remote.visible=false;}
  function previewShow(path:string):void {preview.show(path,null);}
  function previewSave(path:string):void {preview.subjectPath=path;preview.saveSubject();}
  function previewClose():void {preview.visible=false;}
  function infoShow():void {info.show({name:"Example document.txt",kind:"Plain text",size:"1,024 bytes",location:"Documents",modified:"Today",owner:"Fixture User",permissions:"Read and write"});}
  function infoClose():void {info.visible=false;}
  function toolShow(page:string):void {tools.page=page;tools.visible=true;}
  function toolClose():void {tools.visible=false;}
  function displayShow():void {root.displayVisible=true;display.outputs=[{name:"TEST-1",label:"Retina Display",mode:"2560x1440@60Hz",scale:2,x:0,y:0,modes:["2560x1440@60Hz"]},{name:"TEST-2",label:"External Display",mode:"1920x1080@60Hz",scale:1,x:1280,y:0,modes:["1920x1080@60Hz"]}];}
  function mediaCheck():string {root.mediaVisible=true;media.selectedPlayer=secondPlayer.dbusName;if(media.player!==secondPlayer)throw Error("Media source selection");media.seekTo(secondPlayer,secondPlayer.uniqueId,90);if(secondPlayer.position!==90 || firstPlayer.position!==20)throw Error("Seek changed wrong source");media.seekTo(secondPlayer,secondPlayer.uniqueId,999);if(secondPlayer.position!==180)throw Error("Seek beyond track");secondPlayer.canSeek=false;media.seekTo(secondPlayer,secondPlayer.uniqueId,30);if(secondPlayer.position!==180)throw Error("Seek ignored source capability");return "PASS media source and seek isolation";}
  function mediaClose():void {root.mediaVisible=false;}
  function displayClose():void {root.displayVisible=false;}
  function theme(dark:bool):void {D.Theme.colors=dark?{background:"#252527",surface:"#333336",field:"#404044",text:"#f5f5f7",muted:"#b0b0b8",border:"#555559",hover:"#45454a",accent:"#0a84ff",selectedText:"#ffffff"}:{background:"#f2f2f4",surface:"#ffffff",field:"#e5e5e8",text:"#202023",muted:"#6b6b70",border:"#c8c8ce",hover:"#dedee3",accent:"#007aff",selectedText:"#ffffff"};}
  function focusButton():void {button.forceActiveFocus();}
  function focusCombo():void {combo.forceActiveFocus();combo.popup.open();}
  function state():string {return JSON.stringify({remoteSelection:remote.selectedId,clicks:root.clicks,index:combo.currentIndex,highlight:combo.highlightedIndex,popup:combo.popup.visible,destructive:button.destructive,scrollOpacity:scroll.opacity,sessionVisible:session.visible,confirmed:root.confirmedAction,helpMatches:help.matches.length,stackRows:stack.rows.length,stackTotal:stack.total});}
  function sessionShow(action:string):void {session.show(action);}
  function sessionConfirm():void {session.confirm();}
  function helpShow():void {help.show();}
  function helpClose():void {help.visible=false;}
  function stackShow(path:string):void {stack.show(path,"");}
  function stackClose():void {stack.visible=false;}
  function colour():string {creative.setColour("#80ff0000");if(Math.abs(creative.chosen.a-128/255)>0.01 || creative.chosen.r<0.99)throw Error("Alpha/RGB round trip");creative.setColour("#007aff");if(creative.chosen.a!==1)throw Error("Opaque colour reset");return "PASS colour alpha and RGB round trip";}
 }
}