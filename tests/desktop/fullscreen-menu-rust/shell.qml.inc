
 IpcHandler {target:"fullscreenMenuAudit"
  function state():string {
  function exitStatus(bar) {
   function walk(item){let found=[item];for(const child of item.children || [])found=found.concat(walk(child));return found;}
   const button=walk(bar.contentItem).find(item=>item.objectName==="fullscreen-exit");
   if(!button)return null;
   const point=button.mapToItem(bar.contentItem,button.width/2,button.height/2);
   return {visible:button.visible,x:bar.screen.x+point.x,y:bar.screen.y+point.y};
  }
  return JSON.stringify({owner:root.popupOwner,available:menuVisibilityState.available,outputs:menuVisibilityState.outputs,error:menuVisibilityState.error,bars:Object.keys(root.bars).map(name=>{const bar=root.bars[name];return {exit:exitStatus(bar),pointer:bar.pointerAudit(),output:name,x:bar.screen.x,y:bar.screen.y,width:bar.width,visible:bar.visible,fullscreen:bar.outputFullscreen,automatic:bar.automaticFullscreen,autoHide:bar.automaticHide,revealed:bar.edgeRevealed,menu:bar.menu,zone:bar.exclusiveZone,layer:bar.WlrLayershell.layer};})});}
  function keyboardMenu():void {root.requestMenu("Desktop","HEADLESS-1");}
  function close():void {root.claimPopup("");}
 }
