import QtQuick
import QtQuick.Controls

Item {
    id: root
    required property var details
    Row {
        id: heading; width: parent.width; height: Math.max(64,headingText.implicitHeight+12); spacing: 12
        Item {
            width: 48; height: 52
            Image {anchors.centerIn:parent;width:44;height:38;source:root.details.kind==="Folder"?"icons/folder.svg":"";fillMode:Image.PreserveAspectFit}
            SystemGlyph {visible:root.details.kind!=="Folder";anchors.centerIn:parent;width:32;height:40;name:"document";ink:Theme.colors.muted}
        }
        Column {
            id:headingText;width: parent.width-60; spacing: 3; anchors.verticalCenter:parent.verticalCenter
            PanelLabel {width:parent.width;text:root.details.name || "Info";font.weight:Font.DemiBold;wrapMode:Text.WrapAnywhere;maximumLineCount:2}
            PanelLabel {width:parent.width;text:root.details.size || "";font.pixelSize:Theme.textRole("metadata");color:Theme.colors.muted}
        }
    }
    ScrollView {
        anchors {top:heading.bottom;bottom:parent.bottom;left:parent.left;right:parent.right}
        clip:true;ScrollBar.vertical:DesktopScrollBar {}
        FocusFlickable {objectName:"file-info-content";clip:true;contentWidth:width;contentHeight:sections.height
        Column {
            id:sections;width:parent.width;spacing:0
            Repeater {
                model:[{title:"General",rows:[{label:"Kind",key:"kind"},{label:"Size",key:"size"},{label:"Where",key:"location"},{label:"Modified",key:"modified"},{label:"Original",key:"target"},{label:"Link Status",key:"linkDetail"}]},{title:"Sharing & Permissions",rows:[{label:"Owner",key:"owner"},{label:"Group",key:"group"},{label:"Access",key:"permissions"}]}]
                Column {
                    id: section; required property var modelData
                    width:parent.width; property bool expanded:true
                    Rectangle {width:parent.width;height:1;color:Theme.hairline}
                    AbstractButton {
                        width:parent.width;height:Math.max(31,sectionCaption.implicitHeight+12);Accessible.name:section.modelData.title;Accessible.description:section.expanded?"Expanded":"Collapsed"
                        onClicked:section.expanded=!section.expanded
                        background:Rectangle {radius:4;color:parent.activeFocus?Theme.selection:"transparent";FocusOutline {focused:parent.parent.activeFocus;radius:4}}
                        contentItem:Row {spacing:6;PanelLabel {text:section.expanded?"▾":"▸";width:12;height:parent.parent.height;verticalAlignment:Text.AlignVCenter;font.pixelSize:Theme.typography.metadata}PanelLabel {id:sectionCaption;text:section.modelData.title;height:parent.parent.height;verticalAlignment:Text.AlignVCenter;font.pixelSize:Theme.textRole("label");font.weight:Font.DemiBold}}
                    }
                    Column {
                        visible:section.expanded;width:parent.width;spacing:6;bottomPadding:12
                        Repeater {
                            model:section.modelData.rows
                            Row {
                                required property var modelData
                                width:parent.width;spacing:10;visible:!!root.details[modelData.key]
                                PanelLabel {width:68+(Theme.textRole("metadata")-11)*6;text:parent.modelData.label+":";horizontalAlignment:Text.AlignRight;font.pixelSize:Theme.textRole("metadata");color:Theme.colors.muted}
                                TextEdit {width:parent.width-78-(Theme.textRole("metadata")-11)*6;readOnly:true;selectByMouse:true;wrapMode:TextEdit.WrapAnywhere;text:root.details[parent.modelData.key] || "";color:Theme.colors.text;font.family:Theme.font;font.pixelSize:Theme.textRole("metadata");Accessible.name:parent.modelData.label+": "+text}
                            }
                        }
                    }
                }
            }
        }
        }
    }
}
