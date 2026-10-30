 IpcHandler {target:"mruAudit"
  function status():string {return JSON.stringify({history:root.applicationActivationHistory,observerHistory:applicationLifecycle.activationHistory,available:applicationLifecycle.available,recoveryAvailable:applicationLifecycle.recoveryAvailable,sessionEpoch:applicationLifecycle.sessionEpoch,apps:root.runningApplications,windows:root.windows.map(window=>window.appId),active:root.active?root.active.appId:"",focusEpoch:root.focusEpoch,switcher:JSON.parse(applicationSwitcher.status())});}
  function restartObserver():void {applicationLifecycle.observer.running=false;}
 }
