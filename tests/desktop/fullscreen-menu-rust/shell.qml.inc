
 IpcHandler {target:"fullscreenMenuAudit"
  function state():string {return JSON.stringify({owner:root.popupOwner,available:menuVisibilityState.available,outputs:menuVisibilityState.outputs,error:menuVisibilityState.error,bars:Object.keys(root.bars).map(name=>{const bar=root.bars[name];return {pointer:bar.pointerAudit(),output:name,x:bar.screen.x,y:bar.screen.y,width:bar.width,visible:bar.visible,fullscreen:bar.outputFullscreen,automatic:bar.automaticFullscreen,autoHide:bar.automaticHide,revealed:bar.edgeRevealed,menu:bar.menu,zone:bar.exclusiveZone,layer:bar.WlrLayershell.layer};})});}
  function keyboardMenu():void {root.requestMenu("Desktop","HEADLESS-1");}
  function close():void {root.claimPopup("");}
 }
