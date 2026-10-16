import QtQuick
import QtQuick.Controls
Column {
    id: root
    objectName:"date-time-"+label.toLowerCase()
    property string text: ""
    property string label: "Date"
    property bool allDay: false
    property bool updating: false
    spacing: 5
    function sync() {
        const match=/^(\d{4})-(\d{2})-(\d{2})(?:T(\d{2}):(\d{2}))?$/.exec(text);
        if(!match)return;
        updating=true;year.value=Number(match[1]);month.currentIndex=Number(match[2])-1;day.value=Number(match[3]);hour.value=Number(match[4] || 9);minute.value=Number(match[5] || 0);updating=false;
    }
    function commit() {
        if(updating)return;
        text=String(year.value).padStart(4,"0")+"-"+String(month.currentIndex+1).padStart(2,"0")+"-"+String(day.value).padStart(2,"0")+(allDay?"":"T"+String(hour.value).padStart(2,"0")+":"+String(minute.value).padStart(2,"0"));
    }
    component DatePart: DesktopSpinBox {}
    onTextChanged: sync()
    PanelLabel {text:root.label;color:Theme.colors.muted;font.pixelSize:11}
    Row {
        spacing:5;width:parent.width
        DatePart {id:day;objectName:root.objectName+"-day";width:65;from:1;to:new Date(year.value,month.currentIndex+1,0).getDate();editable:true;Accessible.name:root.label+" day";onValueModified:root.commit()}
        DesktopComboBox {id:month;objectName:root.objectName+"-month";width:parent.width-160;model:Array.from({length:12},(_,i)=>Qt.locale().standaloneMonthName(i,Locale.LongFormat));Accessible.name:root.label+" month";onActivated:root.commit()}
        DatePart {id:year;objectName:root.objectName+"-year";width:85;from:1;to:9999;value:2026;editable:true;textFromValue:value=>String(value);valueFromText:text=>Number(text);Accessible.name:root.label+" year";onValueModified:root.commit()}
    }
    Row {
        visible:!root.allDay;spacing:6
        DatePart {id:hour;objectName:root.objectName+"-hour";width:90;from:0;to:23;editable:true;Accessible.name:root.label+" hour";onValueModified:root.commit()}
        PanelLabel {text:":";height:32;verticalAlignment:Text.AlignVCenter}
        DatePart {id:minute;objectName:root.objectName+"-minute";width:90;from:0;to:59;editable:true;textFromValue:value=>String(value).padStart(2,"0");valueFromText:text=>Number(text);Accessible.name:root.label+" minute";onValueModified:root.commit()}
    }
}
