pragma Singleton
import QtQuick
QtObject {
    property QtObject current:null
    function claim(owner){if(current && current!==owner)current.dismiss();current=owner;}
    function release(owner){if(current===owner)current=null;}
    function dismiss(){if(current)current.dismiss();current=null;}
}
