import QtQuick
PanelSurface {
    id:root
    required property var state
    readonly property var snapshot:state?state.status:({available:false,rows:[],coverage:"Other capture paths are not assessed."})
    height:body.height+32
    Column {
        id:body;x:16;y:16;width:parent.width-32;spacing:10
        PanelLabel {width:parent.width;wrapMode:Text.WordWrap;text:"Capture observations";font.weight:Font.DemiBold}
        PanelLabel {width:parent.width;wrapMode:Text.WordWrap;text:root.snapshot.available?(root.snapshot.rows.length?"Active PipeWire capture links":"No active PipeWire capture links observed"):"Capture observations unavailable"}
        Repeater {model:root.snapshot.available?root.snapshot.rows:[]
            Column {required property var modelData;width:body.width;spacing:4
                PanelLabel {width:parent.width;wrapMode:Text.WordWrap;text:(parent.modelData.kind==="audio-input"?"Audio capture":"Video capture")+" · "+parent.modelData.reportedApplication;textFormat:Text.PlainText}
                PanelLabel {width:parent.width;wrapMode:Text.WordWrap;text:parent.modelData.attribution;textFormat:Text.PlainText;color:Theme.colors.muted;font.pixelSize:11}
            }
        }
        PanelLabel {width:parent.width;wrapMode:Text.WordWrap;text:root.snapshot.coverage;textFormat:Text.PlainText;color:Theme.colors.muted}
        PanelLabel {width:parent.width;wrapMode:Text.WordWrap;text:"Audio may include system-audio monitoring. Video observations do not identify camera versus screen capture. These observations do not grant or revoke permissions.";color:Theme.colors.muted}
    }
}
