import QtQuick
import QtQuick.Window
import Quickshell
import Quickshell.Io
import "assets/shell" as D
ShellRoot {
 id:fixture
 QtObject {id:service
  property var menuBarPreferences:D.Theme.menuBarPreferences
  property var preferences:D.Theme.preferences
  property var desktopPreferences:D.Theme.desktopPreferences
  property var windowsPreferences:D.Theme.windowsPreferences
  property var notificationsPreferences:({preview:"always"})
  property var searchPreferences:({disabledProviders:[]})
  property var appearance:({})
  property var system:({system:"Fixture",desktop:"Fixture"})
  property bool available:true
  property bool busy:false
  property bool applying:false
  property string error:""
  property string warning:""
  function refresh(){}
  function setPreference(key,value){const next=Object.assign({},preferences);next[key]=value;preferences=next;D.Theme.preferences=next;}
  function setMenuBarPreference(key,value){}
  function setDesktopPreference(key,value){}
  function setWindowPreference(key,value){}
  function setNotificationPreference(key,value){}
  function setSearchPreference(key,value){}
 }
 QtObject {id:stub
  property string volume:"Volume: 0.5"
  property var audioState:({available:false,muted:false,outputs:[],inputs:[],streams:[],inputAvailable:false,sink:null,source:null})
  property var notifications:({dnd:false,available:false,applications:[],policies:{},history:[],error:""})
  property var search:({available:false,scanning:false,indexed:0})
  property var systemState:({})
  property var vpn:({})
 }
 D.SettingsWindow {id:settings;service:service;shell:stub;visible:true;page:"Accessibility";implicitWidth:480;implicitHeight:320}
 function find(name,item){if(item.objectName===name)return item;for(const child of item.children || []){const result=find(name,child);if(result)return result;}return null;}
 function rectangle(item){const at=item.mapToItem(settings.contentItem,0,0);return {x:at.x,y:at.y,width:item.width,height:item.height};}
 IpcHandler {target:"filesAudit"
  function state():string {const scroll=fixture.find("settings-content-scroll",settings.contentItem);const focus=settings.contentItem.Window.window.activeFocusItem;return JSON.stringify({ready:settings.visible,scale:D.Theme.textScale,width:settings.width,height:settings.height,motion:!!service.preferences.reduceMotion,focus:focus?focus.objectName:"",focused:focus?fixture.rectangle(focus):null,viewport:fixture.rectangle(scroll),scrollY:scroll.contentY,contentHeight:scroll.contentHeight});}
  function navigate(name:string):void {settings.navigate(name);}
  function focus(name:string):void {fixture.find(name,settings.contentItem).forceActiveFocus();}
  function geometry(name:string):string {const item=fixture.find(name,settings.contentItem);return item?JSON.stringify(fixture.rectangle(item)):"null";}
 }
}