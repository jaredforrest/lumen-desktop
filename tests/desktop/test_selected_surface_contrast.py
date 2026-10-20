"""Selected controls must use the resolved foreground across every accent/mode."""
import importlib.util,json,os,re,shutil,subprocess,tempfile
from pathlib import Path
repo=Path(__file__).resolve().parents[2]
spec=importlib.util.spec_from_file_location('selected_appearance',repo/'res/lumen-desktop/appearance/appearance.py')
appearance=importlib.util.module_from_spec(spec);spec.loader.exec_module(appearance)
palette=json.loads((repo/'res/lumen-desktop/appearance/palette.json').read_text())
cases=[dict(mode=mode,accent=accent,colors=appearance.resolve_palette(palette,mode,{**appearance.DEFAULTS,'accent':accent,'increaseContrast':contrast})) for mode in ('light','dark') for accent in appearance.ACCENTS for contrast in (False,True)]
launcher=os.environ.get('LUMEN_TEST_SHELL_LAUNCHER') or shutil.which('lumen-desktop-shell')
prefix=re.split(r'\n[ \t]*exec quickshell ',Path(launcher).read_text(),maxsplit=1)[0]
with tempfile.TemporaryDirectory(prefix='lumen-selection-colors-') as directory:
 env=dict(os.environ,HOME=directory,XDG_RUNTIME_DIR=directory,DBUS_SYSTEM_BUS_ADDRESS='unix:path='+directory+'/absent-system')
 for key in ('CONFIG','STATE','CACHE','DATA'):env['XDG_'+key+'_HOME']=directory+'/'+key.lower()
 for key in ('WAYLAND_DISPLAY','DISPLAY','DBUS_SESSION_BUS_ADDRESS','AT_SPI_BUS_ADDRESS','SWAYSOCK'):env.pop(key,None)
 exported=subprocess.check_output(['bash','-c',prefix+'\nexec env -0'],env=env)
 env.update(dict(value.decode().split('=',1) for value in exported.split(b'\0') if b'=' in value))
 env.update(QT_QPA_PLATFORM='offscreen',QT_QPA_PLATFORMTHEME='generic',QT_QUICK_BACKEND='software',QT_ACCESSIBILITY='0',QT_LINUX_ACCESSIBILITY_ALWAYS_ON='0',NO_AT_BRIDGE='1',GTK_A11Y='none')
 state=Path(env['XDG_STATE_HOME'])/'lumen-desktop';state.mkdir(parents=True)
 (state/'appearance.json').write_text(json.dumps(dict(palette=cases[0]['colors'],font=palette['font'],preferences=appearance.DEFAULTS)))
 (Path(directory)/'desktop').symlink_to(repo/'res/lumen-desktop/shell',target_is_directory=True)
 qml=Path(directory)/'shell.qml'
 qml.write_text('''import QtQuick
import Quickshell
import "desktop" as D
ShellRoot {
 id:root
 property var cases:'''+json.dumps(cases)+'''
 property int index:0
 D.AppPreview {id:preview;kind:"Files";listMode:true;selectedFile:0;files:[{name:"Selected fixture",directory:false}]}
 function find(node,text) {
  if(node.text===text && node.color!==undefined)return node;
  const children=node.children || [];
  for(let i=0;i<children.length;i++){const value=find(children[i],text);if(value)return value;}
  return null;
 }
 Timer {interval:30;running:true;repeat:true;onTriggered:{try {
  if(root.index>=root.cases.length){console.log("SELECTED_CONTRAST_PASS "+root.cases.length);Qt.quit();return;}
  const entry=root.cases[root.index];D.Theme.colors=entry.colors;
  preview.files=[{name:"Selected fixture",directory:false}];preview.selectedFile=0;
  const targets=[[preview,"Selected fixture"],[preview,"Document"]];
  for(const target of targets){const item=root.find(target[0],target[1]);if(!item)throw Error("Missing actual control: "+target[1]);if(String(item.color)!==String(Qt.color(entry.colors.selectedText)))throw Error(entry.mode+"/"+entry.accent+" "+target[1]+" foreground "+item.color+" expected "+entry.colors.selectedText);}
  const detail=root.find(preview,"Document"), glyph=Array.from(detail.parent.children).find(item=>item.ink!==undefined);
  if(String(detail.parent.color)!==String(Qt.color(entry.colors.accent)) || !glyph || String(glyph.ink)!==String(Qt.color(entry.colors.selectedText)))throw Error("Selected row fill/icon bypass resolved palette");
  preview.selectedFile=-1;
  if(String(detail.color)!==String(Qt.color(entry.colors.muted)))throw Error("Unselected detail retains selected foreground");
  root.index++;
 }catch(error){console.error("SELECTED_CONTRAST_FAIL "+error);Qt.quit();}}}
}''')
 result=subprocess.run(['quickshell','-p',str(qml)],env=env,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,timeout=20)
 assert result.returncode==0 and 'SELECTED_CONTRAST_PASS '+str(len(cases)) in result.stdout,result.stdout
 assert not any(value in result.stdout for value in ('SELECTED_CONTRAST_FAIL','Binding loop','ReferenceError:','TypeError:','Failed to load configuration')),result.stdout
print('PASS actual file-preview selected foregrounds in',len(cases),'mode/accent/contrast cases')
