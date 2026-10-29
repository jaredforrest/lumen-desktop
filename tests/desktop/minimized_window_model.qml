import QtQuick
import Quickshell
import "."
import "DockModel.js" as Model
import "DockInteraction.js" as Input
ShellRoot {id:fixture
 property var preferences:({destination:"windows"})
 QtObject {id:a;property string title:"Same title";property bool minimized:true;property int activations:0;function activate(){activations++;}}
 QtObject {id:b;property string title:"Same title";property bool minimized:true;property int activations:0;function activate(){activations++;}}
 QtObject {id:replacement;property string title:"Same title";property bool minimized:true;property int activations:0;function activate(){activations++;}}
 MinimizedWindowState {id:state;enabled:fixture.preferences.destination==="windows";windows:[a,b]}
 property var old:null
 property int step:0
 function check(ok,message){if(!ok)throw Error(message);}
 Timer {interval:40;repeat:true;running:true;onTriggered:{try {
 if(step===0){check(state.items.length===2,"two minimized handles");old=state.items[0];check(old.key!==state.items[1].key,"same-title collision");check(state.activate(state.items[1]) && b.activations===1 && a.activations===0,"retargeted activation");a.minimized=false;step++;return;}
 if(step===1){check(state.records.length===1 && state.items.length===1 && !state.valid(old) && !state.activate(old),"restored resource replay");a.minimized=true;step++;return;}
 if(step===2){check(state.items.length===2,"minimized property binding");old=state.items[0];a.minimized=false;a.minimized=true;check(!state.activate(old),"same-turn minimize epoch resurrection");old=state.items[0];fixture.preferences={destination:"application"};fixture.preferences={destination:"windows"};check(!state.activate(old),"same-turn mode epoch resurrection");old=state.items[0];state.windows=[replacement,b];step++;return;}
 if(step===3){check(!state.activate(old) && replacement.activations===0,"replacement inherited capability");fixture.preferences={destination:"application"};check(!state.activate(old) && state.items.length===0,"disabled destination action");fixture.preferences={destination:"windows"};const current=state.items[0];check(current.key!==old.key,"retired token reused");check(Input.releaseTarget(state.items,old.key,0)===null,"stale pointer retargeted");const entry={id:"app",name:"App",icon:""};const app=Model.build([], [entry],[entry],[],[],false,[],[]);check(app.length===1,"application default changed");const separate=Model.build([], [entry],[entry],[],[],false,[],[],state.items);check(separate.length===3 && separate.filter(x=>x.action==="minimized-window").length===2,"window destination missing");state.windows=[];step++;return;}
 if(step===4){check(state.items.length===0,"retired entries retained");console.log("PASS retained minimized capabilities, live bindings, stale pointer/replacement refusal and application default");Qt.quit();}
 }catch(e){console.error("FAIL "+e);Qt.exit(1);}}}
}
