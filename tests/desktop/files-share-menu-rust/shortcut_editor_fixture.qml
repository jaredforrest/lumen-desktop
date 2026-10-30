import QtQuick
import QtQuick.Controls
import QtTest
import "assets/shell" as D
ApplicationWindow {
 id:window;width:WIDTH;height:760;visible:true
 D.ShortcutEditorPane {id:pane;width:window.width-24;x:12;y:12;active:true}
 TestCase {
  id:test;name:"ActualShortcutEditor";when:windowShown
  function find(item,name){if(item.objectName===name)return item;for(const child of item.children || []){const value=find(child,name);if(value)return value;}return null;}
  function waitUntil(f){tryVerify(f,6000);}
  function typeChord(value){const field=find(pane,"shortcut-editor-chord");mouseClick(field,field.width/2,field.height/2);keyClick(Qt.Key_A,Qt.ControlModifier);for(let c of value){const shifted=c>="A"&&c<="Z";keyClick(c.toUpperCase().charCodeAt(0),shifted?Qt.ShiftModifier:Qt.NoModifier);}compare(field.text,value);verify(pane.dirty);}
  function click(name){const item=find(pane,name);verify(item.enabled,name+" disabled");mouseClick(item,item.width/2,item.height/2);}
  function test_actual(){
   waitUntil(()=>pane.service.available && !pane.service.busy);compare(pane.service.editable,EDITABLE);
   if(!EDITABLE){verify(!find(pane,"shortcut-editor-chord").enabled);verify(!find(pane,"shortcut-editor-apply").enabled);verify(!find(pane,"shortcut-editor-disable").enabled);verify(!find(pane,"shortcut-editor-restore").enabled);console.log("SHORTCUT_EDITOR_READONLY_PASS");return;}
   const selector=find(pane,"shortcut-editor-action");mouseClick(selector,selector.width/2,selector.height/2);keyClick(Qt.Key_Escape);selector.forceActiveFocus();keyClick(Qt.Key_Home);
   const index=pane.service.bindings.findIndex(row=>row.id==="window.fullscreen.toggle");verify(index>=0);for(let i=0;i<index;i++)keyClick(Qt.Key_Down);compare(pane.row.id,"window.fullscreen.toggle");
   typeChord("Mod4+Control+x");click("shortcut-editor-apply");waitUntil(()=>!pane.service.busy && pane.service.message.length>0);compare(pane.row.chord,"Mod4+Control+x");verify(pane.service.message.includes("confirmed"));
   typeChord("Super+Ctrl+Shift+F");click("shortcut-editor-apply");waitUntil(()=>!pane.service.busy && pane.service.error.length>0);compare(pane.row.chord,"Mod4+Control+x");compare(pane.service.message,"");
   click("shortcut-editor-disable");waitUntil(()=>!pane.service.busy && !pane.row.enabled);compare(find(pane,"shortcut-editor-chord").text,"");
   click("shortcut-editor-restore");waitUntil(()=>!pane.service.busy && pane.row.enabled && pane.row.chord===pane.row.defaultChord);verify(pane.service.message.includes("confirmed"));
   verify(pane.height<=window.height-24);for(const name of ["shortcut-editor-apply","shortcut-editor-disable","shortcut-editor-restore","shortcut-editor-discard"]){const item=find(pane,name),at=item.mapToItem(window.contentItem,0,0);verify(at.x>=0 && at.x+item.width<=window.width,name+" clipped");}
   const image=grabImage(window.contentItem);verify(image.save("SCREENSHOT"));console.log("SHORTCUT_EDITOR_ACTUAL_PASS");
  }
  function cleanupTestCase(){Qt.quit();}
 }
 Timer {interval:18000;running:true;onTriggered:{console.error("SHORTCUT_EDITOR_TIMEOUT");Qt.exit(2);}}
}
