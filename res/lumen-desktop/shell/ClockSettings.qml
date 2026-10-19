import QtQuick
import "StatusStripModel.js" as StatusModel
Column {
    id:root
    required property var service
    width:parent ? parent.width : 400;spacing:10
    PanelLabel {text:"Menu bar and clock";font.weight:Font.DemiBold}
    PanelLabel {width:parent.width;wrapMode:Text.WordWrap;text:"Hold Super and drag a status item to move it. Keyboard: focus an item and press Control Shift Left or Right. The clock stays at the right edge.";color:Theme.colors.muted}
    Repeater {model:StatusModel.normalize(root.service.menuBarPreferences.order)
        Row {required property string modelData;required property int index;width:root.width;spacing:8
            PanelLabel {width:Math.max(60,parent.width-160);text:modelData.charAt(0).toUpperCase()+modelData.slice(1);anchors.verticalCenter:parent.verticalCenter}
            DialogButton {text:"←";enabled:index>0 && !root.service.busy;Accessible.name:"Move "+modelData+" left";onClicked:root.service.setMenuBarPreference("order",StatusModel.move(root.service.menuBarPreferences.order,modelData,index-1))}
            DialogButton {text:"→";enabled:index<4 && !root.service.busy;Accessible.name:"Move "+modelData+" right";onClicked:root.service.setMenuBarPreference("order",StatusModel.move(root.service.menuBarPreferences.order,modelData,index+1))}
        }
    }
    DesktopComboBox {width:parent.width;model:["Locale time format","12-hour time","24-hour time"];currentIndex:["locale","12","24"].indexOf(root.service.menuBarPreferences.hourCycle || "locale");Accessible.name:"Clock hour format";onActivated:index=>root.service.setMenuBarPreference("hourCycle",["locale","12","24"][index])}
    DesktopComboBox {width:parent.width;model:["System locale","English (United Kingdom)","English (United States)","French","German","Japanese"];currentIndex:["","en_GB","en_US","fr_FR","de_DE","ja_JP"].indexOf(root.service.menuBarPreferences.locale || "");Accessible.name:"Clock display locale";onActivated:index=>root.service.setMenuBarPreference("locale",["","en_GB","en_US","fr_FR","de_DE","ja_JP"][index])}
    Repeater {model:[{key:"showDate",label:"Show date"},{key:"showWeekday",label:"Show weekday"},{key:"showSeconds",label:"Show seconds"}]
        SettingsToggle {required property var modelData;width:root.width;text:modelData.label;checked:root.service.menuBarPreferences[modelData.key]===true;enabled:root.service.available && !root.service.busy;onClicked:root.service.setMenuBarPreference(modelData.key,checked)}
    }
    PanelLabel {width:parent.width;wrapMode:Text.WordWrap;text:"Formatting changes presentation only. Time and timezone follow the system.";color:Theme.colors.muted}
}
