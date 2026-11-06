import QtQuick
import QtQuick.Controls
Column {
    id:root
    required property var service
    property real maximumHeight:420 // Compatibility: the outer panel now scrolls.
    property bool reduceTransparency:false
    property var expandedGroups:({})
    spacing:12
    function stackStatus(){const rows=[];for(let i=0;i<groupRepeater.count;i++){const group=groupRepeater.itemAt(i);if(group && group.visible){const point=group.mapToItem(null,0,0);rows.push({key:group.modelData.key,expanded:group.expanded,count:group.modelData.rows.length,x:point.x,y:point.y,width:group.width,height:group.height});}}return rows;}
    ControlRow {visible:root.service.available && root.service.history.length>0;width:parent.width;height:26;text:"Clear All";onClicked:root.service.command({action:"clear"})}
    PanelSurface {
        visible:!root.service.available || root.service.history.length===0;width:parent.width;height:Math.max(65,emptyLabel.implicitHeight+28);radius:18;color:root.reduceTransparency?Theme.colors.background:Theme.popoverMaterial
        PanelLabel {
        id:emptyLabel;anchors.fill:parent;anchors.margins:14
        horizontalAlignment:Text.AlignHCenter;verticalAlignment:Text.AlignVCenter;wrapMode:Text.WordWrap
        text:!root.service.available?"Connecting to notifications…":root.service.dnd?"Do Not Disturb is on\nNo notifications":"No notifications";color:Theme.colors.muted
        }
    }
    Repeater {
        id:groupRepeater
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
                    Canvas {
                        id:backplates;anchors.fill:parent;visible:!group.expanded && group.modelData.rows.length>1
                        property color farFill:Theme.colors.field
                        property color nearFill:Theme.colors.background
                        property color outline:Theme.hairline
                        property real frontHeight:card.height
                        onFarFillChanged:requestPaint()
                        onNearFillChanged:requestPaint()
                        onOutlineChanged:requestPaint()
                        onFrontHeightChanged:requestPaint()
                        onWidthChanged:requestPaint()
                        onHeightChanged:requestPaint()
                        onVisibleChanged:if(visible)requestPaint()
                        function rounded(ctx,x,y,w,h,r){ctx.beginPath();ctx.moveTo(x+r,y);ctx.lineTo(x+w-r,y);ctx.quadraticCurveTo(x+w,y,x+w,y+r);ctx.lineTo(x+w,y+h-r);ctx.quadraticCurveTo(x+w,y+h,x+w-r,y+h);ctx.lineTo(x+r,y+h);ctx.quadraticCurveTo(x,y+h,x,y+h-r);ctx.lineTo(x,y+r);ctx.quadraticCurveTo(x,y,x+r,y);ctx.closePath();}
                        onPaint:{
                            const ctx=getContext("2d");ctx.reset();ctx.clearRect(0,0,width,height);ctx.lineWidth=1;ctx.strokeStyle=outline;
                            if(group.modelData.rows.length>2){rounded(ctx,8,8,width-16,frontHeight,18);ctx.fillStyle=farFill;ctx.fill();ctx.stroke();}
                            rounded(ctx,4,4,width-8,frontHeight,18);ctx.fillStyle=nearFill;ctx.fill();ctx.stroke();
                            // Backplates exist only outside the front silhouette: the
                            // translucent card samples the desktop, not another card.
                            ctx.globalCompositeOperation="destination-out";rounded(ctx,0,0,width,frontHeight,18);ctx.fill();
                        }
                    }
                    NotificationCard {id:card;width:parent.width;notification:stack.modelData;service:root.service;stackCount:group.modelData.rows.length;expandStack:!group.expanded && stackCount>1;reduceTransparency:root.reduceTransparency;onExpandRequested:group.toggle()}
                }
            }
        }
    }
    PanelLabel {visible:root.service.error!=="";width:parent.width;wrapMode:Text.WordWrap;text:root.service.error;color:Theme.colors.muted}
}
