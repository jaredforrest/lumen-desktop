import QtQuick
import QtQuick.Controls
import QtQuick.Window
import Quickshell
PanelSurface {
    id: root
    required property var shell
    Keys.onEscapePressed:root.closeRequested()
    signal closeRequested()
    readonly property var disabledProviders: Theme.searchPreferences.disabledProviders || []
    readonly property var providerChoices: ["All","Applications","Files","Actions","Clipboard"].filter(name=>name==="All" || !disabledProviders.includes(name.toLowerCase()))
    function providerEnabled(name){return !disabledProviders.includes(name.toLowerCase());}
    property string provider: "All"
    onDisabledProvidersChanged:{if(provider!=="All" && !providerEnabled(provider))setProvider("All");if(!providerEnabled("Files")){service.search("");service.requestPreview("");}}
    property bool gridMode: false
    property bool previewOpen: false
    SearchHistoryState {id:queryHistory;active:root.visible}
    property int historyIndex:-1
    property string historyDraft:""
    function recallHistory(delta){if(!queryHistory.enabled || !queryHistory.queries.length)return;if(historyIndex<0)historyDraft=search.text;historyIndex=Math.max(-1,Math.min(queryHistory.queries.length-1,historyIndex+delta));search.text=historyIndex<0?historyDraft:queryHistory.queries[historyIndex];search.cursorPosition=search.text.length;}
    property int selectedIndex: 0
    readonly property string queryText:search.text
    property string selectedKey:""
    property string selectionQuery:""
    function focusSearch(){if(visible && root.Window.window && root.Window.window.visible)search.forceActiveFocus();}
    function activateResult(key){if(visible && root.Window.window && root.Window.window.visible && showingResults)launchKey(key,false);}
    function resultDescription(item){const text=item.subtitle || "";return item.kind==="file"?text.replace(Quickshell.env("HOME"),"~"):text;}
    function resultKey(item){return item?item.kind+":"+(item.path || item.id)+":"+(item.token || ""):"";}
    onSelectedIndexChanged:selectedKey=resultKey(results[selectedIndex])
    readonly property var service: shell.search
    readonly property bool showingResults: provider==="Clipboard" || gridMode || search.text.trim().length>0 || provider==="Actions"
    ClipboardState {id:clipboard;active:root.visible && root.provider==="Clipboard" && root.providerEnabled("Clipboard")}
    readonly property bool splitPreview: previewOpen && width>=740
    readonly property var current: results[selectedIndex] || null
    readonly property var actions: [
        {id:"text-tools",title:"Text Tools",subtitle:"Dictionary, read aloud and shared replacements",glyph:"document",enabled:true},
        {id:"overview",title:"Window Overview",subtitle:"Windows, workspaces and displays",glyph:"screen",enabled:true},
        {id:"focus",title:shell.notifications.dnd?"Turn Focus Off":"Turn Focus On",subtitle:shell.notifications.dnd?"Resume notification banners":"Silence notification banners",glyph:"moon",enabled:shell.notifications.available},
        {id:"mute",title:shell.audioState.muted?"Unmute Sound":"Mute Sound",subtitle:"Change the current sound output",glyph:"speaker",enabled:shell.audioState.available},
        {id:"up",title:"Increase Volume",subtitle:"Raise volume by 5%",glyph:"speaker",enabled:shell.audioState.available},
        {id:"down",title:"Decrease Volume",subtitle:"Lower volume by 5%",glyph:"speaker-low",enabled:shell.audioState.available},
        {id:"emoji",title:"Emoji & Symbols",subtitle:"Open the Character Viewer",glyph:"grid",enabled:true},
        {id:"capture",title:"Take a Screenshot",subtitle:"Open screenshot capture options",glyph:"capture",enabled:true},
        {id:"controls",title:"Open Control Centre",subtitle:"Sound, connectivity and display controls",glyph:"controls",enabled:true},
        {id:"calendar",title:"Open Calendar",subtitle:"Show calendar and notifications",glyph:"calendar",enabled:true},
        {id:"lock",title:"Lock Screen",subtitle:"Lock this desktop session",glyph:"lock",enabled:true}
    ]
    property var results: {
        const query=search.text.toLowerCase().trim(),words=query.split(/\s+/).filter(Boolean);
        const match=text=>words.every(w=>text.toLowerCase().includes(w));
        let apps=providerEnabled("Applications")?shell.entries.filter(e=>!e.noDisplay && match(shell.appLabel(e)+" "+e.id+" "+(e.genericName || "")))
            .sort((a,b)=>Number(shell.appLabel(b).toLowerCase().startsWith(query))-Number(shell.appLabel(a).toLowerCase().startsWith(query)) || shell.appLabel(a).localeCompare(shell.appLabel(b)))
            .map(e=>({kind:"app",id:e.id,title:shell.appLabel(e),subtitle:e.genericName || "Application",entry:e})):[];
        let menuRows=[];
        const exported=shell.exportedMenu;
        function collect(node,parents){
            if(!node.visible || !node.enabled)return;
            const title=exported.label(node.label),parts=parents.concat(title?[title]:[]);
            if(!node.submenu && !node.separator && title && match(parts.join(" ")))menuRows.push({kind:"menu-command",id:node.id,token:exported.state.token,title:title,subtitle:parts.join(" › "),glyph:"list"});
            for(const child of node.children || [])collect(child,parts);
        }
        if(providerEnabled("Actions") && exported.state.menu)for(const child of exported.state.menu.children)collect(child,[]);
        let actionRows=providerEnabled("Actions")?actions.filter(a=>a.enabled && match(a.title+" "+a.subtitle)).map(a=>Object.assign({kind:"action"},a)):[];
        if(provider==="Clipboard")return clipboard.entries.map(e=>({kind:"clipboard",id:e.id,title:e.text,subtitle:"Clipboard history · Select to copy",glyph:"document"}));
        if(provider==="Applications")return apps;
        if(provider==="Files")return providerEnabled("Files")?service.files:[];
        if(provider==="Actions")return actionRows.concat(menuRows);
        if(!query)return [];
        return apps.slice(0,6).concat(providerEnabled("Files")?service.files.slice(0,12):[],actionRows.slice(0,5),menuRows.slice(0,8));
    }
    width:Math.min(previewOpen?920:660,parent ? parent.width-32 : 660)
    height:footer.y+footer.height+12
    radius:22
    onVisibleChanged: if(!visible){if(provider!=="Clipboard" && search.text.trim())queryHistory.request("record",search.text);}else {
        provider="All";historyIndex=-1;historyDraft="";search.text="";selectedIndex=0;gridMode=false;previewOpen=false;service.search("");search.forceActiveFocus();
    }
    onCurrentChanged: if(visible && previewOpen)service.requestPreview(current && current.kind==="file"?current.path:"",current?current.token:"")
    onPreviewOpenChanged:if(visible)service.requestPreview(previewOpen && current && current.kind==="file"?current.path:"",current?current.token:"")
    onResultsChanged: {
        const changedQuery=selectionQuery!==queryText;
        selectionQuery=queryText;
        const previous=changedQuery?"":selectedKey;
        const index=results.findIndex(item=>resultKey(item)===previous);
        selectedIndex=index>=0?index:previous?-1:changedQuery?0:selectedIndex<0?-1:0;
        selectedKey=resultKey(results[selectedIndex]);
    }
    Connections {target:root.service;function onOpened(){if(root.visible)root.closeRequested();}function onBrowseRequested(path){if(root.visible){root.closeRequested();root.shell.openFiles(path);}}}
    function status() {return {provider:provider,count:results.length,selected:selectedIndex,kind:current?current.kind:"",preview:previewOpen,previewType:service.preview.type || "",grid:gridMode};}
    function setProvider(name) {if(!providerChoices.includes(name))return;provider=name;gridMode=false;selectedKey="";selectedIndex=0;search.forceActiveFocus();if(name==="Clipboard"){previewOpen=false;clipboard.search(search.text);}else if(providerEnabled("Files") && (name==="Files" || name==="All"))service.search(search.text);}
    function cycleProvider() {const choices=providerChoices;setProvider(choices[(choices.indexOf(provider)+1)%choices.length]);}
    function launchKey(key,parent){const index=results.findIndex(item=>resultKey(item)===key);if(index>=0)launch(index,parent);}
    function launch(index, parent) {
        if(index<0 || index>=results.length || service.opening)return;
        const item=results[index];
        if(item.kind==="clipboard"){if(!clipboard.busy)clipboard.copy(item.id);return;}
        if(item.kind==="file"){service.open(item.path,!!parent,item.token,!!(parent || item.folder));return;}
        if(item.kind==="menu-command"){
            closeRequested();root.shell.exportedMenu.activateCaptured(item.token,item.id);return;
        }
        closeRequested();
        activation.item=item;activation.restart();
    }
    Timer {id:activation;property var item;interval:100;onTriggered: {
        if(item.kind==="app"){root.shell.activate(item.entry,true);return;}
        switch(item.id) {
        case "text-tools":root.shell.openTextTools();break;
        case "overview":root.shell.openOverview(false);break;
        case "focus":root.shell.notifications.command({action:"dnd"});break;
        case "mute":root.shell.audioKey("mute");break;
        case "up":root.shell.audioKey("up");break;
        case "down":root.shell.audioKey("down");break;
        case "controls":root.shell.showMenu("Controls");break;
        case "calendar":root.shell.showMenu("Date");break;
        case "emoji":AppLaunch.run(["lumen-character-viewer"]);break;
        case "capture":AppLaunch.run(["lumen-conveniences","capture"]);break;
        case "lock":AppLaunch.run(["lumen-lock"]);break;
        }
    }}
    function moveSelection(delta) {
        selectedIndex=Math.max(0,Math.min(results.length-1,selectedIndex+delta));
        if(gridMode)grid.positionViewAtIndex(selectedIndex,GridView.Contain);else list.positionViewAtIndex(selectedIndex,ListView.Contain);
    }
    function fileIcon(item) {
        if(item.folder)return "folder";
        if((item.mime || "").startsWith("image/"))return "image-x-generic";
        if(item.mime==="application/pdf")return "application-pdf";
        return "text-x-generic";
    }
    function icon(item) {return item.kind==="app"?shell.appIcon(item.entry):Quickshell.iconPath(fileIcon(item),true) || Qt.resolvedUrl("icons/application.svg");}
    function metadata(item) {
        if(!item || item.kind!=="file")return "";
        const size=item.folder?"Folder":item.size<1024?item.size+" bytes":item.size<1048576?(item.size/1024).toFixed(1)+" KB":(item.size/1048576).toFixed(1)+" MB";
        return size+" · "+new Date(item.mtime*1000).toLocaleDateString();
    }
    Item {
        id:searchArea;width:parent.width;height:66
        SystemGlyph {x:22;anchors.verticalCenter:parent.verticalCenter;name:"search";width:24;height:24;ink:Theme.colors.muted}
        TextInput {
            id:search;x:60;anchors.verticalCenter:parent.verticalCenter;width:parent.width-84
            font.family:Theme.font;font.pixelSize:24;color:Theme.colors.text;selectionColor:Theme.colors.accent;clip:true
            onTextChanged:{root.selectedIndex=0;if(root.visible){if(root.provider==="Clipboard")clipboard.search(text);else if(root.providerEnabled("Files"))root.service.search(text);}}
            PanelLabel {visible:search.text.length===0;text:"Search";font.pixelSize:24;color:Theme.colors.muted}
            Keys.onPressed:event=>{
                if((event.modifiers & Qt.ControlModifier) && event.key===Qt.Key_1){if(root.providerEnabled("Applications")){root.provider="Applications";root.gridMode=true;}event.accepted=true;}
                else if((event.modifiers & Qt.ControlModifier) && event.key===Qt.Key_2){root.setProvider("Files");event.accepted=true;}
                else if((event.modifiers & Qt.ControlModifier) && event.key===Qt.Key_3){root.setProvider("Actions");event.accepted=true;}
                else if((event.modifiers & Qt.ControlModifier) && event.key===Qt.Key_4){root.setProvider("Clipboard");event.accepted=true;}
                else if((event.modifiers & Qt.ControlModifier) && event.key===Qt.Key_Space){root.previewOpen=!root.previewOpen;event.accepted=true;}
                else if((event.modifiers & Qt.ControlModifier) && event.key===Qt.Key_Return){root.launch(root.selectedIndex,true);event.accepted=true;}
                else if((event.modifiers & Qt.ControlModifier) && event.key===Qt.Key_R){root.service.refresh();event.accepted=true;}
                else if(event.key===Qt.Key_Tab && (event.modifiers & Qt.ControlModifier)){root.cycleProvider();event.accepted=true;}
                else event.accepted=false;
            }
            Keys.onDownPressed:event=>{if(event.modifiers & Qt.AltModifier)root.recallHistory(-1);else root.moveSelection(root.gridMode?5:1);event.accepted=true;}
            Keys.onUpPressed:event=>{if(event.modifiers & Qt.AltModifier)root.recallHistory(1);else root.moveSelection(root.gridMode?-5:-1);event.accepted=true;}
            Keys.onReturnPressed:event=>root.launch(root.selectedIndex,!!(event.modifiers & Qt.ControlModifier))
            Keys.onEscapePressed:root.closeRequested()
            Accessible.name:"Search applications, files, actions and clipboard history"
        }
    }
    Item {
        id:filters;y:searchArea.height;width:parent.width;height:40
        Row {x:16;spacing:3
            Repeater {
                model:root.providerChoices
                AbstractButton {
                    id:filterButton
                    required property string modelData
                    width:filterLabel.implicitWidth+18;height:25
                    contentItem:PanelLabel {id:filterLabel;text:filterButton.modelData;font.pixelSize:11;horizontalAlignment:Text.AlignHCenter;verticalAlignment:Text.AlignVCenter}
                    background:Rectangle {radius:12;color:filterButton.modelData===root.provider?Theme.selection:filterButton.activeFocus?Theme.colors.hover:"transparent"}
                    onClicked:root.setProvider(modelData)
                    Accessible.name:modelData+(modelData===root.provider?", selected":"")
                }
            }
        }
        Row {anchors.right:parent.right;anchors.rightMargin:18;spacing:4
            AbstractButton {
                width:28;height:25;leftPadding:6;rightPadding:6;topPadding:4.5;bottomPadding:4.5;checkable:true;checked:root.previewOpen
                contentItem:SystemGlyph {name:"info";width:16;height:16;Accessible.ignored:true}
                background:Rectangle {radius:6;color:root.previewOpen?Theme.selection:"transparent";border.width:parent.activeFocus?2:0;border.color:Theme.colors.accent}
                onClicked:root.previewOpen=!root.previewOpen
                Accessible.name:"Toggle preview";Accessible.role:Accessible.CheckBox
            }
            AbstractButton {
                width:28;height:25;leftPadding:6;rightPadding:6;topPadding:4.5;bottomPadding:4.5;checkable:true;checked:root.gridMode;enabled:root.providerEnabled("Applications")
                contentItem:SystemGlyph {name:root.gridMode?"list":"grid";width:16;height:16;Accessible.ignored:true}
                background:Rectangle {radius:6;color:root.gridMode?Theme.selection:"transparent";border.width:parent.activeFocus?2:0;border.color:Theme.colors.accent}
                onClicked:{if(root.providerEnabled("Applications")){root.provider="Applications";root.gridMode=!root.gridMode;}}
                Accessible.name:"Application grid";Accessible.role:Accessible.CheckBox
            }
        }
    }
    Item {
        id:resultArea;y:filters.y+filters.height;width:parent.width
        height:Math.min(Math.max(100,(root.parent?root.parent.height:1000)-Theme.barHeight-230),!root.showingResults?0:root.previewOpen?330:root.provider==="Clipboard" && !clipboard.enabled?140:root.results.length===0?90:root.gridMode?Math.min(300,Math.ceil(root.results.length/5)*100):Math.min(330,root.results.length*50))
        Item {
            id:rows;anchors.left:parent.left;anchors.top:parent.top;anchors.bottom:parent.bottom;width:root.splitPreview?parent.width-300:parent.width
            visible:!root.previewOpen || root.splitPreview
            PanelLabel {
                visible:root.showingResults && root.results.length===0 && root.provider!=="Clipboard";anchors.centerIn:parent
                text:root.service.searching && (root.provider==="Files" || root.provider==="All")?"Searching…":root.provider==="Files" && !search.text.trim()?"Type a filename to search":"No results found"
                color:Theme.colors.muted
            }
            Column {
                visible:root.provider==="Clipboard" && !clipboard.enabled
                x:20;y:10;width:parent.width-40;spacing:6
                PanelLabel {width:parent.width;wrapMode:Text.WordWrap;text:"Clipboard history is off. Enabling it saves future supported copied text on this computer. Sensitive text may be included unless its source marks it secret.";font.pixelSize:12}
                DialogButton {text:"Enable Clipboard History";enabled:!clipboard.busy;onClicked:clipboard.enable()}
            }
            ListView {
                id:list;visible:root.showingResults && !root.gridMode && (root.provider!=="Clipboard" || clipboard.enabled);anchors.fill:parent;anchors.leftMargin:10;anchors.rightMargin:10;clip:true;model:root.results
                delegate:Rectangle {
                    required property var modelData
                    required property int index
                    property string focusKey:""
                    activeFocusOnTab:true;enabled:!root.service.opening
                    Accessible.role:Accessible.Button;Accessible.name:modelData.title;Accessible.description:root.resultDescription(modelData);Accessible.selected:index===root.selectedIndex
                    Accessible.onPressAction:if(enabled && visible)root.activateResult(root.resultKey(modelData))
                    onActiveFocusChanged:if(activeFocus){focusKey=root.resultKey(modelData);root.selectedIndex=index;}
                    onModelDataChanged:if(activeFocus && focusKey && focusKey!==root.resultKey(modelData))root.focusSearch()
                    Component.onDestruction:if(activeFocus)root.focusSearch()
                    Keys.onReturnPressed:root.activateResult(focusKey)
                    Keys.onSpacePressed:root.activateResult(focusKey)
                    border.width:activeFocus?2:0;border.color:Theme.colors.accent
                    width:list.width;height:50;radius:9;color:root.selectedIndex===index?Theme.selection:"transparent"
                    AppIcon {visible:modelData.kind==="app";x:12;anchors.verticalCenter:parent.verticalCenter;width:28;height:28;iconSource:root.icon(modelData);calendarDate:root.shell.currentDate;sourceSize.width:64;sourceSize.height:64}
                    SystemGlyph {visible:modelData.kind!=="app";x:14;anchors.verticalCenter:parent.verticalCenter;width:24;height:24;name:modelData.kind==="file"?(modelData.folder?"folder":"document"):modelData.glyph || "info";ink:modelData.kind==="file" && modelData.folder?Theme.colors.accent:Theme.colors.text}
                    PanelLabel {x:52;y:6;width:parent.width-80;text:modelData.title;textFormat:Text.PlainText;font.pixelSize:14}
                    PanelLabel {x:52;y:28;width:parent.width-80;text:modelData.kind==="file"?modelData.subtitle.replace(Quickshell.env("HOME"),"~"):modelData.subtitle;textFormat:Text.PlainText;font.pixelSize:10;color:Theme.colors.muted}
                    MouseArea {anchors.fill:parent;hoverEnabled:true;property string pressedKey:"";onEntered:root.selectedIndex=index;onPressed:pressedKey=root.resultKey(modelData);onCanceled:pressedKey="";onClicked:{root.launchKey(pressedKey,false);pressedKey="";}}
                }
            }
            GridView {
                id:grid;visible:root.gridMode;anchors.fill:parent;anchors.leftMargin:12;anchors.rightMargin:12;clip:true;model:root.results
                cellWidth:width/5;cellHeight:100
                delegate:Rectangle {
                    required property var modelData
                    required property int index
                    property string focusKey:""
                    activeFocusOnTab:true;enabled:!root.service.opening
                    Accessible.role:Accessible.Button;Accessible.name:modelData.title;Accessible.description:root.resultDescription(modelData);Accessible.selected:index===root.selectedIndex
                    Accessible.onPressAction:if(enabled && visible)root.activateResult(root.resultKey(modelData))
                    onActiveFocusChanged:if(activeFocus){focusKey=root.resultKey(modelData);root.selectedIndex=index;}
                    onModelDataChanged:if(activeFocus && focusKey && focusKey!==root.resultKey(modelData))root.focusSearch()
                    Component.onDestruction:if(activeFocus)root.focusSearch()
                    Keys.onReturnPressed:root.activateResult(focusKey)
                    Keys.onSpacePressed:root.activateResult(focusKey)
                    border.width:activeFocus?2:0;border.color:Theme.colors.accent
                    width:grid.cellWidth-4;height:96;radius:10;color:root.selectedIndex===index?Theme.selection:"transparent"
                    AppIcon {anchors.horizontalCenter:parent.horizontalCenter;y:8;width:52;height:52;iconSource:root.icon(modelData);calendarDate:root.shell.currentDate;sourceSize.width:128;sourceSize.height:128}
                    PanelLabel {x:5;y:67;width:parent.width-10;text:modelData.title;textFormat:Text.PlainText;font.pixelSize:11;horizontalAlignment:Text.AlignHCenter}
                    MouseArea {anchors.fill:parent;hoverEnabled:true;property string pressedKey:"";onEntered:root.selectedIndex=index;onPressed:pressedKey=root.resultKey(modelData);onCanceled:pressedKey="";onClicked:{root.launchKey(pressedKey,false);pressedKey="";}}
                }
            }
        }
        Item {
            id:previewPane;visible:root.previewOpen;anchors.right:parent.right;width:root.splitPreview?300:parent.width;height:parent.height
            Rectangle {visible:root.splitPreview;width:1;height:parent.height;color:Theme.hairline}
            Column {x:18;y:8;width:parent.width-36;spacing:8
                PanelLabel {width:parent.width;text:root.current?root.current.title:"Select a result";textFormat:Text.PlainText;font.pixelSize:15;font.weight:Font.DemiBold}
                PanelLabel {width:parent.width;text:root.current && root.current.kind==="file"?root.metadata(root.current):root.current?root.current.subtitle:"";textFormat:Text.PlainText;font.pixelSize:10;color:Theme.colors.muted}
                Image {visible:root.current && root.current.kind==="file" && root.service.preview.type==="image";width:parent.width;height:visible?Math.max(60,previewPane.height-120):0;source:visible?root.service.preview.uri || "":"";fillMode:Image.PreserveAspectFit;sourceSize.width:640;sourceSize.height:640;asynchronous:true;PanelLabel {anchors.centerIn:parent;visible:parent.status===Image.Error;text:"Image preview unavailable";font.pixelSize:11;color:Theme.colors.muted}}
                Flickable {
                    visible:!root.current || root.current.kind!=="file" || root.service.preview.type!=="image"
                    width:parent.width;height:Math.max(40,previewPane.height-120);clip:true;contentHeight:previewText.height
                    PanelLabel {id:previewText;width:parent.width;wrapMode:Text.WrapAnywhere;elide:Text.ElideNone;textFormat:Text.PlainText;font.pixelSize:12;text:!root.current?"":root.current.kind==="file"?(root.service.preview.text || (root.service.preview.type?"No preview available":"Loading preview…")):root.current.kind==="app"?"Open this application with Return.":root.current.subtitle}
                }
                PanelLabel {width:parent.width;visible:!!root.service.preview.truncated && root.current && root.current.kind==="file";text:"Preview shortened";font.pixelSize:10;color:Theme.colors.muted}
                Row {spacing:8;visible:!!root.current
                    Repeater {model:root.current && root.current.kind==="file"?["Open","Show Folder"]:["Open"]
                        Rectangle {required property string modelData;width:label.implicitWidth+20;height:26;radius:6;color:Theme.colors.field
                            PanelLabel {id:label;anchors.centerIn:parent;text:parent.modelData;font.pixelSize:11}
                            MouseArea {anchors.fill:parent;property string pressedKey:"";onPressed:pressedKey=root.resultKey(root.current);onCanceled:pressedKey="";onClicked:{root.launchKey(pressedKey,parent.modelData==="Show Folder");pressedKey="";}}
                        }
                    }
                }
            }
        }
    }
    Row {
        visible:root.provider==="Clipboard" && clipboard.enabled
        x:20;y:resultArea.y+resultArea.height;spacing:8
        DialogButton {text:"Clear History";enabled:!clipboard.busy;onClicked:clipboard.clear()}
        DialogButton {text:"Disable History";enabled:!clipboard.busy;onClicked:clipboard.disable()}
    }
    Item {
        id:footer;y:resultArea.y+resultArea.height+(root.provider==="Clipboard" && clipboard.enabled?34:0);width:parent.width;height:40
        Rectangle {x:16;width:parent.width-32;height:1;color:Theme.hairline}
        PanelLabel {x:22;y:7;width:parent.width-44;text:(root.provider==="Clipboard"?(clipboard.error || clipboard.message || (clipboard.busy?"Loading clipboard history…":clipboard.enabled?"Select a result to copy. Paste explicitly in the destination.":"Clipboard history requires your consent.")):"") || root.service.error || (root.service.opening?"Opening…":root.service.scanning?"Indexing files · "+root.service.indexed.toLocaleString()+" found":root.service.limited?"Index limit reached · Some files are not included":root.service.unreadable?"Some folders could not be read":root.showingResults?root.results.length+(root.service.more?"+":"")+(root.results.length===1?" result":" results"):"Search apps, files and actions");textFormat:Text.PlainText;font.pixelSize:10;color:Theme.colors.muted}
        PanelLabel {x:22;y:23;width:parent.width-44;text:root.provider==="Clipboard"?"↑↓ to select   Return to copy   Tab for controls   Escape to close":"↑↓ to select   Return to open   Ctrl+Tab for categories   Escape to close";font.pixelSize:10;color:Theme.colors.muted}
    }
}
