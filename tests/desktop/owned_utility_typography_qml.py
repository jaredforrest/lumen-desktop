"""Finite production utility text-role/reflow paints in private software Qt."""
import argparse,json,os,shutil,signal,subprocess as sp,sys
from pathlib import Path
from application_fixture import offscreen
from private_native_bus import start_private_bus
from fixture_resources import close_reaped_pipes
p=argparse.ArgumentParser();p.add_argument('--launcher',type=Path,required=True);p.add_argument('--font-config',type=Path,required=True);p.add_argument('--output',type=Path,required=True);p.add_argument('--consumer',choices=['TextTools','Remote','Info']);p.add_argument('--mode',choices=['light','dark']);p.add_argument('--inside',action='store_true');a=p.parse_args();repo=Path(__file__).resolve().parents[2]
if not a.inside:
 a.output.mkdir(parents=True,exist_ok=False);records=[]
 for consumer in ([a.consumer]if a.consumer else ['TextTools','Remote','Info']):
  for mode in ([a.mode]if a.mode else ['light','dark']):
   target=a.output/(consumer+'-'+mode);code=offscreen.run(repo,target,a.launcher,a.font_config,[sys.executable,'/source/tests/desktop/owned_utility_typography_qml.py','--inside','--consumer',consumer,'--mode',mode,'--launcher',str(a.launcher.resolve()),'--font-config',str(a.font_config.resolve()),'--output','/artifacts/result']);assert code==0,(consumer,mode,code);records+=json.loads((target/'result/result.json').read_text())['cases']
 (a.output/'result.json').write_text(json.dumps(dict(passed=True,cases=records),indent=2));raise SystemExit(0)
