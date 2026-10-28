
 IpcHandler {target:"lifecycleAudit"
  function status():string {const e=DesktopEntries.byId("org.example.Lifecycle");return JSON.stringify({command:applicationLifecycle.observer.command,observerRunning:applicationLifecycle.observer.running,recoveryAvailable:applicationLifecycle.recoveryAvailable,available:applicationLifecycle.available,observerError:applicationLifecycle.error,apps:root.runningApplications,entry:!!e,running:e?root.runningFor(e):false,error:root.applicationCommandError,pending:applicationLifecycle.pending,extras:root.runningExtras.map(e=>e.id)});}
  function restartObserver():void{applicationLifecycle.observer.running=false;}
  function quit():void{root.quitApplication(DesktopEntries.byId("org.example.Lifecycle"));}
 }
