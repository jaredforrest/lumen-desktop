"""Actual production calendar visibility marks in private software Qt frames."""
import argparse,hashlib,json,os,signal,subprocess as sp,sys
from pathlib import Path
from application_fixture import offscreen
from fixture_resources import close_reaped_pipes
from private_native_bus import start_private_bus
p=argparse.ArgumentParser();p.add_argument('--launcher',type=Path,required=True);p.add_argument('--font-config',type=Path,required=True);p.add_argument('--output',type=Path,required=True);p.add_argument('--inside',action='store_true');a=p.parse_args();repo=Path(__file__).resolve().parents[2]
if not a.inside:
 command=[sys.executable,'/source/tests/desktop/calendar_toggle_contrast.py','--inside','--launcher',str(a.launcher.resolve()),'--font-config',str(a.font_config.resolve()),'--output','/artifacts/result']
 raise SystemExit(offscreen.run(repo,a.output,a.launcher,a.font_config,command))
env=offscreen.environment();out=a.output;out.mkdir(parents=True,exist_ok=False);processes=[]
(out/'desktop').symlink_to(repo/'res/lumen-desktop/shell',target_is_directory=True)
palettes=json.loads((repo/'res/lumen-desktop/appearance/palette.json').read_text());state=Path(env['XDG_STATE_HOME'])/'lumen-desktop';state.mkdir(parents=True,exist_ok=True);(state/'appearance.json').write_text(json.dumps(dict(font=palettes['font'],palette=palettes['light'],preferences=dict(reduceMotion=True))))
qml=out/'calendar.qml';qml.write_text('''import QtQuick
import Quickshell
import "desktop" as D
ShellRoot {
 id:root
 property var records:[]
 property int step:0
 property int stage:0
 property bool painting:false
 property var cases:[{mode:"light",contrast:false},{mode:"light",contrast:true},{mode:"dark",contrast:false},{mode:"dark",contrast:true}]
 property var palettes:'''+json.dumps(palettes)+'''
 QtObject {id:stub;property date currentDate:new Date(2026,9,20);function claimPopup(name){}function focusOwnedWindow(name){}function openSettings(name){}}
 D.CalendarWindow {id:window;shell:stub}
 Component.onCompleted:{window.calendarState.active=false;window.visible=true;}
 function find(item,predicate){if(predicate(item))return item;for(const child of item.children || []){const found=find(child,predicate);if(found)return found;}return null;}
 function rgba(value){return {r:value.r,g:value.g,b:value.b,a:value.a};}
 function luminance(c){return [c.r,c.g,c.b].map(v=>v<=.04045?v/12.92:Math.pow((v+.055)/1.055,2.4)).reduce((s,v,i)=>s+v*[.2126,.7152,.0722][i],0);}
 function ratio(a,b){const x=luminance(a),y=luminance(b);return (Math.max(x,y)+.05)/(Math.min(x,y)+.05);}
 function rows(){const found=[];for(let i=0;i<100;i++){const name="Calendar "+i,color=window.tint({provider:"Private",calendar:name});if(!found.some(row=>row.color===String(color)))found.push({name:name,color:String(color)});if(found.length===5)break;}if(found.length!==5)throw Error("Expected all five actual calendar tints");return found;}
 function button(name){return find(window.contentItem,item=>item.checkable===true && item.modelData && item.modelData.name===name);}
 Timer {interval:30;repeat:true;running:true;onTriggered:{try {
  if(root.painting)return;
  if(root.step===root.cases.length){console.log("CALENDAR_TOGGLE_RESULT "+JSON.stringify(root.records));Qt.quit();return;}
  const entry=root.cases[root.step];
  if(root.stage===0){D.Theme.colors=root.palettes[entry.mode];D.Theme.preferences={reduceMotion:true,increaseContrast:entry.contrast};window.hiddenCalendars=[];window.calendarState.providers=[{name:"Private",calendars:root.rows().map(row=>({name:row.name}))}];root.stage=1;return;}
  const samples=[];
  for(const row of root.rows()){
   const control=root.button(row.name);if(!control)return;
   const mark=root.find(control.contentItem,item=>item.color!==undefined && item.width===13 && item.height===13);
   const glyph=mark?root.find(mark,item=>item.text==="✓"):null;if(!mark || !glyph || !control.checked)throw Error("Missing actual checked calendar mark");
   const contrast=root.ratio(glyph.color,mark.color);if(contrast<4.5)throw Error("Calendar checked mark "+entry.mode+" "+mark.color+" foreground "+glyph.color+" contrast "+contrast);
   const point=mark.mapToItem(window.contentItem,0,0);samples.push({name:row.name,fill:root.rgba(mark.color),foreground:root.rgba(glyph.color),contrast:contrast,rect:{x:point.x,y:point.y,width:mark.width,height:mark.height}});
  }
  root.painting=true;
  const paintItem=Array.from(window.contentItem.children).find(item=>item.visible && item.width===window.width && item.height===window.height);if(!paintItem)throw Error("No production QML content item");
  if(!paintItem.grabToImage(result=>{try {
   const file="'''+str(out)+'''/"+entry.mode+"-"+entry.contrast+".png";if(!result.saveToFile(file))throw Error("Actual frame capture failed");
   const first=root.button(root.rows()[0].name);first.clicked();const mark=root.find(first.contentItem,item=>item.color!==undefined && item.width===13 && item.height===13);if(first.checked || root.find(mark,item=>item.text==="✓") || mark.color.a!==0)throw Error("Unchecked calendar retained filled mark");
   root.records=root.records.concat([{mode:entry.mode,increaseContrast:entry.contrast,samples:samples,capture:file,unchecked:true}]);root.step++;root.stage=0;root.painting=false;
  }catch(error){console.error("CALENDAR_TOGGLE_FAILURE "+error);Qt.quit();}},Qt.size(window.width*2,window.height*2)))throw Error("Actual frame request rejected");
 }catch(error){console.error("CALENDAR_TOGGLE_FAILURE "+error);Qt.quit();}}}
}''')
try:
 start_private_bus(env,out,processes)
 with (out/'qt.log').open('w') as log:process=sp.Popen(['quickshell','-p',str(qml)],env=env,stdout=log,stderr=sp.STDOUT,start_new_session=True)
 processes.append(process);process.wait(timeout=20);text=(out/'qt.log').read_text();assert process.returncode==0 and 'CALENDAR_TOGGLE_FAILURE' not in text,text
 marker=next(line.split('CALENDAR_TOGGLE_RESULT ',1)[1] for line in text.splitlines() if 'CALENDAR_TOGGLE_RESULT ' in line);records=json.loads(marker);assert len(records)==4
 from PIL import Image
 for case in records:
  with Image.open(case['capture']) as source:image=source.convert('RGB')
  for index,sample in enumerate(case['samples']):
   rect=sample['rect'];crop=image.crop(tuple(round(value*2) for value in (rect['x'],rect['y'],rect['x']+rect['width'],rect['y']+rect['height'])));crop.save(out/(case['mode']+'-'+str(case['increaseContrast'])+'-'+str(index)+'-mark.png'))
   def matches(color):
    expected=[round(color[key]*255) for key in ('r','g','b')];return sum(max(abs(pixel[i]-expected[i]) for i in range(3))<=3 for pixel in crop.get_flattened_data())
   sample['paintedInk']=matches(sample['foreground']);sample['paintedFill']=matches(sample['fill']);assert sample['paintedInk']>=1 and sample['paintedFill']>=12,sample
 (out/'result.json').write_text(json.dumps(dict(passed=True,cases=records,sourceSha256=hashlib.sha256((repo/'res/lumen-desktop/shell/CalendarWindow.qml').read_bytes()).hexdigest(),scope='Actual CalendarWindow visibility marks, all five tints in light/dark with both contrast preferences; genuine Qt software frames and unchecked state, not physical-input or reference-exact typography'),indent=2))
finally:
 for process in reversed(processes):
  if process.poll() is None:
   os.killpg(process.pid,signal.SIGTERM)
   try:process.wait(timeout=5)
   except sp.TimeoutExpired:os.killpg(process.pid,signal.SIGKILL);process.wait(timeout=5)
 close_reaped_pipes(processes)
