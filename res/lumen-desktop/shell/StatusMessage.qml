import QtQuick
PanelLabel {
    id:root
    property string announcedMessage:""
    function announceMessage(){
        if(!visible || !text.trim()){announcedMessage="";return;}
        const capturedText=text,capturedSeverity=severity;
        Qt.callLater(()=>{
            if(!visible || text!==capturedText || severity!==capturedSeverity)return;
            const message=Accessible.name;
            if(announcedMessage!==message){
                announcedMessage=message;
                Accessible.announce(message,severity==="error"?Accessible.Assertive:Accessible.Polite);
            }
        });
    }
    onTextChanged:announceMessage()
    onVisibleChanged:announceMessage()
    onSeverityChanged:announceMessage()
    Component.onCompleted:announceMessage()
    property string severity:"info"
    visible: text.length>0
    wrapMode: Text.WordWrap
    elide: Text.ElideNone
    color: severity==="error" ? Theme.errorText : severity==="success" ? Theme.successText : severity==="warning" ? Theme.warningText : Theme.colors.muted
    font.pixelSize:Theme.textSize(12)
    leftPadding:severityLabel.visible?severityLabel.implicitWidth+6:0
    PanelLabel {
        id:severityLabel
        text:root.severity==="error"?"Error:":root.severity==="success"?"Success:":root.severity==="warning"?"Warning:":""
        visible:text.length>0
        x:0;y:root.topPadding
        font:root.font
        color:root.color
        Accessible.ignored:true
    }
    Accessible.role: Accessible.StaticText
    Accessible.name:(severity==="error"?"Error: ":severity==="success"?"Success: ":severity==="warning"?"Warning: ":"")+text
    Accessible.description: severity
}
