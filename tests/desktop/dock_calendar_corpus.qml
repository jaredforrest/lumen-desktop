import QtQuick
import Quickshell
import Quickshell.Io
import "assets/shell" as D
import "broken/shell" as B
ShellRoot {id:root
 property string captureDir:Quickshell.env("LUMEN_CALENDAR_CAPTURE_DIR")
 property var palettes:__PALETTES__
 property var dates:__DATES__
 property int step:0
 property int stage:0
 property bool painting:false
 property date currentDate:new Date(root.dates[0])
 property var records:[]
 property var recoveries:[]
 property int recoveryStage:0
 property bool copyComplete:false
 FloatingWindow {id:window;visible:true;implicitWidth:600;implicitHeight:190;color:D.Theme.colors.background
  Row {id:frame;x:10;y:10;spacing:20
   D.AppIcon {id:normal;width:128;height:128;calendarDate:root.currentDate;iconSource:Qt.resolvedUrl("assets/shell/reference-icons/calendar.svg")}
   B.AppIcon {id:unavailable;width:128;height:128;calendarDate:root.currentDate;iconSource:Qt.resolvedUrl("broken/shell/reference-icons/calendar.svg")}
   D.AppIcon {id:generic;width:128;height:128;iconSource:Qt.resolvedUrl("absent/application.svg")}
  }
 }
 Process {id:restoreDark;command:["cp",root.captureDir+"/assets/shell/reference-icons/dark/calendar.svg",root.captureDir+"/broken/shell/reference-icons/dark/calendar.svg"];onExited:(code,status)=>{if(code!==0){console.error("CALENDAR_CORPUS_FAILURE restore dark");Qt.exit(1);return;}restoreLight.running=true;}}
 Process {id:restoreLight;command:["cp",root.captureDir+"/assets/shell/reference-icons/calendar.svg",root.captureDir+"/broken/shell/reference-icons/calendar.svg"];onExited:(code,status)=>{if(code!==0){console.error("CALENDAR_CORPUS_FAILURE restore light");Qt.exit(1);return;}root.copyComplete=true;}}
 function recovery(mode,next){root.painting=true;const observed={mode:mode,source:unavailable.source.toString(),failed:unavailable.failed,labels:root.labels(unavailable)};if(!frame.grabToImage(result=>{const path=root.captureDir+"/"+Qt.locale().name+"-recovery-"+mode+".png";if(!result.saveToFile(path)){console.error("CALENDAR_CORPUS_FAILURE recovery frame");Qt.exit(1);return;}observed.capture=path;root.recoveries=root.recoveries.concat([observed]);root.recoveryStage=next;root.painting=false;},Qt.size(frame.width,frame.height)))throw Error("Recovery capture rejected");}
 function labels(icon){return Array.from(icon.children).filter(child=>child.text!==undefined && child.visible).map(child=>({text:child.text,width:child.implicitWidth,height:child.implicitHeight,font:child.font.pixelSize}));}
 Timer {interval:20;repeat:true;running:true;onTriggered:{try {
  if(root.painting)return;
  if(root.step===root.dates.length*2){
   if(root.recoveryStage===0){restoreDark.running=true;root.recoveryStage=1;return;}
   if(root.recoveryStage===1){if(!root.copyComplete)return;unavailable.iconSource=Qt.resolvedUrl("assets/shell/icons/application.svg");root.recoveryStage=2;return;}
   if(root.recoveryStage===2){if(unavailable.status!==Image.Ready || unavailable.variantName!=="")return;unavailable.iconSource=Qt.resolvedUrl("broken/shell/reference-icons/calendar.svg");root.recoveryStage=3;return;}
   if(root.recoveryStage===3){if(unavailable.status!==Image.Ready || unavailable.failed)return;root.recovery("dark",4);return;}
   if(root.recoveryStage===4){B.Theme.preferences={reduceMotion:true,iconAppearance:"light"};root.recoveryStage=5;return;}
   if(root.recoveryStage===5){if(unavailable.status!==Image.Ready || unavailable.failed || unavailable.usingDarkVariant)return;root.recovery("light",6);return;}
   console.log("CALENDAR_CORPUS_RESULT "+JSON.stringify({locale:Qt.locale().name,records:root.records,recoveries:root.recoveries}));Qt.quit();return;
  }
  const mode=root.step%2?"dark":"light",index=Math.floor(root.step/2);
  if(root.stage===0){D.Theme.colors=root.palettes[mode];B.Theme.colors=root.palettes[mode];D.Theme.preferences={reduceMotion:true,iconAppearance:mode};B.Theme.preferences={reduceMotion:true,iconAppearance:mode};root.currentDate=new Date(root.dates[index]);root.stage=1;return;}
  if(normal.status!==Image.Ready || !unavailable.failed || !generic.failed || generic.status!==Image.Ready)return;
  const observed={iso:root.dates[index],mode:mode,labels:root.labels(normal),normalSource:normal.source.toString(),normalDark:normal.usingDarkVariant,unavailableLabels:root.labels(unavailable),unavailableSource:unavailable.source.toString(),unavailableFailed:unavailable.failed,genericLabels:root.labels(generic),genericSource:generic.source.toString()};
  root.painting=true;
  if(!frame.grabToImage(result=>{try {const path=root.captureDir+"/"+Qt.locale().name+"-"+root.step+".png";if(!result.saveToFile(path))throw Error("Frame save failed");observed.capture=path;root.records=root.records.concat([observed]);root.step++;root.stage=0;root.painting=false;}catch(error){console.error("CALENDAR_CORPUS_FAILURE "+error);Qt.exit(1);}},Qt.size(frame.width,frame.height)))throw Error("Frame capture rejected");
 }catch(error){console.error("CALENDAR_CORPUS_FAILURE "+error);Qt.exit(1);}}}
}
