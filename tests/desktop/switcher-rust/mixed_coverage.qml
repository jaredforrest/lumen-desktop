import QtQuick
import Quickshell
import "AppSwitcherModel.js" as SwitcherModel
ShellRoot {
 id:root;property bool visible:false
 property string pendingNativeRequest:""
 property var previewNativeFocus:null
 property var previewInventoryWitness:null
 property var items:[]
 property int selected:-1
 property int commits:0
 property string phase:"closed"
 property var screen:null
 property var observations:[]
 property int caseIndex:0
 property bool unknownEntry:false
 property bool forbidResolver:false
 property bool unrelatedGroup:false
 property QtObject keyboard:QtObject {function forceActiveFocus(){}}
 property QtObject view:QtObject {function positionViewAtIndex(a,b){}}
 property QtObject recentWindow:QtObject {property string appId:"shared.legacy";property string title:"Duplicate";property string foreignToplevelIdentifier:"recent-id"}
 property QtObject olderWindow:QtObject {property string appId:"shared.native";property string title:"Duplicate";property string foreignToplevelIdentifier:"captured-native"}
 property QtObject unsupportedWindow:QtObject {property string appId:"unrelated.legacy";property string title:"Unrelated";property string foreignToplevelIdentifier:"legacy-exact"}
 property QtObject shell:QtObject {
  property var windows:[recentWindow,olderWindow]
  property var activationHistory:windows
  property bool nativeCommitBlocked:false
  property var nativeFocusedCapture:({kind:"none",epoch:"synthetic"})
  property var nativeWindowCaptures:[{appId:"shared.native",foreignId:"captured-native",conId:41,instanceId:"synthetic",epoch:"synthetic"}]
  property var nativeWindowCoverageComplete:undefined
  property var nativeWindowObservation:null
  property var runningApplications:[]
  property var applicationActivationHistory:[]
  property int focusEpoch:0
  property var lastNative:null
  property var lastFallback:null
  function entryFor(window){if(root.forbidResolver)throw Error("Pending native commit consulted live aliases");return root.unknownEntry?null:{id:window.appId==="unrelated.legacy"?"unrelated.desktop":"shared.desktop",name:"Shared application"};}
  function appLabel(entry){return entry.name;}
  function appIcon(entry){return "";}
  function nameForAppId(id,title){return title || id;}
  function preferredOutput(){return "";}
  function claimPopup(owner){}
  function focusCapturedNative(capture,source,witness){root.check(witness && witness.digest===root.previewInventoryWitness.digest,"Inventory witness lost");lastNative=capture;return "synthetic-request";}
  function activate(entry,newWindow){lastFallback=activationHistory[0];}
  function focusWindow(window){lastFallback=window;}
 }
 __PRODUCTION_IDENTITY__
 __PRODUCTION_COMMAND__
 __PRODUCTION_COMMIT__
 function check(value,message){if(!value)throw Error(message);}
 function nextCase(){
  const cases=[{name:"mixed alias",coverage:false,sameId:false,native:false},{name:"unknown coverage",coverage:undefined,sameId:false,native:false},{name:"mixed same app ID",coverage:false,sameId:true,native:false},{name:"nonboolean coverage",coverage:"true",sameId:true,native:false},{name:"complete exact native",coverage:true,sameId:true,native:true},{name:"missing bridge",coverage:true,sameId:true,native:false,fault:"missing"},{name:"wrong exact ID",coverage:true,sameId:true,native:false,fault:"wrong"},{name:"duplicate QObject ID",coverage:true,sameId:true,native:false,fault:"duplicate-window"},{name:"duplicate captured ID",coverage:true,sameId:true,native:false,fault:"duplicate-capture"},{name:"broker-only record",coverage:true,sameId:true,native:false,fault:"extra"},{name:"unsupported group member",coverage:true,sameId:true,native:false,fault:"incomplete"},{name:"unknown app exact target label",coverage:true,sameId:true,native:true,unknown:true},
   {name:"unrelated unsupported group",coverage:false,sameId:true,native:true,unrelated:true},
   {name:"frozen pending aliases",coverage:true,sameId:true,native:true,aliasEdit:true},
   {name:"missing witness",coverage:true,sameId:true,native:false,observationFault:"missing"},
   {name:"mismatched witness epoch",coverage:true,sameId:true,native:false,observationFault:"epoch"},
   {name:"fractional revision",coverage:true,sameId:true,native:false,observationFault:"revision"},
   {name:"unsupported source",coverage:true,sameId:true,native:false,observationFault:"source"},
   {name:"unknown witness fields",coverage:true,sameId:true,native:false,observationFault:"extra"},
   {name:"unsafe integer revision",coverage:true,sameId:true,native:false,observationFault:"overflow"}];
  if(caseIndex===cases.length){console.log("SWITCHER_COVERAGE_PASS "+JSON.stringify(observations));Qt.quit();return;}
  const test=cases[caseIndex++];
  root.visible=false;root.phase="closed";root.forbidResolver=false;shell.lastNative=null;shell.lastFallback=null;
  shell.windows=test.unrelated?[recentWindow,olderWindow,unsupportedWindow]:[recentWindow,olderWindow];
  shell.activationHistory=shell.windows;
  recentWindow.appId=test.sameId?"shared.native":"shared.legacy";
  root.unknownEntry=!!test.unknown;recentWindow.title=test.unknown?"Recent fallback title":"Duplicate";olderWindow.title=test.unknown?"Older exact target title":"Duplicate";
  recentWindow.foreignToplevelIdentifier=test.fault==="missing"?"":test.fault==="wrong"?"wrong-id":test.fault==="duplicate-window"?"captured-native":"recent-id";
  shell.nativeWindowCaptures=[{appId:"shared.native",foreignId:"captured-native",conId:41,instanceId:"original-process",epoch:"original-epoch"},{appId:"shared.native",foreignId:test.fault==="duplicate-capture"?"captured-native":"recent-id",conId:42,instanceId:"recent-process",epoch:"original-epoch"}];
  if(test.fault==="incomplete")shell.nativeWindowCaptures=shell.nativeWindowCaptures.slice(0,1);
  if(test.fault==="extra")shell.nativeWindowCaptures=shell.nativeWindowCaptures.concat([{appId:"shared.native",foreignId:"unrepresented",conId:43,instanceId:"other",epoch:"original-epoch"}]);
  shell.nativeWindowCoverageComplete=test.coverage;
  const epoch="0123456789abcdef01234567";
  shell.nativeWindowObservation=test.coverage===undefined || test.coverage==="true"?null:{version:1,epoch:epoch,source:{kind:"none",epoch:epoch},captures:shell.nativeWindowCaptures,inventoryWitness:{version:1,epoch:epoch,observer:"a".repeat(64),digest:"b".repeat(64),count:2,revision:1}};
  if(test.coverage===false && !test.unrelated)shell.nativeWindowObservation.source.kind="unsupported";
  if(test.observationFault==="missing")delete shell.nativeWindowObservation.inventoryWitness;
  if(test.observationFault==="epoch")shell.nativeWindowObservation.inventoryWitness.epoch="f".repeat(24);
  if(test.observationFault==="revision")shell.nativeWindowObservation.inventoryWitness.revision=1.5;
  if(test.observationFault==="source")shell.nativeWindowObservation.source.kind="unsupported";
  if(test.observationFault==="extra")shell.nativeWindowObservation.inventoryWitness.unexpected=true;
  if(test.observationFault==="overflow")shell.nativeWindowObservation.inventoryWitness.revision=9007199254740992;
  root.command("begin");
  check(root.items.length===(test.unrelated?2:1),"Unexpected application membership: "+test.name);
  check(root.items[0].window===(test.native?olderWindow:recentWindow),"Exact paired/fallback QObject membership changed: "+test.name);
  if(test.unknown)check(root.items[0].label===olderWindow.title,"Label describes another QObject: "+root.items[0].label);
  check(!!root.items[0].nativeCapture===test.native,"Coverage retargeted unsupported membership: "+test.name);
  root.forbidResolver=!!test.aliasEdit;
  root.commit();
  Qt.callLater(()=>{try{
   if(test.native){check(shell.lastNative && shell.lastNative.foreignId==="captured-native" && shell.lastNative.instanceId==="original-process" && shell.lastNative.epoch==="original-epoch","Exact capture witness changed");check(shell.lastFallback===null,"Native and fallback both requested");}
   else {check(shell.lastNative===null,"Unsupported/unknown row redirected to native");check(shell.lastFallback===recentWindow,"Recent QObject fallback was lost");}
   observations.push({name:test.name,nativeRequested:!!shell.lastNative,recentFallback:shell.lastFallback===recentWindow,label:root.items[0].label});
   root.forbidResolver=false;
   nextCase();
  }catch(error){console.error("SWITCHER_COVERAGE_FAILED "+error);Qt.exit(1);}});
 }
 Timer {interval:1;running:true;onTriggered:{try{nextCase();}catch(error){console.error("SWITCHER_COVERAGE_FAILED "+error);Qt.exit(1);}}}
 Timer {interval:5000;running:true;onTriggered:{console.error("SWITCHER_COVERAGE_TIMEOUT");Qt.exit(2);}}
}
