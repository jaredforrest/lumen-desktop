import QtQuick
import Quickshell
import Quickshell.Wayland
PanelWindow {
    id:root
    required property var controller
    visible:controller.showing && controller.state.permitted
    anchors {top:true;bottom:true;left:true;right:true}
    exclusionMode:ExclusionMode.Ignore
    color:"#111216"
    WlrLayershell.namespace:"lumen-screen-saver"
    WlrLayershell.layer:WlrLayer.Overlay
    WlrLayershell.keyboardFocus:visible?WlrKeyboardFocus.Exclusive:WlrKeyboardFocus.None
    Item {anchors.fill:parent;focus:root.visible
        Keys.onPressed:event=>{event.accepted=true;root.controller.dismiss();}
        Repeater {model:3
            Rectangle {required property int index
                width:Math.min(parent.width,parent.height)*(.22+index*.06);height:width;radius:width/2
                x:parent.width*(.15+.55*(.5+.5*Math.sin((root.controller.phase+index/3)*Math.PI*2)))-width/2
                y:parent.height*(.15+.55*(.5+.5*Math.cos((root.controller.phase+index/3)*Math.PI*2)))-height/2
                color:index===0?"#325b82":index===1?"#635586":"#32665f";opacity:.65;antialiasing:true
            }
        }
        Column {anchors.horizontalCenter:parent.horizontalCenter;anchors.bottom:parent.bottom;anchors.bottomMargin:48;spacing:8;width:Math.min(440,parent.width-48)
            PanelLabel {width:parent.width;horizontalAlignment:Text.AlignHCenter;wrapMode:Text.Wrap;text:"Screen Saver";color:"#d8d8de";font.pixelSize:Theme.textSize(18)}
            PanelLabel {width:parent.width;horizontalAlignment:Text.AlignHCenter;wrapMode:Text.Wrap;text:"Move the pointer or press a key to return. This is not a security lock.";color:"#b3b3ba"}
        }
        MouseArea {anchors.fill:parent;hoverEnabled:true;acceptedButtons:Qt.AllButtons
            property real previousX:-1;property real previousY:-1
            onVisibleChanged:{previousX=-1;previousY=-1;}
            onPositionChanged:event=>{if(previousX>=0 && (Math.abs(event.x-previousX)+Math.abs(event.y-previousY)>0)){root.controller.dismiss();}previousX=event.x;previousY=event.y;}
            onPressed:event=>{event.accepted=true;}
            onReleased:event=>{event.accepted=true;root.controller.dismiss();}
            onWheel:event=>{event.accepted=true;root.controller.dismiss();}
        }
    }
}
