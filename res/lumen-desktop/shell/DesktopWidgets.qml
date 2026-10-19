import QtQuick
import QtQml.Models
import Quickshell
import Quickshell.Wayland
PanelWindow {
    id:root
    required property var shell
    required property string outputName
    property int maskRevision:0
    property bool initialized:false
    readonly property var items:shell.widgets.desktopItems.filter(item=>item.output==="*" || item.output===outputName)
    readonly property real widgetWidth:cards.count && cards.itemAt(0)?cards.itemAt(0).width:0
    readonly property real widgetHeight:cards.count && cards.itemAt(0)?cards.itemAt(0).height:0
    readonly property var preferences:shell.widgets.preferences
    readonly property bool monochrome:preferences.desktopColor==="monochrome" || preferences.desktopColor==="auto" && (shell.exportedMenu.latestState || shell.exportedMenu.state).windowId>0
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
    Component.onCompleted:{initialized=true;syncItems();root.shell.widgetSurfaces[outputName]=root;}
    Component.onDestruction:delete root.shell.widgetSurfaces[outputName]
    visible:preferences.desktopVisible && cards.count>0
    anchors {left:true;right:true;top:true;bottom:true}
    color:"transparent";exclusiveZone:0
    exclusionMode:ExclusionMode.Ignore
    WlrLayershell.layer:WlrLayer.Bottom
    WlrLayershell.namespace:"lumen-desktop-widgets"
    WlrLayershell.keyboardFocus:visible?WlrKeyboardFocus.OnDemand:WlrKeyboardFocus.None
    mask:Region {id:inputMask}
    Instantiator {
        model:cards.count
        delegate:Region {required property int index;item:{root.maskRevision;return cards.itemAt(index);}}
        onObjectAdded:(index,object)=>inputMask.regions.push(object)
        onObjectRemoved:(index,object)=>{const position=inputMask.regions.indexOf(object);if(position>=0)inputMask.regions.splice(position,1);}
    }
    ListModel {id:stable;dynamicRoles:true}
    Repeater {id:cards;model:stable
        delegate:DesktopWidgetItem {required property var widget;required property int index;shell:root.shell;descriptor:widget;outputName:root.outputName;widgetIndex:index;availableWidth:root.width;availableHeight:root.height;monochrome:root.monochrome;active:root.visible}
        onItemAdded:root.maskRevision++
        onItemRemoved:root.maskRevision++
    }
    onVisibleChanged:if(!visible)for(let index=0;index<cards.count;index++)if(cards.itemAt(index))cards.itemAt(index).cancelMove()
}
