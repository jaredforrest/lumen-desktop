 property var projectionResult:null
 Connections {target:applicationLifecycle;function onNativeFocusResult(result){root.projectionResult=result;}}
 IpcHandler {target:"projectionAudit"
  function status():string {return JSON.stringify({available:applicationLifecycle.available,observation:applicationLifecycle.nativeObservation,nativeWindows:applicationLifecycle.nativeWindows,result:root.projectionResult,switcher:JSON.parse(applicationSwitcher.status()),windows:Array.from(root.windows).map(w=>({appId:w.appId,foreignId:w.foreignToplevelIdentifier})),active:root.active?{appId:root.active.appId,foreignId:root.active.foreignToplevelIdentifier}:null});}
  function switchTo(appId:string):string {
   applicationSwitcher.command("begin");
   let value=JSON.parse(applicationSwitcher.status());
   const count=value.items?value.items.length:0;
   for(let i=0;i<count&&value.key!==appId;i++){applicationSwitcher.command("next");value=JSON.parse(applicationSwitcher.status());}
   if(value.key!==appId){applicationSwitcher.command("cancel");return JSON.stringify({missing:true,before:value});}
   const before=value;
   applicationSwitcher.command("commit");
   const after=JSON.parse(applicationSwitcher.status());
   return JSON.stringify({before:before,after:after});
  }
 }
