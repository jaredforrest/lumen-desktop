import QtQuick
import "BatteryModel.js" as Model
Item {
    id:root
    readonly property var status: snapshot
    property var snapshot:Model.unavailable()
    readonly property string description:Model.description(snapshot)
    readonly property string estimate:Model.estimate(snapshot)
    property int freshnessMilliseconds:10000
    function publish(raw){snapshot=Model.normalize(raw);expiry.restart();}
    function clear(){expiry.stop();snapshot=Model.unavailable();}
    Timer {id:expiry;interval:root.freshnessMilliseconds;onTriggered:root.clear()}
}
