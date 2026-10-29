import QtQuick
import Quickshell
import Quickshell.Wayland
import Quickshell.Io
PanelWindow {
 anchors {top:true;bottom:true;left:true;right:true}
 WlrLayershell.keyboardFocus: WlrKeyboardFocus.Exclusive
 AuthView { id:view; anchors.fill:parent }
 IpcHandler { target:"preview"
  function power(): void { view.openPowerMenu(); }
  function secretRetention(): bool {
   function walk(item){let result=[item];for(const child of item.children || [])result=result.concat(walk(child));return result;}
   const field=walk(view).find(n=>n.objectName==="password");
   field.text="synthetic";view.busy=true;let cleared=field.text.length===0;view.busy=false;
   field.text="synthetic";view.echoResponse=true;cleared=cleared && field.text.length===0;
   field.text="synthetic";view.echoResponse=false;cleared=cleared && field.text.length===0;
   field.text="synthetic";view.visible=false;cleared=cleared && field.text.length===0;view.visible=true;
   return cleared;
  }
  function passwordReveal(): bool {
   function walk(item){let result=[item];for(const child of item.children || [])result=result.concat(walk(child));return result;}
   const field=walk(view).find(n=>n.objectName==="password");
   const toggle=walk(view).find(n=>n.objectName==="passwordVisibility");
   field.text="synthetic";toggle.checked=true;
   const shown=field.echoMode===TextInput.Normal && field.text==="synthetic";
   toggle.checked=false;const hidden=field.echoMode===TextInput.Password && field.text==="synthetic";
   toggle.checked=true;view.submit();
   return shown && hidden && field.text.length===0 && !toggle.checked;
  }
  function bounds(): string {
   function walk(item){let result=[item];for(const child of item.children || [])result=result.concat(walk(child));return result;}
   const field=walk(view).find(n=>n.objectName==="password");const position=field.mapToItem(view,0,0);
   return JSON.stringify({width:view.width,height:view.height,x:position.x,y:position.y,fieldWidth:field.width,fieldHeight:field.height});
  }
 }
}
