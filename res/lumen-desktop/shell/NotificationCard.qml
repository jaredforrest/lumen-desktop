import QtQuick
import QtQuick.Controls
import Quickshell
PanelSurface {
    id: root;objectName:"notification-card"
    required property var notification
    required property var service
    property bool compact: false
    property alias deliveryMenu:options
    property int stackCount:1
    property bool expandStack:false
    property bool reduceTransparency:false
    readonly property bool opaqueMaterial:reduceTransparency || !!Theme.preferences.reduceTransparency || Theme.increaseContrast
    signal expandRequested()
    activeFocusOnTab:true
    Accessible.role:Accessible.Button
    onNotificationChanged:timeLabel=stamp()
    Accessible.name:notification.app+", "+notification.summary+(expandStack?", "+stackCount+" notifications. Expand stack":expanded?", details expanded":"")
    Accessible.description:notification.body
    Accessible.onPressAction:activate()
    Keys.onReturnPressed:activate()
    Keys.onSpacePressed:activate()
    function activate(){if(expandStack)expandRequested();else if(notification.actions.some(a=>a.id==="default"))service.invoke(notification,"default");else detailsRequested();}
    function appEntry(){
        if(notification.desktop){const direct=DesktopEntries.byId(notification.desktop.replace(/\.desktop$/,""));if(direct)return direct;}
        const normal=s=>s.toLowerCase().replace(/[^a-z0-9]/g,"");const name=normal(notification.app);
        const aliases={chatgpt:"codex-desktop",codex:"codex-desktop",brave:"brave-browser",localsend:"LocalSend",bitwarden:"bitwarden"};
        if(aliases[name]){const entry=DesktopEntries.byId(aliases[name]);if(entry)return entry;}
        return DesktopEntries.applications.values.find(e=>normal(e.name)===name || normal(e.id)===name) || null;
    }
    HoverHandler {id:cardHover}
    color:Theme.compositorEffects && !root.opaqueMaterial?Qt.alpha(Theme.colors.background,.91):Theme.colors.background
    readonly property bool expanded: !!service.expandedCards[notification.id]
    signal detailsRequested()
    implicitHeight: content.height + 28
    border.width:root.opaqueMaterial?1:0
    radius: 18
    function stamp(){const age=Math.max(0,(Date.now()-notification.timestamp)/60000);return age<1?"now":age<60?Math.floor(age)+"m ago":age<1440?Math.floor(age/60)+"h ago":Qt.formatDateTime(new Date(notification.timestamp),"d MMM");}
    property string timeLabel: stamp()
    Timer {interval:60000;running:root.visible;repeat:true;onTriggered:root.timeLabel=root.stamp()}
    MouseArea {anchors.fill:parent;hoverEnabled:true;onEntered:root.service.command({action:"hold",id:root.notification.id,enabled:true});onExited:root.service.command({action:"hold",id:root.notification.id,enabled:false});onClicked:root.activate()}
    Column {
        id:content;x:14;y:14;width:parent.width-28;spacing:6
        Row {
            width:parent.width;spacing:8
            Image {
                id:appIcon
                width:28;height:28;sourceSize.width:56;sourceSize.height:56
                source:{const icon=root.notification.icon;if(icon.startsWith("file:"))return icon;const entry=root.appEntry();return Quickshell.iconPath(icon || (entry?entry.icon:""),true) || Qt.resolvedUrl("icons/application.svg");}
                asynchronous:true;fillMode:Image.PreserveAspectFit
                Image {anchors.fill:parent;visible:appIcon.status===Image.Error;source:Qt.resolvedUrl("icons/application.svg");sourceSize.width:56;sourceSize.height:56}
            }
            PanelLabel {width:parent.width-(optionsButton.visible?121:89);anchors.verticalCenter:parent.verticalCenter;text:root.notification.app+(root.expandStack?" · "+root.stackCount:"");elide:Text.ElideRight;textFormat:Text.PlainText;font.pixelSize:11;font.weight:Font.DemiBold}
            PanelLabel {width:45;anchors.verticalCenter:parent.verticalCenter;horizontalAlignment:Text.AlignRight;text:root.timeLabel;font.pixelSize:10;color:Theme.colors.muted}
            AbstractButton {
                id:dismissButton;objectName:"notification-dismiss";parent:root;x:4;y:-8;width:20;height:20;visible:cardHover.hovered || hovered || root.activeFocus || activeFocus || root.expanded;enabled:root.service.available;Accessible.name:"Dismiss notification"
                contentItem:PanelLabel {text:"×";font.pixelSize:18;horizontalAlignment:Text.AlignHCenter;verticalAlignment:Text.AlignVCenter}
                background:Rectangle {radius:10;color:parent.hovered || parent.activeFocus?Theme.colors.hover:Theme.colors.surface;border.color:Theme.hairline;border.width:1}
                onClicked:root.service.command({action:"dismiss",id:root.notification.id})
            }
        AbstractButton {
            id:optionsButton;width:24;height:24;visible:cardHover.hovered || root.activeFocus || activeFocus || options.visible
            Accessible.name:"Notification options for "+root.notification.app
            contentItem:PanelLabel {text:"…";horizontalAlignment:Text.AlignHCenter}
            onClicked:options.open()
            Menu {id:options
                MenuItem {text:"Allow Banners";onTriggered:root.service.command({action:"policy",app:root.notification.desktop || root.notification.app,policy:"banners"})}
                MenuItem {objectName:"notification-quiet";text:"Deliver Quietly";onTriggered:root.service.command({action:"policy",app:root.notification.desktop || root.notification.app,policy:"silent"})}
                MenuItem {text:"Turn Off Notifications";onTriggered:root.service.command({action:"policy",app:root.notification.desktop || root.notification.app,policy:"off"})}
            }
        }
        }
        PanelLabel {width:parent.width;text:root.notification.summary;textFormat:Text.PlainText;font.weight:Font.DemiBold;wrapMode:Text.Wrap;maximumLineCount:root.expanded?100:2}
        PanelLabel {id:message;visible:text!=="";width:parent.width;text:root.notification.body;textFormat:Text.PlainText;wrapMode:Text.Wrap;maximumLineCount:root.expanded?1000:root.compact?2:4;font.pixelSize:12}
        Image {visible:source.toString()!=="";source:root.notification.image;width:parent.width;height:visible?(root.compact?90:140):0;fillMode:Image.PreserveAspectFit;asynchronous:true}
        ControlRow {
            visible:message.truncated || root.expanded
            height:26;width:parent.width;text:root.expanded?"Show Less":"Show More"
            onClicked:{if(root.compact)root.detailsRequested();else root.service.toggleExpanded(root.notification.id);}
        }
        Flow {
            visible:!root.expandStack
            width:parent.width;spacing:4
            Repeater {
                model:root.notification.actions.filter(a=>a.id!=="default").slice(0,root.compact?2:16)
                ControlRow {required property var modelData;width:Math.min(content.width,Math.max(70,metrics.width+16));text:modelData.label;enabled:root.service.available;TextMetrics {id:metrics;text:modelData.label;font.family:Theme.font;font.pixelSize:13}onClicked:root.service.invoke(root.notification,modelData.id)}
            }
            ControlRow {visible:root.notification.actions.some(a=>a.id==="default");width:80;text:"Open";enabled:root.service.available;onClicked:root.service.invoke(root.notification,"default")}
            ControlRow {visible:root.compact && root.notification.actions.length>2;width:80;text:"More…";onClicked:root.detailsRequested()}
        }
    }
}
