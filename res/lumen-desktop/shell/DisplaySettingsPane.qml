import QtQuick
import QtQuick.Controls
import Quickshell
import Quickshell.Io
Column {
    id:root
    spacing:10
    property var outputs:[]
    property string selected:""
    readonly property var output:outputs.find(o=>o.name===selected) || outputs[0] || null
    property string pending:""
    property int seconds:0
    property string error:""
    property var request:({})
    function send(data){if(worker.running)return;error="";request=data;worker.running=true;}
    function refresh(){send({action:"status"});}
    Component.onCompleted:refresh()
    function layoutBounds(){let left=0,top=0,right=1,bottom=1;for(const o of outputs){const parts=o.mode.split("@")[0].split("x"),w=Number(parts[0])/o.scale,h=Number(parts[1])/o.scale;left=Math.min(left,o.x);top=Math.min(top,o.y);right=Math.max(right,o.x+w);bottom=Math.max(bottom,o.y+h);}return {x:left,y:top,width:right-left,height:bottom-top};}
    readonly property var bounds:layoutBounds()
    PanelLabel {text:"Arrange Displays";font.weight:Font.DemiBold}
    Rectangle {id:arrangement;width:parent.width;height:160;radius:10;color:Theme.colors.field;clip:true
        readonly property real factor:Math.min((width-32)/root.bounds.width,(height-32)/root.bounds.height)
        Repeater {model:root.outputs
            Rectangle {id:monitor;required property var modelData;required property int index
                property real originX:0;property real originY:0
                x:16+(modelData.x-root.bounds.x)*arrangement.factor;y:16+(modelData.y-root.bounds.y)*arrangement.factor
                width:Math.max(30,Number(modelData.mode.split("@")[0].split("x")[0])/modelData.scale*arrangement.factor)
                height:Math.max(24,Number(modelData.mode.split("@")[0].split("x")[1])/modelData.scale*arrangement.factor)
                color:root.output && root.output.name===modelData.name?Theme.colors.accent:Theme.colors.surface;radius:5;border.color:Theme.outline
                PanelLabel {anchors.centerIn:parent;width:parent.width-8;text:modelData.label || "Display "+(index+1);horizontalAlignment:Text.AlignHCenter;color:root.output && root.output.name===modelData.name?Theme.colors.selectedText:Theme.colors.text;font.pixelSize:10}
                MouseArea {anchors.fill:parent;enabled:!root.pending;drag.target:monitor;drag.axis:Drag.XAndYAxis
                    onPressed:{root.selected=modelData.name;monitor.originX=monitor.x;monitor.originY=monitor.y;}
                    onReleased:{const fields=position.children;fields.find(c=>c.objectName==="displayX").text=String(Math.round(modelData.x+(monitor.x-monitor.originX)/arrangement.factor));fields.find(c=>c.objectName==="displayY").text=String(Math.round(modelData.y+(monitor.y-monitor.originY)/arrangement.factor));}
                }
            }
        }
    }
    PanelLabel {width:parent.width;text:"Select a display, then drag it into position. Apply to review the change.";font.pixelSize:11;wrapMode:Text.WordWrap;color:Theme.colors.muted}
    DesktopComboBox {width:parent.width;model:root.outputs;textRole:"label";currentIndex:root.outputs.findIndex(o=>o===root.output);Accessible.name:"Display";onActivated:index=>root.selected=root.outputs[index].name}
    PanelLabel {text:"Resolution and refresh rate";font.pixelSize:12}
    DesktopComboBox {id:mode;width:parent.width;model:root.output?root.output.modes:[];currentIndex:root.output?model.indexOf(root.output.mode):-1;Accessible.name:"Display resolution and refresh rate";enabled:!!root.output && !root.pending}
    Row {width:parent.width;spacing:8
        Repeater {model:[{label:"Larger Text",scale:2},{label:"Balanced",scale:1.5},{label:"More Space",scale:1}]
            DialogButton {required property var modelData;width:(parent.width-16)/3;text:modelData.label;enabled:!!root.output && !root.pending;onClicked:scaling.currentIndex=[1,1.25,1.5,1.75,2,2.5,3].indexOf(modelData.scale)}
        }
    }
    PanelLabel {text:"Scale";font.pixelSize:12}
    DesktopComboBox {id:scaling;width:parent.width;model:["100%","125%","150%","175%","200%","250%","300%"];currentIndex:root.output?[1,1.25,1.5,1.75,2,2.5,3].indexOf(root.output.scale):0;Accessible.name:"Display scale";enabled:!!root.output && !root.pending}
    PanelLabel {text:"Position in desktop points";font.pixelSize:12}
    Row {width:parent.width;spacing:8
        Repeater {model:["x","y"]
            TextField {required property string modelData;objectName:modelData==="x"?"displayX":"displayY";width:(parent.width-8)/2;height:30;text:root.output?String(root.output[modelData]):"0";validator:IntValidator {bottom:-32768;top:32767} enabled:!root.pending;font.family:Theme.font;color:Theme.colors.text;selectByMouse:true;Accessible.name:"Display "+modelData+" position";background:Rectangle {radius:6;color:Theme.colors.field;border.color:parent.activeFocus?Theme.colors.accent:Theme.hairline}}
        }
        id:position
    }
    DialogButton {width:parent.width;text:"Apply Display Settings";primary:true;enabled:!!root.output && !root.pending && !worker.running && mode.currentIndex>=0;onClicked:root.send({action:"apply",name:root.output.name,mode:mode.currentText,scale:[1,1.25,1.5,1.75,2,2.5,3][scaling.currentIndex],x:position.children.find(c=>c.objectName==="displayX").text,y:position.children.find(c=>c.objectName==="displayY").text})}
    PanelLabel {visible:!!root.pending;width:parent.width;wrapMode:Text.WordWrap;text:"Keep these display settings? Reverting in "+root.seconds+" seconds.";Accessible.name:text}
    Row {visible:!!root.pending;spacing:8;width:parent.width
        DialogButton {width:(parent.width-8)/2;text:"Revert";onClicked:root.send({action:"revert",token:root.pending})}
        DialogButton {width:(parent.width-8)/2;text:"Keep Changes";primary:true;onClicked:root.send({action:"confirm",token:root.pending})}
    }
    StatusMessage {width:parent.width;text:root.error;severity:"error"}
    PanelLabel {width:parent.width;wrapMode:Text.WordWrap;text:"Confirmed settings update your Nix configuration and are applied on login or reconnect.";font.pixelSize:11;color:Theme.colors.muted}
    ControlRow {width:parent.width;text:"Forget Saved Display Layout";enabled:!root.pending;onClicked:root.send({action:"forget"})}
    Process {
        id:worker;stdinEnabled:true
        command:["lumen-display-settings"]
        onStarted:write(JSON.stringify(root.request)+"\n")
        stdout:StdioCollector {onStreamFinished:{try{const data=JSON.parse(text);if(data.error){root.error=data.error;return;}if(data.outputs)root.outputs=data.outputs;root.pending=data.pending || "";root.seconds=data.seconds || 0;}catch(e){root.error="Unable to read display settings";}}}
        onExited:{if(root.request.action!=="status")Qt.callLater(root.refresh);}
    }
    Timer {interval:1000;running:!!root.pending;repeat:true;onTriggered:root.refresh()}
}
