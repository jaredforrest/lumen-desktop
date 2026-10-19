import QtQuick
import QtQuick.Controls
Column {
    id:root
    property bool active:false
    property var service:owned
    property string selected:""
    property var forgetTarget:null
    readonly property var device:service.devices.find(row=>row.key===selected) || service.devices[0] || null
    function prepareForget(){
        forgetTarget=JSON.parse(JSON.stringify(device));Qt.callLater(()=>{
            let ancestor=root.parent;
            while(ancestor){
                if(ancestor.contentY!==undefined && ancestor.contentItem){const at=forgetConfirmation.mapToItem(ancestor.contentItem,0,0);ancestor.contentY=Math.max(0,Math.min(ancestor.contentHeight-ancestor.height,at.y-12));break;}
                ancestor=ancestor.parent;
            }
            forgetCancel.forceActiveFocus();
        });
    }
    Keys.onEscapePressed:event=>{if(forgetTarget!==null)forgetTarget=null;else event.accepted=false;}
    spacing:10
    InputSettingsState {id:owned;active:root.active && root.service===owned}
    PanelLabel {width:root.width;wrapMode:Text.Wrap;text:"Keyboard, mouse and trackpad";font.weight:Font.DemiBold}
    PanelLabel {width:root.width;wrapMode:Text.Wrap;text:"Choose one compositor device identity. Changes never use an all-devices or device-type selector.";color:Theme.colors.muted}
    DesktopComboBox {objectName:"input-device-selector";width:root.width;model:root.service.devices.map(row=>row.identity.name+" · "+row.identity.type+(row.connected?"":" · disconnected"));currentIndex:root.device?root.service.devices.findIndex(row=>row.key===root.device.key):-1;Accessible.name:"Input device";enabled:!root.service.busy && count>0;onActivated:index=>root.selected=root.service.devices[index].key}
    PanelLabel {width:root.width;wrapMode:Text.WrapAnywhere;text:root.device?root.device.identity.identifier:"No input devices reported.";font.pixelSize:Theme.textSize(11);color:Theme.colors.muted}
    PanelLabel {width:root.width;wrapMode:Text.Wrap;text:root.device?root.device.detail:"The compositor provider must be available to inspect devices.";color:Theme.colors.muted}
    StatusMessage {width:root.width;text:root.service.error;severity:"error"}
    StatusMessage {width:root.width;text:root.service.message;severity:"success"}
    Repeater {model:root.device?root.device.controls:[]
        Column {
            required property var modelData
            property var option:modelData
            width:root.width;spacing:6
            SettingsToggle {objectName:"input-option-"+option.key;visible:option.kind==="bool";width:parent.width;text:option.label;detail:option.supported?"Only "+root.device.identity.name:"This option is not supported by the reported device state.";checked:option.value===true;enabled:option.supported && !root.service.busy;onClicked:root.service.setValue(root.device.key,option.key,checked)}
            PanelLabel {visible:option.kind==="number";width:parent.width;text:option.label+(option.supported?": "+Number(option.value).toFixed(option.step<1?1:0):" · not reported");wrapMode:Text.Wrap}
            SystemSlider {objectName:"input-slider-"+option.key;visible:option.kind==="number";width:parent.width;from:option.minimum || 0;to:option.maximum || 1;stepSize:option.step || 1;value:option.value===null?from:option.value;enabled:option.supported && !root.service.busy;Accessible.name:option.label+" for "+(root.device?root.device.identity.identifier:"");onMoved:if(!pressed)root.service.setValue(root.device.key,option.key,Number(value.toFixed(option.step<1?1:0)));onPressedChanged:if(!pressed && enabled && Math.abs(value-option.value)>0.001)root.service.setValue(root.device.key,option.key,Number(value.toFixed(option.step<1?1:0)))}
            PanelLabel {visible:option.saved && !option.inSync;width:parent.width;wrapMode:Text.Wrap;text:"The saved value is not currently applied. Current device behavior is shown above.";color:Theme.colors.muted}
            DialogButton {objectName:"input-restore-"+option.key;visible:option.saved;width:Math.min(implicitWidth,parent.width);text:"Restore previous value";enabled:option.supported && !root.service.busy;Accessible.name:"Restore previous "+option.label+" for "+(root.device?root.device.identity.identifier:"");onClicked:root.service.restore(root.device.key,option.key)}
        }
    }
    PanelLabel {width:root.width;wrapMode:Text.Wrap;text:"Gesture bindings are currently global, not per-device. Layout, modifier remapping and security policy remain in the host configuration. Devices with indistinguishable identifiers cannot be controlled separately.";color:Theme.colors.muted}
    DialogButton {objectName:"input-forget-profile";visible:!!root.device && root.device.saved;width:Math.min(implicitWidth,parent.width);text:"Forget saved device profile…";enabled:!root.service.busy;onClicked:root.prepareForget()}
    Column {id:forgetConfirmation;visible:root.forgetTarget!==null;width:parent.width;spacing:6
        PanelLabel {width:parent.width;wrapMode:Text.WrapAnywhere;text:root.forgetTarget?"Forget saved settings for "+root.forgetTarget.identity.name+"\n"+root.forgetTarget.identity.identifier+"? Current device values will stay unchanged.":""}
        DialogButton {objectName:"input-forget-confirm";text:"Forget profile";destructive:true;enabled:!root.service.busy;onClicked:{const key=root.forgetTarget.key;root.forgetTarget=null;root.service.forget(key);}}
        DialogButton {id:forgetCancel;objectName:"input-forget-cancel";text:"Cancel";onClicked:root.forgetTarget=null}
    }
    ControlRow {width:parent.width;text:root.service.busy?"Reading device settings…":"Refresh devices";enabled:!root.service.busy;onClicked:root.service.refresh()}
}
