import QtQuick

// Caption capacity follows actual shaping while the optical icon remains fixed.
Item {
    id:root
    property var captions:[]
    readonly property int pixelSize:Theme.textRole("metadata")
    readonly property real capacityRatio:{
        let ratio=Math.max(1,pixelSize/11);
        for(let i=0;i<measures.count;i++){
            const item=measures.itemAt(i);
            if(item && item.baselineWidth>0)ratio=Math.max(ratio,item.currentWidth/item.baselineWidth);
        }
        return ratio;
    }
    readonly property int captionWidth:Math.ceil(98*capacityRatio)
    readonly property int captionHeight:Math.max(34,Math.ceil(fontMetrics.height*2+6))
    readonly property int tileWidth:captionWidth+10
    readonly property int tileHeight:Math.max(100,61+captionHeight+5)
    FontMetrics {id:fontMetrics;font.family:Theme.font;font.pixelSize:root.pixelSize}
    Repeater {id:measures;model:root.captions
        Item {
            required property string modelData
            readonly property real baselineWidth:baseline.implicitWidth
            readonly property real currentWidth:current.implicitWidth
            Text {id:baseline;visible:false;text:parent.modelData;font.family:Theme.font;font.pixelSize:11}
            Text {id:current;visible:false;text:parent.modelData;font.family:Theme.font;font.pixelSize:root.pixelSize}
        }
    }
}
