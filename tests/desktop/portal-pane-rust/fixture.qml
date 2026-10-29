import QtQuick
import Quickshell
import "@SHELL_URI@" as D
ShellRoot {
 id:test
 property int step:0
 property string oldRevision:""
 D.PortalPermissionState {id:service;active:false}
 D.PortalPermissionsPane {id:pane;width:360;service:service}
 function find(name,item){if(item.objectName===name)return item;for(const child of item.children || []){const found=find(name,child);if(found)return found;}return null;}
 function camera(){return service.facts.rows.find(row=>row.appId==="org.example.First" && row.capability==="Camera");}
 Timer {interval:80;running:true;repeat:true;onTriggered:{try {
  if(service.busy)return;
  if(test.step===0){service.refresh();test.step++;return;}
  if(test.step===1){
   if(!service.facts.available || service.facts.rows.length!==8)throw Error("Provider rows "+service.error);
   if(pane.pending!==null)throw Error("Implicit confirmation");
   const row=camera(),button=find("portal-permission-deny-"+row.id,pane);
   if(!button || !button.Accessible.name.includes(row.appId) || !button.Accessible.name.includes("camera"))throw Error("Action identity");
   button.clicked();
   const text=find("portal-permission-exact-target",pane).text;
   if(!text.includes(row.appId) || !text.includes("Resource: camera") || !text.includes("Allowed"))throw Error("Exact confirmation missing "+text);
   if(camera().decision!=="Allowed" || service.busy)throw Error("Changed before confirmation");
   pane.cancel();if(pane.pending!==null)throw Error("Cancel failed");
   pane.prepare(row,"deny");test.oldRevision=row.revision;find("portal-permission-confirm",pane).clicked();test.step++;return;
  }
  if(test.step===2){
   if(service.error || camera().decision!=="Denied" || !service.message.includes("Existing capture may continue"))throw Error("Deny not confirmed "+service.error);
   if(service.facts.rows.find(row=>row.appId==="org.example.Second" && row.capability==="Camera").decision!=="Denied")throw Error("Sibling changed");
   const stale=JSON.parse(JSON.stringify(camera()));stale.revision=test.oldRevision;pane.prepare(stale,"reset");find("portal-permission-confirm",pane).clicked();test.step++;return;
  }
  if(test.step===3){
   if(!service.error.includes("changed") || service.message)throw Error("Stale action misreported "+service.error);
   console.log("PORTAL_PANE_PASS");Qt.quit();
  }
 }catch(error){console.error("PORTAL_PANE_FAIL "+error);Qt.quit();}}}
}
