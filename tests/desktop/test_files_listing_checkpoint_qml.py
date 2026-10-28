"""Production transport publishes authority only after a complete current reply."""
import os,re,shutil,subprocess,sys,tempfile
from pathlib import Path
source=Path(__file__).resolve().parents[2]/'res/lumen-desktop/shell'
launcher=os.environ.get('LUMEN_TEST_SHELL_LAUNCHER') or shutil.which('lumen-desktop-shell')
prefix=re.split(r'\n[ \t]*exec quickshell ',Path(launcher).read_text(),maxsplit=1)[0]
installed=dict(value.decode().split('=',1)for value in subprocess.check_output(['bash','-c',prefix+'\nexec env -0']).split(b'\0')if b'='in value)
with tempfile.TemporaryDirectory(prefix='lumen-list-checkpoint-') as temporary:
 root=Path(temporary);shutil.copytree(source,root/'assets')
 (root/'assets').chmod(0o700)
 (root/'assets/files.py').unlink()  # Replace only the copied transport, including from read-only store source.
 (root/'assets/files.py').write_text('''import json,sys
for line in sys.stdin:
 d=json.loads(line)
 if d['action']!='list':continue
 p=d['path'];r=dict(id=d['id'],action='list',path=p,parent='/',rows=[],breadcrumbs=[],writable=True)
 if p=='/error':r['error']='private expected error'
 if p=='/partial':r['limited']=True
 if p=='/unreadable':r['unreadable']=3
 if p=='/stale':r['id']-=1
 print(json.dumps(r),flush=True)
''')
 (root/'shell.qml').write_text('''import QtQuick
import Quickshell
import "assets" as D
ShellRoot {id:test
 property int phase:0
 property int ticks:0
 D.FilesState {id:state;persistViewPreferences:false}
 function check(ok,message){if(!ok)throw Error(message);}
 Timer {interval:60;repeat:true;running:true;onTriggered:{try {
  if(++ticks>80)throw Error("checkpoint timed out");
  if(phase===0){check(!state.listingCurrent && !state.listingComplete,"uninitialized authority");state.navigate("/complete");check(!state.listingCurrent,"new request retained authority");phase++;return;}
  if(phase===1){if(state.busy)return;check(state.listingCurrent && state.listingComplete,"complete reply not committed");state.navigate("/partial");phase++;return;}
  if(phase===2){if(state.busy)return;check(state.listingCurrent && !state.listingComplete,"limited reply allowed metadata pruning");state.navigate("/unreadable");phase++;return;}
  if(phase===3){if(state.busy)return;check(state.listingCurrent && !state.listingComplete,"unreadable reply allowed metadata pruning");state.navigate("/error");phase++;return;}
  if(phase===4){if(state.busy)return;check(!state.listingCurrent && !state.listingComplete,"error became authoritative");state.navigate("/stale");phase++;return;}
  if(phase===5){if(ticks<10)return;check(state.busy && !state.listingCurrent,"stale reply committed");state.navigate("/complete");phase++;return;}
  if(phase===6){if(state.busy)return;check(state.listingCurrent && state.listingComplete,"fresh reply failed recovery");state.path="/another";check(!state.listingCurrent,"path changed without invalidating authority");console.log("FILES_CHECKPOINT_PASS");Qt.quit();}
 }catch(error){console.error("FILES_CHECKPOINT_FAIL "+error);Qt.exit(1);}}}
}''')
 env=dict(installed,GLOBAL_MENU_PYTHON=sys.executable,HOME=str(root/'home'),XDG_RUNTIME_DIR=str(root/'runtime'),XDG_CONFIG_HOME=str(root/'config'),XDG_CACHE_HOME=str(root/'cache'),XDG_STATE_HOME=str(root/'state'),XDG_DATA_HOME=str(root/'data'),QT_QPA_PLATFORM='offscreen',QT_QPA_PLATFORMTHEME='generic',QT_QUICK_BACKEND='software',QT_ACCESSIBILITY='0',NO_AT_BRIDGE='1',GTK_A11Y='none',DBUS_SESSION_BUS_ADDRESS='unix:path='+str(root/'no-session'),DBUS_SYSTEM_BUS_ADDRESS='unix:path='+str(root/'no-system'))
 for name in ('HOME','XDG_RUNTIME_DIR'):Path(env[name]).mkdir(mode=0o700)
 for key in ('WAYLAND_DISPLAY','DISPLAY','SWAYSOCK','AT_SPI_BUS_ADDRESS'):env.pop(key,None)
 result=subprocess.run(['quickshell','-p',str(root/'shell.qml')],env=env,capture_output=True,text=True,timeout=12)
 text=result.stdout+result.stderr
 assert result.returncode==0 and 'FILES_CHECKPOINT_PASS'in text,text
 assert not any(marker in text for marker in ('WARN','ReferenceError:','TypeError:','Binding loop','Failed to load')),text
 print('PASS actual current/complete/stale/error/path-change listing authority transport')
