import QtQuick
Item {
    id:root
    property var entry:({})
    readonly property string extension:(entry.name || "").split(".").pop().toUpperCase()
    readonly property string category:entry.directory?"folder":["PNG","JPG","JPEG","WEBP","GIF","BMP","SVG"].includes(extension)?"image":["CSV","XLS","XLSX","ODS"].includes(extension)?"table":["WAV","MP3","OGG","FLAC","M4A"].includes(extension)?"audio":["ZIP","TAR","GZ","7Z"].includes(extension)?"archive":["JSON","HTML","CSS","JS","RS","NIX"].includes(extension)?"source":"document"
    property bool overrideFailed:false
    onIconOverrideChanged:overrideFailed=false
    readonly property string activeOverride:overrideFailed?"":iconOverride
    readonly property string iconOverride:{const icons=Theme.fileIconOverrides;const value=icons["."+extension.toLowerCase()] || icons[entry.kind || ""] || icons[category];return typeof value==="string"?value:"";}
    readonly property color typeColor:({image:"#a24fc3",table:"#23824a",audio:"#cf456b",archive:"#a57532",source:"#3478c6",document:"#737783",folder:"#238ce8"})[category]
    readonly property bool hasPreview:(activeOverride!=="" || (category==="image" && Number(entry.size || 0)<=33554432)) && picture.status===Image.Ready
    readonly property real visualWidth:entry.directory?width:hasPreview?picture.paintedWidth:width*.72
    readonly property real visualHeight:entry.directory?height*.8:hasPreview?picture.paintedHeight:height
    implicitWidth:48;implicitHeight:54
    Image {id:picture;anchors.fill:parent;source:root.activeOverride || (root.entry.directory?"icons/folder.svg":root.category==="image" && Number(root.entry.size || 0)<=33554432?root.entry.uri || "":"");sourceSize:Qt.size(root.width*3,root.height*3);fillMode:Image.PreserveAspectFit;onStatusChanged:if(status===Image.Error && root.activeOverride!=="")root.overrideFailed=true;asynchronous:true;smooth:true;visible:root.entry.directory || root.hasPreview}
    Rectangle {visible:!root.entry.directory && !root.hasPreview;width:parent.width*.72;height:parent.height;anchors.centerIn:parent;radius:Math.max(2,width*.08);color:Theme.dark?"#ececf0":"#ffffff";border.color:Theme.dark?"#b8b8c0":"#b9bac4";border.width:1
        Rectangle {x:parent.width*.15;y:parent.height*.2;width:parent.width*.55;height:Math.max(1,parent.height*.025);color:"#b1b3bd"}
        Rectangle {x:parent.width*.15;y:parent.height*.3;width:parent.width*.4;height:Math.max(1,parent.height*.025);color:"#b1b3bd"}
        Rectangle {width:parent.width+4;height:Math.max(4,parent.height*.3);x:-2;y:parent.height*.52;radius:2;color:root.typeColor
            Text {anchors.centerIn:parent;visible:root.width>=28;text:root.extension.slice(0,4);textFormat:Text.PlainText;font.family:Theme.font;font.pixelSize:Math.max(7,Math.min(12,parent.height*.65));font.weight:Font.Bold;color:"white"}
        }
    }
}
