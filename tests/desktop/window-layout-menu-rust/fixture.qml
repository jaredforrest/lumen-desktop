import QtQuick
import Quickshell
import "@SHELL_ASSETS@" as D

ShellRoot {
    QtObject {
        id: stub
        property string popupOwner: ""
        property double dragged: 0
        function dragOverview(id) { dragged = id; }
        signal dismissPopups(string owner)
        function claimPopup(owner) { popupOwner = owner; dismissPopups(owner); }
    }
    D.WindowLayoutMenu { id: menu; shell: stub }
    Timer {
        id: finish
        interval: 200
        onTriggered: {
            console.log("LAYOUT_MENU_PASS");
            menu.dismiss();
            Qt.quit();
        }
    }
    function check(value, message) { if (!value) throw new Error(message); }
    Timer {
        interval: 100
        running: true
        onTriggered: {
            menu.event('{"lumen_layout":1,"id":42,"x":20,"y":30}');
            check(menu.target === 42, "valid target");
            const capture={version:1,conId:43,pid:123,foreignId:"0123456789abcdef0123456789abcdef",appId:"org.quickshell",title:"Captured window",key:"43:123:0123456789abcdef0123456789abcdef"};
            menu.event(JSON.stringify({lumen_layout:1,id:43,x:20,y:30,capture:capture}));
            check(menu.target===43 && menu.targetCapture.key===capture.key,"capture retained");
            menu.event('{"lumen_layout":1,"id":0}');
            check(menu.targetCapture.key===capture.key,"native leave cannot cancel captured hover");
            menu.event(JSON.stringify({lumen_layout:1,id:44,x:20,y:30,capture:capture}));
            check(menu.target===43,"reject capture for another window");
            menu.pairedLayoutsSupported=true;
            check(menu.choices.length===11,"captured menu excludes uncaptured pairing actions");
            menu.dismiss();
            menu.pairedLayoutsSupported=false;
            menu.event('{"lumen_layout":1,"id":42,"x":20,"y":30}');
            menu.event('{"lumen_layout":1,"id":"43","x":20,"y":30}');
            check(menu.target === 42, "reject string target");
            menu.event('{"lumen_layout":1,"id":-1,"x":20,"y":30}');
            check(menu.target === 42, "reject negative target");
            menu.event('{"lumen_layout":1,"id":43,"x":"bad","y":30}');
            check(menu.target === 42, "reject nonfinite coordinate");
            check(menu.choices.length === 11, "layout choices");
            menu.updateCapabilities('{"native_edge_tiling":true}');
            menu.event('{"lumen_drag_overview":1,"id":99}');
            check(stub.dragged === 99, "captured drag target");
            menu.event('{"lumen_drag_overview":1,"id":"100"}');
            check(stub.dragged === 99, "reject string drag target");
            menu.updateCapabilities('{}');
            menu.event('{"lumen_drag_overview":1,"id":100}');
            check(stub.dragged === 99, "ignore unavailable drag capability");
            menu.updateCapabilities('{"native_window_layouts":true,"native_show_desktop":true}');
            check(menu.supported && menu.showDesktopSupported, "native capabilities accepted");
            menu.updateCapabilities('{"native_window_layouts":"true","native_show_desktop":1}');
            check(!menu.supported && !menu.showDesktopSupported, "nonboolean capabilities rejected");
            menu.updateCapabilities('{"native_window_layouts":true,"native_show_desktop":true}');
            menu.updateCapabilities('invalid');
            check(!menu.supported && !menu.showDesktopSupported, "invalid capabilities reset both");
            menu.updateCapabilities('{"native_window_layouts":true,"native_show_desktop":true}');
            stub.dismissPopups("other");
            check(menu.target === 0, "cancel pending on other popup");
            menu.event('{"lumen_layout":1,"id":42,"x":20,"y":30}');
            menu.open();
            check(menu.visible, "layout menu opens on available screen");
            finish.start();
        }
    }
}
