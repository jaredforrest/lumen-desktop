import QtQuick
import Quickshell
import "AppSwitcherModel.js" as SwitcherModel
Item {
 id:root;visible:false
 property string pendingNativeRequest:""
 property var previewNativeFocus:null
 property var items:[]
 property int selected:-1
 property int commits:0
 property string phase:"closed"
 property var screen:null
 property var observations:[]
 property int caseIndex:0
 property QtObject keyboard:QtObject {function forceActiveFocus(){}}
 property QtObject view:QtObject {function positionViewAtIndex(a,b){}}
 property QtObject recentWindow:QtObject {property string appId:"shared.legacy";property string title:"Duplicate"}
 property QtObject olderWindow:QtObject {property string appId:"shared.native";property string title:"Duplicate"}
 property QtObject shell:QtObject {
  property var windows:[recentWindow,olderWindow]
  property var activationHistory:windows
  property bool nativeCommitBlocked:false
  property var nativeFocusedCapture:({kind:"none",epoch:"synthetic"})
  property var nativeWindowCaptures:[{appId:"shared.native",foreignId:"captured-native",conId:41,instanceId:"synthetic",epoch:"synthetic"}]
  property var nativeWindowCoverageComplete:undefined
  property var runningApplications:[]
  property var applicationActivationHistory:[]
  property int focusEpoch:0
  property var lastNative:null
  property var lastFallback:null
  function entryFor(window){return {id:"shared.desktop",name:"Shared application"};}
  function appLabel(entry){return entry.name;}
  function appIcon(entry){return "";}
  function nameForAppId(id,title){return id;}
  function preferredOutput(){return "";}
  function claimPopup(owner){}
  function focusCapturedNative(capture,source){lastNative=capture;return "synthetic-request";}
  function activate(entry,newWindow){lastFallback=activationHistory[0];}
  function focusWindow(window){lastFallback=window;}
 }
 __PRODUCTION_IDENTITY__
 __PRODUCTION_COMMAND__
 __PRODUCTION_COMMIT__
 function check(value,message){if(!value)throw Error(message);}
 function nextCase(){
  const cases=[{name:"mixed alias",coverage:false,sameId:false,native:false},{name:"unknown coverage",coverage:undefined,sameId:false,native:false},{name:"mixed same app ID",coverage:false,sameId:true,native:false},{name:"nonboolean coverage",coverage:"true",sameId:true,native:false},{name:"complete native",coverage:true,sameId:true,native:true}];
  if(caseIndex===cases.length){console.log("SWITCHER_COVERAGE_PASS "+JSON.stringify(observations));Qt.quit();return;}
  const test=cases[caseIndex++];
  root.visible=false;root.phase="closed";shell.lastNative=null;shell.lastFallback=null;
  recentWindow.appId=test.sameId?"shared.native":"shared.legacy";
  shell.nativeWindowCoverageComplete=test.coverage;
  root.command("begin");
  check(root.items.length===1,"Unexpected application membership: "+test.name);
  check(root.items[0].window===recentWindow,"Recent QObject membership changed: "+test.name);
  check(!!root.items[0].nativeCapture===test.native,"Coverage retargeted unsupported membership: "+test.name);
  root.commit();
  Qt.callLater(()=>{try{
   if(test.native){check(shell.lastNative && shell.lastNative.foreignId==="captured-native","Supported capture not requested");check(shell.lastFallback===null,"Native and fallback both requested");}
   else {check(shell.lastNative===null,"Unsupported/unknown row redirected to native");check(shell.lastFallback===recentWindow,"Recent QObject fallback was lost");}
   observations.push({name:test.name,nativeRequested:!!shell.lastNative,recentFallback:shell.lastFallback===recentWindow});
   nextCase();
  }catch(error){console.error("SWITCHER_COVERAGE_FAILED "+error);Qt.exit(1);}});
 }
 Timer {interval:1;running:true;onTriggered:{try{nextCase();}catch(error){console.error("SWITCHER_COVERAGE_FAILED "+error);Qt.exit(1);}}}
 Timer {interval:5000;running:true;onTriggered:{console.error("SWITCHER_COVERAGE_TIMEOUT");Qt.exit(2);}}
}
