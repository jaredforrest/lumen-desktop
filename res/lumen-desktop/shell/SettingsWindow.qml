import QtQuick
import QtQuick.Controls
import QtQuick.Window
import QtQuick.Effects
import Quickshell
import "ScrollGeometry.js" as ScrollGeometry

FloatingWindow {
    id:root
    readonly property bool selectionActive:content.Window.window?content.Window.window.active:false
    readonly property color selectionFill:selectionActive?Theme.colors.accent:Theme.selection
    readonly property color selectionText:selectionActive?Theme.colors.selectedText:Theme.colors.text
    required property var shell
    required property var service
    title:"System Settings"
    visible:false
    implicitWidth:790;implicitHeight:640
    minimumSize:Qt.size(480,320)
    color:"transparent"
    surfaceFormat.opaque:false
    SearchHistoryState {id:settingsHistory;active:root.visible && root.page==="Search"}
    ClipboardState {id:settingsClipboard;active:root.visible && root.page==="Search";listWhenActive:false}
    property bool minimized:false
    property string page:"Appearance"
    property var history:["Appearance"]
    property int historyIndex:0
    readonly property int sidebarWidth:width<680?182:214
    readonly property var pages:[
        {name:"Wi-Fi",icon:"wifi",color:"#007aff",terms:"wireless network internet"},
        {name:"Bluetooth",icon:"bluetooth",color:"#007aff",terms:"devices pairing"},
        {name:"VPN",icon:"network",color:"#007aff",terms:"mullvad tailscale private network exit node"},
        {name:"General",icon:"gear",color:"#8e8e93",terms:"about system version operating"},
        {name:"Privacy & Security",icon:"gear",color:"#8e8e93",terms:"passwords bitwarden encryption secure boot"},
        {name:"Appearance",icon:"contrast",color:"#56565b",terms:"light dark auto theme accent colour sunset icons"},
        {name:"Wallpaper",icon:"screen",color:"#56565b",terms:"image picture background displays"},
        {name:"Input Devices",icon:"controls",color:"#56565b",terms:"keyboard mouse trackpad pointer tap scroll repeat"},
        {name:"Accessibility",icon:"person",color:"#007aff",terms:"reduce motion animations"},
        {name:"Desktop & Dock",icon:"screen",color:"#007aff",terms:"magnification icons"},
        {name:"Displays",icon:"sun",color:"#007aff",terms:"brightness screen monitor resolution"},
        {name:"Sound",icon:"speaker",color:"#ff3b30",terms:"volume mute output audio speaker"},
        {name:"Internet Accounts",icon:"person",color:"#007aff",terms:"calendar google icloud accounts"},
        {name:"Notifications",icon:"bell",color:"#ff3b30",terms:"focus do not disturb alerts"},
        {name:"Search",icon:"search",color:"#56565b",terms:"files applications search index"}
    ]
    function fitScreen() {
        if(!visible || !screen)return;
        const fitWidth=Math.max(480,Math.min(width,screen.width-32));
        const fitHeight=Math.max(320,Math.min(height,screen.height-150));
        implicitWidth=fitWidth;implicitHeight=fitHeight;
        if(Quickshell.env("SWAYSOCK"))
            Quickshell.execDetached(["swaymsg",'[app_id="org.quickshell" title="^System Settings$"] resize set width '+Math.round(fitWidth)+' px height '+Math.round(fitHeight)+' px, move position center']);
    }
    Timer {id:fitTimer;interval:100;onTriggered:root.fitScreen()}
    Connections {target:root.screen;function onWidthChanged(){fitTimer.restart();}function onHeightChanged(){fitTimer.restart();}}
    function navigate(name) {
        if(!pages.some(p=>p.name===name) || page===name)return;
        history=history.slice(0,historyIndex+1).concat([name]);historyIndex=history.length-1;page=name;
    }
    function openPage(name) {
        if(name)navigate(name);
        if(minimized && Quickshell.env("SWAYSOCK")){
            Quickshell.execDetached(["swaymsg",'[app_id="org.quickshell" title="^System Settings$"] move workspace current, focus']);
            minimized=false;
        }
        const wasVisible=visible;
        visible=true;service.refresh();if(!wasVisible)fitTimer.restart();
        Qt.callLater(()=>{if(content.Window.window){if(content.Window.window.visibility===Window.Minimized)content.Window.window.showNormal();content.Window.window.requestActivate();}content.forceActiveFocus();});
    }
    function back(delta) {
        const next=historyIndex+delta;if(next<0 || next>=history.length)return;
        historyIndex=next;page=history[next];
    }
    function revealFocus(){
        if(!content.Window.window)return;
        const item=content.Window.window.activeFocusItem;
        if(!item)return;
        for(const viewport of [scroller,sidebarScroll.contentItem]){
            if(!viewport || viewport.contentY===undefined)continue;
            let ancestor=item;
            while(ancestor && ancestor!==viewport.contentItem)ancestor=ancestor.parent;
            if(!ancestor)continue;
            const at=item.mapToItem(viewport.contentItem,0,0);
            viewport.contentY=ScrollGeometry.reveal(viewport.contentY,viewport.height,viewport.contentHeight,at.y,item.height,8);
        }
    }
    Connections {target:content.Window.window;function onActiveFocusItemChanged(){Qt.callLater(root.revealFocus);}}
    Connections {target:Theme;function onTextScaleChanged(){Qt.callLater(root.revealFocus);}}
    onWidthChanged:Qt.callLater(root.revealFocus)
    onHeightChanged:Qt.callLater(root.revealFocus)
    onPageChanged:scroller.contentY=0
    Rectangle {id:windowMask;width:root.width;height:root.height;radius:Theme.nativeWindowFrame?0:12;layer.enabled:!Theme.nativeWindowFrame && GraphicsInfo.api !== GraphicsInfo.Software;visible:false}
    Item {
        layer.enabled:!Theme.nativeWindowFrame && GraphicsInfo.api !== GraphicsInfo.Software
        layer.effect:MultiEffect {maskEnabled:true;maskSource:windowMask}
        id:content;anchors.fill:parent;focus:true
        Keys.onEscapePressed:root.visible=false
        Keys.onPressed:event=>{
            if((event.modifiers & Qt.ControlModifier) && event.key===Qt.Key_F){search.forceActiveFocus();event.accepted=true;}
            else if((event.modifiers & Qt.ControlModifier) && event.key===Qt.Key_W){root.visible=false;event.accepted=true;}
            else if(event.modifiers & Qt.AltModifier){if(event.key===Qt.Key_Left){root.back(-1);event.accepted=true;}else if(event.key===Qt.Key_Right){root.back(1);event.accepted=true;}}
        }
        Rectangle {x:root.sidebarWidth;width:parent.width-x;height:parent.height;color:Theme.colors.background}
        Rectangle {width:root.sidebarWidth;height:parent.height;color:Theme.sidebarMaterial}
        // Use the native Wayland move request so dragging keeps compositor rules.
        MouseArea {x:0;y:0;width:parent.width;height:48;onPressed:if(content.Window.window)content.Window.window.startSystemMove()}
        WindowControls {
            windowName:"System Settings"
            windowActive:content.Window.window ? content.Window.window.active : false
            onTriggered: index => {
                const win=content.Window.window;
                if(index===0)root.visible=false;
                else if(win && index===1){
                    if(Quickshell.env("SWAYSOCK")){root.minimized=true;Quickshell.execDetached(["swaymsg",'[app_id="org.quickshell" title="^System Settings$"] move scratchpad']);}
                    else win.showMinimized();
                }
                else if(win){if(win.visibility===Window.FullScreen)win.showNormal();else win.showFullScreen();}
            }
        }
        TextField {
            placeholderTextColor:Theme.colors.muted;selectionColor:Theme.colors.accent;selectedTextColor:Theme.colors.selectedText

            id:search;x:14;y:56;width:root.sidebarWidth-28;height:Math.max(30,implicitHeight+8)
            placeholderText:"Search";color:Theme.colors.text;font.family:Theme.font;font.pixelSize:Theme.textSize(13)
            selectByMouse:true;Accessible.name:"Search settings"
            background:Rectangle {radius:7;color:Theme.colors.background;border.color:search.activeFocus?Theme.colors.accent:Theme.hairline}
            onAccepted:{const found=root.pages.filter(p=>(p.name+" "+p.terms).toLowerCase().includes(text.toLowerCase()));if(found.length)root.navigate(found[0].name);}
        }
        ScrollView {
            id:sidebarScroll
            ScrollBar.vertical:DesktopScrollBar {}
            x:12;y:search.y+search.height+15;width:root.sidebarWidth-24;height:parent.height-y-12;clip:true
            Column {
                width:root.sidebarWidth-24;spacing:2
                Repeater {
                    model:root.pages.filter(p=>(p.name+" "+p.terms).toLowerCase().includes(search.text.toLowerCase()))
                    AbstractButton {
                        required property var modelData
                        width:parent.width;height:Math.max(34,Theme.sidebarIconSize+14,navigationLabel.implicitHeight+14)
                        objectName:"settings-navigation-"+modelData.name
                        Accessible.name:modelData.name
                        onClicked:root.navigate(modelData.name)
                        background:Rectangle {radius:7;color:root.page===parent.modelData.name?root.selectionFill:parent.hovered || parent.activeFocus?Theme.colors.hover:"transparent"}
                        contentItem:Item {
                            Rectangle {x:7;anchors.verticalCenter:parent.verticalCenter;width:Theme.sidebarIconSize+2;height:width;radius:5;color:modelData.color;SystemGlyph {anchors.centerIn:parent;name:modelData.icon;width:Theme.sidebarIconSize-4;height:width;ink:"white"}}
                            PanelLabel {id:navigationLabel;x:Theme.sidebarIconSize+17;anchors.verticalCenter:parent.verticalCenter;width:parent.width-x-5;text:modelData.name;wrapMode:Text.Wrap;elide:Text.ElideNone;color:root.page===modelData.name?root.selectionText:Theme.colors.text}
                        }
                    }
                }
                PanelLabel {visible:root.pages.filter(p=>(p.name+" "+p.terms).toLowerCase().includes(search.text.toLowerCase())).length===0;text:"No matching settings";width:parent.width;wrapMode:Text.WordWrap;color:Theme.colors.muted}
            }
        }
        Row {
            id:pageHeader;x:root.sidebarWidth+14;y:9;spacing:3;width:parent.width-x-12
            Repeater {
                model:[-1,1]
                AbstractButton {
                    required property int modelData
                    width:28;height:30;enabled:modelData<0?root.historyIndex>0:root.historyIndex<root.history.length-1
                    Accessible.name:modelData<0?"Back":"Forward"
                    onClicked:root.back(modelData)
                    background:Rectangle {radius:7;color:parent.hovered?Theme.colors.hover:"transparent"}
                    contentItem:SystemGlyph {name:parent.modelData<0?"chevron-left":"chevron-right";ink:Theme.colors.text;opacity:parent.enabled?1:0.3}
                }
            }
            PanelLabel {width:parent.width-62;height:Math.max(30,implicitHeight);wrapMode:Text.Wrap;elide:Text.ElideNone;verticalAlignment:Text.AlignVCenter;text:root.page;font.weight:Font.DemiBold;font.pixelSize:Theme.textSize(15)}
        }
        Flickable {
            id:scroller;objectName:"settings-content-scroll";x:root.sidebarWidth+24;y:Math.max(63,pageHeader.y+pageHeader.height+20);width:parent.width-x-24;height:parent.height-y-18
            clip:true;contentWidth:width;contentHeight:body.height;boundsBehavior:Flickable.StopAtBounds
            ScrollBar.vertical:DesktopScrollBar {}
            Column {
                id:body;width:scroller.width;spacing:16
                PanelLabel {width:parent.width;wrapMode:Text.WordWrap;visible:root.service.busy;text:root.service.applying===true?"Applying your preferences…":"Loading your preferences…";font.pixelSize:Theme.textSize(11);color:Theme.colors.muted}
                PanelLabel {visible:root.service.error!=="" || root.service.warning!=="";text:root.service.error || root.service.warning;wrapMode:Text.WordWrap;width:parent.width;color:Theme.colors.muted}
                Column {
                    visible:root.page==="Appearance";width:parent.width;spacing:14
                    Rectangle {
                        width:parent.width;height:145;radius:12;color:Theme.colors.surface;border.color:Theme.hairline
                        Row {
                            x:14;y:14;width:parent.width-28;spacing:12
                            Repeater {
                                model:["light","dark","auto"]
                                AbstractButton {
                                    required property string modelData
                                    width:(parent.width-24)/3;height:116;enabled:root.service.available && !root.service.busy
                                    Accessible.name:modelData==="auto"?"Automatic appearance":modelData+" appearance"
                                    onClicked:root.service.setPreference("mode",modelData)
                                    contentItem:Column {
                                        spacing:9
                                        Rectangle {
                                            width:parent.width;height:82;radius:8;clip:true;color:modelData==="dark"?"#27272a":"#f1f1f4"
                                            Rectangle {visible:modelData==="auto";x:parent.width/2;width:parent.width/2;height:parent.height;color:"#27272a"}
                                            Rectangle {x:8;y:10;width:parent.width-16;height:8;radius:3;color:modelData==="dark"?"#636368":"#c6c6cd"}
                                            Rectangle {x:8;y:26;width:parent.width*.27;height:45;radius:4;color:modelData==="dark"?"#424248":"#d8d8df"}
                                            Rectangle {x:parent.width*.4;y:28;width:parent.width*.5;height:6;radius:2;color:"#b1b1b9"}
                                            Rectangle {x:parent.width*.4;y:40;width:parent.width*.4;height:6;radius:2;color:"#b1b1b9"}
                                            Rectangle {anchors.fill:parent;radius:8;color:"transparent";border.width:root.service.preferences.mode===modelData?3:1;border.color:root.service.preferences.mode===modelData?Theme.colors.accent:Theme.outline}
                                        }
                                        PanelLabel {width:parent.width;text:modelData==="auto"?"Auto":modelData==="dark"?"Dark":"Light";horizontalAlignment:Text.AlignHCenter;font.pixelSize:Theme.textSize(12)}
                                    }
                                    background:Rectangle {color:"transparent";border.color:parent.activeFocus?Theme.colors.accent:"transparent";radius:8}
                                }
                            }
                        }
                    }
                    Rectangle {
                        width:parent.width;height:accentChoices.y+accentChoices.height+14;radius:10;color:Theme.colors.surface;border.color:Theme.hairline
                        PanelLabel {id:accentTitle;x:14;y:12;width:parent.width-28;wrapMode:Text.Wrap;text:"Accent colour"}
                        Flow {
                            id:accentChoices;x:14;y:accentTitle.y+accentTitle.height+12;width:parent.width-28;spacing:12
                            Repeater {
                                model:[{key:"graphite",color:"#8e8e93"},{key:"blue",color:"#007aff"},{key:"purple",color:"#a855c7"},{key:"pink",color:"#ee5d9b"},{key:"red",color:"#ff453a"},{key:"orange",color:"#ff9f0a"},{key:"yellow",color:"#ffd60a"},{key:"green",color:"#30b65c"}]
                                AbstractButton {
                                    required property var modelData
                                    width:24;height:24;enabled:root.service.available && !root.service.busy
                                    objectName:"settings-accent-"+modelData.key
                                    Accessible.name:modelData.key+" accent"
                                    onClicked:root.service.setPreference("accent",modelData.key)
                                    background:Rectangle {radius:12;color:parent.modelData.color;border.width:root.service.preferences.accent===parent.modelData.key || parent.activeFocus?3:0;border.color:Theme.colors.text}
                                }
                            }
                        }
                    }
                    Rectangle {
                        objectName:"settings-icon-style-card";width:parent.width;height:iconStyleDescription.y+iconStyleDescription.height+14;radius:10;color:Theme.colors.surface;border.color:Theme.hairline
                        PanelLabel {id:iconStyleTitle;x:14;y:12;width:parent.width-28;wrapMode:Text.Wrap;text:"Icon style"}
                        Flow {
                            id:iconStyleChoices;objectName:"settings-icon-style-choices";x:14;y:iconStyleTitle.y+iconStyleTitle.height+12;width:parent.width-28;spacing:8
                            Repeater {
                                model:[{key:"default",label:"Default"},{key:"auto",label:"Automatic"},{key:"dark",label:"Dark"}]
                                AbstractButton {
                                    required property var modelData
                                    width:Math.min(parent.width,Math.max(90,iconChoiceLabel.implicitWidth+20));height:Math.max(29,iconChoiceLabel.implicitHeight+12)
                                    enabled:root.service.available && !root.service.busy
                                    objectName:"settings-icon-choice-"+modelData.key
                                    Accessible.name:modelData.label+" icon style"
                                    onClicked:root.service.setPreference("iconAppearance",modelData.key)
                                    contentItem:PanelLabel {id:iconChoiceLabel;wrapMode:Text.Wrap;elide:Text.ElideNone;text:parent.modelData.label;horizontalAlignment:Text.AlignHCenter;verticalAlignment:Text.AlignVCenter;font.pixelSize:Theme.textSize(12);color:(root.service.preferences.iconAppearance || "auto")===parent.modelData.key?Theme.colors.selectedText:Theme.colors.text}
                                    background:Rectangle {radius:6;color:(root.service.preferences.iconAppearance || "auto")===parent.modelData.key?Theme.colors.accent:Theme.colors.field;border.color:parent.activeFocus?Theme.colors.accent:"transparent"}
                                }
                            }
                        }
                        PanelLabel {id:iconStyleDescription;objectName:"settings-icon-style-description";x:14;y:iconStyleChoices.y+iconStyleChoices.height+10;width:parent.width-28;wrapMode:Text.WordWrap;font.pixelSize:Theme.textSize(11);color:Theme.colors.muted;text:"Automatic follows appearance. Apps without a dark icon keep their original artwork."}
                    }
                    PanelLabel {width:parent.width;wrapMode:Text.WordWrap;text:root.service.preferences.mode==="auto"?"Automatic appearance changes at sunrise and sunset in London.":"Automatic sunrise and sunset switching is paused while this appearance is selected.";color:Theme.colors.muted;font.pixelSize:Theme.textSize(12)}
                    PanelLabel {width:parent.width;wrapMode:Text.WordWrap;text:"Sunrise "+Qt.formatDateTime(new Date(root.service.appearance.sunrise || 0),"h:mm AP")+"   ·   Sunset "+Qt.formatDateTime(new Date(root.service.appearance.sunset || 0),"h:mm AP");color:Theme.colors.muted;font.pixelSize:Theme.textSize(12)}
                }
                Column {
                    visible:root.page==="Desktop & Dock";width:parent.width;spacing:12
                    ClockSettings {width:parent.width;service:root.service}
                    HotCornerSettings {width:parent.width}
                    PanelLabel {text:"Desktop files";font.weight:Font.DemiBold}
                    SettingsToggle {width:parent.width;text:"Show files on the desktop";detail:"Display the configured Desktop folder on the primary display";checked:root.service.desktopPreferences.showFiles===true;enabled:root.service.available && !root.service.busy;onClicked:root.service.setDesktopPreference("showFiles",checked)}
                    DesktopComboBox {width:parent.width;model:["Name","Modified","Size","Kind"];currentIndex:["name","modified","size","kind"].indexOf(root.service.desktopPreferences.fileSort || "name");enabled:root.service.available && !root.service.busy;Accessible.name:"Sort desktop files";onActivated:index=>root.service.setDesktopPreference("fileSort",["name","modified","size","kind"][index])}
                    SettingsToggle {width:parent.width;text:"Descending desktop order";checked:root.service.desktopPreferences.fileDescending===true;enabled:root.service.available && !root.service.busy;onClicked:root.service.setDesktopPreference("fileDescending",checked)}
                    PanelLabel {text:"Windows";font.weight:Font.DemiBold}
                    SettingsToggle {width:parent.width;text:"Leave margins around tiled windows";checked:root.service.windowsPreferences.tileMargins!==false;enabled:root.service.available && !root.service.busy;onClicked:root.service.setWindowPreference("tileMargins",checked)}
                    SettingsToggle {width:parent.width;text:"Drag windows to screen edges to tile";checked:root.service.windowsPreferences.edgeTiling!==false;enabled:root.service.available && !root.service.busy;onClicked:root.service.setWindowPreference("edgeTiling",checked)}
                    SettingsToggle {width:parent.width;text:"Hold Option while dragging to tile";checked:root.service.windowsPreferences.optionTiling!==false;enabled:root.service.available && !root.service.busy;onClicked:root.service.setWindowPreference("optionTiling",checked)}
                    SettingsToggle {width:parent.width;text:"Drag windows to the menu bar to fill the screen";checked:root.service.windowsPreferences.topEdgeFill!==false;enabled:root.service.available && !root.service.busy;onClicked:root.service.setWindowPreference("topEdgeFill",checked)}
                    SettingsToggle {width:parent.width;text:"Hold a window at the top edge to open Overview";checked:root.service.windowsPreferences.topEdgeOverview!==false;enabled:root.service.available && !root.service.busy;onClicked:root.service.setWindowPreference("topEdgeOverview",checked)}
                    PanelLabel {text:"Tile gap: "+Math.round(tileGapSlider.value)}
                    SystemSlider {id:tileGapSlider;width:parent.width;from:0;to:32;stepSize:1;value:root.service.windowsPreferences.tileGap ?? 8;enabled:root.service.available && !root.service.busy && root.service.windowsPreferences.tileMargins!==false;Accessible.name:"Tile gap";onMoved:if(!pressed)root.service.setWindowPreference("tileGap",Math.round(value));onPressedChanged:if(!pressed && Math.round(value)!==(root.service.windowsPreferences.tileGap ?? 8))root.service.setWindowPreference("tileGap",Math.round(value))}
                    PanelLabel {text:"Double-click a title bar to"}
                    DesktopComboBox {width:parent.width;model:["Fill","Minimize","Do nothing"];currentIndex:["fill","minimize","none"].indexOf(root.service.windowsPreferences.titlebarDoubleClick || "fill");enabled:root.service.available && !root.service.busy;Accessible.name:"Title bar double-click action";onActivated:index=>root.service.setWindowPreference("titlebarDoubleClick",["fill","minimize","none"][index])}
                    PanelLabel {text:"Dock position"}
                    DesktopComboBox {width:parent.width;model:["Bottom","Left","Right"];currentIndex:["bottom","left","right"].indexOf(root.service.preferences.dockPosition || "bottom");enabled:root.service.available && !root.service.busy;Accessible.name:"Dock position";onActivated:index=>root.service.setPreference("dockPosition",["bottom","left","right"][index])}
                    PanelLabel {text:"Dock size"}
                    DesktopComboBox {width:parent.width;model:["Small","Medium","Large"];currentIndex:(root.service.preferences.dockSize || 48)<40?0:(root.service.preferences.dockSize || 48)>60?2:1;enabled:root.service.available && !root.service.busy;Accessible.name:"Dock size";onActivated:index=>root.service.setPreference("dockSize",[32,48,72][index])}
                    SettingsToggle {width:parent.width;text:"Automatically hide the Dock";checked:!!root.service.preferences.dockAutoHide;enabled:root.service.available && !root.service.busy;onClicked:root.service.setPreference("dockAutoHide",checked)}
                    PanelLabel {text:"Dock files";font.weight:Font.DemiBold}
                    Repeater {model:root.service.preferences.dockFiles || []
                        Row {required property string modelData;required property int index;width:parent.width;spacing:8
                            PanelLabel {width:parent.width-90;height:30;verticalAlignment:Text.AlignVCenter;text:modelData;textFormat:Text.PlainText;elide:Text.ElideMiddle}
                            DialogButton {width:82;text:"Remove";enabled:root.service.available && !root.service.busy;onClicked:root.service.setPreference("dockFiles",root.service.preferences.dockFiles.filter((_,i)=>i!==index))}
                        }
                    }
                    Row {width:parent.width;spacing:8
                        TextField {id:dockFilePath;width:parent.width-90;placeholderText:"/absolute/file/path";Accessible.name:"Add file to Dock";color:Theme.colors.text;background:Rectangle {radius:6;color:Theme.colors.field;border.color:dockFilePath.activeFocus?Theme.colors.accent:Theme.hairline}}
                        DialogButton {width:82;text:"Add File";enabled:root.service.available && !root.service.busy && dockFilePath.text.startsWith("/") && !(root.service.preferences.dockFiles || []).includes(dockFilePath.text);onClicked:{root.service.setPreference("dockFiles",(root.service.preferences.dockFiles || []).concat([dockFilePath.text]));dockFilePath.text="";}}
                    }
                    PanelLabel {text:"Dock folders";font.weight:Font.DemiBold}
                    Repeater {model:root.service.preferences.dockFolders || []
                        Column {required property var modelData;required property int index;width:parent.width;spacing:4
                            PanelLabel {width:parent.width;text:modelData.path;textFormat:Text.PlainText;elide:Text.ElideMiddle}
                            Row {width:parent.width;spacing:5
                                DesktopComboBox {width:(parent.width-10)/3;model:["Name","Modified","Size","Type"];currentIndex:["name","modified","size","type"].indexOf(modelData.sort || "modified");enabled:root.service.available && !root.service.busy;Accessible.name:"Sort folder "+modelData.path;onActivated:choice=>{const folders=root.service.preferences.dockFolders.slice();folders[index]=Object.assign({},modelData,{sort:["name","modified","size","type"][choice]});root.service.setPreference("dockFolders",folders);}}
                                DesktopComboBox {width:(parent.width-10)/3;model:["Automatic","List","Grid"];currentIndex:["auto","list","grid"].indexOf(modelData.presentation || "auto");enabled:root.service.available && !root.service.busy;Accessible.name:"Folder presentation "+modelData.path;onActivated:choice=>{const folders=root.service.preferences.dockFolders.slice();folders[index]=Object.assign({},modelData,{presentation:["auto","list","grid"][choice]});root.service.setPreference("dockFolders",folders);}}
                                DialogButton {width:(parent.width-10)/3;text:"Remove";enabled:root.service.available && !root.service.busy;onClicked:root.service.setPreference("dockFolders",root.service.preferences.dockFolders.filter((_,i)=>i!==index))}
                            }
                        }
                    }
                    Row {width:parent.width;spacing:8
                        TextField {id:dockFolderPath;width:parent.width-90;placeholderText:"/absolute/folder/path";Accessible.name:"Add folder to Dock";color:Theme.colors.text;background:Rectangle {radius:6;color:Theme.colors.field;border.color:dockFolderPath.activeFocus?Theme.colors.accent:Theme.hairline}}
                        DialogButton {width:82;text:"Add Folder";enabled:root.service.available && !root.service.busy && dockFolderPath.text.startsWith("/") && !(root.service.preferences.dockFolders || []).some(f=>f.path===dockFolderPath.text);onClicked:{root.service.setPreference("dockFolders",(root.service.preferences.dockFolders || []).concat([{path:dockFolderPath.text,sort:"modified",descending:true,presentation:"auto"}]));dockFolderPath.text="";}}
                    }
                    PanelLabel {text:"Show scroll bars"}
                    DesktopComboBox {width:parent.width;model:["Automatically","When scrolling","Always"];currentIndex:["auto","when_scrolling","always"].indexOf(root.service.preferences.scrollbars || "auto");enabled:root.service.available && !root.service.busy;Accessible.name:"Show scroll bars";onActivated:index=>root.service.setPreference("scrollbars",["auto","when_scrolling","always"][index])}
                    PanelLabel {text:"Sidebar icon size"}
                    DesktopComboBox {width:parent.width;model:["Small","Medium","Large"];currentIndex:["small","medium","large"].indexOf(root.service.preferences.sidebarIconSize || "medium");enabled:root.service.available && !root.service.busy;Accessible.name:"Sidebar icon size";onActivated:index=>root.service.setPreference("sidebarIconSize",["small","medium","large"][index])}
                    SettingsToggle {width:parent.width;text:"Show recent applications";detail:"Include recently used apps after pinned apps";checked:root.service.preferences.dockRecentApps!==false;enabled:root.service.available && !root.service.busy;onClicked:root.service.setPreference("dockRecentApps",checked)}
                    SettingsToggle {width:parent.width;text:"Show indicators for open applications";checked:root.service.preferences.dockIndicators!==false;enabled:root.service.available && !root.service.busy;onClicked:root.service.setPreference("dockIndicators",checked)}
                    SettingsToggle {width:parent.width;text:"Magnification";detail:"Enlarge Dock icons when you move the pointer over them";checked:!!root.service.preferences.dockMagnification;enabled:root.service.available && !root.service.busy;onClicked:root.service.setPreference("dockMagnification",checked)}
                }
                Column {
                    visible:root.page==="Accessibility";width:parent.width;spacing:12
                    PanelLabel {width:parent.width;text:"Interface text size";wrapMode:Text.Wrap;font.weight:Font.DemiBold}
                    DesktopComboBox {objectName:"settings-text-size";width:parent.width;model:["Default (100%)","Larger (115%)","Large (130%)","Largest (150%)"];currentIndex:[1,1.15,1.3,1.5].indexOf(root.service.preferences.textScale || 1);Accessible.name:"Interface text size";enabled:root.service.available && !root.service.busy;onActivated:index=>root.service.setPreference("textScale",[1,1.15,1.3,1.5][index])}
                    PanelLabel {width:parent.width;wrapMode:Text.Wrap;text:"Enlarges shared desktop text and supported controls. Individual app interfaces and output scaling have separate settings.";color:Theme.colors.muted}
                    SettingsToggle {objectName:"settings-increase-contrast";width:parent.width;text:"Increase contrast";detail:"Stronger text and boundaries with opaque desktop materials";checked:!!root.service.preferences.increaseContrast;enabled:root.service.available && !root.service.busy;onClicked:root.service.setPreference("increaseContrast",checked)}
                    SettingsToggle {width:parent.width;text:"Reduce transparency";detail:"Use opaque desktop sidebars and popovers";checked:!!root.service.preferences.reduceTransparency;enabled:root.service.available && !root.service.busy;onClicked:root.service.setPreference("reduceTransparency",checked)}
                    SettingsToggle {objectName:"settings-reduce-motion";width:parent.width;text:"Reduce motion";detail:"Reduce desktop interface animations";checked:!!root.service.preferences.reduceMotion;enabled:root.service.available && !root.service.busy;onClicked:root.service.setPreference("reduceMotion",checked)}
                }
                Column {
                    visible:root.page==="General";width:parent.width;spacing:14
                    Image {source:"reference-icons/settings.svg";width:70;height:70;sourceSize:Qt.size(140,140)}
                    PanelLabel {text:root.service.system.system || "System information unavailable";width:parent.width;wrapMode:Text.WordWrap;font.pixelSize:Theme.textSize(18);font.weight:Font.DemiBold}
                    PanelLabel {text:"Desktop · "+(root.service.system.desktop || "Wayland");width:parent.width;wrapMode:Text.WordWrap}
                }
                WallpaperSettingsPane {visible:root.page==="Wallpaper";active:root.visible && visible;width:parent.width}
                InputSettingsPane {visible:root.page==="Input Devices";active:root.visible && visible;width:parent.width}
                PrivacySecurityPane {
                    visible:root.page==="Privacy & Security";width:parent.width
                    active:root.visible && visible
                }
                Column {
                    visible:root.page==="Sound";width:parent.width;spacing:12
                    SoundControl {width:parent.width;shell:root.shell}
                    SettingsToggle {width:parent.width;text:"Mute";checked:root.shell.audioState.muted;enabled:root.shell.audioState.available;onClicked:root.shell.audioState.toggleMute()}
                    PanelLabel {text:"Output";font.weight:Font.DemiBold}
                    Repeater {
                        model:root.shell.audioState.outputs
                        ControlRow {required property var modelData;width:parent.width;text:modelData.nickname || modelData.description || modelData.name;selected:root.shell.audioState.sink===modelData;onClicked:root.shell.audioState.selectOutput(modelData)}
                    }
                    PanelLabel {visible:root.shell.audioState.outputs.length===0;text:"No output devices available";color:Theme.colors.muted}
                    PanelLabel {text:"Input";font.weight:Font.DemiBold}
                    Repeater {model:root.shell.audioState.inputs
                        ControlRow {required property var modelData;width:parent.width;text:modelData.nickname || modelData.description || modelData.name;selected:root.shell.audioState.source===modelData;onClicked:root.shell.audioState.selectInput(modelData)}
                    }
                    Loader {width:parent.width;active:root.shell.audioState.inputAvailable;sourceComponent:Component {AudioDeviceControl {node:root.shell.audioState.source}}}
                    PanelLabel {text:"Applications";font.weight:Font.DemiBold;visible:root.shell.audioState.streams.length>0}
                    Repeater {model:root.shell.audioState.streams;AudioDeviceControl {required property var modelData;node:modelData}}
                    ControlRow {width:parent.width;text:"Advanced Sound Settings…";detail:"Input devices and per-application volume";onClicked:AppLaunch.run(["pavucontrol"])}
                }
                Column {
                    visible:root.page==="Displays";width:parent.width;spacing:14
                    Repeater {
                        model:Quickshell.screens
                        Rectangle {
                            required property var modelData
                            width:parent.width;height:86;radius:10;color:Theme.colors.surface;border.color:Theme.hairline
                            SystemGlyph {x:16;y:22;name:"screen";width:40;height:35;ink:Theme.colors.accent}
                            Column {x:72;y:20;spacing:8
                                PanelLabel {text:modelData.name;font.weight:Font.DemiBold}
                                PanelLabel {text:modelData.width+" × "+modelData.height+" points";color:Theme.colors.muted}
                            }
                        }
                    }
                    Loader {width:parent.width;height:item?item.implicitHeight:0;active:root.visible && root.page==="Displays";sourceComponent:Component{BrightnessControl {width:body.width}}}
                    Loader {width:parent.width;height:item?item.implicitHeight:0;active:root.visible && root.page==="Displays";sourceComponent:Component {DisplaySettingsPane {width:body.width}}}
                }
                Loader {width:parent.width;height:item?item.implicitHeight:0;active:root.visible && root.page==="Internet Accounts";sourceComponent:Component {CalendarAccountsPane {width:body.width}}}
                Column {
                    visible:root.page==="Notifications";width:parent.width;spacing:12
                    PanelLabel {text:"Show notification previews"}
                    DesktopComboBox {width:parent.width;model:["Always","When Unlocked","Never"];currentIndex:["always","when_unlocked","never"].indexOf(root.service.notificationsPreferences.preview);enabled:root.service.available && !root.service.busy;Accessible.name:"Show notification previews";onActivated:index=>root.service.setNotificationPreference("preview",["always","when_unlocked","never"][index])}
                    PanelLabel {width:parent.width;wrapMode:Text.WordWrap;text:"Never hides message content and actions while keeping the source application and time visible. Previews while locked also require system permission.";font.pixelSize:Theme.textSize(11);color:Theme.colors.muted}
                    SettingsToggle {width:parent.width;text:"Do Not Disturb";detail:"Silence banners while keeping notifications in Notification Centre";checked:root.shell.notifications.dnd;enabled:root.shell.notifications.available;onClicked:root.shell.notifications.command({action:"dnd",enabled:checked})}
                    Repeater {model:root.shell.notifications.applications
                        Column {required property var modelData;width:parent.width;spacing:5
                            PanelLabel {width:parent.width;text:modelData.name;textFormat:Text.PlainText;font.weight:Font.DemiBold}
                            DesktopComboBox {width:parent.width;model:["Banners and history","History only","Off"];currentIndex:["banners","silent","off"].indexOf(root.shell.notifications.policies[parent.modelData.key] || "banners");Accessible.name:parent.modelData.name+" notification policy";onActivated:index=>root.shell.notifications.command({action:"policy",app:parent.modelData.key,policy:["banners","silent","off"][index]})}
                        }
                    }
                    ControlRow {width:parent.width;text:"Open Notification Centre";detail:root.shell.notifications.history.length+" saved notifications";onClicked:{root.visible=false;root.shell.showMenu("Date");}}
                    PanelLabel {width:parent.width;wrapMode:Text.WordWrap;text:root.shell.notifications.available?"Notifications are available.":"Connecting to notifications…";color:Theme.colors.muted}
                    PanelLabel {visible:root.shell.notifications.error!=="";width:parent.width;wrapMode:Text.WordWrap;text:root.shell.notifications.error;color:Theme.colors.muted}
                }
                Loader {
                    width:parent.width;height:item?item.height:0
                    active:root.visible && ["Wi-Fi","Bluetooth"].includes(root.page)
                    sourceComponent:Component {ConnectivityPanel {width:body.width;kind:root.page;controls:root.shell.systemState;embedded:true}}
                }
                Loader {width:parent.width;height:item?item.implicitHeight:0;active:root.visible && root.page==="VPN";sourceComponent:Component {VpnPanel {width:body.width;service:root.shell.vpn}}}
                Column {
                    visible:root.page==="Search";width:parent.width;spacing:12
                    PanelLabel {width:parent.width;wrapMode:Text.WordWrap;text:"Search applications, personal folders and ~/data.";font.pixelSize:Theme.textSize(14)}
                    PanelLabel {width:parent.width;wrapMode:Text.WordWrap;text:"Desktop, Documents, Downloads, Pictures, Music, Videos and data. Filters: kind:folder, kind:file, ext:pdf, in:Documents, since:2026-01-01, before:2027-01-01. Dates filter the last modification time.";color:Theme.colors.muted}
                    SettingsToggle {width:parent.width;text:"Save recent search queries";detail:"Keep up to 20 queries on this device. Queries can contain sensitive text. Clipboard searches are excluded.";checked:root.service.searchPreferences.queryHistory===true;enabled:root.service.available && !root.service.busy;onClicked:root.service.setSearchPreference("queryHistory",checked)}
                    ControlRow {width:parent.width;text:"Clear Search History";enabled:!settingsHistory.busy;onClicked:settingsHistory.clear()}
                    PanelLabel {visible:!!settingsHistory.error;width:parent.width;wrapMode:Text.WordWrap;text:settingsHistory.error;color:Theme.colors.muted}
                    PanelLabel {text:"Include in Search";font.weight:Font.DemiBold}
                    Repeater {model:[{id:"applications",label:"Applications"},{id:"files",label:"Files and folders"},{id:"actions",label:"Desktop and application actions"},{id:"clipboard",label:"Clipboard history"}]
                        SettingsToggle {required property var modelData;width:parent.width;text:modelData.label
                            checked:!(root.service.searchPreferences.disabledProviders || []).includes(modelData.id)
                            enabled:root.service.available && !root.service.busy
                            onClicked:{let disabled=(root.service.searchPreferences.disabledProviders || []).filter(id=>id!==modelData.id);if(!checked)disabled.push(modelData.id);root.service.setSearchPreference("disabledProviders",disabled);}
                        }
                    }
                    ControlRow {width:parent.width;text:"Disable Clipboard History";detail:settingsClipboard.enabled?"Stop collection and remove saved entries":"History collection is disabled";enabled:settingsClipboard.enabled && !settingsClipboard.busy;onClicked:settingsClipboard.disable()}
                    ControlRow {width:parent.width;text:"Clear Clipboard History";enabled:settingsClipboard.enabled && !settingsClipboard.busy;onClicked:settingsClipboard.clear()}
                    PanelLabel {visible:!!settingsClipboard.error;width:parent.width;wrapMode:Text.WordWrap;text:settingsClipboard.error;color:Theme.colors.muted}
                    PanelLabel {width:parent.width;wrapMode:Text.WordWrap;text:"Disabled providers are excluded from results. Disabling Clipboard here does not revoke collection consent; use Disable Clipboard History above to stop collection. Query history is saved only when enabled above.";color:Theme.colors.muted;font.pixelSize:Theme.textSize(11)}
                    ControlRow {width:parent.width;text:"Open Search";detail:"Super+P";onClicked:{root.visible=false;root.shell.launchApps();}}
                    ControlRow {width:parent.width;text:"Refresh file index";enabled:root.shell.search.available && !root.shell.search.scanning;onClicked:root.shell.search.refresh()}
                    PanelLabel {width:parent.width;wrapMode:Text.WordWrap;text:!root.shell.search.available?"The index starts when Search opens.":root.shell.search.scanning?"Updating the index…":root.shell.search.indexed+" files and folders indexed";color:Theme.colors.muted}
                }
            }
        }
    }
}
