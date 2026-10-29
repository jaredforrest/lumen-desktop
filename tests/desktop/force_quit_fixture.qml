import QtQuick
import Quickshell
import "."
ShellRoot {id:mock
 property string popupOwner:""
 signal dismissPopups(string owner)
 function preferredOutput(){return "";}
 function claimPopup(owner){popupOwner=owner;dismissPopups(owner);}
 ForceQuitDialog {id:dialog;shell:mock}
 Timer {property int stage:0;interval:30;running:true;repeat:true
  onTriggered:{try {
   if(stage===0){dialog.show("Old",[{appId:"old",title:"slow"}]);stage++;}
   else if(stage===1){dialog.show("New",[{appId:"new",title:"fast"}]);stage++;}
   else if(stage===2 && dialog.phase==="ready"){
    if(dialog.applicationName!=="New" || dialog.token!=="00000000000000000000000000000002")throw Error("Old token inherited");
    dialog.cancel();dialog.show("Final",[{appId:"final",title:"fast"}]);stage++;
   }else if(stage===3 && dialog.phase==="ready"){
    if(dialog.token!=="00000000000000000000000000000004")throw Error("Cancelled token inherited");
    dialog.confirm();stage++;
   }else if(stage===4 && dialog.phase==="closed") {console.log("PASS Force Quit selection identity and explicit confirmation");Qt.quit();}
   if(dialog.phase==="error")throw Error(dialog.error);
  }catch(error){console.error("FAIL "+error);Qt.quit();}}
 }
}