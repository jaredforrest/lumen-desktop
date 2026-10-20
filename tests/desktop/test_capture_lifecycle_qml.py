"""Actual canceled stdout completion cannot republish a retired capture generation."""
import json,os,re,shutil,subprocess,sys,tempfile
from pathlib import Path
repo=Path(__file__).resolve().parents[2]
launcher=os.environ.get('LUMEN_TEST_SHELL_LAUNCHER') or shutil.which('lumen-desktop-shell')
prefix=re.split(r'\n[ \t]*exec quickshell ',Path(launcher).read_text(),maxsplit=1)[0]
with tempfile.TemporaryDirectory(prefix='lumen-capture-retire-') as private:
 root=Path(private);counter=root/'counter.json';counter.write_text('{"serial":0}')
 env=dict(os.environ,HOME=private,XDG_RUNTIME_DIR=private)
 for key in ('CONFIG','STATE','CACHE','DATA'):env['XDG_'+key+'_HOME']=str(root/key.lower())
 for key in ('DISPLAY','WAYLAND_DISPLAY','SWAYSOCK','DBUS_SESSION_BUS_ADDRESS','AT_SPI_BUS_ADDRESS'):env.pop(key,None)
 raw=subprocess.check_output(['bash','-c',prefix+'\nexec env -0'],env=env);env.update(dict(v.decode().split('=',1) for v in raw.split(b'\0') if b'=' in v))
 helper=root/'private-python';helper.write_text('#!'+sys.executable+'\n'+'''import json,signal,time
from pathlib import Path
root=Path(__file__).parent;counter=root/'counter.json';serial=json.loads(counter.read_text())['serial']+1
if serial==1:
 def finish(signum,frame):time.sleep(.1);raise SystemExit(0)
 signal.signal(signal.SIGTERM,finish)
print(json.dumps(dict(available=True,rows=[dict(kind='audio-input',reportedApplication='retired' if serial==1 else 'fresh',attribution='Synthetic private lifecycle')],coverage='Synthetic only')),flush=True)
counter.write_text(json.dumps(dict(serial=serial)))
if serial==1:time.sleep(6)
''');helper.chmod(0o700)
 env.update(GLOBAL_MENU_PYTHON=str(helper),QT_QPA_PLATFORM='offscreen',QT_QPA_PLATFORMTHEME='generic',QT_QUICK_BACKEND='software',QT_ACCESSIBILITY='0',QT_LINUX_ACCESSIBILITY_ALWAYS_ON='0',NO_AT_BRIDGE='1',GTK_A11Y='none',DBUS_SYSTEM_BUS_ADDRESS='unix:'+private+'/no-system',PULSE_SERVER='unix:'+private+'/no-audio',PIPEWIRE_REMOTE='no-pipewire',PYTHONWARNINGS='error')
 qml=root/'shell.qml';qml.write_text('''import QtQuick
import Quickshell
import Quickshell.Io
import "'''+(repo/'res/lumen-desktop/shell').as_uri()+'''" as D
ShellRoot {
 property int serial:0;property int phase:0;property double retiredAt:0
 FileView {path:'''+json.dumps(str(counter))+''';watchChanges:true;onFileChanged:reload();onLoaded:serial=JSON.parse(text()).serial}
 D.CaptureState {id:state;active:false}
 Timer {interval:10;running:true;repeat:true;triggeredOnStart:true;onTriggered:{try {
  if(phase===0){phase=1;state.active=true;return;}
  if(phase===1 && serial===1){state.active=false;state.active=true;phase=2;return;}
  if(phase===2){
   if(state.status.available && state.status.rows[0].reportedApplication==="retired")throw Error("Retired stdout was republished");
   if(serial===2 && state.status.available){if(state.status.rows[0].reportedApplication!=="fresh")throw Error("Wrong generation");state.active=false;retiredAt=Date.now();phase=3;}
   return;
  }
  if(phase===3 && Date.now()-retiredAt>150){if(state.audio || state.status.available)throw Error("Inactive state restored");console.log("CAPTURE_RETIRE_PASS");Qt.quit();}
 }catch(error){console.error("CAPTURE_RETIRE_FAIL "+error);Qt.quit();}}}
}''')
 result=subprocess.run(['quickshell','-p',str(qml)],env=env,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,timeout=15)
 assert result.returncode==0 and 'CAPTURE_RETIRE_PASS' in result.stdout,result.stdout
 assert not any(value in result.stdout for value in ['CAPTURE_RETIRE_FAIL','ReferenceError:','TypeError:','Binding loop','Failed to load configuration','Cannot assign']),result.stdout
 print('PASS real child canceled completion, generation retirement and fresh reactivation')
