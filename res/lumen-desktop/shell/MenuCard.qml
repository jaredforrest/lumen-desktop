import QtQuick
PanelSurface {
    id:card
    required property var host
    required property var menuNode
    required property int depth
    required property real availableHeight
    property real maximumWidth:410
    function debugRows() {let a=[];for(let i=0;i<delegates.count;i++){const r=delegates.itemAt(i);a.push({id:r.modelData.id,y:card.y+scroll.y+r.y-scroll.contentY,hover:r.pointerInside});}return a;}
    function rowY(id) {for(let i=0;i<delegates.count;i++){const r=delegates.itemAt(i);if(r.modelData.id===id)return card.y+scroll.y+r.y-scroll.contentY;}return card.y+6;}
    function reveal(id) {for(let i=0;i<delegates.count;i++){const r=delegates.itemAt(i);if(r.modelData.id!==id)continue;if(r.y<scroll.contentY)scroll.contentY=r.y;else if(r.y+r.height>scroll.contentY+scroll.height)scroll.contentY=r.y+r.height-scroll.height;}}
    readonly property bool overflow:naturalHeight>availableHeight
    readonly property bool canScrollUp:overflow && scroll.contentY>1
    readonly property bool canScrollDown:overflow && scroll.contentY+scroll.height<scroll.contentHeight-1
    readonly property var rows:menuNode?menuNode.children.filter(n=>n.visible):[]
    readonly property real naturalHeight:rows.reduce((h,n)=>h+(n.separator?9:Theme.menuRowHeight)+1,12)
    width:Math.min(maximumWidth,host.widthFor(menuNode));height:Math.min(naturalHeight,availableHeight);radius:Theme.menuRadius
    Flickable {
        id:scroll;anchors.fill:parent;anchors.margins:6;anchors.topMargin:card.overflow?16:6;anchors.bottomMargin:card.overflow?16:6;contentWidth:width;contentHeight:contents.height;clip:true;boundsBehavior:Flickable.StopAtBounds
        Column {
            id:contents;width:card.width-12;spacing:1
            Repeater {
                id:delegates;model:card.rows
                Rectangle {
                    id:row
                    required property var modelData
                    readonly property bool pointerInside:hit.containsMouse
                    property bool selected:modelData.enabled && (card.host.highlighted[card.depth]===modelData.id || (modelData.submenu && card.host.path[card.depth+1]===modelData.id))
                    width:contents.width;height:modelData.separator?9:Theme.menuRowHeight;radius:5
                    color:selected?Theme.colors.accent:"transparent"
                    Rectangle {visible:modelData.separator;x:8;anchors.verticalCenter:parent.verticalCenter;width:parent.width-16;height:1;color:Theme.hairline}
                    Text {visible:!modelData.separator && modelData.checked;x:6;width:17;anchors.verticalCenter:parent.verticalCenter;text:modelData.toggle==="radio"?"●":"✓";font.family:Theme.font;font.pixelSize:12;color:row.selected?Theme.colors.selectedText:Theme.colors.text;horizontalAlignment:Text.AlignHCenter}
                    SystemGlyph {visible:!modelData.separator && !modelData.checked && name!=="";x:7;anchors.verticalCenter:parent.verticalCenter;width:14;height:14;name:card.host.symbolFor(modelData.label);ink:row.selected?Theme.colors.selectedText:Theme.colors.text;opacity:modelData.enabled?1:0.4}
                    PanelLabel {visible:!modelData.separator;x:28;anchors.verticalCenter:parent.verticalCenter;width:parent.width-shortcut.implicitWidth-56;text:card.host.label(modelData.label);color:row.selected?Theme.colors.selectedText:Theme.colors.text;opacity:modelData.enabled?1:0.4}
                    PanelLabel {id:shortcut;visible:!modelData.separator && !modelData.submenu;anchors.right:parent.right;anchors.rightMargin:9;anchors.verticalCenter:parent.verticalCenter;text:card.host.shortcut(modelData.shortcut);font.pixelSize:12;color:row.selected?Theme.colors.selectedText:Theme.colors.muted;opacity:modelData.enabled?1:0.5}
                    SystemGlyph {visible:modelData.submenu;anchors.right:parent.right;anchors.rightMargin:7;anchors.verticalCenter:parent.verticalCenter;name:"chevron-right";width:12;height:12;ink:row.selected?Theme.colors.selectedText:Theme.colors.text;opacity:modelData.enabled?1:0.4}
                    Timer {
                        id:expand;interval:160
                        onTriggered:{if(row.modelData.submenu)card.host.openChild(card.depth,row.modelData.id,card.y+scroll.y+row.y-scroll.contentY);else card.host.closeBelow(card.depth);}
                    }
                    MouseArea {
                        id:hit;anchors.fill:parent;hoverEnabled:true;enabled:!modelData.separator
                        onEntered:if(row.modelData.enabled){
                            card.host.select(card.depth,row.modelData.id);
                            expand.interval=card.host.hoverDelay(card.depth,card.x+row.x+mouseX,card.y+scroll.y+row.y-scroll.contentY+mouseY);expand.restart();
                        }
                        onPositionChanged:mouse=>card.host.rememberPointer(card.x+row.x+mouse.x,card.y+scroll.y+row.y-scroll.contentY+mouse.y)
                        onExited:expand.stop()
                        onClicked:{
                            if(!modelData.enabled)return;
                            expand.stop();
                            if(modelData.submenu)card.host.openChild(card.depth,modelData.id,card.y+scroll.y+row.y-scroll.contentY);
                            else card.host.activateItem(modelData.id);
                        }
                    }
                }
            }
        }
    }
    Repeater {
        model:2
        Rectangle {
            required property int index
            visible:index===0?card.canScrollUp:card.canScrollDown
            x:6;y:index===0?2:card.height-16;width:card.width-12;height:14;radius:4;color:Theme.colors.background
            Text {anchors.centerIn:parent;text:parent.index===0?"▴":"▾";color:Theme.colors.text;font.pixelSize:12}
            MouseArea {id:scrollHover;anchors.fill:parent;hoverEnabled:true;onClicked:scroll.contentY=Math.max(0,Math.min(scroll.contentHeight-scroll.height,scroll.contentY+(parent.index===0?-80:80)))}
            Timer {interval:65;repeat:true;running:scrollHover.containsMouse;onTriggered:scroll.contentY=Math.max(0,Math.min(scroll.contentHeight-scroll.height,scroll.contentY+(parent.index===0?-8:8)))}
        }
    }
}
