"""Actual production column and information labels disclose unavailable links."""
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile
import unittest

ROOT=Path(__file__).resolve().parents[2]


class LinkLabelsQt(unittest.TestCase):
    def test_actual_column_and_information_components(self):
        launcher=Path(os.environ.get('LUMEN_TEST_SHELL_LAUNCHER') or shutil.which('lumen-desktop-shell'))
        prefix=re.split(r'\n[ \t]*exec quickshell ',launcher.read_text(),maxsplit=1)[0]
        with tempfile.TemporaryDirectory(prefix='lumen-link-labels-') as temporary:
            root=Path(temporary);env=dict(os.environ,HOME=temporary,XDG_RUNTIME_DIR=temporary,DBUS_SYSTEM_BUS_ADDRESS='unix:path='+temporary+'/no-system-bus',PULSE_SERVER='unix:'+temporary+'/no-pulse',PIPEWIRE_REMOTE='no-pipewire')
            for key in ('CONFIG','STATE','CACHE','DATA'):env['XDG_'+key+'_HOME']=str(root/key.lower())
            for key in ('DISPLAY','WAYLAND_DISPLAY','SWAYSOCK','DBUS_SESSION_BUS_ADDRESS','AT_SPI_BUS_ADDRESS'):env.pop(key,None)
            raw=subprocess.check_output(['bash','-c',prefix+'\nexec env -0'],env=env)
            env.update(dict(value.decode().split('=',1) for value in raw.split(b'\0') if b'=' in value))
            env.update(QT_QPA_PLATFORM='offscreen',QT_QPA_PLATFORMTHEME='generic',QT_QUICK_BACKEND='software',QT_ACCESSIBILITY='0',QT_LINUX_ACCESSIBILITY_ALWAYS_ON='0',NO_AT_BRIDGE='1',GTK_A11Y='none')
            shutil.copytree(ROOT/'res/lumen-desktop',root/'assets');qml=root/'shell.qml'
            qml.write_text('''import QtQuick
import Quickshell
import "assets/shell" as D
import "assets/shell/FileLinks.js" as Links
ShellRoot {
 property int ticks:0
 property var rows:[{name:"ordinary",path:"/fixture/ordinary",directory:false,link:false},{name:"resolved",path:"/fixture/resolved",directory:false,link:true,linkStatus:"resolved"},{name:"missing",path:"/fixture/missing",directory:false,link:true,linkStatus:"broken"},{name:"chain",path:"/fixture/chain",directory:false,link:true,linkStatus:"chain-limit"},{name:"denied",path:"/fixture/denied",directory:false,link:true,linkStatus:"permission-denied"}]
 QtObject {id:drop;property var hoverTarget:null;function registerTarget(value){}function unregisterTarget(value){}}
 FloatingWindow {visible:true;implicitWidth:900;implicitHeight:600
  D.ColumnView {id:columns;width:450;height:400;dropCoordinator:drop;columns:[{path:"/fixture",name:"Fixture",loaded:true,rows:rows}]}
  D.FileInfoPanel {id:info;x:460;width:350;height:550;details:({name:"missing",kind:"Symbolic link",target:"/fixture/old",linkDetail:"The target is missing. Use File Info to inspect the target."})}
 }
 function labels(item,out){if(item.text!==undefined)out.push(item.text);for(const child of item.children || [])labels(child,out);return out;}
 Timer {interval:40;repeat:true;running:true;onTriggered:{try {
  if(++ticks>75)throw Error("Link label readiness failed");
  const values=labels(columns,[]),expected=rows.map(row=>Links.markedLabel(row));
  if(!expected.every(value=>values.includes(value)))return;
  if(Links.accessible(rows[0])!=="ordinary" || !Links.accessible(rows[2]).includes("symbolic link") || Links.label(rows[3])!=="chain (unresolved link)" || !values.includes("↗ resolved"))throw Error("Misleading link semantics");
  const details=labels(info,[]);if(!details.includes("Link Status:") || !details.includes(info.details.linkDetail))throw Error("Missing recoverable information");
  console.log("LINK_LABELS_QT_PASS");Qt.quit();
 }catch(error){console.error("LINK_LABELS_QT_FAIL "+error);Qt.quit();}}}
}''')
            result=subprocess.run(['dbus-run-session','--','quickshell','-p',str(qml)],env=env,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,timeout=15)
            self.assertEqual(result.returncode,0,result.stdout);self.assertIn('LINK_LABELS_QT_PASS',result.stdout,result.stdout)
            for marker in ('LINK_LABELS_QT_FAIL','ReferenceError:','TypeError:','Binding loop','Failed to load configuration','Cannot assign','ResourceWarning','Exception ignored'):
                self.assertNotIn(marker,result.stdout,result.stdout)


if __name__=='__main__':unittest.main()
