import QtQuick
import QtQuick.Controls
import QtQuick.Window
import Quickshell
import Quickshell.Io
FloatingWindow {
    id:root
    required property var shell
    title:"Remote Desktop";visible:false;color:"transparent"
    implicitWidth:780;implicitHeight:520;minimumSize:Qt.size(480,320)
    property var connections:[]
    property string query:""
    property string filter:"All"
    property string selectedId:""
    property string error:""
    property var request:({})
    readonly property var matches:connections.filter(c=>(filter==="All" || c.protocol===filter) && (c.name+" "+c.host).toLowerCase().includes(query.toLowerCase()))
    readonly property var current:matches.find(c=>c.id===selectedId) || null
    function send(data){if(worker.running)return;request=data;error="";worker.running=true;}
    function show(){shell.claimPopup("");visible=true;send({action:"list"});Qt.callLater(()=>{search.forceActiveFocus();if(body.Window.window)body.Window.window.requestActivate();});}
    function choose(index){if(index<0 || index>=matches.length)return;selectedId=matches[index].id;grid.currentIndex=index;grid.positionViewAtIndex(index,GridView.Contain);if(grid.currentItem)grid.currentItem.forceActiveFocus();}
    function connect(){if(current)send({action:"connect",id:current.id});}
    onVisibleChanged:if(visible && screen){implicitWidth=Math.max(480,Math.min(780,screen.width-32));implicitHeight=Math.max(320,Math.min(520,screen.height-92));}
    Rectangle {id:body;anchors.fill:parent;color:Theme.colors.background;radius:Theme.nativeWindowFrame?0:12
        Rectangle {width:150;height:parent.height;color:Theme.sidebarMaterial;radius:Theme.nativeWindowFrame?0:12}
        Rectangle {x:138;width:12;height:parent.height;color:Theme.sidebarMaterial}
        MouseArea {visible:!Theme.nativeWindowFrame;width:parent.width;height:48;onPressed:body.Window.window.startSystemMove()}
        WindowControls {windowName:"Remote Desktop";canFullscreen:false;windowActive:body.Window.window?body.Window.window.active:false;onTriggered:index=>{if(index===0)root.visible=false;else if(index===1)body.Window.window.showMinimized();}}
        PanelLabel {x:168;y:15;visible:!Theme.nativeWindowFrame;text:"Remote Desktop";font.weight:Font.DemiBold}
        Column {x:12;y:Theme.nativeWindowFrame?18:62;width:126;spacing:5
            PanelLabel {x:8;text:"Connections";font.pixelSize:Theme.textRole("metadata");font.weight:Font.DemiBold;color:Theme.colors.muted;height:Math.max(24,implicitHeight+6)}
            Repeater {model:["All","RDP","VNC","SSH"]
                AbstractButton {required property string modelData;width:126;height:Math.max(34,filterCaption.implicitHeight+12);Accessible.name:modelData==="All"?"All Computers":modelData;onClicked:root.filter=modelData
                    background:Rectangle {radius:7;color:root.filter===parent.modelData?Theme.colors.accent:parent.hovered || parent.activeFocus?Theme.colors.hover:"transparent"}
                    contentItem:PanelLabel {id:filterCaption;wrapMode:Text.Wrap;text:parent.modelData==="All"?"All Computers":parent.modelData;leftPadding:10;verticalAlignment:Text.AlignVCenter;color:root.filter===parent.modelData?Theme.colors.selectedText:Theme.colors.text}
                }
            }
            ControlRow {width:126;text:"Advanced…";onClicked:root.send({action:"advanced"})}
        }
        DesktopTextField {id:search;x:168;y:Theme.nativeWindowFrame?14:58;width:parent.width-186;height:Math.max(32,Theme.textRole("body")+16);placeholderText:"Search computers";placeholderTextColor:Theme.colors.muted;Accessible.name:"Search computers";onTextChanged:root.query=text;color:Theme.colors.text;font.family:Theme.font;font.pixelSize:Theme.textRole("body");background:Rectangle {radius:7;color:Theme.colors.field;border.color:search.activeFocus?Theme.colors.accent:Theme.hairline}}
        GridView {id:grid;x:168;y:Theme.nativeWindowFrame?60:104;width:parent.width-186;height:Math.max(50,parent.height-y-112);cellWidth:Math.max(140,Math.floor(width/Math.max(1,Math.floor(width/180))));cellHeight:root.height<420?104:144+(Theme.textRole("label")-12)*4;model:root.matches;clip:true;ScrollBar.vertical:DesktopScrollBar {}
            delegate:AbstractButton {required property var modelData;required property int index;width:grid.cellWidth-10;height:grid.cellHeight-12;Accessible.name:modelData.name+", "+modelData.protocol
                onClicked:root.choose(index)
                onDoubleClicked:{root.selectedId=modelData.id;root.connect();}
                Keys.onRightPressed:root.choose(Math.min(root.matches.length-1,index+1))
                Keys.onLeftPressed:root.choose(Math.max(0,index-1))
                Keys.onDownPressed:root.choose(Math.min(root.matches.length-1,index+Math.max(1,Math.floor(grid.width/grid.cellWidth))))
                Keys.onUpPressed:root.choose(Math.max(0,index-Math.max(1,Math.floor(grid.width/grid.cellWidth))))
                Keys.onReturnPressed:{root.selectedId=modelData.id;root.connect();}
                background:Rectangle {radius:12;color:Theme.colors.surface;border.width:root.selectedId===modelData.id || parent.activeFocus?2:1;border.color:root.selectedId===modelData.id || parent.activeFocus?Theme.colors.accent:Theme.hairline}
                contentItem:Item {
                    SystemGlyph {visible:root.height>=420;anchors.horizontalCenter:parent.horizontalCenter;y:16;width:48;height:42;name:"screen";ink:Theme.colors.accent}
                    PanelLabel {x:10;y:root.height<420?8:68;width:parent.width-20;text:modelData.name;textFormat:Text.PlainText;wrapMode:Text.Wrap;horizontalAlignment:Text.AlignHCenter;font.weight:Font.DemiBold;font.pixelSize:Theme.textRole("label")}
                    PanelLabel {x:10;y:(root.height<420?8:68)+Theme.textRole("label")*2+8;width:parent.width-20;wrapMode:Text.Wrap;text:modelData.protocol+(modelData.printer?" · Printing enabled":"");horizontalAlignment:Text.AlignHCenter;font.pixelSize:Theme.textRole("caption");color:Theme.colors.muted}
                }
            }
            PanelLabel {visible:root.matches.length===0;anchors.centerIn:parent;width:parent.width;text:worker.running?"Loading computers…":root.query?"No matching computers":"No saved computers. Add a connection below.";wrapMode:Text.WordWrap;horizontalAlignment:Text.AlignHCenter;color:Theme.colors.muted}
        }
        StatusMessage {x:168;y:parent.height-134;width:parent.width-186;text:root.error;severity:"error"}
        Row {x:168;y:parent.height-104;width:parent.width-186;spacing:8
            DialogButton {width:(parent.width-16)/3;text:"Connect";primary:true;enabled:!!root.current && !worker.running;onClicked:root.connect()}
            DialogButton {width:(parent.width-16)/3;text:"Settings…";enabled:!!root.current && !worker.running;onClicked:root.send({action:"edit",id:root.current.id})}
            DialogButton {width:(parent.width-16)/3;text:"Add…";enabled:!worker.running;onClicked:root.send({action:"new"})}
        }
        Row {x:168;y:parent.height-62;width:parent.width-186;spacing:8
            DesktopComboBox {id:protocol;width:80;model:["RDP","VNC"];Accessible.name:"Connection protocol"}
            DesktopTextField {id:address;width:Math.max(100,parent.width-176);height:30;placeholderText:"Hostname or IP address";placeholderTextColor:Theme.colors.muted;Accessible.name:"Remote computer address";color:Theme.colors.text;font.family:Theme.font;font.pixelSize:Theme.textRole("body");onAccepted:if(text.trim())root.send({action:"quick",address:text.trim(),protocol:protocol.currentText});background:Rectangle {radius:6;color:Theme.colors.field;border.color:address.activeFocus?Theme.colors.accent:Theme.hairline}}
            DialogButton {width:80;text:"Open";enabled:!!address.text.trim() && !worker.running;onClicked:root.send({action:"quick",address:address.text.trim(),protocol:protocol.currentText})}
        }
        Shortcut {sequence:"Escape";enabled:!InputOwnership.composing(root.contentItem.Window.window);onActivated:root.visible=false}
    }
    Process {id:worker;stdinEnabled:true;command:["lumen-remote-desktop"]
        onStarted:write(JSON.stringify(root.request)+"\n")
        stdout:StdioCollector {onStreamFinished:{try{const data=JSON.parse(text);root.error=data.error || "";if(data.connections)root.connections=data.connections;if(data.started)root.visible=false;}catch(e){root.error="Remote Desktop could not complete the request.";}}}
    }
}
