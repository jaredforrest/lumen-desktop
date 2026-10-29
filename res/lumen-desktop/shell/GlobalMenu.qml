import QtQuick
import Quickshell
import Quickshell.Io
import Quickshell.Wayland

Item {
    id: root
    required property var shell
    property var output: null
    Connections {target:root.shell; function onDismissPopups(owner) {if(owner!=="exported")root.path=[];}function onWindowsChanged(){if(!root.invokingAlive())root.path=[];} }
    property string menuFont: Theme.font
    property var state: ({token:"", title:"", supported:false, menu:null, windowId:0, appName:"Desktop"})
    property bool holdDisplay: false
    property var latestState: state
    function acceptState(next) {
        latestState=next;
        if (holdDisplay || resume.running) return;
        if(next.token!==state.token || next.windowId!==state.windowId) path=[];
        if(next!==state){selection=null;highlighted={};hoverBlock=null;}
        state=next;
        for(let depth=0;depth<path.length;depth++){const n=node(path[depth],state.menu);if(!n || !n.visible || !n.enabled || !n.submenu){path=path.slice(0,depth);break;}}
    }
    onHoldDisplayChanged: {if(holdDisplay){release.stop();if(state.supported && state.token)command("hold",0);}else{resume.restart();if(!path.length)release.restart();}}
    Timer {id:resume;interval:120;onTriggered:root.acceptState(root.latestState)}
    onPathChanged: if(!path.length) {release.restart();highlighted={};selection=null;hoverBlock=null;if(shell.popupOwner==="exported")shell.popupOwner="";}
    property var path: []
    property var invokingContext:null
    property var selection:null
    property real menuX: 120
    function label(s) { return (s || "").replace(/__/g,"\u0000").replace(/_/g,"").replace(/\u0000/g,"_"); }
    function command(action,id) { bridge.write(JSON.stringify({action:action,token:state.token,id:id})+"\n"); }
    function node(id,n) {
        if (!n) return null;
        if (n.id === id) return n;
        for (const c of n.children) { const result=node(id,c); if(result) return result; }
        return null;
    }
    function openMenu(id,x,screen,context) {
        if(context)invokingContext=context;else if(!path.length)invokingContext={window:shell.active,hasWindow:!!shell.active,files:shell.filesWindow || null,epoch:shell.focusEpoch || 0,appName:shell.activeName,exportState:state};
        const closing=path[0]===id && output===screen;
        shell.claimPopup("exported");output=screen || Quickshell.screens.find(s=>s.name===shell.preferredOutput()) || Quickshell.screens[0];
        command("hold",id);menuX=x;highlighted={};selection=null;hoverBlock=null;pathY=[];path=closing?[]:[id];if(path.length)command("open",id);
    }
    property var highlighted: ({})
    function capture(depth,id){const n=node(id,state.menu),parent=node(path[depth],state.menu);return path.length>depth && parent && parent.children.includes(n) && n && n.enabled && n.visible && !n.separator?{token:state.token,revision:state.revision,state:state,node:n,depth:depth,id:id,path:path.slice(0,depth+1)}:null;}
    function invokingAlive(){return !invokingContext || !invokingContext.hasWindow || (invokingContext.window && shell.windows.includes(invokingContext.window));}
    function valid(intent){return invokingAlive() && intent && intent.state===state && intent.token===state.token && intent.revision===state.revision && node(intent.id,state.menu)===intent.node && intent.node.enabled && intent.node.visible && !intent.node.separator && intent.path.every((id,i)=>path[i]===id);}
    function hover(depth,id){selection=null;const copy=Object.assign({},highlighted);copy[depth]=id;highlighted=copy;}
    function select(depth,id) {selection=capture(depth,id);const copy=Object.assign({},highlighted);copy[depth]=id;highlighted=copy;}
    function activateIntent(intent){if(!valid(intent) || intent.node.submenu)return;path=[];bridge.write(JSON.stringify({action:"activate_when_focused",token:intent.token,id:intent.id,revision:intent.revision})+"\n");}
    function step(delta) {
        const depth=path.length-1,c=cards.itemAt(depth);if(!c)return;
        const rows=c.rows.filter(n=>n.enabled && !n.separator);if(!rows.length)return;
        const i=rows.findIndex(n=>n.id===highlighted[depth]);
        const next=rows[(i<0?(delta>0?0:rows.length-1):(i+delta+rows.length)%rows.length)];
        select(depth,next.id);c.reveal(next.id);c.focusItem(next.id);
    }
    Timer {id:release;interval:150;onTriggered:if(!root.path.length && !root.holdDisplay)root.command("release",0)}
    function activateCaptured(token,id){bridge.write(JSON.stringify({action:"activate_when_focused",token:token,id:id})+"\n");}
    function activateItem(id) {activateIntent(capture(path.length-1,id));}
    function enterItem() {
        const intent=selection,depth=path.length-1,c=cards.itemAt(depth);
        if(!c || !valid(intent) || intent.depth!==depth)return;
        if(intent.node.submenu){openChild(depth,intent.id,c.rowY(intent.id),intent);Qt.callLater(()=>{if(path[depth+1]===intent.id)step(1);});}else activateIntent(intent);
    }
    function back() {if(path.length>1){blockClosedHover(path.length-2,path[path.length-1]);path=path.slice(0,-1);const depth=path.length-1;select(depth,highlighted[depth]);const c=cards.itemAt(depth);if(c)c.focusItem(highlighted[depth]);}else path=[];}
    function switchTop(delta) {const bar=output?shell.bars[output.name]:null;if(bar)bar.switchHeading(delta,path[0]);}
    property var hoverBlock:null
    property var hoverPosition:({known:false,x:0,y:0})
    function trackHoverPointer(x,y) {
        const block=hoverBlock;
        if(block && (Math.abs(x-block.x)>0.01 || Math.abs(y-block.y)>0.01))hoverBlock=null;
        hoverPosition={known:true,x:x,y:y};
    }
    function blockClosedHover(depth,id) {
        const card=cards.itemAt(depth),intent=capture(depth,id),point=hoverPosition;
        hoverBlock=card && card.pointerOver(id) && intent && point.known?{intent:intent,x:point.x,y:point.y}:null;
    }
    function hoverBlocked(depth,id) {
        const block=hoverBlock;
        return !!block && block.intent.depth===depth && block.intent.id===id && valid(block.intent);
    }
    property var pointer: ({x:0,y:0,time:0})
    function hoverDelay(depth,x,y) {
        const child=cards.itemAt(depth+1),p=pointer;
        if(!child || Date.now()-p.time>350)return 160;
        const edge=child.x>x?child.x:child.x+child.width;
        const span=edge-p.x;
        if(!span || (x-p.x)/span<0 || (x-p.x)/span>1)return 160;
        const t=(x-p.x)/span;
        const lo=p.y+t*(child.y-8-p.y),hi=p.y+t*(child.y+child.height+8-p.y);
        return y>=Math.min(lo,hi) && y<=Math.max(lo,hi)?450:160;
    }
    function rememberPointer(x,y) {pointer={x:x,y:y,time:Date.now()};}
    FontMetrics { id:menuMetrics; font.family:root.menuFont; font.pixelSize:Theme.textRole("body") }
    property var pathY: []
    property var selected: path.length ? node(path[path.length-1],state.menu) : null
    function shortcut(value) {
        return (value || "").split(" / ").map(v=>v.split("+").map(k=>({Control:"⌃",Ctrl:"⌃",Shift:"⇧",Alt:"⌥",Super:"⌘",Meta:"⌘"}[k] || k)).join("")).join(" / ");
    }
    function symbolFor(value) {
        const text=label(value).toLowerCase();
        if(text.startsWith("new"))return "document";
        if(text.startsWith("open"))return "folder";
        if(text.startsWith("save") || text.startsWith("export"))return "save";
        if(text.startsWith("copy") || text.startsWith("duplicate"))return "copy";
        if(text.startsWith("undo"))return "undo";
        if(text.startsWith("redo"))return "redo";
        if(text.startsWith("close"))return "close";
        if(text.includes("preferences") || text.includes("settings"))return "gear";
        if(text.includes("full screen"))return "screen";
        if(text.startsWith("find") || text.startsWith("search"))return "search";
        return "";
    }
    function widthFor(n) {
        return n ? Math.max(220,Math.min(Math.round(410*Theme.textScale),n.children.filter(c=>c.visible).reduce((w,c)=>Math.max(w,menuMetrics.advanceWidth(label(c.label))+menuMetrics.advanceWidth(shortcut(c.shortcut))+76),0))) : 220;
    }
    function closeBelow(depth) { if(path.length>depth+1)path=path.slice(0,depth+1); }
    function openChild(depth,id,y,intent) {
        if(!valid(intent || capture(depth,id)))return;
        if(path[depth+1]===id)return;
        const ys=pathY.slice(0,depth+1);ys[depth+1]=y;pathY=ys;
        path=path.slice(0,depth+1).concat([id]);command("open",id);
    }
    Process {
        id: bridge
        command: [Quickshell.env("GLOBAL_MENU_PYTHON"), Quickshell.env("GLOBAL_MENU_BRIDGE")]
        running: Quickshell.env("SWAYSOCK") !== ""
        stdinEnabled:true
        stdout: SplitParser {
            onRead: data => {
                try {
                    const next=JSON.parse(data);
                    root.acceptState(next);
                } catch(e) { console.warn("Invalid bridge message",e); }
            }
        }
    }
    IpcHandler {
        target:"globalmenu"
        function status():string {return JSON.stringify(root.state);}
        function open(id:int):void {root.openMenu(id,120,Quickshell.screens.find(s=>s.name===root.shell.preferredOutput()) || Quickshell.screens[0]);}
        function openOn(id:int,x:int,outputName:string):void {root.openMenu(id,x,Quickshell.screens.find(s=>s.name===outputName));}
        function expand(depth:int,id:int,y:int):void {root.openChild(depth,id,y);}
        function layout():string {let result=[];for(let i=0;i<cards.count;i++){const c=cards.itemAt(i);if(c)result.push({id:root.path[i],x:c.x,y:c.y,width:c.width,height:c.height,rows:c.debugRows()});}return JSON.stringify(result);}
        function activate(id:int):void {root.activateItem(id);}
    }
    PanelWindow {
        id:popup
        screen:root.output
        visible:root.path.length>0 && root.selected!==null
        anchors {top:true;bottom:true;left:true;right:true}
        exclusionMode:ExclusionMode.Ignore
        mask:Region {y:Theme.barHeight;width:popup.width;height:popup.height-Theme.barHeight}
        color:"transparent"
        WlrLayershell.namespace:"global-menu-prototype-popup"
        WlrLayershell.layer:WlrLayer.Overlay
        // Release the layer before dispatching an app action; bridge still validates its focus token.
        WlrLayershell.keyboardFocus:WlrKeyboardFocus.Exclusive
        FocusScope {
            anchors.fill:parent;focus:true
            HoverHandler {blocking:false;onPointChanged:if(hovered)root.trackHoverPointer(point.position.x,point.position.y);onHoveredChanged:if(!hovered)root.hoverBlock=null}
            Keys.onEscapePressed:root.back()
            Keys.onDownPressed:root.step(1)
            Keys.onUpPressed:root.step(-1)
            Keys.onReturnPressed:root.enterItem()
            Keys.onEnterPressed:root.enterItem()
            Keys.onRightPressed:{const n=root.node(root.highlighted[root.path.length-1],root.state.menu);if(n && n.submenu)root.enterItem();else if(root.path.length===1)root.switchTop(1);}
            Keys.onLeftPressed:{if(root.path.length>1)root.back();else root.switchTop(-1);}
        MouseArea {anchors.fill:parent;onClicked:root.path=[]}
        Repeater {
            id:cards;model:root.path
            MenuCard {
                required property int modelData
                required property int index
                host:root;menuNode:root.node(modelData,root.state.menu);depth:index
                availableHeight:Math.max(40,popup.height-Theme.barHeight-100)
                maximumWidth:popup.width-16
                x:{
                    if(index===0)return Math.max(8,Math.min(root.menuX,popup.width-width-8));
                    const previous=cards.itemAt(index-1);
                    if(!previous)return 8;
                    return previous.x+previous.width+width+4<=popup.width-8?previous.x+previous.width+4:Math.max(8,previous.x-width-4);
                }
                y:Math.max(Theme.barHeight+1,Math.min(index===0?Theme.barHeight+1:(root.pathY[index] || Theme.barHeight+1),popup.height-height-92))
            }
        }
        }
    }
}
