import QtQuick
import QtQuick.Controls
import QtTest
import "assets/shell" as D
ApplicationWindow {
 id:window;width:WIDTH;height:760;visible:true;color:D.Theme.colors.background
 property bool imageSaved:false
 function check(value,message){if(!value)throw Error(message || "Assertion failed");}
 function equal(a,b){check(a===b,"Expected "+b+", got "+a);}
 Rectangle {id:frame;anchors.fill:parent;color:D.Theme.colors.background
  D.ShortcutEditorPane {id:pane;width:window.width-24;x:12;y:12;active:true}
 }
 TestCase {
  id:test;name:"ActualShortcutEditor";when:false
  function find(item,name){if(item.objectName===name)return item;for(const child of item.children || []){const value=find(child,name);if(value)return value;}return null;}
  function waitUntil(f){for(let elapsed=0;elapsed<6000;elapsed+=20){if(f())return;wait(20);}check(false,"Shortcut state deadline");}
  function typeChord(value){const field=find(pane,"shortcut-editor-chord");mouseClick(field,field.width/2,field.height/2);keyClick(Qt.Key_A,Qt.ControlModifier);for(let c of value)keyClick(c);equal(field.text,value);check(pane.dirty);}
  function click(name){const item=find(pane,name);check(item.enabled,name+" disabled");mouseClick(item,item.width/2,item.height/2);}
  function test_actual(){
   waitUntil(()=>pane.service.available && !pane.service.busy);equal(pane.service.editable,EDITABLE);
   if(!EDITABLE){check(!find(pane,"shortcut-editor-chord").enabled);check(!find(pane,"shortcut-editor-apply").enabled);check(!find(pane,"shortcut-editor-disable").enabled);check(!find(pane,"shortcut-editor-restore").enabled);console.log("SHORTCUT_EDITOR_READONLY_PASS");return;}
   const selector=find(pane,"shortcut-editor-action");mouseClick(selector,selector.width/2,selector.height/2);keyClick(Qt.Key_Escape);selector.forceActiveFocus();keyClick(Qt.Key_Home);
   const index=pane.service.bindings.findIndex(row=>row.id==="window.fullscreen.toggle");check(index>=0);for(let i=0;i<index;i++)keyClick(Qt.Key_Down);equal(pane.row.id,"window.fullscreen.toggle");
   typeChord("Mod4+Control+x");click("shortcut-editor-apply");waitUntil(()=>!pane.service.busy && pane.service.message.length>0);equal(pane.row.chord,"Mod4+Control+x");check(pane.service.message.includes("confirmed"));
   typeChord("Super+Ctrl+Shift+F");click("shortcut-editor-apply");waitUntil(()=>!pane.service.busy && pane.service.error.length>0);equal(pane.row.chord,"Mod4+Control+x");equal(pane.service.message,"");
   click("shortcut-editor-disable");waitUntil(()=>!pane.service.busy && !pane.row.enabled);equal(find(pane,"shortcut-editor-chord").text,"");
   click("shortcut-editor-restore");waitUntil(()=>!pane.service.busy && pane.row.enabled && pane.row.chord===pane.row.defaultChord);check(pane.service.message.includes("confirmed"));
   check(pane.height<=window.height-24);for(const name of ["shortcut-editor-apply","shortcut-editor-disable","shortcut-editor-restore","shortcut-editor-discard"]){const item=find(pane,name),at=item.mapToItem(window.contentItem,0,0);check(at.x>=0 && at.x+item.width<=window.width,name+" clipped");}
   equal(selector.currentIndex,pane.service.bindings.findIndex(row=>row.id==="window.fullscreen.toggle"));
   equal(selector.displayText,"Toggle full screen · Super+Ctrl+F");
   frame.grabToImage(result=>{window.imageSaved=result.saveToFile("SCREENSHOT");});waitUntil(()=>window.imageSaved);console.log("SHORTCUT_EDITOR_ACTUAL_PASS");
  }
 }
 Timer {interval:100;running:true;onTriggered:{try{test.test_actual();Qt.quit();}catch(error){console.error("SHORTCUT_EDITOR_FAILED "+error+" "+error.stack);Qt.exit(1);}}}
 Timer {interval:18000;running:true;onTriggered:{console.error("SHORTCUT_EDITOR_TIMEOUT");Qt.exit(2);}}
}
