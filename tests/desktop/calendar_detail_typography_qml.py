"""Paint the production details inspector with private read-only calendar data."""
import argparse,json,os,shutil,signal,subprocess as sp,sys,time
from pathlib import Path
from application_fixture import offscreen
from private_native_bus import start_private_bus
from fixture_resources import close_reaped_pipes
p=argparse.ArgumentParser();p.add_argument('--launcher',type=Path,required=True);p.add_argument('--font-config',type=Path,required=True);p.add_argument('--output',type=Path,required=True);p.add_argument('--mode',choices=['light','dark']);p.add_argument('--scale',type=float,choices=[1,1.15,1.3,1.5]);p.add_argument('--readonly',action='store_true');p.add_argument('--inside',action='store_true');a=p.parse_args();repo=Path(__file__).resolve().parents[2]
if not a.inside:
 a.output.mkdir(mode=0o700,parents=True,exist_ok=False);rows=[];segments=[]
 for mode in ([a.mode]if a.mode else ['light','dark']):
  for scale in ([a.scale]if a.scale else [1,1.15,1.3,1.5]):
   for readonly in [False,True]:
    target=a.output/(mode+'-'+str(scale)+'-'+('readonly'if readonly else'editable'));started=time.monotonic();code=offscreen.run(repo,target,a.launcher,a.font_config,[sys.executable,'/source/tests/desktop/calendar_detail_typography_qml.py','--inside','--mode',mode,'--scale',str(scale),*(['--readonly']if readonly else[]),'--launcher',str(a.launcher.resolve()),'--font-config',str(a.font_config.resolve()),'--output','/artifacts/result']);segments.append(dict(mode=mode,scale=scale,readonly=readonly,elapsedSeconds=time.monotonic()-started,exitCode=code));(a.output/'segments.json').write_text(json.dumps(segments,indent=2));assert code==0,(mode,scale,readonly,code);rows+=json.loads((target/'result/result.json').read_text())['cases']
 expected={(mode,scale,small,readonly,phase)for mode in ([a.mode]if a.mode else ['light','dark'])for scale in ([a.scale]if a.scale else [1,1.15,1.3,1.5])for small in [False,True]for readonly in [False,True]for phase in ['date','metadata','notes','status','action','provider']};actual=[(r['entry']['mode'],r['entry']['scale'],r['entry']['small'],r['entry']['readonly'],r['phase'])for r in rows];assert len(actual)==len(set(actual))and set(actual)==expected
 (a.output/'result.json').write_text(json.dumps(dict(passed=True,cases=rows),indent=2));raise SystemExit(0)
