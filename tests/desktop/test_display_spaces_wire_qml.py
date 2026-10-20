"""Production UI parser consumes real private helper frames; no compositor access."""
import hashlib,json,os,re,shutil,signal,subprocess,sys,tempfile
from pathlib import Path
repo=Path(__file__).resolve().parents[2]
with tempfile.TemporaryDirectory(prefix='lumen-display-wire-') as directory:
 root=Path(directory);shutil.copytree(repo/'res/lumen-desktop',root/'assets')
 env=dict(os.environ,QT_QPA_PLATFORM='offscreen',QT_QUICK_BACKEND='software',QT_ACCESSIBILITY='0',NO_AT_BRIDGE='1',GTK_A11Y='none',QT_QPA_PLATFORMTHEME='generic',QT_NO_XDG_DESKTOP_PORTAL='1')
 for name in ('HOME','XDG_RUNTIME_DIR','XDG_STATE_HOME','XDG_CONFIG_HOME','XDG_CACHE_HOME','XDG_DATA_HOME'):env[name]=str(root/name);Path(env[name]).mkdir(mode=0o700)
 for name in ('WAYLAND_DISPLAY','DISPLAY','SWAYSOCK','AT_SPI_BUS_ADDRESS','DBUS_SESSION_BUS_ADDRESS'):env.pop(name,None)
 publisher=root/'publisher.py';publisher.write_text('import sys\nfor line in sys.stdin:print(line.rstrip("\\n"),flush=True)\n')
 target=root/'assets/shell/DisplaySpacesState.qml';target.chmod(0o600);original=target.read_text();lines=original.splitlines();commands=[i for i,line in enumerate(lines) if line.strip().startswith('command:')];assert len(commands)==1
 lines[commands[0]]='        command:'+json.dumps([sys.executable,str(publisher)])
 modified='\n'.join(lines)+'\n';assert modified.count('running:true')==1;modified=modified.replace('running:true','running:true;stdinEnabled:true');target.write_text(modified)
 qml=root/'shell.qml';qml.write_text('''import QtQuick
import Quickshell
import "assets/shell" as D
ShellRoot {
 D.DisplaySpacesState {id:p}
 property int step:0
 property int changes:0
 property int expected:0
 property int turns:0
 Connections {target:p;function onStateChanged(){changes++;}}
 property var validFrame:({protocolVersion:1,nativeSupported:true,session:"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",generation:"2",available:true,applied:true,separateDisplays:false,independent:false,primary:"A",primaryDisplay:null,outputs:[{name:"A",label:"Display",identity:{kind:"connector",name:"A",make:null,model:null,serial:null}}],error:""})
 function send(value){expected=changes+1;p.observer.write((typeof value==="string"?value:JSON.stringify(value))+"\\n");}
 function check(value,message){if(!value)throw Error(message);}
 Timer {interval:50;repeat:true;running:true;onTriggered:{try{
  turns++;check(turns<120,"wire acknowledgement deadline");
  if(step===0){if(!p.observer.running)return;check(p.routing.mode==="independent","startup hid Dock");send(validFrame);step++;return;}
  if(changes<expected)return;
  if(step===1){check(p.state.applied && p.routing.mode==="main","valid frame refused");send("{broken");step++;return;}
  if(step===2){check(!p.state.applied && p.routing.mode==="main","malformed JSON lost routing or kept authority");send(Object.assign({},validFrame,{generation:"1"}));step++;return;}
  if(step===3){check(!p.state.applied && p.lastAccepted.generation==="2","stale generation accepted");send(Object.assign({},validFrame,{independent:true}));step++;return;}
  if(step===4){check(!p.state.applied && p.routing.mode==="main" && p.lastAccepted.generation==="2","contradictory independent authority changed routing");send(Object.assign({},validFrame,{session:"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",generation:"0"}));step++;return;}
  if(step===5){check(p.state.applied && p.lastAccepted.generation==="0","new session refused");send(Object.assign({},validFrame,{nativeSupported:false,session:null,generation:null,available:false,applied:false,independent:true,primary:"",error:"Upgrade required"}));step++;return;}
  check(!p.state.applied && p.lastAccepted.nativeSupported===false && p.routing.mode==="independent","legacy fallback claimed authority or lost routing");console.log("DISPLAY_WIRE_PASS");Qt.quit();
 }catch(error){console.error("DISPLAY_WIRE_FAIL "+error);Qt.quit();}}}
}''')
 child=subprocess.Popen(['quickshell','-p',str(qml)],env=env,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,text=True,start_new_session=True)
 try:output,_=child.communicate(timeout=10)
 finally:
  if child.poll() is None:
   os.killpg(child.pid,signal.SIGTERM)
   try:child.wait(timeout=3)
   except subprocess.TimeoutExpired:os.killpg(child.pid,signal.SIGKILL);child.wait(timeout=3)
  if child.stdout:child.stdout.close()
 assert child.returncode==0 and 'DISPLAY_WIRE_PASS' in output and 'DISPLAY_WIRE_FAIL' not in output,output
 assert not any(error in output for error in ('ReferenceError:','TypeError:','Binding loop','Failed to load configuration','Cannot assign','Unable to assign')),output
 print('PASS actual Qt helper JSON transport: typed authority, stale generation, fresh session, malformed-frame retained routing and legacy fallback')
 print(json.dumps(dict(original=hashlib.sha256(original.encode()).hexdigest(),modified=hashlib.sha256(modified.encode()).hexdigest(),transform='Only fixed helper argv and stdin transport are substituted in the disposable copy; validation/receiver code is unchanged')))
