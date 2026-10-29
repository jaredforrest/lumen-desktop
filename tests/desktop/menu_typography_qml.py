"""Production exported menu labels, accelerator roles and shaped row capacity."""
import argparse,hashlib,json,os,shutil,signal,subprocess as sp,sys
from pathlib import Path
from application_fixture import offscreen
from private_native_bus import start_private_bus
from fixture_resources import close_reaped_pipes
p=argparse.ArgumentParser();p.add_argument('--launcher',type=Path,required=True);p.add_argument('--font-config',type=Path,required=True);p.add_argument('--output',type=Path,required=True);p.add_argument('--inside',action='store_true');a=p.parse_args();repo=Path(__file__).resolve().parents[2]
if not a.inside:
 raise SystemExit(offscreen.run(repo,a.output,a.launcher,a.font_config,[sys.executable,'/source/tests/desktop/menu_typography_qml.py','--inside','--launcher',str(a.launcher.resolve()),'--font-config',str(a.font_config.resolve()),'--output','/artifacts/result']))
env=offscreen.environment();out=a.output;out.mkdir(parents=True,exist_ok=False);assets=out/'desktop';shutil.copytree(repo/'res/lumen-desktop/shell',assets)
palette=json.loads((repo/'res/lumen-desktop/appearance/palette.json').read_text());state=Path(env['XDG_STATE_HOME'])/'lumen-desktop';state.mkdir(parents=True,exist_ok=True);(state/'appearance.json').write_text(json.dumps(dict(font=palette['font'],palette=palette['light'],preferences=dict(reduceMotion=True))))
cases=[dict(mode=mode,scale=scale,width=width)for mode in ['light','dark']for width in [380,700]for scale in [1,1.15,1.3,1.5]]
qml=out/'fixture.qml';qml.write_text('''import QtQuick
import QtQuick.Controls
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
 property var node:({id:1,label:"File",children:[{id:2,label:"Save Document As…",shortcut:"Control+Shift+S",visible:true,enabled:true,separator:false,checked:false,toggle:"",submenu:false},{id:3,label:"Show Private Details",shortcut:"Control+Alt+I",visible:true,enabled:true,separator:false,checked:true,toggle:"check",submenu:false},{id:4,label:"Unavailable Action",shortcut:"Control+P",visible:true,enabled:false,separator:false,checked:false,toggle:"",submenu:false}]})
 // Inert host is limited to renderer state; native corpus measures real GlobalMenu widths/routing.
 QtObject {id:host;property var highlighted:({});property var path:[]
  function widthFor(node){return 350;}function capture(depth,id){return null;}function valid(intent){return false;}
  function label(value){return value;}function shortcut(value){return value.split("+").map(key=>({Control:"⌃",Shift:"⇧",Alt:"⌥"}[key] || key)).join("");}function symbolFor(value){return "";}
 }
 ApplicationWindow {id:window;visible:true;width:700;height:280;color:D.Theme.colors.background
  D.MenuCard {id:card;host:host;menuNode:audit.node;depth:0;availableHeight:window.height-24;maximumWidth:window.width-24;x:12;y:12}
 }
 function check(ok,message){if(!ok)throw Error(message+" "+JSON.stringify(cases[index]));}
 function walk(item){let all=[item];for(const child of item.children || [])all=all.concat(walk(child));return all;}
 function rect(item){const point=item.mapToItem(card,0,0);return {x:point.x,y:point.y,width:item.width,height:item.height};}
 Timer {interval:35;repeat:true;running:true;onTriggered:{try {
  if(painting)return;
  if(index===cases.length){console.log("MENU_TEXT_RESULT "+JSON.stringify(records));Qt.quit();return;}
  const c=cases[index];
  if(stage===0){D.Theme.colors=palettes[c.mode];D.Theme.preferences={textScale:c.scale,reduceMotion:true};window.width=c.width;stage=1;return;}
  const texts=walk(card).filter(item=>item.text && item.fontInfo && ["Save Document As…","⌃⇧S"].includes(item.text));check(texts.length===2,"Production label and accelerator not found");
  const label=texts.find(item=>item.text==="Save Document As…"),shortcut=texts.find(item=>item.text==="⌃⇧S");
  const receipt={case:c,label:{rect:rect(label),size:label.font.pixelSize,color:label.color.toString(),naturalWidth:label.implicitWidth,family:label.fontInfo.family,text:label.text},shortcut:{rect:rect(shortcut),size:shortcut.font.pixelSize,color:shortcut.color.toString(),naturalWidth:shortcut.implicitWidth,family:shortcut.fontInfo.family,text:shortcut.text},card:{width:card.width,height:card.height}};
  console.log("MENU_TEXT_RECEIPT "+JSON.stringify(receipt));
  const failures=[];if(label.fontInfo.family!==palettes.font || shortcut.fontInfo.family!==palettes.font)failures.push("Actual public font family");
  if(label.font.pixelSize!==Math.round(13*c.scale) || shortcut.font.pixelSize!==Math.round(12*c.scale))failures.push("Essential menu text does not follow the text preference");
  if(label.height>label.parent.height || shortcut.height>shortcut.parent.height)failures.push("Text escapes its production row");
  if(label.x+label.width>shortcut.x || label.width<=0)failures.push("Label and accelerator overlap");
  if(label.truncated || shortcut.truncated)failures.push("Bounded menu labels lost caption capacity");
  painting=true;const capture='''+json.dumps(str(out))+'''+"/menu-"+index+".png";
  check(card.grabToImage(result=>{try {check(result.saveToFile(capture),"Saved menu paint");receipt.capture=capture;records=records.concat([receipt]);check(!failures.length,failures.join("; "));painting=false;stage=0;index++;}catch(error){console.error("MENU_TEXT_FAILURE "+error);Qt.quit();}},Qt.size(card.width,card.height)),"Grab accepted");
 }catch(error){console.error("MENU_TEXT_FAILURE "+error);Qt.quit();}}}
}''')
processes=[]
try:
 start_private_bus(env,out,processes)
 with (out/'qt.log').open('w')as log:child=sp.Popen(['quickshell','-p',str(qml)],env=env,stdout=log,stderr=sp.STDOUT,start_new_session=True)
 processes.append(child);child.wait(timeout=25);text=(out/'qt.log').read_text();assert child.returncode==0 and 'MENU_TEXT_FAILURE'not in text,text
 assert not any(marker in text for marker in ('WARN','ReferenceError:','TypeError:','Binding loop')),text
 records=json.loads(next(line.split('MENU_TEXT_RESULT ',1)[1]for line in text.splitlines()if 'MENU_TEXT_RESULT 'in line));assert len(records)==16
 from PIL import Image,ImageColor
 for row in records:
  with Image.open(row['capture'])as image:
   for name in ('label','shortcut'):
    value=row[name];r=value['rect'];crop=image.convert('RGB').crop(tuple(round(v)for v in (r['x'],r['y'],r['x']+r['width'],r['y']+r['height'])));color=ImageColor.getrgb(value['color']);ink=sum(max(abs(pixel[c]-color[c])for c in range(3))<30 for pixel in crop.get_flattened_data());assert 4<=ink<crop.width*crop.height*.7,(row,name,ink);value['ink']=ink
 (out/'result.json').write_text(json.dumps(dict(passed=True,cases=records),indent=2))
finally:
 for child in reversed(processes):
  if child.poll()is None:
   os.killpg(child.pid,signal.SIGTERM)
   try:child.wait(timeout=5)
   except sp.TimeoutExpired:os.killpg(child.pid,signal.SIGKILL);child.wait(timeout=5)
 close_reaped_pipes(processes)
