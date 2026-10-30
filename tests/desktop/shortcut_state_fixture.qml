import QtQuick
import Quickshell
import "LUMEN_ASSETS" as D

ShellRoot {
    D.ShortcutState { id: shortcuts }
    Component.onCompleted:{
        D.ShortcutLayoutState.receive(JSON.stringify({version:1,available:true,generation:"a".repeat(64),activeMode:"default",diagnostics:[],seats:[{seat:"seat0",name:"Fixture keyboard",declared:[{id:"test",action:"window.fill",keycaps:["Ctrl++"],hintAvailable:true,collision:true}],native:[{identity:{kind:"code",mode:"default",device:"*",group:null,keys:[21]},conflicts:[1],keycaps:["Ctrl++"]}]}]}));
        const layout=D.ShortcutLayoutState;
        if(layout.rows("test").length!==1 || layout.keycaps("test")!=="Ctrl++" || !layout.collision("test") || layout.hint("test")!=="Ctrl++" || !layout.nativeSummary.includes("command identity unverified") || !layout.nativeSummary.includes("code") || !layout.bindingSummary.includes("Fill the work area"))throw Error("Actual shortcut layout presentation failed");
        layout.receive("{bad");
        if(layout.available || layout.generation!=="" || layout.rows("test").length!==0)throw Error("Invalid layout observation retained hints");
        console.log("SHORTCUT_LAYOUT_MODEL passed");
    }
    property string last: "unreported"
    Timer {
        running: true
        repeat: true
        interval: 25
        onTriggered: {
            const value = JSON.stringify({available: shortcuts.available,
                hint: shortcuts.hint("window.fullscreen.toggle"), bindings: shortcuts.bindings.length});
            if (value !== last) { last = value; console.log("SHORTCUT_STATE " + value); }
        }
    }
}
