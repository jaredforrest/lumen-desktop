
 property var auditWindows:[]
 IpcHandler {target:"popoverAudit"
  function open(name:string):void {root.claimPopup("");sessionOptions.visible=false;if(name==="session"){sessionOptions.executeActions=false;sessionOptions.show("reboot");sessionOptions.error="A deliberately long translated diagnostic describes why the application observation is unavailable. Save your documents and cancel to keep this session open.";}else root.showMenu(name);}
  function longLabels():void {for(const window of root.auditWindows){function scan(item){if(item.label!==undefined && item.label==="Displays")item.label="Display configuration and arrangement preferences";for(const child of item.children || [])scan(child);}scan(window.contentItem);}}
  function ready():bool{return true;}
  function state():string {
   let focused=null,texts=[];
   for(const window of root.auditWindows.concat([sessionOptions])){if(!window.visible || !window.contentItem)continue;
    function scan(item){if(!item.visible)return;
     if(item.text!==undefined && item.font!==undefined && item.width>0 && item.height>0)texts.push({text:String(item.text),size:item.font.pixelSize,width:item.width,height:item.height,truncated:!!item.truncated});
     if(item.activeFocus && item.activeFocusOnTab){const pos=item.mapToItem(window.contentItem,0,0);let reachable=pos.x>=0 && pos.y>=0 && pos.x+item.width<=window.width+1 && pos.y+item.height<=window.height+1;let ancestor=item.parent;while(ancestor){if(ancestor.clip){const at=item.mapToItem(ancestor,0,0);reachable=reachable && at.y>=-1 && at.y+item.height<=ancestor.height+1;}ancestor=ancestor.parent;}focused={text:item.text || item.accessibleLabel || item.objectName || String(item),x:pos.x,y:pos.y,width:item.width,height:item.height,reachable:reachable};}
     for(const child of item.children || [])scan(child);
    }scan(window.contentItem);
   }return JSON.stringify({focused:focused,texts:texts});
  }
 }
