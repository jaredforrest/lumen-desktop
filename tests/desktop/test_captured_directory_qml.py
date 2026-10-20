"""Actual Files transport rejects changed captured initial navigation without commit."""
import hashlib,json,os,re,shutil,subprocess,sys,tempfile
from pathlib import Path
from private_native_bus import start_private_bus
from fixture_resources import close_reaped_pipes
repo=Path(__file__).resolve().parents[2]
with tempfile.TemporaryDirectory(prefix='lumen-directory-consumer-') as td:
 root=Path(td);assets=root/'assets';shutil.copytree(repo/'res/lumen-desktop',assets);initial=root/'initial';target=root/'target';initial.mkdir();target.mkdir();(initial/'retained.txt').write_text('retained');(target/'original.txt').write_text('original');info=target.lstat();facts=[str(getattr(info,k)) for k in ('st_dev','st_ino','st_mode','st_size','st_mtime_ns','st_ctime_ns')];target.rename(root/'retired');target.mkdir();(target/'replacement.txt').write_text('replacement')
 qml=root/'shell.qml';qml.write_text('''import QtQuick
import Quickshell
import "assets/shell" as D
ShellRoot {
 D.FilesState {id:state;persistViewPreferences:false;active:false}
 property int step:0
 property string initial:'''+json.dumps(str(initial))+'''
 property string target:'''+json.dumps(str(target))+'''
 property var expected:'''+json.dumps(facts)+'''
 function check(ok,message){if(!ok)throw Error(message);}
 Timer {interval:50;repeat:true;running:true;onTriggered:{try {
  if(step===0){state.navigate(initial);step++;return;}
  if(state.busy || !state.ready)return;
  if(step===1){check(state.path===initial && state.rows.length===1,"initial list");state.navigate(target,undefined,false,expected);step++;return;}
  if(step===2){check(state.error!=="","captured replacement accepted");check(state.path===initial && state.rows.length===1 && state.rows[0].name==="retained.txt","wrong rows committed");check(state.history.length===1,"failed navigation committed history");console.log("PASS actual Files expected-directory request and rejection preserve rows/path/history");Qt.quit();}
 }catch(e){console.error("FAIL "+e);Qt.exit(1);}}}
}''')
 launcher=os.environ.get('LUMEN_TEST_SHELL_LAUNCHER') or shutil.which('lumen-desktop-shell');assert launcher
 prefix=re.split(r'\n[ \t]*exec quickshell ',Path(launcher).read_text(),maxsplit=1)[0]
 installed=dict(v.decode().split('=',1) for v in subprocess.check_output(['bash','-c',prefix+'\nexec env -0']).split(b'\0') if b'=' in v)
 env=dict(os.environ);env.update({k:installed[k] for k in ('PATH','QML_IMPORT_PATH','QT_PLUGIN_PATH') if k in installed})
 env.update(QT_QPA_PLATFORM='offscreen',QT_QUICK_BACKEND='software',QT_QPA_PLATFORMTHEME='generic',QT_ACCESSIBILITY='0',NO_AT_BRIDGE='1',GTK_A11Y='none',GLOBAL_MENU_PYTHON=sys.executable,DBUS_SYSTEM_BUS_ADDRESS='unix:'+str(root/'no-system'),PULSE_SERVER='unix:'+str(root/'no-audio'))
 for name in ('HOME','XDG_RUNTIME_DIR','XDG_CONFIG_HOME','XDG_STATE_HOME','XDG_CACHE_HOME','XDG_DATA_HOME'):env[name]=str(root/name);Path(env[name]).mkdir(mode=0o700)
 for name in ('WAYLAND_DISPLAY','DISPLAY','SWAYSOCK','AT_SPI_BUS_ADDRESS','DBUS_SESSION_BUS_ADDRESS'):env.pop(name,None)
 palette=json.loads((assets/'appearance/palette.json').read_text());appearance=root/'appearance.json';appearance.write_text(json.dumps(dict(font=palette['font'],palette=palette['dark'],preferences={})));env['LUMEN_APPEARANCE_STATE']=str(appearance);children=[]
 try:
  start_private_bus(env,root,children);run=subprocess.run(['quickshell','-p',str(qml)],env=env,capture_output=True,text=True,timeout=15)
 finally:
  for child in reversed(children):
   if child.poll() is None:child.terminate()
   try:child.wait(timeout=3)
   except subprocess.TimeoutExpired:child.kill();child.wait(timeout=3)
  close_reaped_pipes(children)
 assert run.returncode==0 and 'PASS actual Files expected-directory' in run.stdout+run.stderr,run.stdout+run.stderr;print(run.stdout+run.stderr)
