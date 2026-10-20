import QtQuick
DialogButton {
    id:root
    property var selection:null
    property var pressedSelection:null
    signal activatedIntent(var captured)
    function capture(){return selection?JSON.parse(JSON.stringify(selection)):null;}
    function commit(){
        const captured=pressedSelection || capture();pressedSelection=null;
        if(captured && selection && captured.capability===selection.capability && captured.session===selection.session && captured.choice===selection.choice && captured.application===selection.application)activatedIntent(captured);
    }
    function beginIntent(){pressedSelection=capture();}
    function cancelIntent(){pressedSelection=null;}
    onPressed:beginIntent()
    onCanceled:cancelIntent()
    onClicked:commit()
}
