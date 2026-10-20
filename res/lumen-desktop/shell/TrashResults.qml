import QtQuick
import QtQuick.Controls
Popup {id:root;objectName:"trash-results";required property var state;modal:true;focus:true;padding:16;width:Math.min(600,parent.width-32);height:Math.min(540,parent.height-32);x:(parent.width-width)/2;y:(parent.height-height)/2
            background:Rectangle {radius:12;color:Theme.colors.surface;border.color:Theme.outline}
            Column {anchors.fill:parent;spacing:10
                PanelLabel {width:parent.width;text:"Trash operation results";font.weight:Font.DemiBold}
                PanelLabel {width:parent.width;text:root.state.error;visible:text!=="";wrapMode:Text.Wrap;color:Theme.errorText}
                ScrollView {width:parent.width;height:Math.max(50,parent.height-120);clip:true
                    Column {width:parent.width;spacing:10
                        Repeater {model:root.state.history
                            DialogButton {required property var modelData;width:parent.width;text:modelData.count+" item(s) · "+modelData.state;onClicked:root.state.inspect(modelData.id)}
                        }
                        TextArea {width:parent.width;readOnly:true;selectByMouse:true;textFormat:TextEdit.PlainText;wrapMode:TextEdit.Wrap;color:Theme.colors.text;font.pixelSize:Theme.textSize(13);background:null
                            text:root.state.operations.map(operation=>operation.message+"\n"+(operation.items || []).map(item=>item.name+": "+item.state+(item.error?" — "+item.error+(item.errorTruncated?" (diagnostic shortened)":""):"")+(item.payloadPath?"\nRemaining contents: "+item.payloadPath:"")+(item.metadataPath?"\nRecovery record: "+item.metadataPath:"")+(item.recoveryPath?"\nRecovery folder: "+item.recoveryPath:"")).join("\n\n")).join("\n\n")
                        }
                    }
                }
                Row {spacing:10
                    DialogButton {text:"Stop Remaining";visible:root.state.busy;onClicked:root.state.stop()}
                    DialogButton {objectName:"trash-results-done";text:"Done";onClicked:root.close()}
                }
            }
        }
