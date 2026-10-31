import QtQuick
import QtQuick.Controls
import QtQuick.Window
import QtTest
import Quickshell
import "assets/shell" as D
ShellRoot {
 id:audit
 property var palettes:__PALETTES__
 property var records:[]
 property var pane:null
 property var viewport:null
 property var paintRoot:null
 property bool imageSaved:false
 QtObject {id:preferences;property bool available:true;property bool busy:false;property string error:"";property string warning:"";property var preferences:({mode:"dark"});property var desktopPreferences:({});property var windowsPreferences:({});property var notificationsPreferences:({preview:"always"});property var searchPreferences:({});property var appearance:({});property var system:({});property var menuBarPreferences:({});property int writes:0;function refresh(){}function setPreference(k,v){++writes;}function setWindowPreference(k,v){++writes;}}
 QtObject {id:audio;property bool available:false;property bool muted:false;property real volume:0;property var outputs:[];property var inputs:[];property var streams:[];property bool inputAvailable:false}
 QtObject {id:notifications;property bool available:false;property bool dnd:false;property var applications:[];property var history:[];property var policies:({});property string error:""}
 QtObject {id:searchState;property bool available:false;property bool scanning:false;property int indexed:0}
 QtObject {id:inventory;property bool available:true;property string error:"";property var bindings:[{enabled:true,action:"overview",chord:"Mod4+Tab",scope:"global",appId:""}]}
 QtObject {id:shellStub;property var audioState:audio;property var notifications:notifications;property var search:searchState;property var shortcuts:inventory;property var screenSaverPreferences:null;property string volume:"";property var windowGroupPresentation:undefined;property string windowGroupPresentationError:""}
 QtObject {id:editorService;property bool available:true;property bool editable:true;property bool configuredEditable:true;property bool runtimeConfirmed:true;property string runtimeDetail:"";property bool busy:false;property var request:({action:"status"});property string generation:"synthetic-typography";property var bindings:[{id:"window.fullscreen.toggle",action:"window.fullscreen.toggle",defaultChord:"Mod4+Control+f",chord:"Mod4+Control+f",enabled:true,editable:true}];property string boundary:"Owned default-mode symbol bindings only. Raw compositor configuration, runtime changes, keycodes and undeclared application shortcuts are not covered.";property string error:"This disposable presentation cannot apply configuration. Correct the shortcut and review the current generation before trying again.";property string message:"";property int writes:0;function change(){++writes;throw Error("Typography acceptance must never mutate settings");}}
 D.SettingsWindow {id:editor;shell:shellStub;service:preferences}
 TestCase {
  id:keys;parent:editor.contentItem;when:false
  function check(value,message){if(!value)throw Error(message);}
  function waitFor(probe,message){for(let elapsed=0;elapsed<3000;elapsed+=20){if(probe())return;wait(20);}check(false,message);}
  function walk(item,seen){if(!item || seen.includes(item))return [];seen.push(item);let rows=[item];for(const child of item.children || [])rows=rows.concat(walk(child,seen));if(item.contentItem)rows=rows.concat(walk(item.contentItem,seen));return rows;}
  function find(name){return walk(editor.contentItem,[]).find(item=>item.objectName===name);}
  function inside(item,ancestor){while(item){if(item===ancestor)return true;item=item.parent;}return false;}
  function shown(item){while(item && item!==paintRoot){if(!item.visible)return false;item=item.parent;}return item===paintRoot;}
  function rectangle(item){const p=item.mapToItem(paintRoot,0,0);return {x:p.x,y:p.y,width:item.width,height:item.height};}
  function focused(item){return inside(editor.contentItem.Window.window.activeFocusItem,item);}
  function revealed(item){const p=item.mapToItem(viewport.contentItem,0,0);return p.y>=viewport.contentY-1 && p.y+item.height<=viewport.contentY+viewport.height+1 && p.x>=-1 && p.x+item.width<=viewport.width+1;}
  function textGeometry(){return walk(pane,[]).filter(item=>item instanceof Text && shown(item) && item.text && item.font.pixelSize>0).map(item=>{
   check(item.contentWidth<=item.width+2,"Text exceeds its laid out width: "+item.text);
   check(item.contentHeight<=item.height+2,"Text exceeds its laid out height: "+item.text);
   const p=item.mapToItem(pane,0,0);check(p.x>=-2 && p.x+item.width<=pane.width+2,"Text outside pane horizontally: "+item.text);
   return {text:item.text,fontSize:item.font.pixelSize,width:item.width,height:item.height,contentWidth:item.contentWidth,contentHeight:item.contentHeight,color:String(item.color)};
  });}
  function capture(path){imageSaved=false;paintRoot.grabToImage(result=>{imageSaved=result.saveToFile(path);});waitFor(()=>imageSaved,"Settings paint capture failed");}
  function groupRetryAvailability(){
   const retry=walk(editor.contentItem,[]).find(item=>item.text==="Retry group presentation");check(retry,"Actual group retry button missing");
   const cases=[{name:"missing",state:undefined,enabled:false},{name:"null",state:null,enabled:false},{name:"malformed",state:{available:"yes",presentationSupported:true,locked:false},enabled:false},{name:"unavailable",state:{available:false,presentationSupported:true,locked:false},enabled:false},{name:"unsupported",state:{available:true,presentationSupported:false,locked:false},enabled:false},{name:"locked",state:{available:true,presentationSupported:true,locked:true},enabled:false},{name:"supported",state:{available:true,presentationSupported:true,locked:false},enabled:true}];
   for(const c of cases){shellStub.windowGroupPresentation=c.state;wait(1);check(retry.enabled===c.enabled,"Group retry availability mismatch: "+c.name);}
   shellStub.windowGroupPresentation=undefined;wait(1);check(retry.enabled===false,"Retired observer left Retry enabled");
  }
  function runCase(c){
   editor.visible=false;D.Theme.font=palettes.font;D.Theme.colors=palettes[c.mode];D.Theme.preferences={textScale:c.scale,reduceMotion:true,reduceTransparency:true};preferences.preferences={mode:c.mode,textScale:c.scale,reduceMotion:true};
   editor.implicitWidth=c.width;editor.implicitHeight=c.height;
   pane=find("settings-configured-shortcuts");check(pane,"Actual shortcut editor missing");pane.service=editorService;pane.dirty=false;pane.synchronize();
   editor.openPage("Accessibility");editor.navigate("Accessibility","configured-shortcuts");
   waitFor(()=>editor.visible && editor.revealedControl===pane && pane.activeFocus,"Settings did not reveal/focus configured shortcuts");
   viewport=find("settings-content-scroll");paintRoot=find("settings-search").parent;
   check(Math.abs(editor.width-c.width)<2 && Math.abs(editor.height-c.height)<2,"Actual Settings size differs from requested case");
   const targets=["shortcut-editor-action","shortcut-editor-chord","shortcut-editor-apply","shortcut-editor-disable","shortcut-editor-restore","shortcut-editor-discard"];
   const frames=[];
   for(let index=0;index<targets.length;index++){
    const item=find(targets[index]);check(item,"Missing target "+targets[index]);
    for(let attempt=0;attempt<20 && !focused(item);attempt++){keyClick(Qt.Key_Tab);wait(30);}
    waitFor(()=>focused(item) && revealed(item),"Tab focus was not revealed: "+targets[index]);
    if(index===1){keyClick(Qt.Key_End);keyClick("X");check(pane.dirty,"Real typing did not create a disposable draft");}
    const texts=textGeometry();
    const label=item.contentItem && item.contentItem.color!==undefined?item.contentItem:item;
    const frame="__OUTPUT__/"+c.name+"-"+index+".png";capture(frame);
    frames.push({target:targets[index],frame:frame,bounds:rectangle(item),color:String(label.color),scrollY:viewport.contentY,texts:texts});
   }
   keyClick(Qt.Key_Backtab,Qt.ShiftModifier);waitFor(()=>focused(find("shortcut-editor-restore")) && revealed(find("shortcut-editor-restore")),"Reverse traversal was not revealed");
   keyClick(Qt.Key_Tab);waitFor(()=>focused(find("shortcut-editor-discard")),"Discard lost keyboard reachability");keyClick(Qt.Key_Space);waitFor(()=>!pane.dirty,"Keyboard discard did not reset disposable draft");
   check(editorService.writes===0 && preferences.writes===0,"Settings typography changed authoritative state");
   records.push({name:c.name,mode:c.mode,scale:c.scale,width:editor.width,height:editor.height,paneHeight:pane.height,viewportHeight:viewport.height,frames:frames});
  }
  function run(){
   groupRetryAvailability();
   const cases=[];for(const mode of ["light","dark"])for(const scale of [1,1.5])for(const compact of [true,false])cases.push({name:mode+"-"+scale+"-"+(compact?"compact":"regular"),mode:mode,scale:scale,width:compact?480:790,height:compact?360:640});
   for(const c of cases)runCase(c);
   console.log("SHORTCUT_SETTINGS_TYPOGRAPHY_PASS "+JSON.stringify(records));Qt.quit();
  }
 }
 Timer {interval:100;running:true;onTriggered:{try{keys.run();}catch(error){console.error("SHORTCUT_SETTINGS_TYPOGRAPHY_FAIL "+error+" "+error.stack);Qt.exit(1);}}}
 Timer {interval:50000;running:true;onTriggered:{console.error("SHORTCUT_SETTINGS_TYPOGRAPHY_TIMEOUT");Qt.exit(2);}}
}
