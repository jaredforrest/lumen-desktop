import QtQuick
import Quickshell
Column {
    id:root
    required property var service
    width:300;spacing:10
    readonly property var displays:[{key:"*",label:"Every display"}].concat(Quickshell.screens.map(screen=>({key:screen.name,label:screen.name})))
    PanelLabel {width:parent.width;text:"Desktop widget catalogue";font.weight:Font.DemiBold}
    PanelLabel {width:parent.width;text:"Calendar and local time widgets. Right-click a desktop widget to return here. Move its handle by pointer or keyboard.";wrapMode:Text.Wrap;color:Theme.colors.muted;font.pixelSize:Theme.textSize(11)}
    Flow {width:parent.width;spacing:8
        DialogButton {objectName:"widget-add-calendar";text:"Add Calendar";enabled:root.service.desktopItems.length<16;onClicked:root.service.addWidget("calendar")}
        DialogButton {objectName:"widget-add-clock";text:"Add Local Time";enabled:root.service.desktopItems.length<16;onClicked:root.service.addWidget("clock")}
    }
    Repeater {model:root.service.desktopItems
        delegate:PanelSurface {
            id:row;required property var modelData;required property int index
            width:root.width;implicitHeight:form.height+20;radius:10
            readonly property var outputChoices:root.displays.some(display=>display.key===modelData.output)?root.displays:root.displays.concat([{key:modelData.output,label:modelData.output+" (disconnected)"}])
            Column {id:form;x:10;y:10;width:parent.width-20;spacing:7
                PanelLabel {width:parent.width;text:(row.modelData.type==="clock"?"Local Time":"Calendar")+" · "+(row.index+1);font.weight:Font.DemiBold}
                DesktopComboBox {objectName:"widget-size-"+row.modelData.id;width:parent.width;model:["Small","Medium","Large"];currentIndex:["small","medium","large"].indexOf(row.modelData.size);Accessible.name:"Size for widget "+(row.index+1);onActivated:index=>root.service.updateWidget(row.modelData.id,{size:["small","medium","large"][index]})}
                DesktopComboBox {width:parent.width;model:row.outputChoices;textRole:"label";currentIndex:row.outputChoices.findIndex(display=>display.key===row.modelData.output);Accessible.name:"Display for widget "+(row.index+1);onActivated:index=>root.service.updateWidget(row.modelData.id,{output:row.outputChoices[index].key})}
                DesktopComboBox {width:parent.width;visible:row.modelData.type==="calendar";model:["Month and agenda","Agenda"];currentIndex:row.modelData.style==="agenda"?1:0;Accessible.name:"Calendar layout for widget "+(row.index+1);onActivated:index=>root.service.updateWidget(row.modelData.id,{style:index===1?"agenda":"month"})}
                DesktopComboBox {width:parent.width;visible:row.modelData.type==="clock";model:["Locale time","12-hour time","24-hour time"];currentIndex:["locale","12","24"].indexOf(row.modelData.hourCycle);Accessible.name:"Time format for widget "+(row.index+1);onActivated:index=>root.service.updateWidget(row.modelData.id,{hourCycle:["locale","12","24"][index]})}
                DialogButton {objectName:"widget-remove-"+row.modelData.id;text:"Remove";Accessible.name:"Remove widget "+(row.index+1);onClicked:root.service.removeWidget(row.modelData.id)}
            }
        }
    }
    PanelLabel {visible:root.service.desktopItems.length===0;width:parent.width;text:"No desktop widgets. Add one above.";wrapMode:Text.Wrap;color:Theme.colors.muted}
}
