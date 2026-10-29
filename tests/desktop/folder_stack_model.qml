import QtQuick
import Quickshell
import "FolderStackModel.js" as Model
ShellRoot {Timer {running:true;interval:10;onTriggered:{try {
 const facts=["1152921504606846977","1152921504606846979","33188","4","-1","1152921504606846981"];
 const a={path:"/fixture/a",actionFacts:facts},b={path:"/fixture/b",actionFacts:facts.slice()};
 const captured=Model.capture([a,b],a,2,"/fixture");
 function check(ok,message){if(!ok)throw Error(message);}
 check(captured && captured.facts[0]===facts[0],"lossless version");
 check(Model.valid(captured,[b,a],2,"/fixture",null),"same row reorder retargeted");
 check(!Model.valid(captured,[a,b],3,"/fixture",null),"reload resurrection");
 check(!Model.valid(captured,[a,b],2,"/other",null),"folder retarget");
 check(!Model.valid(captured,[{path:a.path,actionFacts:facts}],2,"/fixture",null),"replacement inherited row");
 a.actionFacts=facts.slice();a.actionFacts[1]="1152921504606846980";
 check(!Model.valid(captured,[a,b],2,"/fixture",null),"mutated facts");
 const footer=Model.captureFolder("/fixture",facts,4);
 check(Model.valid(footer,[],4,"/fixture",facts),"footer rejected");
 check(!Model.valid(footer,[],4,"/fixture",a.actionFacts),"footer source changed");
 check(Model.capture([a],b,1,"/fixture")===null,"unlisted row capture");
 console.log("PASS stack captured row/generation/folder/version, footer and uint64 strings");Qt.quit();
 }catch(e){console.error("FAIL "+e);Qt.exit(1);}}}}
