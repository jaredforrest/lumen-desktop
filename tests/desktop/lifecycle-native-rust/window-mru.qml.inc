 IpcHandler {target:"windowMruAudit"
  function status():string {return JSON.stringify({available:applicationLifecycle.available,sessionEpoch:applicationLifecycle.sessionEpoch,nativeWindows:applicationLifecycle.nativeWindows,apps:root.runningApplications,windows:root.windows.map(w=>({appId:w.appId,title:w.title})),active:root.active?{appId:root.active.appId,title:root.active.title}:null,switcher:JSON.parse(applicationSwitcher.status())});}
  function restartObserver():void {applicationLifecycle.observer.running=false;}
 }
