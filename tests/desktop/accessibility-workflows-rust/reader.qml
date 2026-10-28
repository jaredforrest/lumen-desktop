import QtQuick
import Quickshell
import Quickshell.Io
import "file:@@SOURCE@@" as D
ShellRoot {id:root;property var launched:[];property var finalActions:[]
 QtObject {id:searchService;property var files:[];property var preview:({});property bool searching:false;property bool opening:false;property bool scanning:false;property int indexed:0;property bool limited:false;property bool unreadable:false;property bool more:false;property string error:'';signal opened();signal browseRequested(string path);function search(value){}function requestPreview(value){}}
 QtObject {id:stub;property var search:searchService;property var entries:[{id:'fixture.alpha',name:'Sentinel Alpha',genericName:'First result',noDisplay:false},{id:'fixture.beta',name:'Sentinel Beta',genericName:'Second result',noDisplay:false}];property var notifications:({dnd:false,available:false});property var audioState:({muted:false,available:false});property var exportedMenu:({state:{menu:null},label:function(value){return value;}});property date currentDate:new Date();function activate(entry,newWindow){root.launched=root.launched.concat([entry.id]);}function appLabel(entry){return entry.name;}function appIcon(entry){return '';}}
 QtObject {id:notices;property bool available:true;property bool dnd:false;property string error:'';property var history:[];property var groups:[];property var expandedCards:({});function command(value){}function invoke(notification,action){}function toggleExpanded(id){}}
 QtObject {id:lifecycle;property bool available:false;property var session:({phase:'idle',token:'',error:''});signal sessionAuthorized(var value);function sessionCommand(action,value){}}
 FloatingWindow {id:main;title:'Reader fixture';visible:true;implicitWidth:950;implicitHeight:850
  D.AppLauncher {id:launcher;anchors.fill:parent;shell:stub;onCloseRequested:main.visible=false}
  D.NotificationCentre {id:centre;visible:false;width:600;service:notices}
 }
 D.SessionOptions {id:modal;executeActions:false;lifecycle:lifecycle;onConfirmed:action=>root.finalActions=root.finalActions.concat([action])}
 function note(id,title){return {id:id,app:'Sentinel Inbox',summary:title,body:id===1?'Arrival details: Sentinel Orchard.':'Delivery details: Sentinel Harbour.',icon:'',image:'',desktop:'',timestamp:Date.now(),actions:[]};}
 IpcHandler {target:'readerFixture'
  function state():string{return JSON.stringify({visible:main.visible,query:launcher.queryText,selectionQuery:launcher.selectionQuery,selected:launcher.selectedKey,results:launcher.results.map(row=>row.id),launched:root.launched,finalActions:root.finalActions,modal:modal.visible,error:modal.error});}
  function search():void{main.visible=true;centre.visible=false;launcher.visible=true;launcher.setProvider('Applications');launcher.focusSearch();}
  function update():void{stub.entries=stub.entries.concat([{id:'fixture.gamma',name:'Sentinel Gamma',genericName:'Late result',noDisplay:false}]);}
  function remove():void{stub.entries=stub.entries.filter(entry=>entry.id!=='fixture.alpha');}
  function notifications():void{launcher.visible=false;centre.visible=true;const rows=[root.note(1,'Sentinel First'),root.note(2,'Sentinel Second')];notices.history=rows;notices.groups=[{key:'inbox',rows:rows}];}
  function modalShow():void{modal.show('reboot');}
  function restore():void{main.visible=true;launcher.visible=true;centre.visible=false;launcher.focusSearch();}
 }
}