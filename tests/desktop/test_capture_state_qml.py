"""Actual production capture state expiry and report rendering, with no provider."""
import os,re,shutil,subprocess,tempfile
from pathlib import Path
repo=Path(__file__).resolve().parents[2]
launcher=os.environ.get('LUMEN_TEST_SHELL_LAUNCHER') or shutil.which('lumen-desktop-shell')
prefix=re.split(r'\n[ \t]*exec quickshell ',Path(launcher).read_text(),maxsplit=1)[0]
with tempfile.TemporaryDirectory(prefix='lumen-capture-state-') as private:
 env=dict(os.environ,HOME=private,XDG_RUNTIME_DIR=private)
 for key in ('CONFIG','STATE','CACHE','DATA'):env['XDG_'+key+'_HOME']=str(Path(private)/key.lower())
 for key in ('DISPLAY','WAYLAND_DISPLAY','SWAYSOCK','DBUS_SESSION_BUS_ADDRESS','AT_SPI_BUS_ADDRESS'):env.pop(key,None)
 raw=subprocess.check_output(['bash','-c',prefix+'\nexec env -0'],env=env);env.update(dict(v.decode().split('=',1) for v in raw.split(b'\0') if b'=' in v))
 env.update(QT_QPA_PLATFORM='offscreen',QT_QPA_PLATFORMTHEME='generic',QT_QUICK_BACKEND='software',QT_ACCESSIBILITY='0',QT_LINUX_ACCESSIBILITY_ALWAYS_ON='0',NO_AT_BRIDGE='1',GTK_A11Y='none',DBUS_SYSTEM_BUS_ADDRESS='unix:'+private+'/no-system',PULSE_SERVER='unix:'+private+'/no-audio',PIPEWIRE_REMOTE='no-pipewire',PYTHONWARNINGS='error')
 path=Path(private)/'shell.qml';path.write_text('''import QtQuick
import Quickshell
import "'''+(repo/'res/lumen-desktop/shell').as_uri()+'''" as D
ShellRoot {
 D.CaptureState {id:state;active:false}
 FloatingWindow {visible:true;implicitWidth:400;implicitHeight:600
  D.CaptureReport {id:report;width:360;state:state}
 }
 Timer {interval:100;running:true;onTriggered:{try {
  if(state.status.available || state.audio || state.video)throw Error("Initial state");
  const row={kind:"audio-input",reportedApplication:"Synthetic recorder <literal>",attribution:"Reported metadata, not authenticated"};
  if(!state.publish({available:true,rows:[row],coverage:"Other paths not assessed"}) || !state.audio || state.video)throw Error("Audio graph");
  state.expire(state.lastPublished+7501);if(state.audio || state.status.available)throw Error("Stale state survived");
  state.publish({available:true,rows:[row],coverage:"Other paths not assessed"});const published=state.lastPublished;state.expire(published+5000);state.expire(published+4000);if(state.audio)throw Error("Clock reversal");
  if(state.publish({available:true,rows:[{kind:"camera",reportedApplication:"invented",attribution:"wrong"}],coverage:""}) || state.status.available)throw Error("Unknown classification accepted");
  state.publish({available:true,rows:[row,{kind:"video-input",reportedApplication:"Synthetic video",attribution:"Reported metadata"}],coverage:"Other paths not assessed"});
  if(!state.audio || !state.video || report.height<=0 || report.height>600)throw Error("Actual report layout");
  console.log("CAPTURE_STATE_PASS");Qt.quit();
 }catch(error){console.error("CAPTURE_STATE_FAIL "+error);Qt.quit();}}}
}''')
 result=subprocess.run(['quickshell','-p',str(path)],env=env,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,timeout=15)
 assert result.returncode==0 and 'CAPTURE_STATE_PASS' in result.stdout,result.stdout
 assert not any(v in result.stdout for v in ['CAPTURE_STATE_FAIL','ReferenceError:','TypeError:','Binding loop','Failed to load configuration','Cannot assign']),result.stdout
 print('PASS actual capture state expiry, fail-closed schema and rendered report')
