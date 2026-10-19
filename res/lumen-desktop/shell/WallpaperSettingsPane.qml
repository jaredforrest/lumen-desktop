import QtQuick
import QtQuick.Controls
Column {
    id:root
    property bool active:false
    property var service:owned
    property string selected:""
    readonly property var output:service.outputs.find(item=>item.identity===selected) || null
    spacing:10
    WallpaperSettingsState {id:owned;active:root.active && root.service===owned}
    PanelLabel {width:parent.width;text:"Wallpaper";font.weight:Font.DemiBold}
    PanelLabel {width:parent.width;wrapMode:Text.Wrap;text:"Choose a still PNG or JPEG, up to 64 million pixels. Files stay in their original location. Disconnected or unreadable images use the installed background.";color:Theme.colors.muted}
    DesktopComboBox {objectName:"wallpaper-scope";width:parent.width;model:["Default for all displays"].concat(root.service.outputs.map(item=>item.name));currentIndex:root.output?root.service.outputs.findIndex(item=>item.identity===root.selected)+1:0;Accessible.name:"Wallpaper scope";enabled:!root.service.busy;onActivated:index=>root.selected=index===0?"":root.service.outputs[index-1].identity}
    PanelLabel {width:parent.width;wrapMode:Text.WrapAnywhere;text:root.output?(root.output.image || "Installed background"):(root.service.defaultImage || "Installed background")}
    Image {objectName:"wallpaper-preview";width:parent.width;height:140;fillMode:Image.PreserveAspectFit;source:{const path=root.output?root.output.image:root.service.defaultImage;return path && (root.output?!root.output.fallback:!!root.service.defaultMetadata)?"file://"+encodeURI(path).replace(/#/g,"%23").replace(/\?/g,"%3F"):Qt.resolvedUrl("wallpaper.png");} Accessible.name:"Selected wallpaper preview"}
    PanelLabel {width:parent.width;wrapMode:Text.Wrap;text:root.output && root.output.metadata?root.output.metadata.width+" × "+root.output.metadata.height+" · "+root.output.metadata.format:"Images fill the display and crop to its aspect ratio.";color:Theme.colors.muted}
    StatusMessage {width:parent.width;text:root.service.error || (root.output?root.output.error:root.service.defaultError);severity:"error"}
    DialogButton {objectName:"wallpaper-choose";text:"Choose image…";enabled:!root.service.busy;width:Math.min(implicitWidth,parent.width);onClicked:{chooser.scope=root.selected?"display":"default";chooser.identity=root.selected;chooser.open();}}
    DialogButton {objectName:"wallpaper-reset";text:root.selected?"Use default background":"Use installed background";enabled:!root.service.busy;width:Math.min(implicitWidth,parent.width);onClicked:root.service.choose(root.selected?"display":"default",root.selected,null)}
    PanelLabel {width:parent.width;wrapMode:Text.Wrap;text:"Display overrides remain saved when a display disconnects. Desktop-specific images are currently unavailable. Images are never downloaded automatically.";color:Theme.colors.muted}
    Repeater {model:root.service.disconnected
        PanelLabel {required property var modelData;width:root.width;wrapMode:Text.WrapAnywhere;text:"Saved disconnected display: "+modelData.identity+"\n"+modelData.image;color:Theme.colors.muted}
    }
    LocalImageChooser {id:chooser;service:root.service;property string scope:"default";property string identity:"";onChosen:path=>root.service.choose(scope,identity,path)}
}
