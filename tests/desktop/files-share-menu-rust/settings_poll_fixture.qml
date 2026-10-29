import QtQuick
import Quickshell
import "assets/shell" as D
ShellRoot {
 id:test
 property int phase:0
 property int ticks:0
 property int checks:0
 property var captured:null
 property var capturedTheme:null
 property int colorEpoch:0
 property int capturedEpoch:0
 property bool themeChanged:false
 Connections {target:D.Theme;function onColorsChanged(){test.colorEpoch++;}}
 property bool readSeen:false
 property bool queued:false
 property bool applyingSeen:false
 property bool identityChanged:false
 function check(value,message){if(!value)throw Error(message);checks++;}
 D.SettingsState {id:settings;active:false}
 Timer {interval:20;running:true;repeat:true;onTriggered:{try{
  if(++test.ticks>200)throw Error("Settings protocol readiness deadline");
  if(test.phase===0){settings.refresh();test.phase=1;}
  else if(test.phase===1){if(!settings.available || settings.busy || JSON.stringify(D.Theme.preferences.dockPins)!=='["fixture-a","fixture-b"]')return;test.captured=settings.preferences;test.capturedTheme=D.Theme.preferences;test.capturedEpoch=test.colorEpoch;test.check(JSON.stringify(settings.preferences.dockPins)==='["fixture-a","fixture-b"]',"Real initial provider values");settings.refresh();test.phase=2;}
  else if(test.phase===2){if(settings.busy){test.check(!settings.applying,"Actual read is busy but never applying");test.readSeen=true;return;}if(test.colorEpoch<=test.capturedEpoch)return;test.check(test.readSeen,"Actual second read observed");test.themeChanged=D.Theme.preferences!==test.capturedTheme;test.check(test.themeChanged===EXPECTED_BASELINE,"Actual Theme file reload identity policy");test.identityChanged=settings.preferences!==test.captured;test.check(test.identityChanged===EXPECTED_BASELINE,"Unchanged preferences identity policy");test.check(JSON.stringify(settings.preferences.dockPins)==='["fixture-a","fixture-b"]' && !settings.error,"Read leaves values unchanged and successful");settings.refresh();test.phase=3;}
  else if(test.phase===3){if(settings.busy && !test.queued){test.check(!settings.applying,"Another read starts without applying");settings.setPreference("dockPins",["fixture-b","fixture-a"]);test.queued=true;}if(settings.applying)test.applyingSeen=true;if(settings.busy || !test.queued)return;test.check(test.applyingSeen,"Queued explicit edit is applying");if(JSON.stringify(D.Theme.preferences.dockPins)!=='["fixture-b","fixture-a"]')return;test.check(settings.preferences!==test.captured && D.Theme.preferences!==test.capturedTheme,"Actual changed config replaces state and Theme identity");test.check(JSON.stringify(settings.preferences.dockPins)==='["fixture-b","fixture-a"]' && !settings.error,"Explicit queued edit survives a running read");console.log("SETTINGS_POLL_PASS "+JSON.stringify({checks:test.checks,baseline:EXPECTED_BASELINE,unchangedReadChangedIdentity:test.identityChanged,unchangedReloadChangedTheme:test.themeChanged,fileReloadObserved:test.colorEpoch>test.capturedEpoch,readSeen:test.readSeen,queuedEditApplying:test.applyingSeen}));Qt.quit();}
 }catch(error){console.error("SETTINGS_POLL_FAIL "+error);Qt.exit(1);}}}
}
