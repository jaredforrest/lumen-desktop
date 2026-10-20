import QtQuick
Item {
    id:root
    property var manager:null
    property var registeredManager:null
    property var sources:[]
    property var mediaSources:[]
    property bool mediaStopped:true
    property bool alive:true
    visible:false
    onManagerChanged:{
        if(registeredManager)registeredManager.unregisterConsumer(root);
        registeredManager=manager;
        if(registeredManager)registeredManager.registerConsumer(root);
    }
    Component.onDestruction:{alive=false;if(registeredManager)registeredManager.unregisterConsumer(root);}
}
