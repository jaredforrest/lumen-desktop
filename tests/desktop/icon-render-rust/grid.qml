import QtQuick
import Quickshell
import Quickshell.Io
import "assets/shell" as D
ShellRoot {id:root
 property bool symbolMode:false
 property var symbols:__SYMBOLS__
 property var icons:__ICONS__
 FloatingWindow {id:window;title:"Icon catalogue";visible:false;implicitWidth:1260;implicitHeight:840;color:D.Theme.colors.background
  Grid {id:appGrid;visible:!root.symbolMode;anchors.fill:parent;columns:8;spacing:0
   Repeater {model:root.icons
    Item {id:cell;required property string modelData;required property int index;width:157;height:168
     Text {x:6;y:4;width:145;elide:Text.ElideMiddle;text:cell.modelData.split('/').slice(-2).join('/');font.pixelSize:11;color:D.Theme.colors.text}
     Row {x:8;y:40;spacing:12
      Repeater {model:[16,24,64]
       D.AppIcon {required property int modelData;property color fixtureBackground:cell.modelData.indexOf("/icons/")>=0?"#777777":D.Theme.colors.background;width:modelData;height:modelData;iconSource:cell.modelData;calendarDate:new Date(2026,9,20);Rectangle {anchors.fill:parent;color:parent.fixtureBackground;z:-1}}
      }
     }
    }
   }
  }
  Grid {id:symbolGrid;visible:root.symbolMode;anchors.fill:parent;columns:8
   Repeater {model:root.symbols
    Item {id:symbolCell;required property string modelData;width:157;height:140
     Text {x:6;y:4;text:symbolCell.modelData;font.pixelSize:11;color:D.Theme.colors.text}
     Row {x:8;y:35;spacing:12
      Repeater {model:[16,24,64]
       D.SystemGlyph {required property int modelData;property bool fixtureSymbol:true;width:modelData;height:modelData;name:symbolCell.modelData;ink:D.Theme.colors.text}
      }
     }
    }
   }
  }
 }
 function visit(item,fn){if(fn(item))return item;for(const child of item.children || []){const result=visit(child,fn);if(result)return result;}return null;}
 IpcHandler {target:"icons"
  function showGrid(value:bool):void{root.symbolMode=false;window.visible=value;}
  function showSymbols():void{root.symbolMode=true;window.visible=true;}
  function state():string {const values=[];function collect(item){if(item.iconSource!==undefined || item.fixtureSymbol===true){const at=item.mapToItem(window.contentItem,0,0);values.push({kind:item.fixtureSymbol===true?"symbol":"asset",ink:item.fixtureSymbol===true?String(item.ink):"",background:item.fixtureSymbol===true?String(D.Theme.colors.background):String(item.fixtureBackground),source:item.fixtureSymbol===true?item.name:String(item.iconSource),desired:item.fixtureSymbol===true?item.name:String(item.desiredSource),ready:item.fixtureSymbol===true?item.available:item.status===Image.Ready,failed:item.fixtureSymbol===true?false:item.failed,x:at.x,y:at.y,width:item.width,height:item.height,rasterWidth:item.fixtureSymbol===true?0:item.sourceSize.width,rasterHeight:item.fixtureSymbol===true?0:item.sourceSize.height});}for(const child of item.children || [])collect(child);}collect(root.symbolMode?symbolGrid:appGrid);return JSON.stringify({visible:window.visible,dark:D.Theme.dark,style:D.Theme.iconAppearance,background:String(D.Theme.colors.background),items:values});}
 }
}