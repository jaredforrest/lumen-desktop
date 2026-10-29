import QtQuick
import QtQuick.Controls
import QtQuick.Window
import Quickshell
import "desktop" as D
ShellRoot {
    id: audit
    property var cases: @CASES@
    property var palettes: @PALETTES@
    property int index: 0
    property int stage: 0
    property bool painting: false
    property var records: []
    property var node: ({id:1,label:"File",children:[
        {id:2,label:"Save Document As…",shortcut:"Control+Shift+S",visible:true,enabled:true,separator:false,checked:false,toggle:"",submenu:false},
        {id:3,label:"Show Private Details",shortcut:"Control+Alt+I",visible:true,enabled:true,separator:false,checked:true,toggle:"check",submenu:false},
        {id:4,label:"Unavailable Action",shortcut:"Control+P",visible:true,enabled:false,separator:false,checked:false,toggle:"",submenu:false}]})
    QtObject { id: host
        property var highlighted: ({})
        property var path: []
        function widthFor(node) { return 350; }
        function capture(depth,id) { return null; }
        function valid(intent) { return false; }
        function label(value) { return value; }
        function shortcut(value) { return value.split("+").map(key=>({Control:"⌃",Shift:"⇧",Alt:"⌥"}[key] || key)).join(""); }
        function symbolFor(value) { return ""; }
    }
    ApplicationWindow { id: window
        visible: true; width: 700; height: 280; color: D.Theme.colors.background
        D.MenuCard { id: card; host: host; menuNode: audit.node; depth: 0
            availableHeight: window.height-24; maximumWidth: window.width-24; x: 12; y: 12 }
    }
    function check(ok,message) { if (!ok) throw Error(message+" "+JSON.stringify(cases[index])); }
    function walk(item) { let all=[item]; for (const child of item.children || []) all=all.concat(walk(child)); return all; }
    function rect(item) { const p=item.mapToItem(card,0,0); return {x:p.x,y:p.y,width:item.width,height:item.height}; }
    Timer { interval: 35; repeat: true; running: true
        onTriggered: { try {
            if (painting) return;
            if (index===cases.length) { console.log("MENU_TEXT_RESULT "+JSON.stringify(records)); Qt.quit(); return; }
            const c=cases[index];
            if (stage===0) { D.Theme.colors=palettes[c.mode]; D.Theme.preferences={textScale:c.scale,reduceMotion:true}; window.width=c.width; stage=1; return; }
            const texts=walk(card).filter(item=>item.text && item.fontInfo && ["Save Document As…","⌃⇧S"].includes(item.text));
            check(texts.length===2,"Production label and accelerator not found");
            const label=texts.find(item=>item.text==="Save Document As…"), shortcut=texts.find(item=>item.text==="⌃⇧S");
            const receipt={case:c,label:{rect:rect(label),size:label.font.pixelSize,color:label.color.toString(),naturalWidth:label.implicitWidth,family:label.fontInfo.family,text:label.text},shortcut:{rect:rect(shortcut),size:shortcut.font.pixelSize,color:shortcut.color.toString(),naturalWidth:shortcut.implicitWidth,family:shortcut.fontInfo.family,text:shortcut.text},card:{width:card.width,height:card.height}};
            check(label.fontInfo.family===palettes.font && shortcut.fontInfo.family===palettes.font,"Public font family");
            check(label.font.pixelSize===Math.round(13*c.scale) && shortcut.font.pixelSize===Math.round(12*c.scale),"Text preference size");
            check(label.height<=label.parent.height && shortcut.height<=shortcut.parent.height,"Text escapes row");
            check(label.x+label.width<=shortcut.x && label.width>0,"Label and accelerator overlap");
            check(!label.truncated && !shortcut.truncated,"Bounded menu caption truncated");
            painting=true;
            const capture=@OUTPUT@+"/menu-"+index+".png";
            check(card.grabToImage(result=>{ try {
                check(result.saveToFile(capture),"Saved menu paint"); receipt.capture=capture;
                records=records.concat([receipt]); painting=false; stage=0; index++;
            } catch(error) { console.error("MENU_TEXT_FAILURE "+error); Qt.quit(); } },Qt.size(card.width,card.height)),"Grab accepted");
        } catch(error) { console.error("MENU_TEXT_FAILURE "+error); Qt.quit(); } }
    }
}
