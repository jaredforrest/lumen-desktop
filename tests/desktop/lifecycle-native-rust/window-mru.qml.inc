 IpcHandler {target:"windowMruAudit"
  function observation():string {return JSON.stringify(applicationLifecycle.nativeObservation);}
  function status():string {return JSON.stringify({available:applicationLifecycle.available,windowsIsArray:Array.isArray(root.windows),nativePairsRaw:SwitcherModel.nativeGroupPairs(root.windows,root.nativeWindowCaptures,w=>w.appId,"org.example.MruA")!==null,nativePairsArray:SwitcherModel.nativeGroupPairs(Array.from(root.windows),root.nativeWindowCaptures,w=>w.appId,"org.example.MruA")!==null,nativeCoverageComplete:applicationLifecycle.nativeCoverageComplete,nativeGuardAvailable:applicationLifecycle.nativeGuardAvailable,sessionEpoch:applicationLifecycle.sessionEpoch,nativeWindows:applicationLifecycle.nativeWindows,nativeFocused:applicationLifecycle.nativeFocused,nativeSerial:applicationLifecycle.nativeSerial,apps:root.runningApplications,windows:root.windows.map(w=>({appId:w.appId,title:w.title,foreignToplevelIdentifier:w.foreignToplevelIdentifier})),active:root.active?{appId:root.active.appId,title:root.active.title}:null,switcher:JSON.parse(applicationSwitcher.status())});}
  function overlappingCommitBegin():string {
   const before=JSON.parse(applicationSwitcher.status()),sourceBefore=JSON.parse(JSON.stringify(applicationSwitcher.previewNativeFocus));
   applicationSwitcher.command("commit");
   const committed=JSON.parse(applicationSwitcher.status());
   applicationSwitcher.command("begin");
   return JSON.stringify({before:before,committed:committed,after:JSON.parse(applicationSwitcher.status()),sourceBefore:sourceBefore,sourceAfter:JSON.parse(JSON.stringify(applicationSwitcher.previewNativeFocus))});
  }
  function restartObserver():void {applicationLifecycle.observer.running=false;}
 }
