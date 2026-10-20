"""Production Files tab captions, constrained layout and identity-preserving models."""
import argparse,json,os,shutil,signal,subprocess as sp,sys
from pathlib import Path
from application_fixture import offscreen
from private_native_bus import start_private_bus
from fixture_resources import close_reaped_pipes
p=argparse.ArgumentParser();p.add_argument('--launcher',type=Path,required=True);p.add_argument('--font-config',type=Path,required=True);p.add_argument('--output',type=Path,required=True);p.add_argument('--inside',action='store_true');a=p.parse_args();repo=Path(__file__).resolve().parents[2]
if not a.inside:
 raise SystemExit(offscreen.run(repo,a.output,a.launcher,a.font_config,[sys.executable,'/source/tests/desktop/document_tabs_typography.py','--inside','--launcher',str(a.launcher.resolve()),'--font-config',str(a.font_config.resolve()),'--output','/artifacts/result']))
env=offscreen.environment();out=a.output;out.mkdir(parents=True,exist_ok=False);assets=out/'desktop';shutil.copytree(repo/'res/lumen-desktop/shell',assets);backend=assets/'files.py';before=backend.read_text();old="    if action=='volumes':\n";assert before.count(old)==1;backend.write_text(before.replace(old,old+'        return dict(volumes=[])\n'))
folder=Path('/home/fixture/Documents');folder.mkdir();(folder/'Retained selection.txt').write_text('Disposable private item')
palette=json.loads((repo/'res/lumen-desktop/appearance/palette.json').read_text());state=Path(env['XDG_STATE_HOME'])/'lumen-desktop';state.mkdir(parents=True,exist_ok=True);(state/'appearance.json').write_text(json.dumps(dict(font=palette['font'],palette=palette['light'],preferences=dict(reduceMotion=True))))
cases=[dict(mode=mode,scale=scale,width=width,height=360 if width==480 else 720)for mode in ['light','dark']for width in [480,1060]for scale in [1,1.15,1.3,1.5]]
qml=out/'fixture.qml';qml.write_text('''import QtQuick
import QtQuick.Window
import Quickshell
import "desktop" as D
ShellRoot {
 id:audit
 property var cases:'''+json.dumps(cases)+'''
 property var palettes:'''+json.dumps(palette)+'''
 property int index:0
 property int stage:0
 property bool painting:false
 property var records:[]
 property var baselines:({})
 QtObject {id:exported;property var state:({})}
 QtObject {id:stub;property int focusEpoch:0;property var windows:[];property var exportedMenu:exported;function activeFiles(window){}function focusWindow(window){}}
 D.FilesState {id:service;persistViewPreferences:false;path:"/home/fixture/Documents";viewMode:"list"}
 D.FilesWindow {id:files;shell:stub;service:service}
 function check(value,message){if(!value)throw Error(message+" "+JSON.stringify(cases[index]));}
 function walk(item){let result=[item];for(const child of item.children || [])result=result.concat(walk(child));return result;}
 function item(name){return walk(files.contentItem).find(child=>child.objectName===name);}
 function rect(item){const p=item.mapToItem(files.contentItem,0,0);return {x:p.x,y:p.y,width:item.width,height:item.height};}
 Component.onCompleted:service.navigate("/home/fixture/Documents")
 Timer {interval:20;repeat:true;running:true;onTriggered:{try{
  if(audit.painting || !service.ready || service.busy)return;
  if(audit.index===audit.cases.length){console.log("TABS_RESULT "+JSON.stringify(audit.records));Qt.quit();return;}
  const entry=audit.cases[audit.index];
  if(audit.stage===0){D.Theme.colors=audit.palettes[entry.mode];D.Theme.preferences={textScale:entry.scale,reduceMotion:true};files.visible=true;files.implicitWidth=entry.width;files.implicitHeight=entry.height;if(files.tabs.length===1){files.newTab();files.newTab();}audit.stage=1;return;}
  if(audit.stage===1){files.contentItem.Window.window.width=entry.width;files.contentItem.Window.window.height=entry.height;audit.stage=2;return;}
  if(files.width!==entry.width || files.height!==entry.height)return;
  if(audit.stage===2){const active=files.activeTabId;files.reorderTab(active,0);audit.check(files.activeTabId===active,"Reorder preserves active identity");files.closeTabId(files.tabs[files.tabs.length-1].id);audit.check(files.activeTabId===active,"Close sibling preserves active identity");files.newTab();files.switchTab(files.tabs.findIndex(row=>row.id===active));audit.stage=3;return;}
  const strip=audit.item("files-tabs"),tab=audit.item("document-tab-"+files.activeTabId),label=audit.walk(tab).find(child=>child.visible && child.text==="Documents"),close=audit.item("document-tab-close-"+files.activeTabId);audit.check(label,"Actual tab caption");audit.check(label.fontInfo.family===audit.palettes.font,"Public family");audit.check(label.font.pixelSize===Math.round(12*entry.scale),"Caption scales actual="+label.font.pixelSize+" policy="+D.Theme.textScale);audit.check(tab.height>=label.implicitHeight,"Caption height");const key=entry.mode+entry.width,capacity=label.width/label.implicitWidth;if(entry.scale===1)audit.baselines[key]=capacity;else audit.check((label.width+1)/label.implicitWidth>=audit.baselines[key],"Caption capacity preserved");audit.check(close.height<=tab.height && strip.height>=tab.height,"Close and tab fit band");const filter=audit.walk(files.contentItem).find(child=>child.placeholderText==="Filter this folder");if(filter)audit.check(audit.rect(filter).y>=audit.rect(strip).y+strip.height,"Filter below tab band");
  const at=audit.rect(label),paint=Array.from(files.contentItem.children).find(child=>child.visible && child.width===files.width && child.height===files.height);audit.check(paint,"Production paint root");audit.painting=true;
  audit.check(paint.grabToImage(result=>{try{const path="'''+str(out)+'''/"+audit.index+".png";audit.check(result.saveToFile(path),"Paint capture");audit.records=audit.records.concat([{entry:entry,label:at,color:label.color.toString(),pixelSize:label.font.pixelSize,resolvedFamily:label.fontInfo.family,strip:audit.rect(strip),tabHeight:tab.height,capacity:capacity,capture:path,activeIdentity:files.activeTabId}]);audit.index++;audit.stage=0;audit.painting=false;}catch(error){console.error("TABS_FAILURE "+error);Qt.quit();}},Qt.size(files.width,files.height)),"Grab accepted");
 }catch(error){console.error("TABS_FAILURE "+error);Qt.quit();}}}
}''')
processes=[]
try:
 start_private_bus(env,out,processes)
 with (out/'qt.log').open('w')as log:child=sp.Popen(['quickshell','-p',str(qml)],env=env,stdout=log,stderr=sp.STDOUT,start_new_session=True)
 processes.append(child);child.wait(timeout=25);text=(out/'qt.log').read_text();assert child.returncode==0 and 'TABS_FAILURE'not in text,text
 records=json.loads(next(line.split('TABS_RESULT ',1)[1]for line in text.splitlines()if 'TABS_RESULT 'in line));assert len(records)==16
 from PIL import Image
 for row in records:
  with Image.open(row['capture'])as image:
   r=row['label'];crop=image.convert('RGB').crop(tuple(round(v)for v in [r['x'],r['y'],r['x']+r['width'],r['y']+r['height']]));color=tuple(int(row['color'][i:i+2],16)for i in [1,3,5]);ink=sum(max(abs(a-b)for a,b in zip(pixel,color))<=32 for pixel in crop.get_flattened_data());assert 4<=ink<crop.width*crop.height*.6,row;row['ink']=ink
 (out/'result.json').write_text(json.dumps(dict(passed=True,cases=records,scope='Production Files/tab role, paint and model calls; no physical-input claim'),indent=2))
finally:
 for child in reversed(processes):
  if child.poll()is None:
   os.killpg(child.pid,signal.SIGTERM)
   try:child.wait(timeout=5)
   except sp.TimeoutExpired:os.killpg(child.pid,signal.SIGKILL);child.wait(timeout=5)
 close_reaped_pipes(processes)
