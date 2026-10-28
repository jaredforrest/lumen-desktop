import QtQuick
import Quickshell
import Quickshell.Io
import "assets/shell" as D
ShellRoot {
 QtObject {id:stub;property var bars:({});function preferredOutput(){return "HEADLESS-1";}function claimPopup(owner){}}
 D.NotificationState {id:notifications;shell:stub}
 FloatingWindow {visible:true;implicitWidth:440;implicitHeight:300;color:D.Theme.colors.background
  D.NotificationCard {id:card;x:20;y:20;width:400;visible:notifications.rows.length>0;service:notifications;notification:notifications.rows[0] || ({id:0,app:"",summary:"",body:"",icon:"",image:"",actions:[],timestamp:0})}
 }
 IpcHandler {target:"notificationAudit"
  function inject(value:string):void{const frame=JSON.parse(value);if(frame.transport===undefined)frame.transport=String(notifications.transportEpoch);notifications.applyState(JSON.stringify(frame));}
  function status():string{return JSON.stringify({available:notifications.available,transport:notifications.transportEpoch,instance:notifications.instance,revision:notifications.revision,rows:notifications.rows,accessible:card.Accessible.name});}
 }
}
