import QtQuick
import Quickshell
import "LUMEN_ASSETS" as D
ShellRoot {
 property int phase:0
 property int ticks:0
 QtObject {id:notifications;property bool available:true;property bool dnd:false;property var focusState:({name:""})}
 QtObject {id:system;property var wifi:({available:false,state:""})}
 QtObject {id:audio;property bool available:false;property bool muted:false;property real volume:0;property var outputs:[]}
 QtObject {id:vpn;property var mullvad:({state:"disconnected"});property var tailscale:({online:false})}
 QtObject {id:stub;property var notifications:notifications;property var systemState:system;property var audioState:audio;property var vpn:vpn;property string volume:"";function editVolume(pressed){}function setVolume(value){}}
 D.ControlCentre {id:editor;shell:stub;preferences:({order:["wifi","bluetooth","sharing","media","displays","appearance","focus","brightness","sound"],hidden:["wifi","bluetooth","media","brightness","sound"]})}
 Timer {interval:100;running:true;repeat:true;onTriggered:{
  try {
   if(++ticks>100)throw Error("Writer timeout");
   if(phase===0){editor.move("focus",-1);
   if(editor.preferences.order[5]!=="focus")throw Error("reorder failed");
   editor.toggle("displays");
   if(!editor.preferences.hidden.includes("displays"))throw Error("hide failed");
   editor.editing=true;
   if(editor.preferences.order.length!==9)throw Error("controls lost");
   phase=1;return;}
   if(editor.saving || Object.keys(editor.pending).length || !editor.sharingAvailable)return;
   if(editor.names.sharing!=="Fixture Sharing" || editor.sharingSubtitle!=="On · 3 min")throw Error("Sharing status/control mismatch");
   if(!D.NearbySharingState.canSend)throw Error("send capability missing");
   D.NearbySharingState.observedAt=Date.now()-8001;D.NearbySharingState.now=Date.now();
   if(editor.sharingAvailable || D.NearbySharingState.canSend || editor.sharingSubtitle!=="Unavailable")throw Error("expired status remained actionable");
   D.NearbySharingState.observedAt=Date.now()+8000;
   if(editor.sharingAvailable)throw Error("backwards clock retained actionable status");
   D.NearbySharingState.state=({available:true,active:false,canSend:false,detail:"Receiving only"});D.NearbySharingState.observedAt=Date.now();D.NearbySharingState.now=Date.now();
   if(!editor.sharingAvailable || D.NearbySharingState.canSend || editor.sharingSubtitle!=="Receiving only")throw Error("receiving-only capability mismatch");
   console.log("FOCUS_EDITOR_PASS");Qt.quit();
  }catch(e){console.error("FOCUS_EDITOR_FAIL "+e);Qt.quit();}
 }}
}
