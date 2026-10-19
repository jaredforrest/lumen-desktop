import QtQuick
import QtQml.Models
import Quickshell
Column {
    id:root
    required property var service
    width:300;spacing:10
    Accessible.role:Accessible.Pane;Accessible.name:"Desktop widget catalogue"
    property bool initialized:false
    readonly property var items:service.desktopItems
    function removeWidget(id){service.removeWidget(id);addCalendar.forceActiveFocus();}
    function syncItems(){
        const ids=items.map(item=>item.id);
        for(let i=stable.count-1;i>=0;i--)if(!ids.includes(stable.get(i).widget.id))stable.remove(i);
        for(let index=0;index<items.length;index++){
            let previous=-1;
            for(let i=index;i<stable.count;i++)if(stable.get(i).widget.id===items[index].id){previous=i;break;}
            if(previous<0)stable.insert(index,{widget:items[index]});
            else {if(previous!==index)stable.move(previous,index,1);if(JSON.stringify(stable.get(index).widget)!==JSON.stringify(items[index]))stable.setProperty(index,"widget",items[index]);}
        }
    }
    onItemsChanged:if(initialized)syncItems()
    Component.onCompleted:{initialized=true;syncItems();}
    ListModel {id:stable;dynamicRoles:true}
    readonly property var displays:[{key:"*",label:"Every display"}].concat(Quickshell.screens.map(screen=>({key:screen.name,label:screen.name})))
    PanelLabel {width:parent.width;text:"Desktop widget catalogue";Accessible.ignored:true;font.weight:Font.DemiBold}
    PanelLabel {width:parent.width;text:"Calendar and local time widgets. Right-click a desktop widget to return here. Move its handle by pointer or keyboard.";wrapMode:Text.Wrap;color:Theme.colors.muted;font.pixelSize:Theme.textSize(11)}
    Flow {width:parent.width;spacing:8
        DialogButton {id:addCalendar;objectName:"widget-add-calendar";text:"Add Calendar";enabled:root.service.desktopItems.length<16;onClicked:root.service.addWidget("calendar")}
        DialogButton {objectName:"widget-add-clock";text:"Add Local Time";enabled:root.service.desktopItems.length<16;onClicked:root.service.addWidget("clock")}
    }
    Repeater {model:stable
        delegate:PanelSurface {
            id:row;required property var widget;readonly property var modelData:widget;required property int index
            width:root.width;implicitHeight:form.height+20;radius:10
            readonly property string accessibleLabel:(modelData.type==="clock"?"Local Time":"Calendar")+" widget "+(index+1)
            Accessible.role:Accessible.Grouping;Accessible.name:accessibleLabel
            readonly property var outputChoices:root.displays.some(display=>display.key===modelData.output)?root.displays:root.displays.concat([{key:modelData.output,label:modelData.output+" (disconnected)"}])
            Column {id:form;x:10;y:10;width:parent.width-20;spacing:7
                PanelLabel {width:parent.width;text:(row.modelData.type==="clock"?"Local Time":"Calendar")+" · "+(row.index+1);Accessible.ignored:true;font.weight:Font.DemiBold}
                DesktopComboBox {objectName:"widget-size-"+row.modelData.id;width:parent.width;model:["Small","Medium","Large"];currentIndex:["small","medium","large"].indexOf(row.modelData.size);Accessible.name:"Size for "+row.accessibleLabel;onActivated:index=>root.service.updateWidget(row.modelData.id,{size:["small","medium","large"][index]})}
                DesktopComboBox {width:parent.width;model:row.outputChoices;textRole:"label";currentIndex:row.outputChoices.findIndex(display=>display.key===row.modelData.output);Accessible.name:"Display for "+row.accessibleLabel;onActivated:index=>root.service.updateWidget(row.modelData.id,{output:row.outputChoices[index].key})}
                DesktopComboBox {width:parent.width;visible:row.modelData.type==="calendar";model:["Month and agenda","Agenda"];currentIndex:row.modelData.style==="agenda"?1:0;Accessible.name:"Calendar layout for "+row.accessibleLabel;onActivated:index=>root.service.updateWidget(row.modelData.id,{style:index===1?"agenda":"month"})}
                DesktopComboBox {width:parent.width;visible:row.modelData.type==="clock";model:["Locale time","12-hour time","24-hour time"];currentIndex:["locale","12","24"].indexOf(row.modelData.hourCycle);Accessible.name:"Time format for "+row.accessibleLabel;onActivated:index=>root.service.updateWidget(row.modelData.id,{hourCycle:["locale","12","24"][index]})}
                DialogButton {objectName:"widget-remove-"+row.modelData.id;text:"Remove";Accessible.name:"Remove "+row.accessibleLabel;onClicked:root.removeWidget(row.modelData.id)}
            }
        }
    }
    PanelLabel {visible:root.service.desktopItems.length===0;width:parent.width;text:"No desktop widgets. Add one above.";wrapMode:Text.Wrap;color:Theme.colors.muted}
}
