import QtQuick
import QtQuick.Controls
import QtQuick.Window
import Quickshell
import Quickshell.Io
import "assets/shell" as D
ShellRoot {
 id:root
 property int closed:0
 property int launches:0
 property bool formMode:false
 property int networkSubmits:0
 property string submittedAddress:""
 Item {id:keyWitness;parent:stage.contentItem;property int presses:0;property int key:0;property int modifiers:0;property string text:"";Keys.onPressed:event=>{presses++;key=event.key;modifiers=event.modifiers;text=event.text;event.accepted=false;}}
 QtObject {id:search;property var files:[];property var preview:({});property bool opening:false;signal opened();signal browseRequested(string path);function search(text){}function requestPreview(path,token){} }
 QtObject {id:notifications;property bool dnd:false;property bool available:false}
 QtObject {id:audio;property bool muted:false;property bool available:false}
 QtObject {id:exported;property var state:({menu:null})}
 QtObject {id:network;property bool available:true;property bool ready:true;property bool busy:false;property var protocols:["sftp"];property var connections:[];property var rows:[];property var current:null;property string selectedId:"";property string error:"";property string localPath:"";property string browseId:"";property bool limited:false;function connectServer(address){root.networkSubmits++;root.submittedAddress=address;} }
 QtObject {id:stub;property var search:search;property var notifications:notifications;property var audioState:audio;property var exportedMenu:exported;property date currentDate:new Date();property var entries:[{id:"first",name:"First"},{id:"second",name:"Second"},{id:"japanese",name:"日本語"}];function appLabel(e){return e.name;}function appIcon(e){return "";}function activate(e,force){root.launches++;} }
 FloatingWindow {id:stage;visible:true;implicitWidth:950;implicitHeight:850;title:"Composition Fixture";color:"#202024"
  D.AppLauncher {id:launcher;visible:!root.formMode;shell:stub;x:70;y:50;onCloseRequested:root.closed++}
  Dialog {id:formDialog;parent:stage.contentItem;anchors.centerIn:parent;visible:root.formMode;modal:true;focus:true;closePolicy:Popup.CloseOnEscape;width:720;height:620;title:"Composition Form";contentItem:D.NetworkLocationsPane {id:form;service:network}}
  D.OwnedToolTip {id:tip;text:"Composition help"}
 }
 function find(item){if(item.objectName==="launcher-query" || item instanceof TextInput)return item;for(const child of item.children || []){const match=find(child);if(match)return match;}return null;}
 IpcHandler {target:"composition"
  function status():string{const field=root.find(root.formMode?form:launcher);return JSON.stringify({query:field?field.text:"",selected:launcher.selectedIndex,provider:launcher.provider,preview:launcher.previewOpen,closed:root.closed,launches:root.launches,composing:field?field.inputMethodComposing:false,preedit:field?field.preeditText:"",focus:field?field.activeFocus:false,tooltip:tip.visible,formVisible:formDialog.visible,networkSubmits:root.networkSubmits,submittedAddress:root.submittedAddress,keys:{presses:keyWitness.presses,key:keyWitness.key,modifiers:keyWitness.modifiers,text:keyWitness.text}});}
  function reset():void{root.formMode=false;const field=root.find(launcher);field.Keys.forwardTo=[keyWitness];field.text="";launcher.provider="Applications";launcher.selectedIndex=0;field.forceActiveFocus();}
  function openForm():void{root.formMode=true;const field=root.find(form);field.Keys.forwardTo=[keyWitness];field.text="sftp://example.invalid/";field.cursorPosition=field.text.length;Qt.callLater(()=>field.forceActiveFocus());}
  function showTip():void{tip.target=root.find(launcher);tip.lifecycle.help();}
  function hideTip():void{tip.lifecycle.dismiss();}
 }
 Component.onCompleted:Qt.callLater(()=>{stage.contentItem.Window.window.requestActivate();launcher.focusSearch();launcher.provider="Applications";})
}
