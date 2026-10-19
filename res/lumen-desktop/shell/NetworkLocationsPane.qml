import QtQuick
import QtQuick.Controls
Column {
    id:root
    required property var service
    signal openLocalFolder(string path)
    spacing:9
    property string selectedUri:""
    PanelLabel {text:"Network Locations";font.pixelSize:17;font.weight:Font.DemiBold}
    PanelLabel {width:parent.width;wrapMode:Text.WordWrap;font.pixelSize:11;color:Theme.colors.muted;text:"Connect to a server using its address. Passwords and keyring choices are handled by the system authentication dialog. Do not include credentials in the address."}
    Row {width:parent.width;spacing:7
        TextField {id:address;width:Math.max(120,parent.width-100);height:32;Accessible.name:"Server address";placeholderText:"smb://server/share or sftp://server/folder";color:Theme.colors.text;font.family:Theme.font;font.pixelSize:12;selectByMouse:true;onAccepted:if(root.service.available && !root.service.busy && text)root.service.connectServer(text);background:Rectangle {radius:6;color:Theme.colors.field;border.color:address.activeFocus?Theme.colors.accent:Theme.hairline}}
        DialogButton {width:93;height:32;text:"Connect";enabled:root.service.available && !root.service.busy && !!address.text;onClicked:root.service.connectServer(address.text)}
    }
    PanelLabel {width:parent.width;wrapMode:Text.WordWrap;text:root.service.ready?(root.service.available?"Available protocols: "+root.service.protocols.join(", "):root.service.reason):"Checking installed network providers…";font.pixelSize:11;color:Theme.colors.muted}
    ListView {id:locations;width:parent.width;height:Math.min(150,root.service.connections.length*38);clip:true;model:root.service.connections;ScrollBar.vertical:DesktopScrollBar {}
        delegate:ControlRow {required property var modelData;width:locations.width;height:38;text:modelData.name+" · "+modelData.state;detail:modelData.uri;selected:root.service.selectedId===modelData.id;onClicked:root.service.select(modelData.id)}
    }
    PanelLabel {visible:root.service.connections.length===0;text:"No network locations are connected.";color:Theme.colors.muted;font.pixelSize:12}
    Row {width:parent.width;spacing:7
        DialogButton {width:(parent.width-21)/4;text:"Reconnect";enabled:!!root.service.current && !root.service.busy && ["disconnected","failed"].includes(root.service.current.state);onClicked:root.service.reconnect()}
        DialogButton {width:(parent.width-21)/4;text:"Disconnect";enabled:!!root.service.current && root.service.current.state==="connected" && root.service.current.canDisconnect && !root.service.busy;onClicked:root.service.disconnectServer()}
        DialogButton {width:(parent.width-21)/4;text:"Cancel";enabled:root.service.busy;onClicked:root.service.cancel()}
        DialogButton {width:(parent.width-21)/4;text:"Refresh";enabled:!root.service.busy;onClicked:root.service.refresh()}
    }
    PanelLabel {visible:!!root.service.current && ["connecting","authenticating","disconnecting"].includes(root.service.current.state);width:parent.width;wrapMode:Text.WordWrap;text:root.service.current && root.service.current.state==="authenticating"?"Complete or cancel the system authentication dialog.":root.service.current?root.service.current.state+"…":"";color:Theme.colors.muted}
    PanelLabel {visible:!!root.service.error;width:parent.width;wrapMode:Text.WordWrap;text:root.service.error;textFormat:Text.PlainText;color:Theme.colors.muted}
    PanelLabel {visible:!!root.service.current && !!root.service.current.error;width:parent.width;wrapMode:Text.WordWrap;text:root.service.current?root.service.current.error:"";textFormat:Text.PlainText;color:Theme.colors.muted}
    Row {visible:!!root.service.current && root.service.current.state==="connected";width:parent.width;spacing:7
        DialogButton {width:65;text:"Up";enabled:!!root.service.parentUri && !root.service.busy;onClicked:root.service.browse(root.service.parentUri)}
        PanelLabel {width:parent.width-150;height:30;verticalAlignment:Text.AlignVCenter;elide:Text.ElideMiddle;text:root.service.uri || (root.service.current?root.service.current.uri:"");textFormat:Text.PlainText}
        DialogButton {width:71;text:"Reload";enabled:!root.service.busy;onClicked:root.service.browse(root.service.uri)}
    }
    ListView {id:browser;visible:!!root.service.current && root.service.current.state==="connected";width:parent.width;height:180;clip:true;model:root.service.rows;ScrollBar.vertical:DesktopScrollBar {}
        delegate:ControlRow {required property var modelData;width:browser.width;text:(modelData.directory?"Folder · ":"")+modelData.name;selected:root.selectedUri===modelData.uri;onClicked:{root.selectedUri=modelData.uri;if(modelData.directory)root.service.browse(modelData.uri);}}
    }
    PanelLabel {visible:!!root.service.current && root.service.current.state==="connected";width:parent.width;wrapMode:Text.WordWrap;font.pixelSize:11;color:Theme.colors.muted;text:root.service.browseId?"Reading the network folder…":root.service.limited?"Showing the first 100 entries. Open a smaller folder or use Files.":root.service.rows.length+" entries · Network browsing is read-only here."}
    DialogButton {width:parent.width;text:"Open This Folder in Files";enabled:!!root.service.localPath && !root.service.busy;onClicked:root.openLocalFolder(root.service.localPath)}
    PanelLabel {visible:!!root.service.current && root.service.current.state==="connected" && !root.service.localPath && !root.service.busy;width:parent.width;wrapMode:Text.WordWrap;font.pixelSize:11;color:Theme.colors.muted;text:"This provider has no local filesystem path. Read-only browsing is available above; full Files operations require the host's GVfs FUSE integration."}
}
