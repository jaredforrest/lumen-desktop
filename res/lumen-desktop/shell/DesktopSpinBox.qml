import QtQuick
import QtQuick.Controls
SpinBox {
        id:part
        font.family:Theme.font;font.pixelSize:Theme.textRole("label")
        background:Rectangle {radius:6;color:Theme.colors.field;border.color:part.activeFocus?Theme.colors.accent:Theme.hairline}
        contentItem:TextInput {
            text:part.textFromValue(part.value,part.locale);font:part.font;color:Theme.colors.text;selectionColor:Theme.colors.accent;selectedTextColor:Theme.colors.selectedText
            horizontalAlignment:Text.AlignHCenter;verticalAlignment:Text.AlignVCenter;readOnly:!part.editable;validator:part.validator;selectByMouse:true
            onEditingFinished:{part.value=part.valueFromText(text,part.locale);part.valueModified();}
        }
        rightPadding:18
        up.indicator:Rectangle {x:part.width-width;y:0;width:16;height:part.height/2;radius:4;color:part.up.hovered?Theme.colors.hover:"transparent";PanelLabel {anchors.centerIn:parent;text:"⌃";font.pixelSize:11;color:Theme.colors.muted}}
        down.indicator:Rectangle {x:part.width-width;y:part.height/2;width:16;height:part.height/2;radius:4;color:part.down.hovered?Theme.colors.hover:"transparent";PanelLabel {anchors.centerIn:parent;text:"⌄";font.pixelSize:11;color:Theme.colors.muted}}
    }
