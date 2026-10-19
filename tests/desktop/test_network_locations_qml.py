"""Real Qt network flow with a private fake provider; no mounts or accounts."""
import os,re,shutil,subprocess,sys,tempfile,unittest
from pathlib import Path
class NetworkLocationsQt(unittest.TestCase):
    def test_provider_capabilities_connection_browse_cancel_and_stale_replies(self):
        repo=Path(__file__).resolve().parents[2];assets=Path(os.environ.get('LUMEN_TEST_SHELL_ASSETS',repo/'res/lumen-desktop/shell'))
        launcher=os.environ.get('LUMEN_TEST_SHELL_LAUNCHER') or shutil.which('lumen-desktop-shell');prefix=re.split(r'\n[ \t]*exec quickshell ',Path(launcher).read_text(),maxsplit=1)[0]
        raw=subprocess.check_output(['bash','-c',prefix+'\nexec env -0']);env=dict(v.decode().split('=',1) for v in raw.split(b'\0') if b'=' in v)
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary);helper=root/'provider.py';helper.write_text('''import json,sys
connections=[]
def emit(data):print(json.dumps(data),flush=True)
def snapshot(event='state'):emit(dict(event=event,connections=connections,protocols=['sftp'],available=True,reason=''))
snapshot('ready')
for line in sys.stdin:
 request=json.loads(line);id=request['id'];action=request['action']
 if action=='status':emit(dict(id=id,action=action,connections=connections,protocols=['sftp'],available=True,reason=''))
 elif action=='connect':
  entry=dict(id='1'*32,name='Fixture',uri=request['uri'],state='authenticating' if request['uri'].endswith('ask') else 'connected',path='',canDisconnect=True,error='');connections[:]=[entry];snapshot()
  if entry['state']=='connected':emit(dict(id=id,action=action,connected=True,connectionId=entry['id']));snapshot()
 elif action=='browse':emit(dict(id=id,action=action,connectionId='1'*32,rows=[dict(name='Folder',uri='sftp://fixture/root/Folder',directory=True,path=''),dict(name='document.txt',uri='sftp://fixture/root/document.txt',directory=False,path='')],uri=request.get('uri','sftp://fixture/root'),parent='',path='',limited=False))
 elif action=='cancel':
  if connections:connections[0]['state']='disconnected'
  emit(dict(id=request['target'],action='connect',cancelled=True,error='Cancelled'));snapshot();emit(dict(id=id,action=action))
''')
            env.update(HOME=temporary,XDG_RUNTIME_DIR=temporary,XDG_STATE_HOME=temporary,XDG_CONFIG_HOME=temporary,XDG_DATA_HOME=temporary,XDG_CACHE_HOME=temporary,QT_QPA_PLATFORM=os.environ.get('LUMEN_NETWORK_QPA','offscreen'),QT_QUICK_BACKEND='software',QT_QPA_PLATFORMTHEME='generic',QT_ACCESSIBILITY='0',QT_LINUX_ACCESSIBILITY_ALWAYS_ON='0',NO_AT_BRIDGE='1',GTK_A11Y='none')
            for key in ('DISPLAY','WAYLAND_DISPLAY','SWAYSOCK'):
                if env['QT_QPA_PLATFORM']=='offscreen':env.pop(key,None)
            qml=root/'shell.qml';qml.write_text('''import QtQuick
import Quickshell
ShellRoot {
 id:root
 property int phase:0
 property int ticks:0
 FloatingWindow {visible:true;implicitWidth:700;implicitHeight:800
  Loader {id:state;Component.onCompleted:setSource("file:'''+str(assets/'NetworkLocationsState.qml')+'''",{command:["'''+sys.executable+'''","'''+str(helper)+'''"],active:true});onLoaded:pane.setSource("file:'''+str(assets/'NetworkLocationsPane.qml')+'''",{service:item})}
  Loader {id:pane;width:parent.width-32;x:16;y:16}
 }
 function check(value,message){if(!value)throw Error(message);}
 Timer {interval:80;running:true;repeat:true;onTriggered:{
  try {
   if(++ticks>100)throw Error("Network fixture timeout phase "+phase);
   if(!state.item || !pane.item)return;
   const service=state.item;
   if(phase===0){if(!service.ready)return;check(service.available && service.protocols[0]==="sftp","Provider capability was not read");service.connectServer("sftp://fixture/root");phase=1;return;}
   if(phase===1){if(service.busy || service.rows.length!==2)return;check(service.current.state==="connected" && service.localPath==="","Missing FUSE path was invented");
    if(Quickshell.env("LUMEN_NETWORK_SCREENSHOT") && phase===1 && ticks<15){Quickshell.execDetached(["grim",Quickshell.env("LUMEN_NETWORK_SCREENSHOT")]);phase=10;return;}
   }
   if(phase===10){if(ticks<15)return;phase=1;}
   if(phase===1){
    service.browseId=300;service.receive({id:299,rows:[{name:"stale",uri:"bad"}],connectionId:service.selectedId});check(service.rows[0].name==="Folder","Stale browse response changed rows");
    service.browseId=0;service.connectServer("sftp://fixture/ask");phase=2;return;}
   if(phase===2){if(!service.current || service.current.state!=="authenticating")return;check(service.busy,"Authentication did not remain cancellable");service.cancel();phase=3;return;}
   if(phase===3){if(service.current && service.current.state!=="disconnected")return;check(!service.busy,"Cancel left UI busy");service.active=false;service.browseId=400;service.receive({id:400,rows:[{name:"late",uri:"bad"}]});check(service.rows.length===0,"Closed view repopulated rows");console.log("NETWORK_QT_PASS");Qt.quit();}
  }catch(error){console.error("NETWORK_QT_FAIL "+error);Qt.quit();}
 }}
}
''')
            result=subprocess.run(['dbus-run-session','--','quickshell','-p',str(qml)],env=env,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,timeout=20)
            self.assertIn('NETWORK_QT_PASS',result.stdout,result.stdout)
            for failure in ('NETWORK_QT_FAIL','Failed to load','ReferenceError:','TypeError:'):self.assertNotIn(failure,result.stdout,result.stdout)
if __name__=='__main__':unittest.main()
