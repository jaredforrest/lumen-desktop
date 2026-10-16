import QtQuick
import QtQuick.Controls
Column {
    id:root
    required property var service
    property real maximumHeight:420 // Compatibility: the outer panel now scrolls.
    property bool reduceTransparency:false
    property var expandedGroups:({})
    spacing:12
    ControlRow {visible:root.service.available && root.service.history.length>0;width:parent.width;height:26;text:"Clear All";onClicked:root.service.command({action:"clear"})}
    PanelLabel {
        visible:!root.service.available || root.service.history.length===0;width:parent.width;height:65
        horizontalAlignment:Text.AlignHCenter;verticalAlignment:Text.AlignVCenter;wrapMode:Text.WordWrap
        text:!root.service.available?"Connecting to notifications…":root.service.dnd?"Do Not Disturb is on\nNo notifications":"No notifications";color:Theme.colors.muted
    }
    Repeater {
        model:root.service.groups
        Column {
            id:group
            required property var modelData
            width:root.width;spacing:6
            readonly property bool expanded:!!root.expandedGroups[modelData.key]
            function toggle(){const next=Object.assign({},root.expandedGroups);next[modelData.key]=!expanded;root.expandedGroups=next;}
            Row {visible:group.expanded && group.modelData.rows.length>1;width:parent.width
                ControlRow {width:parent.width-72;height:26;text:"Show Less";onClicked:group.toggle()}
                ControlRow {width:72;height:26;text:"Clear";onClicked:root.service.command({action:"clear",group:group.modelData.key})}
            }
            Repeater {
                model:group.expanded?group.modelData.rows:group.modelData.rows.slice(0,1)
                Item {
                    id:stack;required property var modelData
                    width:group.width;height:card.height+(!group.expanded && group.modelData.rows.length>1?8:0)
                    Rectangle {visible:!group.expanded && group.modelData.rows.length>2;x:8;y:8;width:parent.width-16;height:card.height;radius:18;color:Theme.colors.field;border.color:Theme.hairline}
                    Rectangle {visible:!group.expanded && group.modelData.rows.length>1;x:4;y:4;width:parent.width-8;height:card.height;radius:18;color:Theme.colors.background;border.color:Theme.hairline}
                    NotificationCard {id:card;width:parent.width;notification:stack.modelData;service:root.service;stackCount:group.modelData.rows.length;expandStack:!group.expanded && stackCount>1;reduceTransparency:root.reduceTransparency;onExpandRequested:group.toggle()}
                }
            }
        }
    }
    PanelLabel {visible:root.service.error!=="";width:parent.width;wrapMode:Text.WordWrap;text:root.service.error;color:Theme.colors.muted}
}
