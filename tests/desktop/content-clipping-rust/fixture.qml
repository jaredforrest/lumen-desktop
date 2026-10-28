import QtQuick
import QtQuick.Controls
import QtQuick.Window
import Quickshell
import Quickshell.Io
import "assets/shell" as D
ShellRoot {id:fixture
 QtObject {id:service
  property var menuBarPreferences:D.Theme.menuBarPreferences
  property var preferences:D.Theme.preferences
  property var desktopPreferences:D.Theme.desktopPreferences
  property var windowsPreferences:D.Theme.windowsPreferences
  property var notificationsPreferences:({preview:"always"})
  property var searchPreferences:({disabledProviders:[]})
  property var appearance:({})
  property var system:({system:"Fixture",desktop:"Fixture"})
  property bool available:true;property bool busy:false;property bool applying:false
  property string error:"";property string warning:""
  property int edits:0
  function refresh(){}
  function setPreference(key,value){edits++;}
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
  property var systemState:({});property var vpn:({})
 }
 FloatingWindow {id:aux;visible:false;title:"System Settings";implicitWidth:1200;implicitHeight:900;minimumSize:Qt.size(200,100);color:"#7755aa"}
 D.WindowGeometry {id:auxGeometry;ownerWindow:aux}
 D.SettingsWindow {id:settings;service:service;shell:stub;visible:true;page:"Accessibility"}
 property var surface:null
 property var declaredMask:null
 property var sentinel:null
 property var realHover:null
 Component {id:viewportHover;HoverHandler {}}
 function visit(item,fn){if(fn(item))return item;for(const child of item.children || []){const found=visit(child,fn);if(found)return found;}return null;}
 function rect(item){const point=item.mapToItem(settings.contentItem,0,0);return {x:point.x,y:point.y,width:item.width,height:item.height};}
 Component {id:edgeContent
  Flickable {id:scroll;property bool pointerInside:hover.hovered;property point pointerPosition:hover.point.position;HoverHandler {id:hover}
   anchors.fill:parent;interactive:false;contentWidth:width*3;contentHeight:height*3;clip:true;boundsBehavior:Flickable.StopAtBounds
   Repeater {model:400
    Rectangle {required property int index;x:(index%20)*128;y:Math.floor(index/20)*128;width:128;height:128;color:["#ff00ff","#00ff00","#00ffff"][(index+Math.floor(index/20)*2)%3]}
   }
  }
 }
 function prepare(){
  const controls=visit(settings.contentItem,item=>item.windowName==="System Settings");
  if(!controls)return false;
  surface=controls.parent;
  declaredMask=visit(settings.contentItem,item=>item!==surface && item.radius!==undefined && item.visible===false && item.width===settings.width && item.height===settings.height);
  for(const child of surface.children || [])if((child.children || []).length>0 || child.acceptedButtons!==undefined)child.z=2;
  sentinel=edgeContent.createObject(surface);const viewport=visit(settings.contentItem,item=>item.objectName==="settings-content-scroll");realHover=viewportHover.createObject(viewport);return !!sentinel && !!realHover;
 }
 IpcHandler {target:"clipping"
  function ready():bool{return !!fixture.sentinel || fixture.prepare();}
  function scroll(bottom:bool):void {fixture.sentinel.contentX=bottom?fixture.sentinel.contentWidth-fixture.sentinel.width:0;fixture.sentinel.contentY=bottom?fixture.sentinel.contentHeight-fixture.sentinel.height:0;}
  function auxiliary(title:string,captured:int):void {aux.title=title;auxGeometry.containerId=captured;aux.visible=true;}
  function hideAuxiliary():void {aux.visible=false;}
  function fitAuxiliary():void {auxGeometry.fit(500,400);}
  function fitRepeated():void {for(let i=0;i<5;i++)settings.fitScreen();}
  function resetViewport():void {const viewport=fixture.visit(settings.contentItem,item=>item.objectName==="settings-content-scroll");viewport.cancelFlick();viewport.contentY=0;}
  function state():string {const geometry=fixture.visit(settings.contentItem,item=>item.ownerWindow===settings && item.completedRequests!==undefined);const controls=fixture.visit(settings.contentItem,item=>item.windowName==="System Settings");const close=controls?fixture.visit(controls,item=>item.index===0 && item.pressed!==undefined):null;const viewport=fixture.visit(settings.contentItem,item=>item.objectName==="settings-content-scroll");const search=fixture.visit(settings.contentItem,item=>item.placeholderText==="Search");return JSON.stringify({auxiliary:{visible:aux.visible,width:aux.width,height:aux.height,busy:auxGeometry.busy,result:auxGeometry.result,error:auxGeometry.error,completed:auxGeometry.completedRequests},fit:geometry?{busy:geometry.busy,result:geometry.result,error:geometry.error,completed:geometry.completedRequests}:null,screen:settings.screen?settings.screen.name:"",closeControl:close?fixture.rect(close):null,viewport:viewport?{rect:fixture.rect(viewport),y:viewport.contentY,contentHeight:viewport.contentHeight,hovered:fixture.realHover?fixture.realHover.hovered:false,pointerX:fixture.realHover?fixture.realHover.point.position.x:0,pointerY:fixture.realHover?fixture.realHover.point.position.y:0}:null,visible:settings.visible,width:settings.width,height:settings.height,graphicsApi:settings.contentItem.GraphicsInfo.api,software:settings.contentItem.GraphicsInfo.api===GraphicsInfo.Software,native:D.Theme.nativeWindowFrame,maskActive:fixture.surface?fixture.surface.layer.enabled:false,maskRadius:fixture.declaredMask?fixture.declaredMask.radius:null,surface:fixture.surface?fixture.rect(fixture.surface):null,sentinel:fixture.sentinel?fixture.rect(fixture.sentinel):null,scroll:fixture.sentinel?{x:fixture.sentinel.contentX,y:fixture.sentinel.contentY,hovered:fixture.sentinel.pointerInside,pointerX:fixture.sentinel.pointerPosition.x,pointerY:fixture.sentinel.pointerPosition.y}:null,search:search?fixture.rect(search):null,query:search?search.text:"",searchFocused:search?search.activeFocus:false,edits:service.edits});}
 }
}