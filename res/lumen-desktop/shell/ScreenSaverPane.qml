import QtQuick
Column {
    id:root
    required property var service
    signal previewRequested()
    spacing:10
    PanelLabel {width:parent.width;text:"Screen Saver";font.weight:Font.DemiBold}
    PanelLabel {width:parent.width;wrapMode:Text.Wrap;text:"A decorative idle display. Security locking, password requirements, display power and their existing timeouts remain controlled by the host configuration.";color:Theme.colors.muted}
    SettingsToggle {objectName:"screen-saver-enabled";width:parent.width;text:"Start after inactivity";detail:"Respects compositor idle inhibitors, including supported presentation and media inhibitors.";checked:root.service.preferences.enabled;enabled:!root.service.busy;onClicked:root.service.setValue("enabled",checked)}
    PanelLabel {width:parent.width;text:"Idle delay"}
    DesktopComboBox {objectName:"screen-saver-delay";width:parent.width;model:["15 seconds","1 minute","2 minutes","5 minutes","10 minutes","30 minutes","1 hour","2 hours"];currentIndex:[15,60,120,300,600,1800,3600,7200].indexOf(root.service.preferences.idleSeconds);displayText:currentIndex<0?root.service.preferences.idleSeconds+" seconds":currentText;Accessible.name:"Screen saver idle delay";enabled:!root.service.busy;onActivated:index=>root.service.setValue("idleSeconds",[15,60,120,300,600,1800,3600,7200][index])}
    SettingsToggle {objectName:"screen-saver-motion";width:parent.width;text:"Animate shapes";detail:"Reduce Motion also pauses this animation.";checked:root.service.preferences.motion;enabled:!root.service.busy;onClicked:root.service.setValue("motion",checked)}
    DialogButton {objectName:"screen-saver-preview";text:"Preview screen saver";enabled:root.service.permitted && !root.service.busy;width:Math.min(implicitWidth,parent.width);onClicked:root.previewRequested()}
    StatusMessage {width:parent.width;text:root.service.error;severity:"error"}
    PanelLabel {width:parent.width;wrapMode:Text.Wrap;text:root.service.permitted?"Press a key or move the pointer to dismiss. If the host locks, its real authentication screen takes precedence.":"Preview and automatic activation require known native unlocked-session status. A locked or unavailable session cannot start this decoration.";color:Theme.colors.muted}
}
