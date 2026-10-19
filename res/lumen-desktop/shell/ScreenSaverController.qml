import QtQuick
import Quickshell
import Quickshell.Wayland
Item {
    id:root
    required property var state
    property bool showing:false
    property string reason:""
    property real phase:0
    function start(preview){if(!state.permitted){reason="Native unlocked-session authority is unavailable or the session is locked.";return false;}showing=true;reason=preview?"preview":"idle";return true;}
    function dismiss(){showing=false;reason="dismissed";}
    Connections {target:root.state
        function onPermittedChanged(){if(!root.state.permitted)root.dismiss();}
        function onPreferencesChanged(){if(!root.state.preferences.enabled && root.reason==="idle")root.dismiss();}
    }
    IdleMonitor {id:idle;enabled:root.state.preferences.enabled;timeout:root.state.preferences.idleSeconds;respectInhibitors:true
        onIsIdleChanged:{if(isIdle && root.state.preferences.enabled)root.start(false);else if(!isIdle && root.showing)root.dismiss();}
    }
    NumberAnimation on phase {from:0;to:1;duration:90000;loops:Animation.Infinite;running:root.showing && root.state.preferences.motion && !Theme.preferences.reduceMotion}
    Variants {model:Quickshell.screens
        delegate:Loader {id:surfaceLoader;required property var modelData;active:root.showing
            sourceComponent:ScreenSaverSurface {screen:surfaceLoader.modelData;controller:root}
        }
    }
}
