"""Actual production focus paints; private software frames, no native ownership claim."""
import argparse,hashlib,json,os,shutil,signal,subprocess as sp,sys
from pathlib import Path
from application_fixture import offscreen
from fixture_resources import close_reaped_pipes
from private_native_bus import start_private_bus
p=argparse.ArgumentParser();p.add_argument('--launcher',type=Path,required=True);p.add_argument('--font-config',type=Path,required=True);p.add_argument('--output',type=Path,required=True);p.add_argument('--inside',action='store_true');a=p.parse_args();repo=Path(__file__).resolve().parents[2]
if not a.inside:raise SystemExit(offscreen.run(repo,a.output,a.launcher,a.font_config,[sys.executable,'/source/tests/desktop/owned_focus_qml.py','--inside','--launcher',str(a.launcher.resolve()),'--font-config',str(a.font_config.resolve()),'--output','/artifacts/result']))
env=offscreen.environment();env.update(LUMEN_UNIFIED_OWNED_HEADERS='1',LUMEN_NATIVE_WINDOW_CONTROLS='0');out=a.output;out.mkdir(parents=True,exist_ok=False);shutil.copytree(repo/'res/lumen-desktop/shell',out/'desktop');palettes=json.loads((repo/'res/lumen-desktop/appearance/palette.json').read_text());qml=out/'fixture.qml'
qml.write_text('''import QtQuick
import QtQuick.Controls
import Quickshell
import "desktop" as D
ShellRoot {
 id:audit
 property var palettes:'''+json.dumps(palettes)+'''
 property var cases:[]
 property var records:[]
 property int index:0
 property int stage:0
 property bool painting:false
 property var target:null
 property var bounds:null
 FloatingWindow {id:window;visible:true;implicitWidth:360;implicitHeight:500;color:"transparent"
  Rectangle {id:body;anchors.fill:parent;color:D.Theme.colors.background
   D.WindowControls {id:controls;windowActive:true;windowName:"Private fixture"}
   D.FileInfoPanel {id:info;x:16;y:60;width:parent.width-32;height:parent.height-y-16;details:({name:"Private fixture.txt",size:"12 bytes",kind:"Text",location:"Private synthetic directory",permissions:"Read and write"})}
   FocusScope {id:neutral;focus:true}
  }
 }
 function walk(item){if(!item)return [];let found=[item];for(const child of item.children || [])found=found.concat(walk(child));if(item.contentItem && !(item.children || []).includes(item.contentItem))found=found.concat(walk(item.contentItem));return found;}
 function rect(item,padding){const point=item.mapToItem(body,0,0);return {x:point.x-padding,y:point.y-padding,width:item.width+2*padding,height:item.height+2*padding};}
 Component.onCompleted:{for(const mode of ["light","dark"])for(const opaque of [false,true])for(const control of ["window-close","info-general"])cases.push({mode:mode,opaque:opaque,control:control});}
 Timer {interval:20;repeat:true;running:true;onTriggered:{try{
  if(audit.painting)return;
  if(audit.index===audit.cases.length){console.log("OWNED_FOCUS_RESULT "+JSON.stringify(audit.records));Qt.quit();return;}
  const entry=audit.cases[audit.index];
  if(audit.stage===0){D.Theme.colors=audit.palettes[entry.mode];D.Theme.preferences={reduceMotion:true,reduceTransparency:entry.opaque};D.Theme.font=audit.palettes.font;audit.target=entry.control==="window-close"?audit.walk(controls).find(item=>item.index===0 && item.background):audit.walk(info).find(item=>item.Accessible.name==="General" && item.background);if(!audit.target)throw Error("Production focus control missing");neutral.forceActiveFocus();audit.bounds=audit.rect(audit.target,entry.control==="window-close"?5:0);audit.stage=1;return;}
  if(audit.stage===1 || audit.stage===3){const focused=audit.stage===3;if(focused && !audit.target.activeFocus)throw Error("Production focus not retained");audit.painting=true;const path="'''+str(out)+'''/"+audit.index+(focused?"-focused":"-ordinary")+".png";if(!body.grabToImage(result=>{if(!result.saveToFile(path)){console.error("OWNED_FOCUS_FAILURE save");Qt.quit();return;}if(focused){audit.records.push({entry:entry,bounds:audit.bounds,focused:true});audit.index++;audit.stage=0;}else audit.stage=2;audit.painting=false;},Qt.size(window.width,window.height)))throw Error("Frame capture rejected");return;}
  audit.target.forceActiveFocus();audit.stage=3;
 }catch(error){console.error("OWNED_FOCUS_FAILURE "+error);Qt.quit();}}}
}''')
processes=[]
try:
 start_private_bus(env,out,processes)
 with (out/'qt.log').open('w')as log:child=sp.Popen(['quickshell','-p',str(qml)],env=env,stdout=log,stderr=sp.STDOUT,start_new_session=True)
 processes.append(child);child.wait(timeout=15);text=(out/'qt.log').read_text();assert child.returncode==0 and not any(word in text for word in ('OWNED_FOCUS_FAILURE','ReferenceError','TypeError','binding loop')),text
 records=json.loads(next(line.split('OWNED_FOCUS_RESULT ',1)[1]for line in text.splitlines()if 'OWNED_FOCUS_RESULT 'in line));assert len(records)==8
 from PIL import Image
 def luminance(pixel):
  linear=[value/255/12.92 if value/255<=.04045 else ((value/255+.055)/1.055)**2.4 for value in pixel[:3]];return sum(value*weight for value,weight in zip(linear,(.2126,.7152,.0722)))
 failures=[]
 for index,row in enumerate(records):
  rect=row['bounds'];box=tuple(round(v)for v in (rect['x'],rect['y'],rect['x']+rect['width'],rect['y']+rect['height']))
  with Image.open(out/(str(index)+'-ordinary.png'))as original,Image.open(out/(str(index)+'-focused.png'))as focused:
   before=list(original.convert('RGB').crop(box).get_flattened_data());after=list(focused.convert('RGB').crop(box).get_flattened_data());contrasts=[(max(luminance(a),luminance(b))+.05)/(min(luminance(a),luminance(b))+.05)for a,b in zip(before,after)];row['changedHighContrastPixels']=sum(value>=3 for value in contrasts);row['maximumChangeContrast']=max(contrasts)
  if row['changedHighContrastPixels']<4:failures.append(row)
 (out/'result.json').write_text(json.dumps(dict(passed=not failures,cases=records,failures=failures,scope='Actual production component ordinary/focused pixel differences;3:1 engineering contrast target, no reference measurement or native ownership claim'),indent=2));assert not failures,json.dumps(failures)
finally:
 for child in reversed(processes):
  if child.poll()is None:
   os.killpg(child.pid,signal.SIGTERM)
   try:child.wait(timeout=5)
   except sp.TimeoutExpired:os.killpg(child.pid,signal.SIGKILL);child.wait(timeout=5)
 close_reaped_pipes(processes)
