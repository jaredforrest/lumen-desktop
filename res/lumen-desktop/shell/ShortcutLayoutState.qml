pragma Singleton
import QtQuick
import Quickshell
import Quickshell.Io
import "ShortcutHints.js" as ShortcutHints
QtObject {
    id:root
    readonly property bool enabled:!!Quickshell.env("SWAYSOCK")
    property bool available:false
    property string generation:""
    property string activeMode:""
    property string error:"Native keyboard layout observation is unavailable; displayed chords are declared symbols."
    property var seats:[]
    property var diagnostics:[]
    function clear(message){available=false;generation="";activeMode="";seats=[];diagnostics=[];error=message;}
    function receive(text){
        try{
            if(text.length>2097152)throw Error("Keyboard observation exceeds limits");
            const data=JSON.parse(text);
            if(data.version!==1 || typeof data.available!=="boolean")throw Error("Invalid keyboard observation");
            if(!data.available){clear(data.error || "Native keyboard layout observation is unavailable");return;}
            if(typeof data.generation!=="string" || !/^[a-f0-9]{64}$/.test(data.generation) || typeof data.activeMode!=="string" || !Array.isArray(data.seats) || data.seats.length>16 || !Array.isArray(data.diagnostics) || data.diagnostics.length>32)throw Error("Invalid keyboard observation");
            for(const seat of data.seats){if(typeof seat.name!=="string" || !Array.isArray(seat.declared) || seat.declared.length>4096 || !Array.isArray(seat.native) || seat.native.length>2048)throw Error("Invalid keyboard seat");}
            seats=data.seats;diagnostics=data.diagnostics;activeMode=data.activeMode;available=true;error="";generation=data.generation;
        }catch(error){clear("Native keyboard layout observation could not be read");}
    }
    function flatten(values,project){let result=[];for(const value of values)result=result.concat(project(value));return result;}
    function rows(id){return flatten(seats,seat=>seat.declared.filter(row=>row.id===id));}
    function keycaps(id){return rows(id).map(row=>row.keycaps.join(" or ")).filter(text=>text.length).filter((text,index,all)=>all.indexOf(text)===index).join(" / ");}
    function collision(id){return rows(id).some(row=>row.collision===true);}
    function hint(id){const candidates=rows(id);if(!available || !candidates.length || candidates.some(row=>row.hintAvailable!==true))return "";return candidates[0].keycaps[0] || "";}
    readonly property string nativeSummary:flatten(seats,seat=>seat.native.filter(row=>row.conflicts.length || row.identity.kind==="code" || row.identity.mode!=="default" || row.identity.device!=="*" || row.identity.group!==null || row.identity.keys.length!==1).slice(0,16).map(row=>"Custom shortcut · "+row.identity.kind+" · mode "+row.identity.mode+" · "+(row.keycaps.join(" or ") || "physical sequence unavailable")+(row.conflicts.length?" · overlaps "+row.conflicts.length+" native binding(s)":"")+" · command identity unverified")).join("\n")
    readonly property string bindingSummary:flatten(seats,seat=>seat.declared.slice(0,16).map(row=>ShortcutHints.actionName(row.action)+": "+row.keycaps.join(" or ")+(row.collision?" · Physical shortcut collision detected":""))).concat(seats.filter(seat=>seat.declared.length>16).map(seat=>(seat.declared.length-16)+" additional configured shortcuts on "+seat.seat)).join("\n")
    readonly property string summary:available?seats.map(seat=>seat.seat+": "+seat.name).join(" · ")+" · mode "+activeMode:error
    property Process observer:Process {
        command:["lumen-shortcut-layout","watch"];running:root.enabled
        stdout:SplitParser {onRead:data=>root.receive(data)}
        onExited:root.clear("Native keyboard layout observer disconnected; declared symbol chords remain available")
    }
    property Timer retry:Timer {interval:2000;repeat:true;running:root.enabled && !root.observer.running;onTriggered:root.observer.running=true}
}
