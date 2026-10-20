import QtQuick
import QtQuick.Controls
Popup {
    id:root
    objectName:"trash-confirmation"
    required property var state
    width:Math.min(520,parent.width-32);height:Math.min(500,parent.height-32)
    x:(parent.width-width)/2;y:(parent.height-height)/2
    modal:true;focus:true;padding:18
    closePolicy:Popup.CloseOnEscape
    background:Rectangle {radius:12;color:Theme.colors.surface;border.color:Theme.colors.border}
    onOpened:Qt.callLater(()=>cancelButton.forceActiveFocus())
    onClosed:state.retire()
    Connections {target:root.state;function onConfirmationChanged(){if(root.state.confirmation)root.open();else if(root.visible)root.close();}}
    Column {
        anchors.fill:parent;spacing:12
        PanelLabel {id:heading;width:parent.width;text:root.state.confirmation?"Permanently delete "+root.state.confirmation.count+" item(s)?":"Trash";font.weight:Font.DemiBold;font.pixelSize:Theme.textSize(18);wrapMode:Text.Wrap}
        PanelLabel {id:consequence;width:parent.width;text:"These captured items are already in personal Trash. This action cannot be undone. Items arriving later are excluded.";wrapMode:Text.Wrap}
        ScrollView {width:parent.width;height:Math.max(20,parent.height-heading.implicitHeight-consequence.implicitHeight-buttons.implicitHeight-3*parent.spacing);clip:true
            TextArea {readOnly:true;selectByMouse:true;text:root.state.confirmation?(root.state.confirmation.names || []).join("\n"):"";textFormat:TextEdit.PlainText;wrapMode:TextEdit.Wrap;color:Theme.colors.text;font.pixelSize:Theme.textSize(13);background:null}
        }
        Row {id:buttons;spacing:10
            DialogButton {id:cancelButton;objectName:"trash-cancel";text:"Cancel";defaultAction:true;onClicked:root.close()}
            DialogButton {objectName:"trash-confirm";text:"Delete Permanently";destructive:true;enabled:!!root.state.confirmation && !root.state.waiting;onClicked:root.state.confirm()}
        }
    }
}