env=offscreen.environment();out=a.output;out.mkdir(parents=True,exist_ok=False);assets=out/'desktop';shutil.copytree(repo/'res/lumen-desktop/shell',assets);palette=json.loads((repo/'res/lumen-desktop/appearance/palette.json').read_text());state=Path(env['XDG_STATE_HOME'])/'lumen-desktop';state.mkdir(parents=True,exist_ok=True);(state/'appearance.json').write_text(json.dumps(dict(font=palette['font'],palette=palette[a.mode],preferences=dict(reduceMotion=True))))
event=dict(id='private-event',title='Private appointment',date='2026-09-28',endDate='2026-09-28',startTime='09:30',endTime='10:45',provider='Google',calendar='Private calendar',location='Room 42, north wing',notes='Private notes: bring the printed agenda and review the schedule.',editable=not a.readonly,recurring=True,accountGeneration='private-generation',calendarId='private',eventId='private-event')
payload=dict(events=[event],days=['2026-09-28'],total=1,configured=True,timezone='Europe/London',providers=[dict(id='google',name='Private provider',generation='private-generation',configured=True,calendars=[dict(id='private',name='Private calendar',accessRole='owner')])]);binpath=out/'bin';binpath.mkdir();env['PATH']=str(binpath)+':'+env['PATH'];provider=binpath/'lumen-calendar';provider.write_text('#!'+sys.executable+'\nimport json,sys\nrequest=json.loads(sys.stdin.readline())\nassert request.get("action") in ("list","load"),"Read-only fixture provider"\nprint('+repr(json.dumps(payload))+')\n');provider.chmod(0o755)
cases=[dict(mode=a.mode,scale=a.scale,small=small,readonly=readonly)for small in [False,True]for readonly in [a.readonly]];qml=out/'fixture.qml'
qml.write_text('''import QtQuick
import QtQuick.Controls
import QtQuick.Window
import Quickshell
import "desktop" as D
ShellRoot {
 id:audit
 property var cases:'''+json.dumps(cases)+'''
 property var palettes:'''+json.dumps(palette)+'''
 property var records:[]
 property var phases:["date","metadata","notes","status","action","provider"]
 property int index:0
 property int phase:0
 property int stage:0
 property bool painting:false
 property var target:null
 QtObject {id:stub;property date currentDate:new Date(2026,8,28);property int accounts:0;function claimPopup(name){}function focusOwnedWindow(name){}function openSettings(name){accounts++;}}
 D.CalendarWindow {id:owner;shell:stub}
 D.PanelLabel {id:probe;visible:false;font:target?(target.clicked?target.contentItem.font:target.font):Qt.font({family:palettes.font,pixelSize:12});text:"0123456789"}
 function check(value,message){if(!value)throw Error(message+" "+JSON.stringify(cases[index])+" phase="+phases[phase]);}
 function walk(item,seen){if(!item || seen.includes(item))return [];seen.push(item);let result=[item];for(const child of Array.from(item.children || []).concat(Array.from(item.resources || [])))result=result.concat(walk(child,seen));if(item.contentItem)result=result.concat(walk(item.contentItem,seen));return result;}
 function shown(item){while(item && item!==owner.contentItem){if(!item.visible)return false;item=item.parent;}return item===owner.contentItem;}
 function rect(item,relative){const at=item.mapToItem(relative || owner.contentItem,0,0);return {x:at.x,y:at.y,width:item.width,height:item.height};}
 function bounds(item){let r=rect(item);for(let p=item.parent;p && p!==owner.contentItem;p=p.parent)if(p.clip){const b=rect(p),x=Math.max(r.x,b.x),y=Math.max(r.y,b.y);r={x:x,y:y,width:Math.min(r.x+r.width,b.x+b.width)-x,height:Math.min(r.y+r.height,b.y+b.height)-y};}return r;}
 Timer {interval:20;repeat:true;running:true;onTriggered:{try{
  if(painting)return;
  if(index===cases.length){console.log("DETAIL_TEXT_RESULT "+JSON.stringify(records));Qt.quit();return;}
  const c=cases[index],card=owner.calendarState;
  if(stage===0){owner.visible=false;owner.inspector=false;card.editing=false;card.detail=null;D.Theme.font=palettes.font;D.Theme.colors=palettes[c.mode];D.Theme.preferences={textScale:c.scale,reduceMotion:true,reduceTransparency:true};owner.implicitWidth=c.small?480:900;owner.implicitHeight=c.small?320:700;owner.visible=true;stage=1;return;}
  if(stage===1){if(!card.loaded || card.busy || !card.events.length)return;const event=Object.assign({},card.events[0],{editable:!c.readonly});owner.eventDetails(event,new Date(2026,8,28));card.optionsOpen=true;stage=2;return;}
  const items=walk(owner.contentItem,[]).filter(item=>typeof item.mapToItem==="function" && shown(item));
  if(stage===2){
   const name=phases[phase];
   if(name==="date")target=items.find(item=>item.font && item.text===Qt.formatDateTime(card.selected,Qt.locale().dateFormat(Locale.ShortFormat)));
   if(name==="metadata")target=items.find(item=>item.font && item.text==="Google · Private calendar · Room 42, north wing");
   if(name==="notes")target=items.find(item=>item.font && item.text==='''+json.dumps(event['notes'])+''');
   if(name==="status")target=items.find(item=>item.font && item.text===(c.readonly?"Some event details require Google Calendar; use Open Google below.":"Changes apply only to this occurrence."));
   if(name==="action")target=items.find(item=>item.text==="New Event…" && item.clicked);
   if(name==="provider")target=items.find(item=>item.font && item.text==="Private provider · Waiting for first sync");
   check(target,"Actual details item missing");
   const viewport=items.find(item=>item.objectName==="calendar-inspector-content");check(viewport,"Production inspector scroll missing");
   if(name==="action")target.forceActiveFocus();else{viewport.forceActiveFocus();const at=rect(target,viewport.contentItem);viewport.contentY=Math.max(0,Math.min(viewport.contentHeight-viewport.height,at.y-8));}
   stage=3;return;
  }
  if(stage===3){
   const name=phases[phase],text=name==="action"?target.contentItem:target,base=name==="date" || name==="action"?13:name==="notes"?12:name==="provider"?10:11;
   check(text.font.pixelSize===D.Theme.textSize(base),"Essential details text ignores scale actual="+text.font.pixelSize+" expected="+D.Theme.textSize(base));check(probe.fontInfo.family===palettes.font,"Resolved family "+probe.fontInfo.family);
   const b=bounds(target);check(b.width>=target.width-1 && b.height>=target.height-1 && b.x>=0 && b.y>=0 && b.x+b.width<=owner.width+1 && b.y+b.height<=owner.height+1,"Details item clipped "+JSON.stringify(b));
   if(name==="action")check(target.activeFocus,"Details action focus lost");else check(text.contentWidth<=text.width+1 && text.contentHeight<=text.height+1,"Details text internal clipping");
   const edit=items.find(item=>item.text==="Edit Occurrence…" && item.clicked),remove=items.find(item=>item.text==="Delete…" && item.clicked);check(edit && remove && edit.enabled===!c.readonly && remove.enabled===!c.readonly,"Read-only action eligibility changed");
   const record={entry:c,phase:name,text:text.text,pixelSize:text.font.pixelSize,color:String(text.color),textBounds:rect(text),target:b,resolvedFamily:probe.fontInfo.family};const painted=Array.from(owner.contentItem.children).find(item=>item.visible && item.width===owner.width && item.height===owner.height);check(painted,"Production paint surface missing");painting=true;check(painted.grabToImage(result=>{check(result.saveToFile("'''+str(out)+'''/"+index+"-"+name+".png"),"Paint save");records.push(record);phase++;if(phase===phases.length){phase=0;index++;stage=0;}else stage=2;painting=false;},Qt.size(painted.width,painted.height)),"Actual details paint");
  }
 }catch(error){console.error("DETAIL_TEXT_FAILURE "+error);Qt.quit();}}}
}''')
processes=[]
try:
 start_private_bus(env,out,processes)
 with (out/'qt.log').open('w')as log:child=sp.Popen(['quickshell','-p',qml],env=env,stdout=log,stderr=sp.STDOUT,start_new_session=True)
 processes.append(child);child.wait(timeout=25);text=(out/'qt.log').read_text();assert child.returncode==0 and not any(word in text for word in ['DETAIL_TEXT_FAILURE','ReferenceError','TypeError','binding loop']),text
 rows=json.loads(next(line.split('DETAIL_TEXT_RESULT ',1)[1]for line in text.splitlines()if 'DETAIL_TEXT_RESULT 'in line));assert len(rows)==len(cases)*6
 from PIL import Image,ImageColor
 for i,row in enumerate(rows):
  r=row['textBounds'];box=tuple(round(v)for v in [r['x'],r['y'],r['x']+r['width'],r['y']+r['height']]);ink=ImageColor.getrgb(row['color'])
  with Image.open(out/(str(i//6)+'-'+row['phase']+'.png'))as image:row['inkPixels']=sum(max(abs(a-b)for a,b in zip(pixel,ink))<=2 for pixel in image.convert('RGB').crop(box).get_flattened_data())
  assert row['inkPixels']>=4,row
 (out/'result.json').write_text(json.dumps(dict(passed=True,cases=rows),indent=2))
finally:
 for child in reversed(processes):
  if child.poll()is None:
   os.killpg(child.pid,signal.SIGTERM)
   try:child.wait(timeout=5)
   except sp.TimeoutExpired:os.killpg(child.pid,signal.SIGKILL);child.wait(timeout=5)
 close_reaped_pipes(processes)
