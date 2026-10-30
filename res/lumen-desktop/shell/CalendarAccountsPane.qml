import QtQuick
import QtQuick.Controls
import Quickshell
import Quickshell.Io
Column {
    id:root
    property string message:""
    property bool configured:false
    spacing:12
    PanelLabel {text:"Google";font.weight:Font.DemiBold}
    PanelLabel {width:parent.width;wrapMode:Text.WordWrap;text:"Connect or manage Google calendars with Online Accounts.";color:Theme.colors.muted}
    DialogButton {width:parent.width;text:"Manage Google Accounts…";onClicked:AppLaunch.run(["lumen-google-calendar-setup"])}
    PanelLabel {text:"iCloud";font.weight:Font.DemiBold}
    PanelLabel {width:parent.width;wrapMode:Text.WordWrap;text:"Download calendars and contacts using an Lumen app-specific password. Your password is saved in the desktop keyring. iCloud calendars are read-only here.";color:Theme.colors.muted}
    DesktopTextField {id:email;width:parent.width;placeholderText:"Lumen Account email";Accessible.name:"Lumen Account email";enabled:!setup.running;selectByMouse:true;color:Theme.colors.text;placeholderTextColor:Theme.colors.muted;background:Rectangle {radius:6;color:Theme.colors.field;border.color:parent.activeFocus?Theme.colors.accent:Theme.hairline}}
    DesktopTextField {id:password;width:parent.width;placeholderText:"App-specific password";Accessible.name:"Lumen app-specific password";echoMode:TextInput.Password;enabled:!setup.running;selectByMouse:true;color:Theme.colors.text;placeholderTextColor:Theme.colors.muted;background:Rectangle {radius:6;color:Theme.colors.field;border.color:parent.activeFocus?Theme.colors.accent:Theme.hairline}}
    ControlRow {width:parent.width;text:"Open cloud account…";onClicked:AppLaunch.run(["xdg-open","https://www.icloud.com/"])}
    DialogButton {width:parent.width;text:setup.running?"Connecting…":"Connect or Update iCloud";enabled:!setup.running && email.text.length>0 && password.text.length>0;onClicked:{root.message="";setup.running=true;}}
    PanelLabel {visible:!!root.message;width:parent.width;wrapMode:Text.WordWrap;text:root.message;textFormat:Text.PlainText;color:Theme.colors.muted}
    Process {
        id:setup;command:["lumen-icloud-setup","setup-json"];stdinEnabled:true
        property bool received:false
        onStarted:{received=false;write(JSON.stringify({email:email.text,password:password.text})+"\n");password.clear();}
        stdout:StdioCollector {onStreamFinished:{try{const data=JSON.parse(text);setup.received=true;root.message=data.success?"iCloud calendars and contacts are connected.":data.error || "iCloud could not connect.";}catch(e){root.message="iCloud could not connect. Try again.";}}}
        onExited:if(!received)root.message="Account setup did not respond. Try again."
    }
}
