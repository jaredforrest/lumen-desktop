import QtQuick
import QtQuick.Controls
import QtQuick.Window
import Quickshell
import Quickshell.Io
FloatingWindow {
    id:root
    required property var shell
    title:"Window Overview";visible:false;color:"transparent"
    implicitWidth:880;implicitHeight:560;minimumSize:Qt.size(560,380)
    property var windows:[]
    property var workspaces:[]
    property var outputs:[]
    property string appFilter:""
    property string error:""
    property int selected:0
    onVisibleChanged:if(visible)fit()
    function fit(){if(screen){implicitWidth=Math.max(400,Math.min(880,screen.width-32));implicitHeight=Math.max(380,Math.min(560,screen.height-130));}}
    property var request:({})
    readonly property var matches:windows.filter(w=>(!appFilter || w.group===appFilter) && (w.title+" "+w.app+" "+w.workspace).toLowerCase().includes(search.text.toLowerCase()))
    readonly property var current:matches[selected] || null
    function show(app){if(visible){visible=false;return;}appFilter=app || "";search.text="";selected=0;shell.claimPopup("");visible=true;send({action:"status"});Qt.callLater(()=>{if(content.Window.window)content.Window.window.requestActivate();search.forceActiveFocus();});}
    function send(data){if(worker.running)return;request=data;error="";worker.running=true;}
    function activate(){if(current){visible=false;activation.data={action:"focus",id:current.id,pid:current.pid};activation.restart();}}
    function step(delta){selected=Math.max(0,Math.min(matches.length-1,selected+delta));grid.positionViewAtIndex(selected,GridView.Contain);}
    Timer {id:activation;property var data;interval:100;onTriggered:root.send(data)}
    Rectangle {id:content;anchors.fill:parent;radius:Theme.nativeWindowFrame?0:12;color:Theme.colors.background;border.color:Theme.outline;focus:true
        Keys.onEscapePressed:root.visible=false
        Keys.onPressed:event=>{
            if(event.key===Qt.Key_Down){root.step(Math.max(1,Math.floor(grid.width/grid.cellWidth)));event.accepted=true;}
            else if(event.key===Qt.Key_Up){root.step(-Math.max(1,Math.floor(grid.width/grid.cellWidth)));event.accepted=true;}
            else if(event.key===Qt.Key_Left){root.step(-1);event.accepted=true;}
            else if(event.key===Qt.Key_Right){root.step(1);event.accepted=true;}
            else if(event.key===Qt.Key_Return || event.key===Qt.Key_Enter){root.activate();event.accepted=true;}
        }
        MouseArea {width:parent.width;height:44;onPressed:if(content.Window.window)content.Window.window.startSystemMove()}
        WindowControls {windowName:"Window Overview";canMinimize:false;canFullscreen:false;windowActive:content.Window.window?content.Window.window.active:false;onTriggered:index=>{if(index===0)root.visible=false;}}
        PanelLabel {x:100;y:14;text:root.appFilter?"Application Windows":"Windows & Workspaces";font.weight:Font.DemiBold}
        TextField {id:search;x:20;y:54;width:parent.width-40;height:32;placeholderText:"Find a window";placeholderTextColor:Theme.colors.muted;color:Theme.colors.text;font.family:Theme.font;selectByMouse:true;Accessible.name:"Find a window";onTextChanged:root.selected=0;onAccepted:root.activate();background:Rectangle {radius:8;color:Theme.colors.field;border.color:search.activeFocus?Theme.colors.accent:Theme.hairline}}
        Flickable {x:20;y:98;width:parent.width-40;height:34;contentWidth:spaces.width;clip:true
            Row {id:spaces;spacing:6
                Repeater {model:root.workspaces
                    DialogButton {required property var modelData;width:Math.max(90,modelData.name.length*8+24);text:"Desktop "+modelData.name;primary:modelData.focused;Accessible.name:"Workspace "+modelData.name+" on "+modelData.output;onClicked:{root.visible=false;activation.data={action:"workspace",workspace:modelData.name};activation.restart();}}
                }
            }
        }
        GridView {id:grid;x:20;y:145;width:parent.width-40;height:parent.height-y-105;clip:true;cellWidth:Math.floor(width/Math.max(1,Math.floor(width/250)));cellHeight:108;model:root.matches;ScrollBar.vertical:DesktopScrollBar {}
            delegate:AbstractButton {
                required property var modelData;required property int index
                width:grid.cellWidth-8;height:100
                Accessible.name:modelData.title+" · "+modelData.app+" · workspace "+modelData.workspace;Accessible.selected:index===root.selected
                onClicked:{root.selected=index;root.activate();}
                background:Rectangle {radius:12;color:Theme.colors.surface;border.width:parent.index===root.selected?2:1;border.color:parent.index===root.selected?Theme.colors.accent:Theme.hairline}
                contentItem:Item {
                    SystemGlyph {x:12;y:15;width:24;height:24;name:"screen";ink:Theme.colors.accent}
                    PanelLabel {x:46;y:13;width:parent.width-58;height:38;text:modelData.title;textFormat:Text.PlainText;wrapMode:Text.Wrap;maximumLineCount:2;font.weight:Font.DemiBold;font.pixelSize:12}
                    PanelLabel {x:12;y:61;width:parent.width-24;text:modelData.workspace==="__i3_scratch"?"Minimised":"Desktop "+modelData.workspace;textFormat:Text.PlainText;color:Theme.colors.muted;font.pixelSize:11}
                }
            }
            PanelLabel {visible:root.matches.length===0;anchors.centerIn:parent;text:"No matching windows";color:Theme.colors.muted}
        }
        Row {x:20;y:parent.height-88;width:parent.width-40;height:30;spacing:8
            DesktopComboBox {id:workspace;width:(parent.width-136)/2;model:[1,2,3,4,5,6,7,8,9].map(n=>({name:String(n),label:"Desktop "+n}));textRole:"label";Accessible.name:"Destination desktop"}
            DialogButton {width:120;text:"Move Window";enabled:!!root.current;onClicked:root.send({action:"move",id:root.current.id,pid:root.current.pid,workspace:workspace.model[workspace.currentIndex].name})}
            DesktopComboBox {id:output;width:(parent.width-136)/2;model:root.outputs;textRole:"label";Accessible.name:"Destination display";onActivated:index=>{if(root.current)root.send({action:"output",id:root.current.id,pid:root.current.pid,output:root.outputs[index].name});}}
        }
        PanelLabel {x:20;y:parent.height-44;width:parent.width-40;font.pixelSize:11;color:Theme.colors.muted;text:root.error || "Arrow keys to select · Return to open · Escape to close";wrapMode:Text.WordWrap}
    }
    Timer {interval:2000;running:root.visible;repeat:true;onTriggered:root.send({action:"status"})}
    Process {id:worker;stdinEnabled:true
        command:[Quickshell.env("GLOBAL_MENU_PYTHON") || "python3",Qt.resolvedUrl("window_overview.py").toString().replace("file://","")]
        onStarted:write(JSON.stringify(root.request)+"\n")
        stdout:StdioCollector {onStreamFinished:{try{const data=JSON.parse(text);root.error=data.error || "";if(data.windows){const selectedId=root.current?root.current.id:0;root.windows=data.windows;root.selected=Math.max(0,root.matches.findIndex(w=>w.id===selectedId));root.workspaces=data.workspaces;root.outputs=data.outputs;root.selected=Math.min(root.selected,Math.max(0,root.matches.length-1));}}catch(e){root.error="Unable to read desktop windows";}}}
        onExited:if(root.request.action!=="status" && root.visible)Qt.callLater(()=>root.send({action:"status"}))
    }
}
