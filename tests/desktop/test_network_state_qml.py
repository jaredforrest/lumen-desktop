"""Actual owned tile/details labels follow connectivity transitions without network operations."""
import os,re,shutil,subprocess,tempfile
from pathlib import Path
repo=Path(__file__).resolve().parents[2];assets=repo/'res/lumen-desktop/shell';launcher=os.environ.get('LUMEN_TEST_SHELL_LAUNCHER') or shutil.which('lumen-desktop-shell');prefix=re.split(r'\n[ \t]*exec quickshell ',Path(launcher).read_text(),maxsplit=1)[0]
with tempfile.TemporaryDirectory() as directory:
 root=Path(directory);env=dict(os.environ,HOME=directory,XDG_RUNTIME_DIR=directory,XDG_CONFIG_HOME=directory,XDG_DATA_HOME=directory,XDG_CACHE_HOME=directory,DBUS_SYSTEM_BUS_ADDRESS='unix:path='+directory+'/absent-system-bus')
 for key in ('SWAYSOCK','WAYLAND_DISPLAY','DISPLAY'):env.pop(key,None)
 qml=root/'shell.qml';qml.write_text('''import QtQuick
import Quickshell
import "file:'''+str(assets)+'''" as D
ShellRoot {
 property int step:0
 property var cases:[['disabled','unknown','Wi-Fi Off'],['disconnected','unknown','Disconnected'],['associating','unknown','Connecting…'],['acquiring','unknown','Acquiring address…'],['connected','portal','Sign-in required'],['connected','limited','Limited Internet'],['connected','none','No Internet'],['connected','full','Connected'],['connected','unknown','Internet status unknown'],['service-unavailable','unknown','Wi-Fi service unavailable']]
 QtObject {id:controls;property var wifi:({available:true,networks:[],phase:'disabled',connectivity:'unknown'});property var bluetoothRadios:({});property bool busy:false;property string error:'';function act(){throw Error('Unexpected network action')}}
 QtObject {id:vpn;property var mullvad:({state:'disconnected'});property var tailscale:({online:false})}
 QtObject {id:notifications;property bool available:false;property bool dnd:false;property var focusState:({name:''})}
 QtObject {id:stub;property var systemState:controls;property var vpn:vpn;property var notifications:notifications;property string volume:''}
 D.ControlCentre {id:centre;shell:stub;preferences:({order:['wifi'],hidden:[]})}
 D.ConnectivityPanel {id:panel;kind:'Wi-Fi';controls:controls}
 function wifiTile(item){if(item.label==='Wi-Fi')return item;for(const child of item.children || []){const found=wifiTile(child);if(found)return found;}return null;}
 function labels(item,out){if(item.text!==undefined)out.push(item.text);if(item.subtitle!==undefined)out.push(item.subtitle);for(const child of item.children || [])labels(child,out);return out;}
 Timer {interval:50;running:true;repeat:true;onTriggered:{try{
   if(step>0){const value=cases[step-1],expected=value[2],tile=labels(centre,[]),details=labels(panel,[]),control=wifiTile(centre);if(!control || control.active!==(value[0]==='connected') || !control.Accessible.name.includes(expected))throw Error('Wrong accessible state '+expected);if(!tile.includes(expected) || !details.some(text=>text===expected || text==='Synthetic · '+expected))throw Error('Wrong labels '+expected+' '+JSON.stringify(tile)+' '+JSON.stringify(details));}
   if(step===cases.length){console.log('NETWORK_STATE_PASS');Qt.quit();return;}
   const value=cases[step++];controls.wifi={available:value[0]!=='service-unavailable',networks:[],name:'Synthetic',phase:value[0],connectivity:value[1]};
 }catch(e){console.error('NETWORK_STATE_FAIL '+e);Qt.quit();}}}
}
''')
 result=subprocess.run(['dbus-run-session','--','bash','-c',prefix+'\nexport QT_QPA_PLATFORM=offscreen QT_QUICK_BACKEND=software QT_ACCESSIBILITY=0 QT_LINUX_ACCESSIBILITY_ALWAYS_ON=0 NO_AT_BRIDGE=1 GTK_A11Y=none QT_QPA_PLATFORMTHEME=generic\nunset WAYLAND_DISPLAY DISPLAY\nexec quickshell -p "$1"','fixture',str(qml)],env=env,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,timeout=25)
 if result.returncode or 'NETWORK_STATE_PASS' not in result.stdout or any(value in result.stdout for value in ('NETWORK_STATE_FAIL','ReferenceError:','TypeError:','Failed to load','Unable to assign')):raise AssertionError(result.stdout)
 print('PASS actual Qt tile and details: disabled/disconnected/association/address/portal/limited/none/full/unknown/loss')
