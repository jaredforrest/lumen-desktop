"""Actual catalogue, instance state and move cancellation using disposable Qt."""
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile

repo=Path(__file__).resolve().parents[2]
launcher=Path(os.environ.get('LUMEN_TEST_SHELL_LAUNCHER') or shutil.which('lumen-desktop-shell'))
prefix=re.split(r'\n[ \t]*exec quickshell ',launcher.read_text(),maxsplit=1)[0]
with tempfile.TemporaryDirectory(prefix='lumen-widget-catalogue-') as directory:
    root=Path(directory)
    env=dict(os.environ,HOME=directory,XDG_RUNTIME_DIR=directory,QT_QPA_PLATFORM='offscreen',QT_QPA_PLATFORMTHEME='generic',QT_QUICK_BACKEND='software',QT_ACCESSIBILITY='0',QT_LINUX_ACCESSIBILITY_ALWAYS_ON='0',NO_AT_BRIDGE='1',GTK_A11Y='none')
    for key in ('CONFIG','STATE','CACHE','DATA'):env['XDG_'+key+'_HOME']=str(root/key.lower())
    for key in ('WAYLAND_DISPLAY','DISPLAY','DBUS_SESSION_BUS_ADDRESS','AT_SPI_BUS_ADDRESS'):env.pop(key,None)
    shutil.copytree(repo/'res/lumen-desktop',root/'assets')
    config=root/'bus.conf';config.write_text('<busconfig><type>session</type><listen>unix:tmpdir='+directory+'</listen><auth>EXTERNAL</auth><policy context="default"><allow send_destination="*"/><allow receive_sender="*"/><allow own="*"/></policy></busconfig>')
    qml=root/'shell.qml'
    qml.write_text('''import QtQuick
import Quickshell
import "assets/shell" as D
ShellRoot {
 id:fixture
 property var restored:null
 Component {id:restorer;D.WidgetState {}}
 D.WidgetState {id:state}
 QtObject {id:stub;property var widgets:state;property date currentDate:new Date(2026,9,19,13,5);function showMenu(kind,output){}function openSettings(page){}function openCalendarAt(day,event){}}
 D.DesktopWidgetItem {id:card;shell:stub;descriptor:state.desktopItems[0] || {id:"fallback",type:"clock",size:"small",positions:{},hourCycle:"locale"};outputName:"test";widgetIndex:0;availableWidth:480;availableHeight:320;active:false}
 D.DesktopWidgetEditor {service:state;width:300}
 property string savedId:""
 Timer {interval:150;running:true;onTriggered:{try {
  state.addWidget("clock");savedId=state.desktopItems[0].id;
  if(state.desktopItems.length!==1 || !state.preferences.desktopVisible)throw Error("Legacy hidden migration");
  state.addWidget("calendar");state.updateWidget(savedId,{size:"large",hourCycle:"24",output:"disconnected"});state.positionWidget(savedId,"test",.25,.5);
  if(state.desktopItems.length!==2 || state.desktopItems[0].size!=="large")throw Error("Catalogue mutation");
  card.beginMove(null);card.moveTo(400,400);card.cancelMove();
  if(card.preview || state.desktopItems[0].positions.test.x!==.25)throw Error("Cancel persisted preview");
  state.updateWidget("stale-id",{size:"small"});
  if(state.desktopItems[0].size!=="large")throw Error("Stale target");
 }catch(error){console.error("WIDGET_FAIL "+error);Qt.quit();}}}
 Timer {interval:600;running:true;onTriggered:fixture.restored=restorer.createObject(fixture)}
 Timer {interval:950;running:true;onTriggered:{if(state.error)console.error("WIDGET_FAIL "+state.error);else if(!fixture.restored || fixture.restored.desktopItems.length!==2 || fixture.restored.desktopItems[0].positions.test.x!==.25)console.error("WIDGET_FAIL Restart state");else console.log("WIDGET_PASS");Qt.quit();}}
}''')
    command=['dbus-run-session','--config-file='+str(config),'--','bash','-c',prefix+'\nexport QT_QPA_PLATFORM=offscreen QT_QPA_PLATFORMTHEME=generic QT_QUICK_BACKEND=software\nexec quickshell -p "$1"','fixture',str(qml)]
    result=subprocess.run(command,env=env,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,timeout=15)
    assert result.returncode==0 and 'WIDGET_PASS' in result.stdout and 'WIDGET_FAIL' not in result.stdout,result.stdout
    assert not any(text in result.stdout for text in ('TypeError:','ReferenceError:','Binding loop','Failed to load configuration')),result.stdout
    path=root/'state/lumen-desktop/session/notification-centre.json'
    value=json.loads(path.read_text())
    assert len(value['desktopItems'])==2 and value['desktopItems'][0]['positions']['test']=={'x':.25,'y':.5}
    print('PASS actual widget catalogue, component creation, scoped state, cancellation and durable persistence')