env=offscreen.environment();env.update(LUMEN_UNIFIED_OWNED_HEADERS='1',LUMEN_NATIVE_WINDOW_CONTROLS='0');out=a.output;out.mkdir(parents=True,exist_ok=False);shutil.copytree(repo/'res/lumen-desktop/shell',out/'desktop');palette=json.loads((repo/'res/lumen-desktop/appearance/palette.json').read_text());state=Path(env['XDG_STATE_HOME'])/'lumen-desktop';state.mkdir(parents=True,exist_ok=True);(state/'appearance.json').write_text(json.dumps(dict(font=palette['font'],palette=palette['light'],preferences=dict(reduceMotion=True))));views=['Dictionary','Dictation','Shortcuts']if a.consumer=='TextTools'else['Connections']if a.consumer=='Remote'else['Information'];cases=[dict(consumer=a.consumer,mode=a.mode,scale=scale,small=small,view=view)for scale in (1,1.15,1.3,1.5)for small in (False,True)for view in views];qml=out/'fixture.qml'
qml.write_text('''import QtQuick
import QtQuick.Controls
import QtQuick.Window
import Quickshell
import "desktop" as D
ShellRoot {
 id:audit
 property var cases:'''+json.dumps(cases)+'''
 property var palette:'''+json.dumps(palette)+'''
 property var records:[]
 property int index:0
 property int stage:0
 property bool painting:false
 property var owner:null
 property var target:null
 QtObject {id:stub;function claimPopup(name){}property var exportedMenu:({state:{windowId:0,appName:"Synthetic application",menu:null}})}
 D.TextToolsWindow {id:tools;shell:stub;definition:"A synthetic private definition with accented words: café, naïve."}
 D.RemoteDesktopWindow {id:remote;shell:stub;connections:[{id:"private",name:"Private computer",protocol:"RDP",host:"192.0.2.1",printer:true}]}
 D.FileInfoWindow {id:info;details:({name:"Private synthetic document.txt",size:"120 bytes",kind:"Text document",location:"/private/synthetic/directory",modified:"20 October 2026 12:34",owner:"Synthetic owner",group:"Synthetic group",permissions:"Read and write"})}

 function check(value,message){if(!value)throw Error(message+" "+JSON.stringify(cases[index]));}
 function walk(item,seen){if(!item || seen.includes(item))return [];seen.push(item);let result=[item];for(const child of item.children || [])result=result.concat(walk(child,seen));if(item.contentItem)result=result.concat(walk(item.contentItem,seen));return result;}
 function shown(item){while(item && item!==owner.contentItem){if(!item.visible)return false;item=item.parent;}return item===owner.contentItem;}
 function rect(item){const p=item.mapToItem(owner.contentItem,0,0);return {x:p.x,y:p.y,width:item.width,height:item.height};}
 function visibleBounds(item){let r=rect(item);for(let parent=item.parent;parent && parent!==owner.contentItem;parent=parent.parent){if(parent.clip){const p=rect(parent),x=Math.max(r.x,p.x),y=Math.max(r.y,p.y);r={x:x,y:y,width:Math.min(r.x+r.width,p.x+p.width)-x,height:Math.min(r.y+r.height,p.y+p.height)-y};}}return r;}
 Timer {interval:20;repeat:true;running:true;onTriggered:{try{
  if(audit.painting)return;
  if(audit.index===audit.cases.length){console.log("UTILITY_TYPOGRAPHY_RESULT "+JSON.stringify(audit.records));Qt.quit();return;}
  const c=audit.cases[audit.index];
  if(audit.stage===0){tools.visible=false;remote.visible=false;info.visible=false;D.Theme.font=audit.palette.font;D.Theme.colors=audit.palette[c.mode];D.Theme.preferences={textScale:c.scale,reduceMotion:true,reduceTransparency:true};audit.owner=c.consumer==="TextTools"?tools:c.consumer==="Remote"?remote:info;owner.implicitWidth=c.consumer==="TextTools"?(c.small?520:780):c.consumer==="Remote"?(c.small?480:780):(c.small?360:600);owner.implicitHeight=c.small?320:570;if(c.consumer==="TextTools")tools.page=c.view;owner.visible=true;audit.stage=1;return;}
  const items=audit.walk(owner.contentItem,[]);
  if(audit.stage===1){if(c.consumer==="TextTools"){const input=items.find(item=>item.Accessible.name==="Text to process");if(input)input.text="Synthetic private text";}const name=c.consumer==="TextTools"?(c.view==="Shortcuts"?"Menu command path":c.view==="Dictation"?"Insert Transcript":"Text to process"):c.consumer==="Remote"?"Remote computer address":"Sharing & Permissions";audit.target=items.find(item=>item.Accessible.name===name && shown(item));check(target,"Essential production control missing");target.forceActiveFocus();audit.stage=2;return;}
  if(audit.stage===2){check(target.activeFocus,"Essential control focus");const b=visibleBounds(target);check(b.width>=target.width-1 && b.height>=target.height-1,"Essential control unreachable/clipped "+JSON.stringify(b));check(b.x>=0 && b.y>=0 && b.x+b.width<=owner.width+1 && b.y+b.height<=owner.height+1,"Essential control outside window");const texts=items.filter(item=>item.font && item.text && shown(item) && item.text!=="▾" && item.text!=="▸");const roles=c.consumer==="Info"?{"120 bytes":11,"General":12,"Sharing & Permissions":12}:c.consumer==="Remote"?{"Connections":11,"Private computer":12,"RDP · Printing enabled":10}:{[c.view]:18};for(const key of Object.keys(roles))check(texts.some(value=>value.text===key && value.font.pixelSize===D.Theme.textSize(roles[key])),"Missing scaled role "+key);if(["Text to process","Menu command path","Remote computer address"].includes(target.Accessible.name))check(target.font.pixelSize===D.Theme.textSize(13),"Input text must scale");if(c.consumer==="Remote"){for(const key of ["Private computer","RDP · Printing enabled"]){const item=texts.find(item=>item.text===key);check(item,"Connection identity text");const b=visibleBounds(item);check(b.width>=item.width-1 && b.height>=item.height-1,"Clipped connection identity "+key);}}const label=items.find(item=>item.font && item.text && shown(item) && visibleBounds(item).height>=item.height-1 && item.color && item.font.pixelSize>=D.Theme.textRole("body"));check(label,"Actual readable text missing");check(label.fontInfo.family===audit.palette.font,"Unexpected resolved font "+label.fontInfo.family);const r=rect(label);audit.painting=true;const record={entry:c,focus:target.Accessible.name,target:b,text:label.text,textBounds:r,color:String(label.color),pixelSize:label.font.pixelSize,resolvedFamily:label.fontInfo.family};const surface=Array.from(owner.contentItem.children).find(item=>item.width===owner.width && item.height===owner.height);check(surface,"Production painted surface");check(surface.grabToImage(result=>{if(!result.saveToFile("'''+str(out)+'''/"+audit.index+".png")){console.error("UTILITY_TYPOGRAPHY_FAILURE save");Qt.quit();return;}audit.records.push(record);audit.index++;audit.stage=0;audit.painting=false;},Qt.size(owner.width,owner.height)),"Production paint capture");}
 }catch(error){console.error("UTILITY_TYPOGRAPHY_FAILURE "+error);Qt.quit();}}}
}''')
processes=[]
try:
 start_private_bus(env,out,processes)
 with (out/'qt.log').open('w')as log:child=sp.Popen(['quickshell','-p',qml],env=env,stdout=log,stderr=sp.STDOUT,start_new_session=True)
 processes.append(child);child.wait(timeout=25);text=(out/'qt.log').read_text();assert child.returncode==0 and not any(word in text for word in ('UTILITY_TYPOGRAPHY_FAILURE','ReferenceError','TypeError','binding loop')),text
 records=json.loads(next(line.split('UTILITY_TYPOGRAPHY_RESULT ',1)[1]for line in text.splitlines()if 'UTILITY_TYPOGRAPHY_RESULT 'in line));assert len(records)==len(cases)
 from PIL import Image,ImageColor
 for i,row in enumerate(records):
  r=row['textBounds'];box=tuple(round(v)for v in (r['x'],r['y'],r['x']+r['width'],r['y']+r['height']));ink=ImageColor.getrgb(row['color'])
  with Image.open(out/(str(i)+'.png'))as image:row['inkPixels']=sum(max(abs(a-b)for a,b in zip(pixel,ink))<=2 for pixel in image.convert('RGB').crop(box).get_flattened_data())
  assert row['inkPixels']>=4,row
 (out/'result.json').write_text(json.dumps(dict(passed=True,cases=records),indent=2))
finally:
 for child in reversed(processes):
  if child.poll()is None:
   os.killpg(child.pid,signal.SIGTERM)
   try:child.wait(timeout=5)
   except sp.TimeoutExpired:os.killpg(child.pid,signal.SIGKILL);child.wait(timeout=5)
 close_reaped_pipes(processes)
