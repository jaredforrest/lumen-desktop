import QtQuick
import QtQuick.Window
import Quickshell
import "assets/shell" as D
ShellRoot {
 id:test
 property int phase:0
 property int ticks:0
 property int outcomeIndex:0
 property bool valid:true
 property bool imageSaved:false
 property bool painting:false
 property bool pendingSeen:false
 property int terminalSignals:0
 property int falseStartupErrors:0
 property var processEvents:[]
 property var fixtureIntent:FIXTURE_INTENT
 Window {
  id:view;visible:true;width:FIXTURE_WIDTH;height:500;color:D.Theme.colors.background
  D.FileShareDialog {id:chooser;intentCurrent:value=>test.valid && !!value && value.selection[0].path===test.fixtureIntent.selection[0].path}
 }
 Connections {target:chooser.state;function onFinished(result){test.terminalSignals++;test.processEvents.push("finished:"+result.state);}function onErrorChanged(){if(chooser.state.error.includes("could not start"))test.falseStartupErrors++;}}
 Connections {target:chooser.state.invoker;function onExited(code,status){test.processEvents.push("exited:"+code);}function onRunningChanged(){test.processEvents.push("running:"+chooser.state.invoker.running);}}
 function check(value,message){if(!value)throw Error(message+" phase="+phase);}
 function find(name,item,seen){if(!item || seen.includes(item))return null;seen.push(item);if(item.objectName===name)return item;const next=Array.from(item.children || []).concat(Array.from(item.resources || []));if(item.contentItem)next.push(item.contentItem);for(const child of next){const result=find(name,child,seen);if(result)return result;}return null;}
 function item(name){return find(name,chooser,[]);}
 function ready(){return chooser.visible && !chooser.state.loading && chooser.state.destinations.length===2;}
 function select(){const row=item("share-destination-fixture");check(row && row.enabled,"advertised ready destination");row.clicked();}
 Timer {interval:40;running:true;repeat:true;onTriggered:{try{
  if(++test.ticks>300)throw Error("Share presentation readiness deadline");
  if(test.phase===0){if(D.Theme.textScale!==FIXTURE_SCALE || D.Theme.dark!==FIXTURE_DARK)return;chooser.openFor(test.fixtureIntent);test.phase=1;}
  else if(test.phase===1){if(!ready())return;check(!item("share-destination-offline").enabled,"offline destination cannot launch");select();check(chooser.destination.id==="fixture" && !chooser.state.working,"selection alone cannot invoke");item("share-open").forceActiveFocus();test.phase=2;}
  else if(test.phase===2){const button=item("share-open");const at=button.mapToItem(chooser.contentItem,0,0);check(at.y>=-1 && at.y+button.height<=chooser.contentItem.height+1,"keyboard focus reveals action in constrained content");item("share-cancel").clicked();check(!chooser.visible,"explicit Cancel closes without dispatch");chooser.openFor(test.fixtureIntent);test.phase=3;}
  else if(test.phase===3){if(!ready())return;select();test.valid=false;item("share-open").clicked();check(!chooser.state.working && chooser.state.error.includes("changed"),"stale captured selection refuses before provider");item("share-cancel").clicked();test.valid=true;test.phase=4;}
  else if(test.phase===4){if(!chooser.visible){chooser.openFor(test.fixtureIntent);return;}if(!ready())return;select();test.pendingSeen=false;item("share-open").clicked();check(chooser.state.working && !item("share-cancel").enabled,"pending invocation is explicit and cannot falsely claim cancellation");test.phase=5;}
  else if(test.phase===5){if(chooser.state.working){const pending=item("share-pending");const at=pending.mapToItem(chooser.contentItem,0,0);check(pending.visible && at.y>=-1 && at.y+pending.height<=chooser.contentItem.height+1,"pending provider feedback is revealed");test.pendingSeen=true;return;}if(!chooser.state.outcome)return;check(test.pendingSeen && item("share-cancel").activeFocus,"completed provider outcome restores reachable dismissal focus");const expected=["offline","cancelled","error","opened"][test.outcomeIndex];check(chooser.state.outcome.state===expected,"inspectable "+expected+" outcome");check(test.terminalSignals===test.outcomeIndex+1 && test.falseStartupErrors===0,"exactly one terminal signal and no false registry failure");check(item("share-outcome").text.length>20,"actionable outcome remains visible");if(test.outcomeIndex===2 && !test.imageSaved){if(test.painting)return;test.painting=true;view.contentItem.grabToImage(result=>{test.imageSaved=result.saveToFile("FIXTURE_IMAGE");});return;}++test.outcomeIndex;if(test.outcomeIndex<4){item("share-cancel").clicked();test.phase=4;}else test.phase=6;}
  else if(test.phase===6){if(!test.imageSaved)return;check(D.AppLaunch.recordedCommands.length===1 && D.AppLaunch.recordedCommands[0][2]===test.fixtureIntent.selection[0].path,"legacy adapter records only explicit literal handoff");console.log("SHARE_DESTINATIONS_PASS "+JSON.stringify({checks:10,commands:D.AppLaunch.recordedCommands,width:view.width,scale:D.Theme.textScale,dark:D.Theme.dark,terminalSignals:test.terminalSignals,falseStartupErrors:test.falseStartupErrors,processEvents:test.processEvents}));Qt.quit();}
 }catch(error){console.error("SHARE_DESTINATIONS_FAIL "+error);Qt.exit(1);}}}
}