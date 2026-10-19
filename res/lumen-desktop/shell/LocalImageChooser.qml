import QtQuick
import QtQuick.Controls
Popup {
    id:root
    required property var service
    property string selectedPath:""
    signal chosen(string path)
    parent:Overlay.overlay
    modal:true;focus:true
    anchors.centerIn:parent
    width:Math.min(600,parent?parent.width-24:600)
    height:Math.min(540,parent?parent.height-24:540)
    padding:16
    closePolicy:Popup.CloseOnEscape
    background:Rectangle {color:Theme.colors.background;radius:12; border.color:Theme.outline}
    onOpened:{selectedPath="";service.browse();Qt.callLater(()=>location.forceActiveFocus());}
    contentItem:ScrollView {
      clip:true
      ScrollBar.vertical:DesktopScrollBar {}
      Column {
        width:root.availableWidth
        spacing:8
        PanelLabel {width:parent.width;text:"Choose Wallpaper";font.weight:Font.DemiBold}
        PanelLabel {width:parent.width;wrapMode:Text.Wrap;text:"Browse local folders or enter an absolute image path. Personal files remain in place.";color:Theme.colors.muted}
        TextField {id:location;objectName:"wallpaper-location";width:parent.width;placeholderText:"Folder or image path";text:root.service.folder;font.family:Theme.font;font.pixelSize:Theme.textSize(13);color:Theme.colors.text;selectionColor:Theme.colors.accent;selectedTextColor:Theme.colors.selectedText;background:Rectangle {radius:6;color:Theme.colors.field;border.color:location.activeFocus?Theme.colors.accent:Theme.outline}
            Accessible.name:"Local image or folder path";onAccepted:{if(/\.(png|jpe?g)$/i.test(text)){root.selectedPath=text;}else root.service.browse(text);}}
        DialogButton {objectName:"wallpaper-folder-up";text:"Parent folder";enabled:!root.service.busy;onClicked:root.service.browse(root.service.parentFolder)}
        ListView {id:files;objectName:"wallpaper-image-list";width:parent.width;height:Math.max(80,root.availableHeight-270);clip:true;model:root.service.entries;currentIndex:-1;keyNavigationEnabled:true;ScrollBar.vertical:DesktopScrollBar {}
            delegate:ItemDelegate {required property var modelData;required property int index;width:files.width;height:Math.max(32,label.implicitHeight+12);text:modelData.name;highlighted:files.currentIndex===index;font.pixelSize:Theme.textSize(13)
                contentItem:PanelLabel {id:label;text:(modelData.directory?"▸ ":"")+modelData.name;elide:Text.ElideMiddle;color:highlighted?Theme.colors.selectedText:Theme.colors.text}
                background:Rectangle {color:highlighted?Theme.colors.accent:parent.hovered?Theme.selection:"transparent";radius:5}
                onClicked:{files.currentIndex=index;if(modelData.directory){root.selectedPath="";root.service.browse(modelData.path);}else root.selectedPath=modelData.path;}
            }
            Keys.onReturnPressed:{if(currentIndex<0)return;const entry=root.service.entries[currentIndex];if(entry.directory){root.selectedPath="";root.service.browse(entry.path);}else root.selectedPath=entry.path;}
        }
        PanelLabel {width:parent.width;wrapMode:Text.WrapAnywhere;text:root.selectedPath || (root.service.truncated?"Showing the first 1,000 entries. Enter a more specific folder.":"Select an image.");color:Theme.colors.muted}
        StatusMessage {width:parent.width;text:root.service.browseError;severity:"error"}
        Row {spacing:8
            DialogButton {objectName:"wallpaper-image-confirm";text:"Choose image";enabled:!!root.selectedPath && !root.service.busy;onClicked:{root.chosen(root.selectedPath);root.close();}}
            DialogButton {objectName:"wallpaper-image-cancel";text:"Cancel";onClicked:root.close()}
        }
    }
      }
}
