"""Real input pane/transport against a private synthetic compositor provider."""
import json,os,re,shutil,subprocess,sys,tempfile
from pathlib import Path
repo=Path(__file__).resolve().parents[2];sys.path.insert(0,str(Path(__file__).parent/'fixtures'))
from input_ipc import InputServer,settings_adapter
launcher=os.environ.get('LUMEN_TEST_SHELL_LAUNCHER') or shutil.which('lumen-desktop-shell')
prefix=re.split(r'\n[ \t]*exec quickshell ',Path(launcher).read_text(),maxsplit=1)[0]
with tempfile.TemporaryDirectory(prefix='lumen-input-qml-') as directory:
    root=Path(directory);env=dict(os.environ,HOME=directory,XDG_RUNTIME_DIR=directory)
    for key in ('CONFIG','STATE','CACHE','DATA'):env['XDG_'+key+'_HOME']=directory+'/'+key.lower()
    for key in ('WAYLAND_DISPLAY','DISPLAY','SWAYSOCK','AT_SPI_BUS_ADDRESS'):env.pop(key,None)
    raw=subprocess.check_output(['bash','-c',prefix+'\nexec env -0'],env=env);env.update(dict(value.decode().split('=',1) for value in raw.split(b'\0') if b'=' in value))
    env.update(QT_QPA_PLATFORM='offscreen',QT_QPA_PLATFORMTHEME='generic',QT_QUICK_BACKEND='software',QT_ACCESSIBILITY='0',QT_LINUX_ACCESSIBILITY_ALWAYS_ON='0',NO_AT_BRIDGE='1',GTK_A11Y='none',GLOBAL_MENU_PYTHON=sys.executable)
    adapter=settings_adapter(root,env['PATH']);env.update(adapter);server=InputServer(root/'input.sock');env['SWAYSOCK']=server.path
    shutil.copytree(repo/'res/lumen-desktop',root/'assets')
    palette=json.loads((repo/'res/lumen-desktop/appearance/palette.json').read_text());appearance=root/'appearance.json';appearance.write_text(json.dumps(dict(font=palette['font'],palette=palette['dark'],preferences=dict(textScale=1.5))));env['LUMEN_APPEARANCE_STATE']=str(appearance)
    path=root/'shell.qml';path.write_text('''import QtQuick
import Quickshell
import "assets/shell" as D
ShellRoot {
 id:test;property int step:0
 D.InputSettingsState {id:service;active:true}
 D.InputSettingsPane {id:pane;width:240;service:service}
 function find(name,item){if(item.objectName===name)return item;for(const child of item.children || []){const found=find(name,child);if(found)return found;}return null;}
 Timer {interval:100;running:true;repeat:true;onTriggered:{try {
  if(service.busy)return;
  if(!service.available || service.error)throw Error("Provider "+service.error);
  if(test.step===0){
   pane.selected=service.devices.find(row=>row.identity.type==="pointer").key;test.step++;return;
  }
  if(test.step===1){
   const toggle=find("input-option-natural_scroll",pane);if(!toggle || !toggle.enabled || toggle.checked)throw Error("Wrong selected capability");
   toggle.checked=true;toggle.clicked();test.step++;return;
  }
  if(test.step===2){
   const option=pane.device.controls.find(row=>row.key==="natural_scroll");if(!option.value || !option.saved || !service.message)throw Error("Save not confirmed");
   const restore=find("input-restore-natural_scroll",pane);if(!restore.visible || !restore.enabled)throw Error("No reversal");restore.clicked();test.step++;return;
  }
  if(test.step===3){
   if(pane.device.controls.find(row=>row.key==="natural_scroll").value)throw Error("Restore not applied");
   pane.selected=service.devices.find(row=>row.identity.type==="touchpad").key;test.step++;return;
  }
  if(test.step===4){
   const toggle=find("input-option-tap",pane);if(!toggle || !toggle.enabled)throw Error("Trackpad tap absent");toggle.checked=true;toggle.clicked();test.step++;return;
  }
  if(test.step===5){
   if(!pane.device.controls.find(row=>row.key==="tap").value)throw Error("Tap not applied");
   find("input-forget-profile",pane).clicked();
   if(pane.forgetTarget.identity.identifier!==pane.device.identity.identifier || service.busy)throw Error("Forget changed before confirmation");
   find("input-forget-cancel",pane).clicked();if(pane.forgetTarget!==null)throw Error("Forget cancel failed");
   console.log("INPUT_SETTINGS_PASS");Qt.quit();
  }
 }catch(error){console.error("INPUT_SETTINGS_FAIL "+error);Qt.quit();}}}
}''')
    try:
        result=subprocess.run(['quickshell','-p',str(path)],env=env,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,timeout=20)
        assert result.returncode==0 and 'INPUT_SETTINGS_PASS' in result.stdout,result.stdout
        assert not any(marker in result.stdout for marker in ('INPUT_SETTINGS_FAIL','ReferenceError:','TypeError:','Binding loop','Failed to load configuration','Cannot assign')),result.stdout
        assert server.devices[1]['libinput']['natural_scroll']=='disabled' and server.devices[2]['libinput']['tap']=='enabled'
        saved=json.loads((root/'settings.json').read_text())['inputs'];assert len(saved)==1 and next(iter(saved.values()))['identity']['type']=='touchpad',saved
    finally:server.close()
print('PASS actual input pane selected-device scope, supported controls, typed request transport and reversal')
