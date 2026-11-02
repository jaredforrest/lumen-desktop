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
  function presentationSemantics(): bool {
   function walk(item){let result=[item];for(const child of item.children || [])result=result.concat(walk(child));return result;}
   const nodes=walk(view);
   const field=nodes.find(n=>n.objectName==="password");
   const label=nodes.find(n=>n.objectName==="displayName");
   const error=nodes.find(n=>n.objectName==="authenticationError");
   const clock=nodes.find(n=>n.objectName==="clock");
   const previous=view.displayName;
   view.displayName="<b>Synthetic user</b>";
   const literal=label.text===view.displayName && label.textFormat===Text.PlainText && error.textFormat===Text.PlainText;
   view.displayName=previous;
   const named=field.Accessible.name==="Password";
   field.text="synthetic";view.passwordVisible=true;view.clearResponse();
   return literal && named && clock.width<=view.width && field.text.length===0 && !view.passwordVisible;
  }
  function bounds(): string {
   function walk(item){let result=[item];for(const child of item.children || [])result=result.concat(walk(child));return result;}
   const field=walk(view).find(n=>n.objectName==="password");const position=field.mapToItem(view,0,0);
   return JSON.stringify({width:view.width,height:view.height,x:position.x,y:position.y,fieldWidth:field.width,fieldHeight:field.height});
  }
 }
}
