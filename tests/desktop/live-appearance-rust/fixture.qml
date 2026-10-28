import QtQuick
import Quickshell
import Quickshell.Io
import "assets/shell" as D

ShellRoot {
    QtObject {
        id: stub
        property string popupOwner: ""
        signal dismissPopups(string owner)
        function claimPopup(owner) { popupOwner = owner; dismissPopups(owner); }
    }
    D.HelpWindow { id: help; shell: stub }
    D.SessionOptions { id: session; executeActions: false }
    D.RemoteDesktopWindow { id: remote; shell: stub }
    D.TextToolsWindow { id: tools; shell: stub }
    D.FileInfoWindow { id: info }
    D.QuickLookWindow { id: preview }
    IpcHandler {
        target: "audit"
        function open(name: string): void {
            for (const window of [help, session, remote, tools, info, preview]) window.visible = false;
            if (name === "help") help.show();
            else if (name === "session") session.show("suspend");
            else if (name === "remote") remote.visible = true;
            else if (name === "text") tools.visible = true;
            else if (name === "info") info.show({name:"Fixture.txt",path:"/fixture/Fixture.txt",size:"42 bytes",type:"Text",permissions:"Read and write"});
            else if (name === "quick") {
                preview.path = "Fixture.txt";
                preview.result = {type:"text",title:"Fixture.txt",text:"Stable preview content"};
                preview.visible = true;
            }
        }
        function state(): string {
            return JSON.stringify({dark:D.Theme.dark,background:D.Theme.colors.background});
        }
    }
}
