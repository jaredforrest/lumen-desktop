import QtQuick
import QtQuick.Controls
Column {
    id:root
    required property var service
    required property string profileId
    property var draft:null
    onProfileIdChanged:draft=null
    readonly property var backendProfile:service.focusState && service.focusState.profiles ? service.focusState.profiles.find(p=>p.id===profileId) || null : null
    readonly property var profile:draft || backendProfile
    Connections {target:root.service;function onFocusStateChanged(){if(root.draft && JSON.stringify(root.backendProfile)===JSON.stringify(root.draft))root.draft=null;}function onErrorChanged(){if(root.service.error)root.draft=null;}}
    width:300;spacing:8
    function update(key,value) {
        if(!profile || service.available===false)return;
        const next=JSON.parse(JSON.stringify(profile));next[key]=value;
        draft=next;service.command({action:"focus",operation:"profile",profile:next});
    }
    function schedule(index,key,value) {
        const next=JSON.parse(JSON.stringify(profile.schedules));next[index][key]=value;update("schedules",next);
    }
    DesktopTextField {id:profileName;width:parent.width;height:32;color:Theme.colors.text;selectionColor:Theme.colors.accent;selectedTextColor:Theme.colors.selectedText;font.family:Theme.font;background:Rectangle {radius:6;color:Theme.colors.field;border.color:profileName.activeFocus?Theme.colors.accent:Theme.hairline}text:root.profile?root.profile.name:"";Accessible.name:"Focus profile name";onEditingFinished:if(text.trim())root.update("name",text.trim())}
    ControlRow {width:parent.width;Accessible.role:Accessible.CheckBox;Accessible.checkable:true;Accessible.checked:!!root.profile && root.profile.allowCritical;Accessible.name:"Allow critical notifications";text:(root.profile && root.profile.allowCritical?"✓  ":"")+"Allow critical notifications";onClicked:root.update("allowCritical",!root.profile.allowCritical)}
    PanelLabel {text:"Allowed apps";font.weight:Font.DemiBold}
    Repeater {model:root.service.applications
        delegate:ControlRow {required property var modelData;width:root.width;Accessible.role:Accessible.CheckBox;Accessible.checkable:true;Accessible.checked:!!root.profile && root.profile.allowedApps.includes(modelData.key);Accessible.name:modelData.name;text:(root.profile && root.profile.allowedApps.includes(modelData.key)?"✓  ":"")+modelData.name
            onClicked:{let apps=root.profile.allowedApps.slice();const index=apps.indexOf(modelData.key);if(index>=0)apps.splice(index,1);else apps.push(modelData.key);root.update("allowedApps",apps);}
        }
    }
    PanelLabel {visible:root.service.applications.length===0;width:parent.width;wrapMode:Text.WordWrap;text:"Apps appear here after sending a notification.";color:Theme.colors.muted}
    PanelLabel {text:"Schedules · local time";font.weight:Font.DemiBold}
    Repeater {model:root.profile?root.profile.schedules:[]
        delegate:Column {id:rule;required property var modelData;required property int index;width:root.width;spacing:4
            Row {spacing:2
                Repeater {model:["M","T","W","T","F","S","S"]
                    delegate:DialogButton {required property string modelData;required property int index;width:35;height:30;text:modelData;primary:checked;checkable:true;checked:rule.modelData.days.includes(index);Accessible.name:["Monday","Tuesday","Wednesday","Thursday","Friday","Saturday","Sunday"][index]
                        onClicked:{let days=rule.modelData.days.slice();const at=days.indexOf(index);if(at>=0){if(days.length===1)return;days.splice(at,1);}else days.push(index);root.schedule(rule.index,"days",days);}
                    }
                }
            }
            Row {spacing:8
                SpinBox {palette.base:Theme.colors.field;palette.text:Theme.colors.text;palette.button:Theme.colors.surface;palette.buttonText:Theme.colors.text;palette.highlight:Theme.colors.accent;palette.highlightedText:Theme.colors.selectedText;width:125;from:0;to:1439;value:rule.modelData.start;Accessible.name:"Schedule start time";textFromValue:(value,locale)=>String(Math.floor(value/60)).padStart(2,"0")+":"+String(value%60).padStart(2,"0");onValueModified:if(value!==rule.modelData.end)root.schedule(rule.index,"start",value)}
                SpinBox {palette.base:Theme.colors.field;palette.text:Theme.colors.text;palette.button:Theme.colors.surface;palette.buttonText:Theme.colors.text;palette.highlight:Theme.colors.accent;palette.highlightedText:Theme.colors.selectedText;width:125;from:0;to:1439;value:rule.modelData.end;Accessible.name:"Schedule end time";textFromValue:(value,locale)=>String(Math.floor(value/60)).padStart(2,"0")+":"+String(value%60).padStart(2,"0");onValueModified:if(value!==rule.modelData.start)root.schedule(rule.index,"end",value)}
            }
            ControlRow {width:parent.width;text:"Remove schedule";onClicked:{let next=root.profile.schedules.slice();next.splice(rule.index,1);root.update("schedules",next);}}
        }
    }
    ControlRow {width:parent.width;text:"Add weekday schedule";enabled:!!root.profile && root.profile.schedules.length<16;onClicked:{let next=root.profile.schedules.slice();next.push({days:[0,1,2,3,4],start:540,end:1020});root.update("schedules",next);}}
    PanelLabel {width:parent.width;wrapMode:Text.WordWrap;color:Theme.colors.muted;text:"End times earlier than start times continue overnight. Use schedules from Focus to enable automatic activation."}
}
