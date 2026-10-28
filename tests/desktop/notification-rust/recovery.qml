import QtQuick
import QtQuick.Controls
import Quickshell
import Quickshell.Io
import "assets/shell" as D
ShellRoot {
 id:root;property int probes:0
 D.WidgetState {id:widgets}
 QtObject {id:stub;property var bars:({});function preferredOutput(){return "HEADLESS-1";}function claimPopup(owner){}}
 D.NotificationState {id:notifications;shell:stub}
 FloatingWindow {visible:true;title:"Recovery notification";implicitWidth:440;implicitHeight:300;color:D.Theme.colors.background
  Button {id:probe;x:20;y:250;text:"Recovery probe";onClicked:root.probes++}
  D.NotificationCard {id:card;x:20;y:20;width:400;visible:notifications.rows.length>0;service:notifications;notification:notifications.rows[0] || ({id:0,app:"",summary:"",body:"",icon:"",image:"",actions:[],timestamp:0})}
 }
 IpcHandler {target:"recoveryAudit"
  function status():string{return JSON.stringify({available:notifications.available,error:notifications.error,transportEpoch:notifications.transportEpoch,instance:notifications.instance,revision:notifications.revision,rows:notifications.rows,layout:widgets.preferences,probes:root.probes,lease:notifications.commandUntil});}
  function activate():void{card.activate();}
  function activateWithLease(value:string):void{const current=notifications.commandUntil;notifications.commandUntil=Number(value);card.activate();notifications.commandUntil=current;}
  function focusProbe():void{probe.forceActiveFocus();}
  function configure():void{widgets.addWidget("clock");const item=widgets.desktopItems[0];widgets.positionWidget(item.id,"HEADLESS-1",0.25,0.4);}
 }
}
