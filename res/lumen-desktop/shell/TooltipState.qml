import QtQuick
QtObject {
    id:root
    property bool eligible:false
    property string identity:""
    property int delay:700
    property int timeout:4000
    property bool shown:false
    property bool retired:false
    function dismiss(){waiting.stop();expiry.stop();shown=false;retired=true;}
    function restart(){waiting.stop();expiry.stop();shown=false;retired=false;if(eligible)waiting.start();}
    function help(){if(!eligible)return;waiting.stop();retired=false;shown=true;expiry.restart();}
    onShownChanged:if(shown)TooltipCoordinator.claim(root);else TooltipCoordinator.release(root)
    onEligibleChanged:if(eligible)restart();else dismiss()
    onIdentityChanged:restart()
    property Timer waiting:Timer {interval:Math.max(100,Math.min(1500,root.delay));onTriggered:{if(root.eligible && !root.retired){root.shown=true;root.expiry.restart();}}}
    property Timer expiry:Timer {interval:Math.max(1000,Math.min(10000,root.timeout));onTriggered:root.dismiss()}
    Component.onCompleted:if(eligible)restart()
    Component.onDestruction:{TooltipCoordinator.release(root);waiting.stop();expiry.stop();}
}
