"""Actual Qt JS engine validates retained stack rows and lossless source versions."""
import os,shutil,subprocess,tempfile
from pathlib import Path
root_source=Path(__file__).resolve().parents[2]/'res/lumen-desktop/shell'
with tempfile.TemporaryDirectory(prefix='lumen-stack-model-') as td:
 root=Path(td);shutil.copy(root_source/'FolderStackModel.js',root/'FolderStackModel.js')
 (root/'shell.qml').write_text('''import QtQuick
import Quickshell
import "FolderStackModel.js" as Model
ShellRoot {Timer {running:true;interval:10;onTriggered:{try {
 const facts=["1152921504606846977","1152921504606846979","33188","4","-1","1152921504606846981"];
 const a={path:"/fixture/a",actionFacts:facts},b={path:"/fixture/b",actionFacts:facts.slice()};
 const captured=Model.capture([a,b],a,2,"/fixture");
 function check(ok,message){if(!ok)throw Error(message);}
 check(captured && captured.facts[0]===facts[0],"lossless version");
 check(Model.valid(captured,[b,a],2,"/fixture",null),"same row reorder retargeted");
 check(!Model.valid(captured,[a,b],3,"/fixture",null),"reload resurrection");
 check(!Model.valid(captured,[a,b],2,"/other",null),"folder retarget");
 check(!Model.valid(captured,[{path:a.path,actionFacts:facts}],2,"/fixture",null),"replacement inherited row");
 a.actionFacts=facts.slice();a.actionFacts[1]="1152921504606846980";
 check(!Model.valid(captured,[a,b],2,"/fixture",null),"mutated facts");
 const footer=Model.captureFolder("/fixture",facts,4);
 check(Model.valid(footer,[],4,"/fixture",facts),"footer rejected");
 check(!Model.valid(footer,[],4,"/fixture",a.actionFacts),"footer source changed");
 check(Model.capture([a],b,1,"/fixture")===null,"unlisted row capture");
 console.log("PASS stack captured row/generation/folder/version, footer and uint64 strings");Qt.quit();
 }catch(e){console.error("FAIL "+e);Qt.exit(1);}}}}
''')
 env=dict(os.environ,HOME=str(root/'home'),XDG_RUNTIME_DIR=str(root/'runtime'),XDG_CONFIG_HOME=str(root/'config'),XDG_CACHE_HOME=str(root/'cache'),XDG_DATA_HOME=str(root/'data'),XDG_STATE_HOME=str(root/'state'),QT_QPA_PLATFORM='offscreen',QT_QUICK_BACKEND='software',QT_QPA_PLATFORMTHEME='generic',QT_ACCESSIBILITY='0',NO_AT_BRIDGE='1',GTK_A11Y='none')
 for name in ('HOME','XDG_RUNTIME_DIR'):Path(env[name]).mkdir(mode=0o700)
 for key in ('WAYLAND_DISPLAY','DISPLAY','SWAYSOCK','AT_SPI_BUS_ADDRESS'):env.pop(key,None)
 run=subprocess.run(['quickshell','-p',str(root)],env=env,capture_output=True,text=True,timeout=10);assert run.returncode==0 and 'PASS stack captured' in run.stdout+run.stderr,run.stdout+run.stderr;print(run.stdout+run.stderr)
