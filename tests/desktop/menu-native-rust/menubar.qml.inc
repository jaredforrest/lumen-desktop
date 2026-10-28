
 property var menuFixtureFocus:[]
 property var menuFixtureSessions:[]
 property var menuFixtureFileActions:[]
 property var menuFixtureFirstFiles:null
 property var menuFixtureSecondFiles:null
 IpcHandler {target:"menuAudit"
  function open(name:string):void{const bar=root.bars["HEADLESS-1"];bar.selectedWindow=root.active;bar.toggle(name,16);}
  function retarget(app:string):void{root.bars["HEADLESS-1"].selectedWindow=root.windows.find(w=>w.appId===app);}
  function status():string {const b=root.bars["HEADLESS-1"];return JSON.stringify({ready:!!b,menu:b?b.menu:"",layout:b?b.popupLayout():null,focuses:root.menuFixtureFocus,sessions:root.menuFixtureSessions,active:root.active?root.active.appId:"",bridge:root.exportedMenu.state,filesContext:b?b.filesFocused:false,fileActions:root.menuFixtureFileActions,windows:root.windows.map(w=>({app:w.appId,title:w.title}))});}
  function clear():void{root.menuFixtureFocus=[];root.menuFixtureSessions=[];root.menuFixtureFileActions=[];}
  function filesStart():void{root.openFiles(Quickshell.env("HOME"));root.menuFixtureFirstFiles=root.filesWindow;}
  function filesSecond():void{root.newFilesWindow(Quickshell.env("HOME"));root.menuFixtureSecondFiles=root.filesWindow;}
  function filesRetarget():void{root.filesWindow=root.menuFixtureFirstFiles;}
  function filesFocusFirst():void{root.filesWindow=root.menuFixtureFirstFiles;root.filesWindow.openPath(Quickshell.env("HOME"));}
  function settingsStart():void{settingsWindow.visible=true;}
  function replaceAssociationEpoch():void{root.ownedToplevelAssociations=root.ownedToplevelAssociations.map(record=>Object.assign({},record));}
  function filesStatus():string{return JSON.stringify({settingsVisible:settingsWindow.visible,firstVisible:root.menuFixtureFirstFiles?root.menuFixtureFirstFiles.visible:false,secondVisible:root.menuFixtureSecondFiles?root.menuFixtureSecondFiles.visible:false,actions:root.menuFixtureFileActions,epoch:root.focusEpoch,menuEpoch:root.bars["HEADLESS-1"].menuFocusEpoch,same:root.bars["HEADLESS-1"].menuFilesWindow===root.filesWindow,visible:root.filesWindow.visible,backendReady:root.filesState.ready,busy:root.filesState.busy,exported:root.exportedMenu.state,expectedTitle:root.filesWindow.title,key:root.ownedContext.keyRole,ready:root.filesWindow.visible && root.filesState.ready && !root.filesState.busy && root.exportedMenu.state.title===root.filesWindow.title && root.exportedMenu.state.appName==="org.quickshell",first:root.menuFixtureFirstFiles?root.menuFixtureFirstFiles.listMode:false,second:root.menuFixtureSecondFiles?root.menuFixtureSecondFiles.listMode:false,focused:root.bars["HEADLESS-1"].filesFocused});}
 }
