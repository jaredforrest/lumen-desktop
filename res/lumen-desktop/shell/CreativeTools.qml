import QtQuick
import QtQuick.Controls
import Quickshell
import Quickshell.Io
Column {
    id:root
    property string page:"Fonts"
    property string fontQuery:""
    property bool favouritesOnly:false
    property var favourites:[]
    property var swatches:[]
    property var fontCatalog:({})
    property string error:""
    property string colourMode:"Wheel"
    readonly property var families:Qt.fontFamilies().filter(f=>(!favouritesOnly || favourites.includes(f)) && f.toLowerCase().includes(fontQuery.toLowerCase()))
    readonly property var styles:fontCatalog[family.currentText] || ["Regular"]
    readonly property color chosen:Qt.rgba(red.value/255,green.value/255,blue.value/255,alpha.value/100)
    signal copyRequested(string value)
    width:parent.width;spacing:10
    function setColour(value){if(!/^#([0-9a-fA-F]{6}|[0-9a-fA-F]{8})$/.test(value))return;const c=Qt.color(value);red.value=Math.round(c.r*255);green.value=Math.round(c.g*255);blue.value=Math.round(c.b*255);alpha.value=Math.round(c.a*100);}
    function save(){preferences.setText(JSON.stringify({favourites:favourites,swatches:swatches}));}
    FileView {id:preferences;path:(Quickshell.env("XDG_STATE_HOME") || Quickshell.env("HOME")+"/.local/state")+"/lumen-desktop/session/creative-tools.json";printErrors:false;atomicWrites:true;watchChanges:true
        onFileChanged:reload()
        onLoaded:{try{const d=JSON.parse(text());root.favourites=Array.isArray(d.favourites)?d.favourites:[];root.swatches=Array.isArray(d.swatches)?d.swatches:[];}catch(e){root.error="Saved collections could not be read.";}}
        onSaveFailed:root.error="The collection could not be saved."
        onSaved:root.error=""
    }
    Process {running:true;command:["fc-list","--format=%{family[0]}\t%{style[0]}\n"]
        stdout:StdioCollector {onStreamFinished:{const catalog={};for(const line of text.split("\n")){const fields=line.split("\t");if(fields.length!==2 || !fields[0] || !fields[1])continue;const list=catalog[fields[0]] || [];if(!list.includes(fields[1]))list.push(fields[1]);catalog[fields[0]]=list;}for(const name of Object.keys(catalog))catalog[name].sort((a,b)=>a==="Regular"?-1:b==="Regular"?1:a.localeCompare(b));root.fontCatalog=catalog;}}
    }
    PanelLabel {width:parent.width;wrapMode:Text.WordWrap;elide:Text.ElideNone;text:root.page==="Fonts"?"Preview a font. Apply formatting in the app that owns your document.":"Choose a colour, save swatches or copy its value.";font.pixelSize:12;color:Theme.colors.muted}
    Column {visible:root.page==="Fonts";width:parent.width;spacing:10
        DesktopTextField {width:parent.width;height:32;placeholderText:"Search font families";Accessible.name:"Search font families";onTextChanged:root.fontQuery=text;color:Theme.colors.text;font.family:Theme.font;background:Rectangle {radius:6;color:Theme.colors.field;border.color:parent.activeFocus?Theme.colors.accent:Theme.hairline}}
        Row {width:parent.width;spacing:8
            DesktopComboBox {id:family;width:parent.width-110;model:root.families;Accessible.name:"Font family";Component.onCompleted:currentIndex=Math.max(0,model.indexOf(Theme.font))}
            DialogButton {width:102;text:root.favourites.includes(family.currentText)?"★ Saved":"☆ Save";enabled:!!family.currentText;Accessible.name:"Save font to favourites";onClicked:{root.favourites=root.favourites.includes(family.currentText)?root.favourites.filter(f=>f!==family.currentText):root.favourites.concat([family.currentText]);root.save();}}
        }
        Row {width:parent.width;spacing:8
            DesktopComboBox {width:112;model:["All Fonts","Favourites"];Accessible.name:"Font collection";onActivated:index=>root.favouritesOnly=index===1}
            DesktopComboBox {id:style;width:Math.max(100,parent.width-240);model:root.styles;Accessible.name:"Font style"}
            DesktopSpinBox {id:pointSize;width:112;from:6;to:144;value:18;editable:true;Accessible.name:"Font size in points"}
        }
        Rectangle {width:parent.width;height:150;radius:8;color:Theme.colors.field;border.color:Theme.hairline
            TextEdit {anchors.fill:parent;anchors.margins:14;text:"The quick brown fox jumps over the lazy dog. 0123456789";wrapMode:TextEdit.Wrap;color:Theme.colors.text;font.family:family.currentText;font.styleName:style.currentText;font.pointSize:pointSize.value;selectByMouse:true;clip:true;Accessible.name:"Font preview"}
        }
        DialogButton {text:"Copy Font Description";width:190;enabled:!!family.currentText;onClicked:root.copyRequested(family.currentText+", "+style.currentText+", "+pointSize.value+" pt")}
    }
    Column {visible:root.page==="Colours";width:parent.width;spacing:10
        Row {width:parent.width;spacing:10
            Rectangle {width:70;height:40;radius:8;color:root.chosen;border.color:Theme.outline;Accessible.name:"Selected colour "+root.chosen.toString()}
            DesktopComboBox {width:130;model:["Wheel","RGB Sliders"];Accessible.name:"Colour picker mode";onActivated:index=>root.colourMode=model[index]}
            DialogButton {width:130;text:"Pick from Screen";enabled:!picker.running;onClicked:{root.error="";picker.running=true;}}
        }
        Item {width:parent.width;height:150;visible:root.colourMode==="Wheel"
            Canvas {id:wheel;width:150;height:150;anchors.horizontalCenter:parent.horizontalCenter
                onPaint:{const c=getContext("2d");c.reset();const cx=width/2,cy=height/2;for(let i=0;i<360;i++){c.beginPath();c.moveTo(cx,cy);c.arc(cx,cy,70,(i-1)*Math.PI/180,(i+1)*Math.PI/180);c.closePath();c.fillStyle=Qt.hsva(i/360,1,1,1);c.fill();}const g=c.createRadialGradient(cx,cy,0,cx,cy,70);g.addColorStop(0,"white");g.addColorStop(1,"transparent");c.fillStyle=g;c.beginPath();c.arc(cx,cy,70,0,2*Math.PI);c.fill();}
                MouseArea {anchors.fill:parent;function choose(x,y){const dx=x-width/2,dy=y-height/2,d=Math.sqrt(dx*dx+dy*dy);if(d>70)return;root.setColour(Qt.hsva((Math.atan2(dy,dx)/(2*Math.PI)+1)%1,Math.min(1,d/70),1,root.chosen.a).toString());}onPressed:mouse=>choose(mouse.x,mouse.y);onPositionChanged:mouse=>{if(pressed)choose(mouse.x,mouse.y);}}
                Accessible.name:"Colour wheel; RGB sliders provide keyboard input"
            }
        }
        Row {width:parent.width;spacing:8;PanelLabel {width:48;height:30;text:"Red";verticalAlignment:Text.AlignVCenter}CreativeSlider {id:red;width:parent.width-112;from:0;to:255;stepSize:1;value:0;Accessible.name:"Red"}PanelLabel {text:Math.round(red.value);width:48;height:30;verticalAlignment:Text.AlignVCenter}}
        Row {width:parent.width;spacing:8;PanelLabel {width:48;height:30;text:"Green";verticalAlignment:Text.AlignVCenter}CreativeSlider {id:green;width:parent.width-112;from:0;to:255;stepSize:1;value:122;Accessible.name:"Green"}PanelLabel {text:Math.round(green.value);width:48;height:30;verticalAlignment:Text.AlignVCenter}}
        Row {width:parent.width;spacing:8;PanelLabel {width:48;height:30;text:"Blue";verticalAlignment:Text.AlignVCenter}CreativeSlider {id:blue;width:parent.width-112;from:0;to:255;stepSize:1;value:255;Accessible.name:"Blue"}PanelLabel {text:Math.round(blue.value);width:48;height:30;verticalAlignment:Text.AlignVCenter}}
        Row {width:parent.width;spacing:8;PanelLabel {width:48;height:30;text:"Opacity";verticalAlignment:Text.AlignVCenter;font.pixelSize:11}CreativeSlider {id:alpha;width:parent.width-112;from:0;to:100;stepSize:1;value:100;Accessible.name:"Colour opacity"}PanelLabel {text:Math.round(alpha.value)+"%";width:48;height:30;verticalAlignment:Text.AlignVCenter}}
        Flow {width:parent.width;spacing:6
            Repeater {model:root.swatches
                AbstractButton {required property string modelData;width:28;height:28;Accessible.name:"Choose saved colour "+modelData;onClicked:root.setColour(modelData);background:Rectangle {radius:5;color:parent.modelData;border.width:parent.activeFocus?2:1;border.color:parent.activeFocus?Theme.colors.accent:Theme.outline}}
            }
        }
        Row {spacing:8;width:parent.width
            DesktopTextField {width:120;height:30;text:root.chosen.toString();color:Theme.colors.text;font.family:Theme.font;Accessible.name:"Hex colour, optional alpha first";selectByMouse:true;validator:RegularExpressionValidator {regularExpression:/#([0-9a-fA-F]{6}|[0-9a-fA-F]{8})/}onEditingFinished:if(acceptableInput)root.setColour(text);background:Rectangle {radius:6;color:Theme.colors.field;border.color:parent.activeFocus?Theme.colors.accent:Theme.hairline}}
            DialogButton {width:90;text:"Copy Hex";onClicked:root.copyRequested(root.chosen.toString())}
            DialogButton {width:110;text:"Save Swatch";onClicked:{const hex=root.chosen.toString();root.swatches=root.swatches.filter(c=>c!==hex).concat([hex]).slice(-20);root.save();}}
        }
    }
    StatusMessage {width:parent.width;text:root.error;severity:"error"}
    Process {id:picker;command:["hyprpicker","--format","hex","--no-fancy"];stdout:StdioCollector {onStreamFinished:{const value=text.trim();if(/^#[0-9a-fA-F]{6}$/.test(value))root.setColour(value);}}onExited:(code,status)=>{if(code!==0)root.error="Screen colour picking was cancelled or is unavailable."}}
}
