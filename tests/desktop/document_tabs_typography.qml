import QtQuick
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
 property var baselines:({})
 QtObject {id:exported;property var state:({})}
 QtObject {id:stub;property int focusEpoch:0;property var windows:[];property var exportedMenu:exported;function activeFiles(window){}function focusWindow(window){}}
 D.FilesState {id:service;persistViewPreferences:false;path:"__DOCUMENTS__";viewMode:"list"}
 D.FilesWindow {id:files;shell:stub;service:service}
 function check(value,message){if(!value)throw Error(message+" "+JSON.stringify(cases[index]));}
 function walk(item){let result=[item];for(const child of item.children || [])result=result.concat(walk(child));return result;}
 function item(name){return walk(files.contentItem).find(child=>child.objectName===name);}
 function rect(item){const p=item.mapToItem(files.contentItem,0,0);return {x:p.x,y:p.y,width:item.width,height:item.height};}
 Component.onCompleted:service.navigate("__DOCUMENTS__")
 Timer {interval:20;repeat:true;running:true;onTriggered:{try{
  if(audit.painting || !service.ready || service.busy)return;
  if(audit.index===audit.cases.length){console.log("TABS_RESULT "+JSON.stringify(audit.records));Qt.quit();return;}
  const entry=audit.cases[audit.index];
  if(audit.stage===0){D.Theme.colors=audit.palettes[entry.mode];D.Theme.preferences={textScale:entry.scale,reduceMotion:true};files.visible=true;files.implicitWidth=entry.width;files.implicitHeight=entry.height;if(files.tabs.length===1){files.newTab();files.newTab();}audit.stage=1;return;}
  if(audit.stage===1){files.contentItem.Window.window.width=entry.width;files.contentItem.Window.window.height=entry.height;audit.stage=2;return;}
  if(files.width!==entry.width || files.height!==entry.height)return;
  if(audit.stage===2){const active=files.activeTabId;files.reorderTab(active,0);audit.check(files.activeTabId===active,"Reorder preserves active identity");files.closeTabId(files.tabs[files.tabs.length-1].id);audit.check(files.activeTabId===active,"Close sibling preserves active identity");files.newTab();files.switchTab(files.tabs.findIndex(row=>row.id===active));audit.stage=3;return;}
  const strip=audit.item("files-tabs"),tab=audit.item("document-tab-"+files.activeTabId),label=audit.walk(tab).find(child=>child.visible && child.text==="Documents"),close=audit.item("document-tab-close-"+files.activeTabId);audit.check(label,"Actual tab caption");audit.check(label.fontInfo.family===audit.palettes.font,"Public family");audit.check(label.font.pixelSize===Math.round(12*entry.scale),"Caption scales actual="+label.font.pixelSize+" policy="+D.Theme.textScale);audit.check(tab.height>=label.implicitHeight,"Caption height");const key=entry.mode+entry.width,capacity=label.width/label.implicitWidth;if(entry.scale===1)audit.baselines[key]=capacity;else audit.check((label.width+1)/label.implicitWidth>=audit.baselines[key],"Caption capacity preserved");audit.check(close.height<=tab.height && strip.height>=tab.height,"Close and tab fit band");const filter=audit.walk(files.contentItem).find(child=>child.placeholderText==="Filter this folder");if(filter)audit.check(audit.rect(filter).y>=audit.rect(strip).y+strip.height,"Filter below tab band");
  const at=audit.rect(label),paint=Array.from(files.contentItem.children).find(child=>child.visible && child.width===files.width && child.height===files.height);audit.check(paint,"Production paint root");audit.painting=true;
  audit.check(paint.grabToImage(result=>{try{const path="__OUTPUT__/"+audit.index+".png";audit.check(result.saveToFile(path),"Paint capture");audit.records=audit.records.concat([{entry:entry,label:at,color:label.color.toString(),pixelSize:label.font.pixelSize,resolvedFamily:label.fontInfo.family,strip:audit.rect(strip),tabHeight:tab.height,capacity:capacity,capture:path,activeIdentity:files.activeTabId}]);audit.index++;audit.stage=0;audit.painting=false;}catch(error){console.error("TABS_FAILURE "+error);Qt.quit();}},Qt.size(files.width,files.height)),"Grab accepted");
 }catch(error){console.error("TABS_FAILURE "+error);Qt.quit();}}}
}
