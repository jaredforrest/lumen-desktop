import QtQuick
import Quickshell
import Quickshell.Io
import "WidgetModel.js" as Model
Item {
    id:root
    property var preferences:({calendarVisible:true,calendarFirst:false,style:"month",reduceTransparency:false,desktopVisible:false,desktopStyle:"month",desktopColor:"auto",positions:({}),desktopItems:null})
    readonly property var desktopItems:Model.effective(preferences)
    signal editorRequested()
    function edit(){editorRequested();}
    property string error:""
    function validate(data) {
        let positions={};
        if(data.positions && typeof data.positions==="object")for(const key of Object.keys(data.positions).slice(0,32)){
            const p=data.positions[key];if(key.length<=256 && p && Number.isFinite(p.x) && Number.isFinite(p.y))positions[key]={x:Math.max(0,Math.min(1,p.x)),y:Math.max(0,Math.min(1,p.y))};
        }
        return {calendarVisible:data.calendarVisible!==false,calendarFirst:data.calendarFirst===true,style:data.style==="agenda"?"agenda":"month",reduceTransparency:data.reduceTransparency===true,desktopVisible:data.desktopVisible===true,desktopStyle:data.desktopStyle==="agenda"?"agenda":"month",desktopColor:["auto","color","monochrome"].includes(data.desktopColor)?data.desktopColor:"auto",positions:positions,desktopItems:Model.normalize(data.desktopItems)};
    }
    function setPreference(key,value){const next=Object.assign({},preferences);next[key]=value;preferences=validate(next);file.setText(JSON.stringify(preferences));}
    function setPosition(output,x,y){const next=Object.assign({},preferences.positions);next[output]={x:x,y:y};setPreference("positions",next);}
    function addWidget(type) {
        if(!["calendar","clock"].includes(type) || desktopItems.length>=16)return;
        const items=preferences.desktopItems===null && !preferences.desktopVisible?[]:desktopItems.slice();
        items.push({id:"widget-"+Date.now().toString(36)+"-"+Math.random().toString(36).slice(2,10),type:type,size:"medium",output:"*",style:"month",hourCycle:"locale",positions:{}});
        const next=Object.assign({},preferences,{desktopItems:items,desktopVisible:true});preferences=validate(next);file.setText(JSON.stringify(preferences));
    }
    function updateWidget(id,patch){const next=Model.updated(desktopItems,id,patch);if(next)setPreference("desktopItems",next);}
    function removeWidget(id){if(desktopItems.some(item=>item.id===id))setPreference("desktopItems",desktopItems.filter(item=>item.id!==id));}
    function positionWidget(id,output,x,y){const item=desktopItems.find(item=>item.id===id);if(!item)return;const positions=Object.assign({},item.positions);positions[output]={x:x,y:y};updateWidget(id,{positions:positions});}
    FileView {id:file;path:(Quickshell.env("XDG_STATE_HOME") || Quickshell.env("HOME")+"/.local/state")+"/lumen-desktop/session/notification-centre.json";printErrors:false;atomicWrites:true;watchChanges:true
        onFileChanged:reload()
        onLoaded:{try{const value=JSON.parse(text());if(!value || typeof value!=="object" || Array.isArray(value))throw Error("Invalid widget settings");root.preferences=root.validate(value);root.error="";}catch(e){root.error="Widget preferences could not be read.";}}
        onSaved:root.error=""
        onSaveFailed:root.error="Widget changes could not be saved."
    }
}
