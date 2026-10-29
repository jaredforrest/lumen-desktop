
 IpcHandler {target:"menuAudit"
  function open(name:string):void{const bar=root.bars["HEADLESS-1"];bar.selectedWindow=root.active;bar.toggle(name,16);}
  function clear():void{}
  function status():string {const b=root.bars["HEADLESS-1"];return JSON.stringify({ready:!!b,text:b?b.fixtureMenuText():null,menu:b?b.menu:"",layout:b?b.popupLayout():null,active:root.active?root.active.appId:"",bridge:root.exportedMenu.state,latestBridge:root.exportedMenu.latestState,headingLayout:b?b.menuLayoutStatus():null,hiddenAnchor:b?b.exportX(37):0,exportPath:root.exportedMenu.path,exportTarget:root.exportedMenu.invokingContext && root.exportedMenu.invokingContext.window?root.exportedMenu.invokingContext.window.title:"",highlighted:root.exportedMenu.highlighted,hoverBlock:root.exportedMenu.hoverBlock?{id:root.exportedMenu.hoverBlock.intent.id,depth:root.exportedMenu.hoverBlock.intent.depth}:null,selection:root.exportedMenu.selection?{id:root.exportedMenu.selection.id,revision:root.exportedMenu.selection.revision}:null,target:b && b.menuWindow?b.menuWindow.title:"",filesContext:b?b.filesFocused:false,windows:root.windows.map(w=>({app:w.appId,title:w.title}))});}
 }
