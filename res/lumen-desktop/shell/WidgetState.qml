import QtQuick
import Quickshell
import Quickshell.Io
Item {
    id:root
    property var preferences:({calendarVisible:true,calendarFirst:false,style:"month",reduceTransparency:false,desktopVisible:false,desktopStyle:"month",desktopColor:"auto",positions:({})})
    property string error:""
    function validate(data) {
        let positions={};
        if(data.positions && typeof data.positions==="object")for(const key of Object.keys(data.positions).slice(0,32)){
            const p=data.positions[key];if(key.length<=256 && p && Number.isFinite(p.x) && Number.isFinite(p.y))positions[key]={x:Math.max(0,Math.min(1,p.x)),y:Math.max(0,Math.min(1,p.y))};
        }
        return {calendarVisible:data.calendarVisible!==false,calendarFirst:data.calendarFirst===true,style:data.style==="agenda"?"agenda":"month",reduceTransparency:data.reduceTransparency===true,desktopVisible:data.desktopVisible===true,desktopStyle:data.desktopStyle==="agenda"?"agenda":"month",desktopColor:["auto","color","monochrome"].includes(data.desktopColor)?data.desktopColor:"auto",positions:positions};
    }
    function setPreference(key,value){const next=Object.assign({},preferences);next[key]=value;preferences=validate(next);file.setText(JSON.stringify(preferences));}
    function setPosition(output,x,y){const next=Object.assign({},preferences.positions);next[output]={x:x,y:y};setPreference("positions",next);}
    FileView {id:file;path:(Quickshell.env("XDG_STATE_HOME") || Quickshell.env("HOME")+"/.local/state")+"/lumen-desktop/session/notification-centre.json";printErrors:false;atomicWrites:true;watchChanges:true
        onFileChanged:reload()
        onLoaded:{try{const value=JSON.parse(text());if(!value || typeof value!=="object" || Array.isArray(value))throw Error("Invalid widget settings");root.preferences=root.validate(value);root.error="";}catch(e){root.error="Widget preferences could not be read.";}}
        onSaved:root.error=""
        onSaveFailed:root.error="Widget changes could not be saved."
    }
}
