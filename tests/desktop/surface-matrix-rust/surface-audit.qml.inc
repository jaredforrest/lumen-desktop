
    FileInfoWindow {id:auditInfo}
    PanelWindow {id:auditFocus;visible:false;anchors {top:true;right:true}margins {top:38;right:12}implicitWidth:300;implicitHeight:auditFocusPanel.height;color:"transparent";exclusionMode:ExclusionMode.Ignore
        SystemPanels {id:auditFocusPanel;width:300;kind:"Focus";shell:root;today:new Date();editingFocus:"work"}
    }
    IpcHandler {target:"surfaceAudit"
        function open(name:string):void {
            root.claimPopup("");auditFocus.visible=false;root.hudVisible=false;notificationState.rows=[];applicationSwitcher.visible=false;windowLayoutMenu.visible=false;
            for(const window of [helpWindow,sessionOptions,remoteDesktopWindow,overview,textTools,quickLook,auditInfo,forceQuitDialog,downloadsStack])window.visible=false;
            if(name==="help")root.openHelp();
            else if(name==="session"){sessionOptions.executeActions=false;root.sessionAction("suspend");}
            else if(name==="remote")root.openRemoteDesktop();
            else if(name==="overview")root.openOverview(false);
            else if(name==="text")root.openTextTools();
            else if(name==="quick")quickLook.show(__EXAMPLE__,null);
            else if(name==="info")auditInfo.show({name:"Example document.txt",path:"/fixture/Example document.txt",size:"123 bytes",type:"Text document",modified:"Today",permissions:"Read and write"});
            else if(name==="focus-editor"){notificationState.focusState={profiles:[{id:"work",name:"Work",allowedApps:[],allowCritical:false,schedules:[{days:[0,1,2,3,4],start:540,end:1020}]}],active:"",name:"",until:0,scheduled:false};auditFocus.visible=true;}
            else if(name==="forcequit")forceQuitDialog.show("Fixture Application",[]);
            else if(name==="folder")downloadsStack.show(__DOCS__,root.preferredOutput(),{presentation:"list"});
            else if(name==="switcher"){applicationSwitcher.items=[{key:"fixture",window:null,label:"Fixture Application",icon:Qt.resolvedUrl("icons/application.svg")}];applicationSwitcher.selected=0;applicationSwitcher.visible=true;}
            else if(name==="layout"){windowLayoutMenu.event(JSON.stringify({lumen_layout:1,id:42,x:300,y:100}));windowLayoutMenu.open();}
            else if(name==="hud"){root.hudOutput=root.preferredOutput();root.hudVisible=true;}
            else if(name==="banner"){notificationState.available=true;notificationState.bannerOutput=root.preferredOutput();notificationState.rows=[{id:1,app:"Fixture",desktop:"",summary:"Saved successfully",body:"A visible notification body.",icon:"",image:"",timestamp:Date.now(),actions:[],banner:true,unread:true,urgency:1,transient:false}];}
            else root.showMenu(name);
        }
        function geometry(name:string):string {
            const window=({forcequit:forceQuitDialog,folder:downloadsStack,switcher:applicationSwitcher,layout:windowLayoutMenu,"focus-editor":auditFocus})[name];
            if(!window || !window.visible)return "null";
            if(name==="folder" && downloadsStack.rows.length===0)return "null";
            if(name==="focus-editor")return JSON.stringify({x:0,y:0,width:300,height:auditFocusPanel.height});
            let result=null;
            function visit(item){if(!item.visible)return;if(item.color!==undefined && item.color.a>0.1 && item.width>100 && item.height>70 && !(item.width>=window.width-2 && item.height>=window.height-2)){const at=item.mapToItem(window.contentItem,0,0);if(!result || item.width*item.height>result.width*result.height)result={x:at.x,y:at.y,width:item.width,height:item.height};}for(const child of item.children || [])visit(child);}
            visit(window.contentItem);return JSON.stringify(result);
        }
        function ready():bool{return true;}
    }
