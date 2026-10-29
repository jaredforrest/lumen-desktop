
 IpcHandler {target:"recoveryProbe"
  function status():string {return JSON.stringify({popupOwner:root.popupOwner,screens:Quickshell.screens.map(s=>s.name),bars:Object.keys(root.bars),windows:root.windows.map(w=>({app:w.appId,title:w.title})),dockItems:root.referenceDock.length});}
 }
