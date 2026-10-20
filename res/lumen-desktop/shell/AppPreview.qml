import QtQuick
import Quickshell
import Quickshell.Io
PanelSurface {
    id: root
    required property string kind
    signal closeRequested()
    property bool isFiles:kind==="Files"
    property string page:isFiles?"Home":"Appearance"
    property var files:[]
    property bool listMode:false
    property int selectedFile:-1
    readonly property int sidebarWidth:isFiles?200:220
    readonly property int contentX:sidebarWidth+24
    width:isFiles?880:790;height:isFiles?550:610;radius:22
    color:isFiles?Theme.content:Theme.colors.background
    scale:parent?Math.min(1,(parent.width-32)/width,(parent.height-Theme.barHeight-110)/height):1
    onKindChanged:{page=kind==="Files"?"Home":"Appearance";selectedFile=-1;}
    Process {
        command:[Quickshell.env("GLOBAL_MENU_PYTHON"),"-c","import os,json; print(json.dumps(sorted([{'name':e.name,'directory':e.is_dir()} for e in os.scandir(os.path.expanduser('~')) if not e.name.startswith('.')],key=lambda e:(not e['directory'],e['name'].lower()))[:60]))"]
        running:root.isFiles
        stdout:StdioCollector {onStreamFinished:{try{root.files=JSON.parse(text);}catch(e){root.files=[];}}}
    }
    Rectangle {x:8;y:8;width:root.sidebarWidth;height:parent.height-16;radius:16;color:Theme.sidebar}
    Row {
        x:22;y:21;spacing:8
        Repeater {
            model:["#ff5f57","#febc2e","#28c840"]
            Rectangle {
                required property int index
                required property string modelData
                width:12;height:12;radius:6;color:modelData;opacity:index===0?1:0.4
                SystemGlyph {visible:closeMouse.containsMouse && parent.index===0;anchors.centerIn:parent;name:"close";width:9;height:9;ink:"#6e1916"}
                MouseArea {id:closeMouse;anchors.fill:parent;hoverEnabled:true;enabled:parent.index===0;onClicked:root.closeRequested()}
            }
        }
    }
    Column {
        x:16;y:root.isFiles?65:56;width:root.sidebarWidth-16;spacing:3
        Rectangle {
            visible:!root.isFiles;width:parent.width;height:28;radius:7;color:Theme.colors.background
            SystemGlyph {x:8;anchors.verticalCenter:parent.verticalCenter;name:"search";width:14;height:14;ink:Theme.colors.muted}
            PanelLabel {x:29;anchors.verticalCenter:parent.verticalCenter;text:"Search";color:Theme.colors.muted}
        }
        Item {width:1;height:8}
        PanelLabel {x:8;text:root.isFiles?"Favourites":"System Settings";font.pixelSize:11;font.weight:Font.DemiBold;color:Theme.colors.muted;height:22}
        Repeater {
            model:root.files?["Recents","Applications","Desktop","Documents","Downloads","Home"]:["Wi-Fi","Bluetooth","Network","General","Appearance","Accessibility","Control Centre","Desktop & Dock","Displays","Sound","Notifications"]
            Rectangle {
                required property string modelData
                required property int index
                property bool connected:root.isFiles?modelData==="Home":["Appearance","General","Sound"].includes(modelData)
                width:root.sidebarWidth-16;height:30;radius:6;color:root.page===modelData?Theme.colors.accent:"transparent"
                Rectangle {
                    x:8;anchors.verticalCenter:parent.verticalCenter;width:20;height:20;radius:5
                    color:root.isFiles?"transparent":["#007aff","#007aff","#007aff","#8e8e93","#56565b","#007aff","#8e8e93","#007aff","#007aff","#ff3b30","#ff3b30"][parent.index]
                    opacity:parent.connected?1:0.55
                    SystemGlyph {anchors.centerIn:parent;width:root.isFiles?17:14;height:width;name:root.isFiles?["clock","grid","screen","document","download","home"][index]:["wifi","bluetooth","network","gear","contrast","person","list","screen","sun","speaker","bell"][index];ink:root.isFiles?(root.page===modelData?Theme.colors.selectedText:Theme.colors.accent):"white"}
                }
                PanelLabel {x:35;anchors.verticalCenter:parent.verticalCenter;text:modelData;color:root.page===modelData?Theme.colors.selectedText:Theme.colors.text;opacity:parent.connected?1:0.55}
                MouseArea {anchors.fill:parent;enabled:parent.connected;onClicked:root.page=parent.modelData}
            }
        }
    }
    Item {
        x:root.contentX;y:14;width:parent.width-x-22;height:40
        Rectangle {
            y:4;width:58;height:28;radius:14;color:Theme.colors.surface
            Row {x:8;anchors.verticalCenter:parent.verticalCenter;spacing:10;opacity:0.35
                SystemGlyph {name:"chevron-left";width:16;height:16}
                SystemGlyph {name:"chevron-right";width:16;height:16}
            }
        }
        PanelLabel {x:72;anchors.verticalCenter:parent.verticalCenter;width:parent.width-230;text:root.page;font.pixelSize:15;font.weight:Font.DemiBold}
        Rectangle {
            visible:root.isFiles;anchors.right:parent.right;y:4;width:142;height:28;radius:14;color:Theme.colors.background
            Row {anchors.centerIn:parent;spacing:10
                Repeater {model:["grid","list","more","search"]
                    Item {required property string modelData;width:22;height:22
                        SystemGlyph {anchors.centerIn:parent;name:parent.modelData;width:16;height:16;ink:Theme.colors.text;opacity:["grid","list"].includes(name)?1:0.35}
                        MouseArea {anchors.fill:parent;enabled:["grid","list"].includes(parent.modelData);onClicked:root.listMode=parent.modelData==="list"}
                    }
                }
            }
        }
    }
    GridView {
        id:fileGrid;visible:root.isFiles && !root.listMode
        x:root.contentX;y:72;width:parent.width-x-16;height:parent.height-y-45
        cellWidth:112;cellHeight:108;clip:true;model:root.files
        delegate:Item {
            required property var modelData
            required property int index
            width:112;height:108
            Rectangle {x:20;y:4;width:72;height:62;radius:7;color:root.selectedFile===index?Theme.selection:"transparent"}
            Image {visible:modelData.directory;x:25;y:8;width:62;height:50;source:"icons/folder.svg";sourceSize.width:124;sourceSize.height:100}
            SystemGlyph {visible:!modelData.directory;x:37;y:13;width:38;height:42;name:"document";ink:Theme.colors.muted}
            Rectangle {x:4;y:72;width:104;height:28;radius:4;color:root.selectedFile===index?Theme.colors.accent:"transparent"}
            PanelLabel {x:5;y:72;width:102;height:28;text:modelData.name;horizontalAlignment:Text.AlignHCenter;verticalAlignment:Text.AlignVCenter;font.pixelSize:12;wrapMode:Text.Wrap;maximumLineCount:2;color:root.selectedFile===index?Theme.colors.selectedText:Theme.colors.text}
            MouseArea {anchors.fill:parent;onClicked:root.selectedFile=index}
        }
    }
    ListView {
        id:fileList;visible:root.isFiles && root.listMode
        x:root.contentX;y:72;width:parent.width-x-22;height:parent.height-y-45;clip:true;model:root.files
        delegate:Rectangle {
            required property var modelData
            required property int index
            width:fileList.width;height:29;radius:5;color:root.selectedFile===index?Theme.colors.accent:index%2?Theme.colors.background:"transparent"
            SystemGlyph {x:8;anchors.verticalCenter:parent.verticalCenter;name:modelData.directory?"folder":"document";width:17;height:17;ink:root.selectedFile===index?Theme.colors.selectedText:Theme.colors.accent}
            PanelLabel {x:33;width:parent.width-140;anchors.verticalCenter:parent.verticalCenter;text:modelData.name;color:root.selectedFile===index?Theme.colors.selectedText:Theme.colors.text}
            PanelLabel {anchors.right:parent.right;anchors.rightMargin:12;anchors.verticalCenter:parent.verticalCenter;text:modelData.directory?"Folder":"Document";font.pixelSize:11;color:root.selectedFile===index?Theme.colors.selectedText:Theme.colors.muted}
            MouseArea {anchors.fill:parent;onClicked:root.selectedFile=index}
        }
    }
    Column {
        visible:!root.isFiles;x:root.contentX;y:76;width:parent.width-x-24;spacing:18
        PanelLabel {text:root.page;font.pixelSize:24;font.weight:Font.DemiBold}
        Rectangle {
            visible:root.page==="Appearance";width:parent.width;height:151;radius:12;color:Theme.colors.surface;border.color:Theme.hairline
            Row {x:18;y:18;spacing:16
                Repeater {
                    model:["Light","Dark","Auto"]
                    Column {
                        required property string modelData
                        spacing:9;width:(root.width-root.contentX-92)/3
                        Rectangle {
                            width:parent.width;height:86;radius:8;color:modelData==="Dark"?"#27272a":"#f1f1f4";border.width:modelData==="Auto"?3:1;border.color:modelData==="Auto"?Theme.colors.accent:Theme.outline;clip:true
                            Rectangle {visible:modelData==="Auto";x:parent.width/2;width:parent.width/2;height:parent.height;radius:8;color:"#27272a"}
                            Rectangle {x:8;y:10;width:parent.width-16;height:8;radius:3;color:modelData==="Dark"?"#636368":"#c6c6cd"}
                            Rectangle {x:8;y:26;width:parent.width*.27;height:51;radius:4;color:modelData==="Dark"?"#424248":"#d8d8df"}
                            Rectangle {x:parent.width*.4;y:28;width:parent.width*.5;height:6;radius:2;color:modelData==="Dark"?"#53535a":"#d8d8df"}
                            Rectangle {x:parent.width*.4;y:40;width:parent.width*.4;height:6;radius:2;color:modelData==="Dark"?"#53535a":"#d8d8df"}
                            Rectangle {visible:modelData==="Auto";anchors.fill:parent;radius:8;color:"transparent";border.width:3;border.color:Theme.colors.accent}
                        }
                        PanelLabel {width:parent.width;text:modelData;horizontalAlignment:Text.AlignHCenter;font.pixelSize:12}
                    }
                }
            }
        }
        Rectangle {
            visible:root.page==="Appearance";width:parent.width;height:54;radius:11;color:Theme.colors.surface;border.color:Theme.hairline
            PanelLabel {x:14;anchors.verticalCenter:parent.verticalCenter;text:"Accent colour"}
            Row {anchors.right:parent.right;anchors.rightMargin:16;anchors.verticalCenter:parent.verticalCenter;spacing:8
                Repeater {model:["#8e8e93","#007aff","#a855c7","#ee5d9b","#ff453a","#ff9f0a","#ffd60a","#30b65c"]
                    Rectangle {required property string modelData;width:18;height:18;radius:9;color:modelData;border.width:modelData==="#007aff"?3:0;border.color:Theme.colors.text}
                }
            }
        }
        Repeater {
            model:root.page==="Appearance"?[{name:"Appearance schedule",value:"Sunrise / sunset · London"}]:root.page==="General"?[{name:"Operating system",value:"NixOS"},{name:"Desktop",value:"Sway"}]:[{name:"Output device",value:"Manage in audio settings"}]
            Rectangle {
                required property var modelData
                width:parent.width;height:46;radius:10;color:Theme.colors.surface;border.color:Theme.hairline
                PanelLabel {x:14;anchors.verticalCenter:parent.verticalCenter;text:modelData.name;font.pixelSize:12}
                PanelLabel {anchors.right:parent.right;anchors.rightMargin:14;anchors.verticalCenter:parent.verticalCenter;text:modelData.value;color:Theme.colors.muted;font.pixelSize:11}
            }
        }
        Rectangle {visible:root.page==="Sound";width:180;height:30;radius:8;color:Theme.colors.surface;border.color:Theme.hairline
            PanelLabel {anchors.centerIn:parent;text:"Open Sound Settings…";font.pixelSize:12}
            MouseArea {anchors.fill:parent;onClicked:AppLaunch.run(["pavucontrol"])}
        }
    }
    Item {
        x:root.contentX;y:parent.height-34;width:parent.width-x-22;height:24
        Rectangle {width:parent.width;height:1;color:Theme.hairline}
        PanelLabel {anchors.verticalCenter:parent.verticalCenter;text:root.isFiles?"Home  ›  "+root.files.length+" items":"Settings preview";font.pixelSize:10;color:Theme.colors.muted}
        PanelLabel {anchors.right:parent.right;anchors.verticalCenter:parent.verticalCenter;text:root.isFiles?"Read-only preview":"Controls not connected";font.pixelSize:10;color:Theme.colors.muted}
    }
}
