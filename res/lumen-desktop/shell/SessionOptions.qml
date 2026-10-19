import QtQuick
import QtQuick.Controls
import QtQuick.Window
import Quickshell
import Quickshell.Io
FloatingWindow {
 id:root;title:"Session Options";visible:false;color:"transparent"
 implicitWidth:420;implicitHeight:300;minimumSize:Qt.size(320,280)
 property string action:""
 readonly property var labels:({suspend:"Sleep",reboot:"Restart",poweroff:"Shut Down",logout:"Log Out"})
 readonly property var commands:({suspend:["systemctl","suspend"],reboot:["systemctl","reboot"],poweroff:["systemctl","poweroff"],logout:["swaymsg","exit"]})
 property string error:""
 signal confirmed(string action)
 property bool executeActions:true
 property var lifecycle:null
 property bool preflighting:false
 property bool authorizing:false
 property string authorizationToken:""
 readonly property var preflight:lifecycle && preflighting?lifecycle.session:({phase:"idle",token:"",error:""})
 function dismiss(){if(worker.running)return;authorizing=false;authorizationToken="";preflighting=false;if(lifecycle)lifecycle.sessionCommand("session_cancel","");visible=false;}
 function show(value){if(!labels[value] || worker.running)return;dismiss();action=value;error="";visible=true;Qt.callLater(()=>{cancel.forceActiveFocus();if(body.Window.window)body.Window.window.requestActivate();});}
 function execute(){confirmed(action);if(!executeActions){visible=false;return;}worker.command=commands[action];worker.running=true;}
 function confirm(){
  if(!commands[action] || worker.running || authorizing || (preflighting && preflight.phase==="idle"))return;
  if(action==="suspend"){execute();return;}
  if(!lifecycle || !lifecycle.available){error="Application observation is unavailable. The session remains open.";return;}
  if(preflight.phase==="waiting" || preflight.phase==="checking")return;
  if(preflight.phase==="ready"){
   authorizationToken=preflight.token;authorizing=true;lifecycle.sessionCommand("session_authorize",authorizationToken);
  }else{error="";preflighting=true;lifecycle.sessionCommand("session_begin",action);}
 }
 Connections {target:root.lifecycle;ignoreUnknownSignals:true
  function onSessionChanged(){if(root.lifecycle.session.phase==="blocked")root.authorizing=false;}
  function onAvailableChanged(){if(!root.lifecycle.available){root.authorizing=false;root.preflighting=false;root.error="Application observation was lost. The session remains open.";}}
  function onSessionAuthorized(authorization){
   if(!root.visible || !root.authorizing || authorization.token!==root.authorizationToken || authorization.action!==root.action)return;
   root.authorizing=false;root.authorizationToken="";root.preflighting=false;root.execute();
  }
 }
 onVisibleChanged:if(!visible && !worker.running){authorizing=false;authorizationToken="";preflighting=false;if(lifecycle)lifecycle.sessionCommand("session_cancel","");}
 Rectangle {id:body;anchors.fill:parent;color:Theme.colors.background;radius:Theme.nativeWindowFrame?0:12
  MouseArea {width:parent.width;height:48;onPressed:body.Window.window.startSystemMove()}
  WindowControls {windowActive:body.Window.window?body.Window.window.active:false;windowName:"Session Options";canMinimize:false;canFullscreen:false;onTriggered:root.dismiss()}
  Column {x:24;y:62;width:parent.width-48;spacing:14
   PanelLabel {width:parent.width;text:(root.labels[root.action] || "Session")+"?";font.pixelSize:20;font.weight:Font.DemiBold}
   PanelLabel {width:parent.width;wrapMode:Text.WordWrap;text:root.action==="suspend"?"Your session will be available when this computer wakes.":root.preflight.phase==="waiting"?"Waiting for "+root.preflight.waiting+". Resolve its own Save, Discard or Cancel prompt. You can cancel this session action at any time.":root.preflight.phase==="ready"?"Applications have quit. Confirm once more to complete this session action.":"Applications will be asked to quit using their own Save and Cancel controls. Unsupported applications keep the session open."}
   StatusMessage {width:parent.width;text:root.error || root.preflight.error;severity:"error"}
   Row {anchors.right:parent.right;spacing:10
    DialogButton {id:cancel;text:"Cancel";enabled:!worker.running;onClicked:root.dismiss()}
    DialogButton {text:root.preflight.phase==="waiting"?"Waiting…":root.preflight.phase==="ready"?(root.labels[root.action] || "Continue"):root.action==="suspend"?"Sleep":"Continue";primary:root.action==="suspend";destructive:root.action!=="suspend";enabled:!worker.running && !root.authorizing && root.preflight.phase!=="waiting" && root.preflight.phase!=="checking";onClicked:root.confirm()}
   }
  }
  Shortcut {sequence:"Escape";onActivated:root.dismiss()}
 }
 Process {id:worker;stderr:StdioCollector {onStreamFinished:if(text.trim())root.error="The session action could not be completed. Check your system permissions."}onExited:(code,status)=>{if(code===0)root.visible=false;else root.error="The session action could not be completed. Try again after saving your work."}}
}
