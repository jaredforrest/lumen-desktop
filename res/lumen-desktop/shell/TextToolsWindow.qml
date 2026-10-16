import QtQuick
import QtQuick.Controls
import QtQuick.Window
import Quickshell
import Quickshell.Io
FloatingWindow {
    id:root
    required property var shell
    title:"Text Tools";visible:false;color:"transparent"
    implicitWidth:660;implicitHeight:570;minimumSize:Qt.size(520,320)
    property var target:0
    property var snippets:[]
    property bool automaticReplacements:true
    property string definition:""
    property string error:""
    property string severity:"info"
    property string language:Qt.locale().name==="en_US"?"en_US":"en_GB"
    property string page:"Dictionary"
    readonly property var pages:["Dictionary","Speech","Replacements","Shortcuts","Spelling","Dictation","Services","Fonts","Colours"]
    property bool recording:false
    property var spelling:[]
    property var detected:[]
    property string targetApp:""
    onVisibleChanged:if(visible)fit();else if(recording || (worker.running && request.action==="dictation-start")){cancelCapture.running=true;root.recording=false;}
    function fit(){if(screen){implicitWidth=Math.max(400,Math.min(660,screen.width-32));implicitHeight=Math.max(320,Math.min(570,screen.height-92));}}
    property var request:({})
    property var menuCommands:[]
    function commandPaths(node,parents){if(!node)return [];const label=shell.exportedMenu.label(node.label || ""),path=parents.concat(label?[label]:[]);let rows=[];if(node.visible && node.enabled && !node.submenu && !node.separator)rows.push(path.join("/"));for(const child of node.children || [])rows=rows.concat(commandPaths(child,path));return rows;}
    function show(){target=shell.exportedMenu.state.windowId;targetApp=shell.exportedMenu.state.appName;menuCommands=commandPaths(shell.exportedMenu.state.menu,[]);shell.claimPopup("");visible=true;send({action:"status"});Qt.callLater(()=>{if(content.Window.window)content.Window.window.requestActivate();input.forceActiveFocus();});}
    function send(data){if(worker.running)return;request=Object.assign({},data,{language:language});error="";severity="info";worker.running=true;}
    Rectangle {id:content;anchors.fill:parent;radius:Theme.nativeWindowFrame?0:12;color:Theme.colors.background;border.color:Theme.outline
        MouseArea {anchors.top:parent.top;width:parent.width;height:45;onPressed:if(content.Window.window)content.Window.window.startSystemMove()}
        WindowControls {windowName:"Text Tools";canFullscreen:false;windowActive:content.Window.window?content.Window.window.active:false;onTriggered:index=>{if(index===0)root.visible=false;else if(content.Window.window && index===1)content.Window.window.showMinimized();}}
        PanelLabel {x:100;y:14;text:"Text Tools";font.weight:Font.DemiBold}
        Column {x:12;y:58;width:132;spacing:3
            Repeater {model:root.pages
                AbstractButton {required property string modelData;width:132;height:34;Accessible.name:modelData;onClicked:root.page=modelData
                    contentItem:PanelLabel {text:parent.modelData;verticalAlignment:Text.AlignVCenter;leftPadding:10;color:root.page===parent.modelData?Theme.colors.selectedText:Theme.colors.text}
                    background:Rectangle {radius:7;color:root.page===parent.modelData?Theme.colors.accent:parent.activeFocus || parent.hovered?Theme.colors.hover:"transparent"}
                }
            }
        }
        Flickable {x:160;y:58;width:parent.width-178;height:parent.height-y-18;clip:true;contentWidth:width;contentHeight:body.height
            ScrollBar.vertical:DesktopScrollBar {}
        Column {id:body;width:parent.width;spacing:10
            PanelLabel {text:root.page;font.pixelSize:18;font.weight:Font.DemiBold}
            DesktopComboBox {visible:["Spelling","Speech"].includes(root.page);width:parent.width;model:["English (United Kingdom)","English (United States)"];currentIndex:root.language==="en_US"?1:0;Accessible.name:"Text language";enabled:!worker.running;onActivated:index=>root.send({action:"language",value:index===1?"en_US":"en_GB"})}
            PanelLabel {visible:!["Fonts","Colours"].includes(root.page);width:parent.width;wrapMode:Text.WordWrap;maximumLineCount:3;font.pixelSize:12;color:Theme.colors.muted;text:root.page==="Dictation"?"Record speech locally, review the transcript, then insert it into the original window.":root.page==="Spelling"?"Check spelling in your chosen English variant and keep a personal dictionary. Corrections are applied only when chosen.":root.page==="Services"?"Copy, share or find actions in your text. Links open only when you choose them.":root.page==="Shortcuts"?"Bind an exported menu command to Super+Alt+F1–F12. Commands run only in the matching focused app.":root.page==="Replacements"?"Expand abbreviations after a space in supported native Wayland text fields. Password fields are excluded. Use Insert in other apps.":root.page==="Dictionary"?"Look up an English word using the local WordNet dictionary.":"Read text locally using your chosen English voice. No text is sent to a server."}
            CreativeTools {visible:["Fonts","Colours"].includes(root.page);width:parent.width;page:root.page;onCopyRequested:value=>root.send({action:"copy",text:value})}
            ControlRow {visible:!["Shortcuts","Fonts","Colours"].includes(root.page);width:parent.width;text:"Use Selected Text";enabled:!worker.running;onClicked:root.send({action:"selection"})}
            ScrollView {visible:!["Shortcuts","Fonts","Colours"].includes(root.page);width:parent.width;height:root.page==="Replacements"?110:80
                TextArea {id:input;color:Theme.colors.text;font.family:Theme.font;font.pixelSize:13;wrapMode:TextEdit.Wrap;selectByMouse:true;placeholderText:"Type or paste text";placeholderTextColor:Theme.colors.muted;Accessible.name:"Text to process";background:Rectangle {radius:7;color:Theme.colors.field;border.color:input.activeFocus?Theme.colors.accent:Theme.hairline}}
            }
            Row {width:parent.width;spacing:8;visible:["Dictionary","Speech"].includes(root.page)
                DialogButton {width:160;text:root.page==="Dictionary"?"Look Up":"Read Aloud";primary:true;enabled:!worker.running && input.text.trim().length>0;onClicked:root.send({action:root.page==="Dictionary"?"lookup":"speak",text:input.text})}
                DialogButton {width:120;visible:root.page==="Speech";text:"Stop Reading";onClicked:root.send({action:"stop"})}
            }
            Row {visible:root.page==="Dictation";spacing:8
                DialogButton {width:145;text:root.recording?"Stop & Transcribe":"Start Dictation";primary:true;enabled:!worker.running;onClicked:root.send({action:root.recording?"dictation-finish":"dictation-start"})}
                DialogButton {width:80;text:"Cancel";enabled:root.recording && !worker.running;onClicked:root.send({action:"dictation-cancel"})}
                DialogButton {width:130;text:"Insert Transcript";enabled:!root.recording && !worker.running && !!input.text;onClicked:{root.visible=false;insertion.text=input.text;insertion.restart();}}
            }
            Row {visible:["Spelling","Services"].includes(root.page);spacing:8
                DialogButton {width:140;text:root.page==="Spelling"?"Check Spelling":"Find Actions";enabled:!worker.running && !!input.text;onClicked:root.send({action:root.page==="Spelling"?"spelling":"detect",text:input.text})}
                DialogButton {width:80;text:"Copy";enabled:!!input.text;onClicked:root.send({action:"copy",text:input.text})}
                DialogButton {width:120;visible:root.page==="Services";text:"Share by Email";enabled:!!input.text;onClicked:AppLaunch.run(["xdg-email","--body",input.text])}
            }
            Column {visible:root.page==="Spelling";width:parent.width;spacing:6
                Repeater {model:root.spelling
                    Row {required property var modelData;width:parent.width;spacing:6
                        PanelLabel {width:100;height:30;verticalAlignment:Text.AlignVCenter;text:modelData.word;textFormat:Text.PlainText}
                        DesktopComboBox {id:suggestion;width:parent.width-276;model:modelData.suggestions;Accessible.name:"Correction for "+modelData.word}
                        DialogButton {width:90;text:"Replace";enabled:!!suggestion.currentText;onClicked:{const pattern=new RegExp("\\b"+modelData.word+"\\b","g");input.text=input.text.replace(pattern,suggestion.currentText);root.send({action:"spelling",text:input.text});}}
                        DialogButton {width:68;text:"Learn";onClicked:root.send({action:"learn",text:modelData.word})}
                    }
                }
            }
            Column {visible:root.page==="Services";width:parent.width;spacing:4
                Repeater {model:root.detected
                    ControlRow {required property var modelData;width:parent.width;text:modelData.label;enabled:!!modelData.uri;onClicked:AppLaunch.run(["xdg-open",modelData.uri])}
                }
            }
            Row {visible:root.page==="Replacements";width:parent.width;spacing:8
                TextField {id:name;width:parent.width-108;height:30;color:Theme.colors.text;font.family:Theme.font;placeholderText:"Abbreviation or name";placeholderTextColor:Theme.colors.muted;Accessible.name:"Replacement name";background:Rectangle {radius:6;color:Theme.colors.field;border.color:parent.activeFocus?Theme.colors.accent:Theme.hairline}}
                DialogButton {width:100;text:"Save";enabled:!!name.text && !!input.text && !worker.running;onClicked:root.send({action:"save",name:name.text,text:input.text})}
            }
            SettingsToggle {visible:root.page==="Replacements";width:parent.width;text:"Automatic replacements in supported fields";checked:root.automaticReplacements;onToggled:root.send({action:"automatic-replacements",enabled:checked})}
            ListView {visible:root.page==="Replacements";width:parent.width;height:Math.max(70,root.height-365);clip:true;model:root.snippets;ScrollBar.vertical:DesktopScrollBar {}
                delegate:Row {required property var modelData;width:ListView.view.width;spacing:6
                    ControlRow {width:parent.width-140;text:modelData.name;onClicked:{name.text=modelData.name;input.text=modelData.text;}}
                    DialogButton {width:72;text:"Insert";onClicked:{root.visible=false;insertion.text=modelData.text;insertion.restart();}}
                    DialogButton {width:56;text:"Delete";onClicked:root.send({action:"delete",name:modelData.name})}
                }
            }
            ScrollView {visible:root.page==="Dictionary";width:parent.width;height:Math.max(60,root.height-355)
                TextArea {readOnly:true;text:root.definition;color:Theme.colors.text;font.family:Theme.font;font.pixelSize:13;wrapMode:TextEdit.Wrap;selectByMouse:true;Accessible.name:"Dictionary definition";background:Rectangle {color:"transparent"}}
            }
            Column {visible:root.page==="Shortcuts";width:parent.width;spacing:10
                PanelLabel {width:parent.width;text:"Application: "+root.targetApp;wrapMode:Text.WordWrap}
                DesktopComboBox {id:slot;width:parent.width;model:Array.from({length:12},(_,i)=>"⌘⌥F"+(i+1));Accessible.name:"Menu shortcut slot"}
                TextField {width:parent.width;height:32;readOnly:true;placeholderText:"Click here, then press ⌘⌥F1–F12";Accessible.name:"Capture application shortcut";color:Theme.colors.text;font.family:Theme.font;background:Rectangle {radius:6;color:Theme.colors.field;border.color:parent.activeFocus?Theme.colors.accent:Theme.hairline}
                    Keys.onPressed:event=>{if((event.modifiers & Qt.MetaModifier) && (event.modifiers & Qt.AltModifier) && event.key>=Qt.Key_F1 && event.key<=Qt.Key_F12){slot.currentIndex=event.key-Qt.Key_F1;text="⌘⌥F"+(slot.currentIndex+1);event.accepted=true;}}
                }
                DesktopComboBox {width:parent.width;model:["Choose an application command…"].concat(root.menuCommands);enabled:root.menuCommands.length>0;Accessible.name:"Application command";onActivated:index=>{if(index>0)menuPath.text=root.menuCommands[index-1];}}
                PanelLabel {width:parent.width;text:root.menuCommands.length?"Choose a command above or enter its menu path below.":"This app does not export menu commands. Shortcuts need an app with an exported menu.";wrapMode:Text.WordWrap;font.pixelSize:11;color:Theme.colors.muted}
                TextField {id:menuPath;width:parent.width;height:32;color:Theme.colors.text;font.family:Theme.font;placeholderText:"Exact menu path, e.g. File/Save";placeholderTextColor:Theme.colors.muted;Accessible.name:"Menu command path";background:Rectangle {radius:6;color:Theme.colors.field;border.color:parent.activeFocus?Theme.colors.accent:Theme.hairline}}
                DialogButton {width:parent.width;text:"Save Application Shortcut";enabled:!!menuPath.text;onClicked:root.send({action:"save-shortcut",slot:slot.currentIndex+1,app:root.targetApp,path:menuPath.text})}
            }
            StatusMessage {width:parent.width;text:root.error;severity:root.severity}
        }
        }
        Shortcut {sequence:"Ctrl+Tab";enabled:root.visible;onActivated:root.page=root.pages[(root.pages.indexOf(root.page)+1)%root.pages.length]}
        Shortcut {sequence:"Ctrl+Shift+Tab";enabled:root.visible;onActivated:root.page=root.pages[(root.pages.indexOf(root.page)+root.pages.length-1)%root.pages.length]}
        Shortcut {sequence:"Escape";enabled:root.visible;onActivated:root.visible=false}
    }
    Timer {interval:121000;running:root.recording;onTriggered:root.send({action:"dictation-finish"})}
    Timer {id:insertion;property string text;interval:150;onTriggered:root.send({action:"insert",text:text,target:root.target})}
    Process {id:cancelCapture;stdinEnabled:true;command:[Quickshell.env("GLOBAL_MENU_PYTHON") || "python3",Qt.resolvedUrl("text_tools.py").toString().replace("file://","")];onStarted:write(JSON.stringify({action:"dictation-cancel"})+"\n")}
    Process {
        id:worker;stdinEnabled:true
        command:[Quickshell.env("GLOBAL_MENU_PYTHON") || "python3",Qt.resolvedUrl("text_tools.py").toString().replace("file://","")]
        onStarted:write(JSON.stringify(root.request)+"\n")
        stdout:StdioCollector {onStreamFinished:{try{const data=JSON.parse(text);root.error=data.error || data.message || "";root.severity=data.error?"error":data.message?"success":"info";if(data.snippets)root.snippets=data.snippets;if(data.language)root.language=data.language;if(data.automaticReplacements!==undefined)root.automaticReplacements=data.automaticReplacements;if(data.recording!==undefined){root.recording=data.recording;if(root.recording && !root.visible){cancelCapture.running=true;root.recording=false;}}if(root.request.action==="dictation-finish" || root.request.action==="dictation-cancel")root.recording=false;if(data.spelling){root.spelling=data.spelling;if(!data.spelling.length){root.error="No spelling errors found";root.severity="success";}}if(data.actions)root.detected=data.actions;if(data.text!==undefined)input.text=data.text;if(data.definition!==undefined)root.definition=data.definition;if(data.error && !root.visible)root.visible=true;}catch(e){root.error="Text tools could not complete the request";root.severity="error";}}}
    }
}
