import QtQuick
import QtQuick.Controls
import QtQuick.Window
import Quickshell
import "desktop" as D
ShellRoot {
 id:audit
 property var cases:__CASES__
 property var palettes:__PALETTES__
 property int index:0
 property int stage:0
 property bool painting:false
 property var records:[]
 property var titleCapacity:({})
 property int phase:0
 QtObject {id:stub;property var groupMembershipDrag:null;function claimPopup(name){}}
 D.WindowOverview {id:overview;shell:stub}
 function check(value,message){if(!value)throw Error(message+" "+JSON.stringify(cases[index]));}
 function walk(item,seen){seen=seen || [];if(!item || seen.includes(item))return [];seen.push(item);let result=[item],children=Array.from(item.children || []).concat(Array.from(item.data || []));if(item.contentItem)children.push(item.contentItem);for(const child of children)result=result.concat(walk(child,seen));return result;}
 function shown(item){if(!(item instanceof Item))return false;const same=item.Window.window===overview.contentItem.Window.window;while(item && item!==overview.contentItem){if(!item.visible)return false;item=item.parent;}return item===overview.contentItem || same;}
 function rect(item,relative){const p=item.mapToItem(relative || overview.contentItem,0,0);return {x:p.x,y:p.y,width:item.width,height:item.height};}
 Component.onCompleted:overview.show("")
 Timer {interval:20;repeat:true;running:true;onTriggered:{try{
  if(audit.painting || overview.busy || overview.windows.length!==1)return;
  if(audit.index===audit.cases.length){console.log("OVERVIEW_TEXT_RESULT "+JSON.stringify(audit.records));Qt.quit();return;}
  const entry=audit.cases[audit.index];
  if(audit.stage===0){D.Theme.colors=audit.palettes[entry.mode];D.Theme.preferences={textScale:entry.scale,reduceMotion:true};overview.implicitWidth=entry.width;overview.implicitHeight=entry.height;audit.stage=1;return;}
  if(audit.stage===1){overview.contentItem.Window.window.width=entry.width;overview.contentItem.Window.window.height=entry.height;audit.stage=2;return;}
  if(overview.width!==entry.width || overview.height!==entry.height)return;
  if(entry.scale===1 && audit.phase===0)console.log("OVERVIEW_BASE_METRICS "+JSON.stringify(audit.walk(overview.contentItem).filter(child=>child.placeholderText!==undefined || child.text!==undefined && child.implicitHeight!==undefined).map(child=>({text:child.text,placeholder:child.placeholderText,font:child.font?child.font.pixelSize:null,height:child.height,implicitHeight:child.implicitHeight,width:child.width,implicitWidth:child.implicitWidth}))));
  if(audit.stage===2){const card=audit.walk(overview.contentItem).find(child=>child instanceof Item && child.Accessible && child.Accessible.name.startsWith('A private document title with several words'+" · "));audit.check(card,"Real window card");card.forceActiveFocus();audit.stage=3;return;}
  if(audit.stage===4){const action=audit.walk(overview.contentItem).find(child=>child instanceof Item && child.Accessible && child.Accessible.name==="Assign Application");audit.check(action,"Real assignment action");action.forceActiveFocus();audit.stage=5;return;}
  if(audit.stage===6){const groups=audit.walk(overview.contentItem).find(child=>child.objectName==="window-groups-open");audit.check(groups && groups.enabled,"Real group dialog route");groups.clicked();audit.stage=7;return;}
  if(audit.stage===7){const done=audit.walk(overview.contentItem).find(child=>child instanceof Item && child.Accessible && child.Accessible.name==="Close window groups" && audit.shown(child));audit.check(done,"Opened real dialog content must be observable: "+JSON.stringify(audit.walk(overview.contentItem).filter(child=>child instanceof Popup).map(child=>({type:String(child),visible:child.visible,content:child.contentItem?String(child.contentItem):""}))));done.forceActiveFocus();audit.stage=8;return;}
  const labels=audit.walk(overview.contentItem).filter(child=>audit.shown(child) && child.font && child.text),title=labels.find(child=>child.text==="A private document title with several words"),metadata=labels.find(child=>child.text==="Private desktop" && child.font.pixelSize===Math.round(11*entry.scale));audit.check(title,"Actual title");audit.check(title.fontInfo.family===audit.palettes.font,"Public family");audit.check(title.font.pixelSize===Math.round(12*entry.scale),"Title label role");audit.check(metadata,"Desktop metadata role");audit.check(title.height+0.01>=title.implicitHeight,"Title lines fit");const key=entry.mode+entry.width;if(audit.phase===0){if(entry.scale===1)audit.titleCapacity[key]=title.width/title.implicitWidth;else audit.check((title.width+1)/title.implicitWidth>=audit.titleCapacity[key],"Title capacity");}const r=audit.rect(title),m=audit.rect(metadata);audit.check(r.y+r.height<=m.y+1,"Title and desktop do not overlap");
  const focus=overview.contentItem.Window.window.activeFocusItem;audit.check(focus,"Actual focused control");let target=focus;while(target && !target.Accessible.name)target=target.parent;audit.check(target,"Named focused control");const focused=audit.rect(target);audit.check(focused.x>=-1 && focused.y>=-1 && focused.x+focused.width<=overview.width+1 && focused.y+focused.height<=overview.height+1,"Focused control inside viewport");for(let parent=target.parent;parent && parent!==overview.contentItem;parent=parent.parent)if(parent.clip){const clip=audit.rect(parent);audit.check(focused.y>=clip.y-1 && focused.y+focused.height<=clip.y+clip.height+1,"Focused control revealed through clip");}
  if(audit.phase===2){const description=labels.find(child=>child.text.startsWith("Groups keep window placement intact."));audit.check(description && description.font.pixelSize===Math.round(11*entry.scale),"Dialog helper metadata role");audit.check(description.height>=description.implicitHeight,"Dialog helper wraps");}
  const paintedLabel=audit.phase===0?title:audit.walk(target).find(child=>child.text && child.font && child.color && audit.shown(child));audit.check(paintedLabel,"Actual focused action text");
  const paint=audit.phase===2?target.parent:Array.from(overview.contentItem.children).find(child=>child.visible && child.width===overview.width && child.height===overview.height);audit.check(paint,"Production paint root");audit.painting=true;
  audit.check(paint.grabToImage(result=>{try{const path="__OUTPUT__/"+audit.index+"-"+audit.phase+".png";audit.check(result.saveToFile(path),"Capture");audit.records=audit.records.concat([{entry:entry,title:r,color:title.color.toString(),font:title.font.pixelSize,metadata:m,capture:path,paintedText:audit.rect(paintedLabel,paint),paintedColor:paintedLabel.color.toString(),phase:audit.phase,focused:focused,focusName:target.Accessible.name}]);if(audit.phase===0){audit.phase=1;audit.stage=4;}else if(audit.phase===1){audit.phase=2;audit.stage=6;}else{const done=audit.walk(overview.contentItem).find(child=>child instanceof Item && child.Accessible && child.Accessible.name==="Close window groups" && audit.shown(child));done.clicked();audit.phase=0;audit.index++;audit.stage=0;}audit.painting=false;}catch(error){console.error("OVERVIEW_TEXT_FAILURE "+error);Qt.quit();}},Qt.size(paint.width,paint.height)),"Grab accepted");
 }catch(error){console.error("OVERVIEW_TEXT_FAILURE "+error);Qt.quit();}}}
}
