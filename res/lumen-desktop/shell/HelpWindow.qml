import QtQuick
import QtQuick.Controls
import QtQuick.Window
import Quickshell
FloatingWindow {
 id:root;title:"Desktop Help";visible:false;color:"transparent"
 implicitWidth:650;implicitHeight:520;minimumSize:Qt.size(400,320)
 required property var shell
 property string query:""
 readonly property var topics:[
  {title:"Find an app or file",body:"Open Search from the menu-bar magnifying glass. Type a name, use the arrow keys to select a result, then press Return.",action:"search"},
  {title:"Use the Dock",body:"Click an icon to open or focus an app. Right-click to see its windows. Shift-click opens a new window where supported.",action:"dock"},
  {title:"Manage windows",body:"Red closes the window. Yellow minimizes it to the Dock. Green fills or restores its size; use the Window menu for Full Screen.",action:"overview"},
  {title:"Browse and preview files",body:"Open Files, choose a file, then press Space for Quick Look. Escape closes the preview. The File menu provides file actions.",action:"files"},
  {title:"Calendar and notifications",body:"Click the menu-bar clock or press Super+N. Use Edit Widgets to choose the calendar layout. Open Calendar for editing.",action:"calendar"},
  {title:"Change appearance",body:"System Settings contains Appearance, Desktop & Dock and Accessibility. Reduce Motion and automatic scroll bars apply to desktop controls.",action:"settings"},
  {title:"Remote keyboard shortcuts",body:"Super means your Windows key. Your local computer or VNC viewer may intercept it. Configure your viewer to send special keys; the menu commands remain available.",action:""},
  {title:"Application menus",body:"Apps that export their menus show commands in the menu bar. Other apps keep their own menus; their app-specific Help remains inside the app.",action:""}
 ]
 readonly property var matches:topics.filter(t=>(t.title+" "+t.body).toLowerCase().includes(query.toLowerCase()))
 function show(){visible=true;Qt.callLater(()=>{search.forceActiveFocus();if(body.Window.window)body.Window.window.requestActivate();});}
 function openTopic(action){if(!action)return;visible=false;if(action==="search")shell.launchApps();else if(action==="files")shell.openFiles("");else if(action==="overview")shell.openOverview(false);else if(action==="calendar")shell.showMenu("Date");else if(action==="settings")shell.openSettings("Appearance");else if(action==="dock")shell.requestDockFocus(shell.preferredOutput());}
 Rectangle {id:body;anchors.fill:parent;color:Theme.colors.background;radius:Theme.nativeWindowFrame?0:12
  MouseArea {visible:!Theme.nativeWindowFrame;width:parent.width;height:48;onPressed:body.Window.window.startSystemMove()}
  WindowControls {windowActive:body.Window.window?body.Window.window.active:false;windowName:"Desktop Help";canFullscreen:false;onTriggered:index=>{if(index===0)root.visible=false;else if(index===1)body.Window.window.showMinimized();}}
  PanelLabel {x:100;y:14;visible:!Theme.nativeWindowFrame;text:"Desktop Help";font.weight:Font.DemiBold}
  DesktopTextField {id:search;x:20;y:Theme.nativeWindowFrame?14:58;width:parent.width-40;height:34;placeholderText:"Search Help";placeholderTextColor:Theme.colors.muted;Accessible.name:"Search Help";color:Theme.colors.text;font.family:Theme.font;onTextChanged:root.query=text;background:Rectangle {radius:7;color:Theme.colors.field;border.color:search.activeFocus?Theme.colors.accent:Theme.hairline}}
  ListView {x:20;y:Theme.nativeWindowFrame?60:104;width:parent.width-40;height:parent.height-y-20;clip:true;spacing:10;model:root.matches;ScrollBar.vertical:DesktopScrollBar {}
   delegate:Rectangle {required property var modelData;width:ListView.view.width;height:topic.height+24;radius:10;color:Theme.colors.surface
    Column {id:topic;x:12;y:12;width:parent.width-24;spacing:8
     PanelLabel {width:parent.width;text:modelData.title;font.weight:Font.DemiBold}
     PanelLabel {width:parent.width;text:modelData.body;wrapMode:Text.WordWrap;elide:Text.ElideNone}
     DialogButton {visible:!!modelData.action;text:"Open";onClicked:root.openTopic(modelData.action)}
    }
   }
   PanelLabel {visible:root.matches.length===0;anchors.centerIn:parent;text:"No matching help topics"}
  }
  Shortcut {sequence:"Escape";enabled:!InputOwnership.composing(root.contentItem.Window.window);onActivated:root.visible=false}
 }
}
