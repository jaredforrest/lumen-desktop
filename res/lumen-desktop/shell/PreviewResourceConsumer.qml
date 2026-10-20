import QtQuick
Item {
    id:root
    property var manager:null
    property var registeredManager:null
    property var sourceObjects:[]
    property var mediaObjects:[]
    property bool alive:true
    visible:false
    function register(){if(registeredManager && alive)registeredManager.registerConsumer(root);}
    onSourceObjectsChanged:register()
    onMediaObjectsChanged:register()
    onManagerChanged:{
        if(registeredManager)registeredManager.unregisterConsumer(root);
        registeredManager=manager;register();
    }
    Component.onDestruction:{alive=false;if(registeredManager)registeredManager.unregisterConsumer(root);}
}
