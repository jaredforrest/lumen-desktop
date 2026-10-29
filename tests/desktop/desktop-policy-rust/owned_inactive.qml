import QtQuick
import QtQuick.Controls
import QtQuick.Window
import Quickshell
import Quickshell.Io
import "assets/shell" as D
ShellRoot {
 id:audit
 property string kind: Quickshell.env("OWNED_INACTIVE_KIND") || "Files"
 D.OwnedWindowContext {id:context}
 QtObject {id:exported;property var state:({appName:"",title:"",windowId:0})}
 QtObject {id:stub;property var ownedContext:context;property date currentDate:new Date(2026,9,20);property int focusEpoch:0;property var windows:[];property var exportedMenu:exported;property string volume:"Volume: 0.5";property var audioState:({available:false,muted:false,outputs:[],inputs:[],streams:[],inputAvailable:false,sink:null,source:null});property var systemState:({});property var search:({available:false,scanning:false,indexed:0});property var notifications:({dnd:false,available:false,applications:[],policies:{},history:[],error:""});function activeFiles(window){}function focusWindow(window){}function claimPopup(name){}function focusOwnedWindow(name){}function openSettings(name){}}
 D.FilesState {id:service;persistViewPreferences:false;path:Quickshell.env("HOME")+"/Documents";viewMode:"list"}
 D.SettingsState {id:settingsService;active:false;available:true;system:({system:"Private fixture",desktop:"Private fixture"})}
 D.FilesWindow {id:files;shell:stub;service:service}
 D.SettingsWindow {id:settings;shell:stub;service:settingsService;page:"Appearance"}
 D.CalendarWindow {id:calendar;shell:stub}
 property var main:kind==="Files"?files:kind==="Settings"?settings:calendar
 Component.onCompleted:{calendar.calendarState.active=false;calendar.calendarState.loaded=true;service.navigate(service.path);main.visible=true;}
 function walk(item,seen){if(!item || seen.includes(item))return [];seen.push(item);let found=[item];for(const child of Array.from(item.children || []).concat(Array.from(item.resources || [])))found=found.concat(walk(child,seen));if(item.contentItem)found=found.concat(walk(item.contentItem,seen));return found;}
 function field(window){return walk(window.contentItem,[]).find(item=>item.placeholderText===(kind==="Files"?"Filter folder or tag:red":kind==="Settings"?"Search":"Search"));}
 function rect(item,w){const point=item.mapToItem(w.contentItem,0,0);return {x:point.x,y:point.y,width:item.width,height:item.height};}
 IpcHandler {target:"ownedInactive"
  function ready():string {return service.ready && !service.busy?"ready":"pending";}
  function selectDocument():void {files.choose(0,0);}
  function openUtility():void {files.informationWindow.show({name:"Private document.txt",size:"26 bytes",kind:"Text",location:"Private synthetic directory",permissions:"Read and write"});}
  function focusControl(utility:bool):void {const w=utility?files.informationWindow:main,target=utility?walk(w.contentItem,[]).find(item=>item.Accessible.name==="General" && item.background):field(w);if(target)target.forceActiveFocus();}
  function state(utility:bool):string {const w=utility?files.informationWindow:main,native=w.contentItem.Window.window,f=native?native.activeFocusItem:null,items=walk(w.contentItem,[]),controls=items.find(item=>item.controlsHovered!==undefined && item.windowName!==undefined),buttons=controls?walk(controls,[]).filter(item=>item.index!==undefined && item.background && item.modelData!==undefined):[],selected=utility?null:kind==="Files"?items.find(item=>item.text==="Private document.txt"):kind==="Settings"?items.find(item=>item.objectName==="settings-navigation-Appearance"):null,selectedItem=selected?(kind==="Files"?selected.parent:selected.background):null,primary=items.find(item=>item.fontInfo && item.text===(utility?"Private document.txt":kind==="Files"?"Private document.txt":kind==="Settings"?"Appearance":Qt.formatDate(calendar.month,"MMMM yyyy")));return JSON.stringify({kind:audit.kind,utility:utility,title:w.title,visible:w.visible,active:!!native && native.active,width:w.width,height:w.height,dark:D.Theme.dark,opaque:!!D.Theme.preferences.reduceTransparency,selectionActive:utility?null:kind==="Files"?files.selectionActive:kind==="Settings"?settings.selectionActive:null,selectionFill:utility?null:kind==="Files"?String(files.selectionFill):kind==="Settings"?String(settings.selectionFill):null,selectedPaths:service.rows.length?files.selectedPaths:[],selectedPage:settings.page,selectionRect:selectedItem?rect(selectedItem,w):null,primary:primary?{rect:rect(primary,w),color:String(primary.color)}:null,body:String(D.Theme.colors.background),text:String(D.Theme.colors.text),controls:buttons.map(item=>({index:item.index,enabled:item.enabled,color:String(item.background.color),rect:rect(item,w)})),focus:f?{name:f.Accessible.name || f.placeholderText || "",rect:rect(f,w),border:f.background?String(f.background.border.color):null}:null,context:{keyMain:context.keyWindow===files,keyUtility:context.keyWindow===files.informationWindow,mainFiles:context.mainWindow===files,application:context.applicationId,role:context.keyRole}});}
 }
}
