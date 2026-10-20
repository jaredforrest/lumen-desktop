"""Actual Overview essential roles and constrained focus-aware presentation."""
import argparse,hashlib,json,os,shutil,signal,subprocess as sp,sys
from pathlib import Path
from application_fixture import offscreen
from private_native_bus import start_private_bus
from fixture_resources import close_reaped_pipes
p=argparse.ArgumentParser();p.add_argument('--launcher',type=Path,required=True);p.add_argument('--font-config',type=Path,required=True);p.add_argument('--output',type=Path,required=True);p.add_argument('--inside',action='store_true');a=p.parse_args();repo=Path(__file__).resolve().parents[2]
if not a.inside:
 raise SystemExit(offscreen.run(repo,a.output,a.launcher,a.font_config,[sys.executable,'/source/tests/desktop/overview_typography_qml.py','--inside','--launcher',str(a.launcher.resolve()),'--font-config',str(a.font_config.resolve()),'--output','/artifacts/result']))
env=offscreen.environment();out=a.output;out.mkdir(parents=True,exist_ok=False);assets=out/'desktop';shutil.copytree(repo/'res/lumen-desktop/shell',assets)
title='A private document title with several words';data=dict(windows=[dict(id=101,pid=11,title=title,app='Private application',group='private',application='private',workspace='1',workspaceLabel='Private desktop')],workspaces=[dict(logicalId='desktop1',name='1',label='Private desktop',outputId='display1',available=True,focused=True)],outputs=[dict(logicalId='display1',name='HEADLESS-1',label='Private display')],revision=1,sessionEpoch='private',autoReorder=True,assignments={},windowGroups=dict(enabled=True,locked=False,separate_displays=True,revision=1,groups=[dict(id=1,members=[dict(id=101,pid=11)],selected=True,suspended=False,workspace='1')]))
backend=assets/'window_overview.py';before=backend.read_bytes();backend.write_text('import json,sys\nr=json.loads(sys.stdin.readline());assert r["action"]=="status"\nprint('+repr(json.dumps(data))+')\n');(out/'provider-seam.json').write_text(json.dumps(dict(beforeSha256=hashlib.sha256(before).hexdigest(),afterSha256=hashlib.sha256(backend.read_bytes()).hexdigest(),scope='Synthetic read-only status; all mutations refuse')))
palette=json.loads((repo/'res/lumen-desktop/appearance/palette.json').read_text());state=Path(env['XDG_STATE_HOME'])/'lumen-desktop';state.mkdir(parents=True,exist_ok=True);(state/'appearance.json').write_text(json.dumps(dict(font=palette['font'],palette=palette['light'],preferences=dict(reduceMotion=True))))
cases=[dict(mode=mode,scale=scale,width=width,height=480 if width==560 else 560)for mode in ['light','dark']for width in [560,880]for scale in [1,1.15,1.3,1.5]]
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
 property var titleCapacity:({})
 property int phase:0
 QtObject {id:stub;property var groupMembershipDrag:null;function claimPopup(name){}}
 D.WindowOverview {id:overview;shell:stub}
 function check(value,message){if(!value)throw Error(message+" "+JSON.stringify(cases[index]));}
 function walk(item,seen){seen=seen || [];if(!item || seen.includes(item))return [];seen.push(item);let result=[item],children=Array.from(item.children || []).concat(Array.from(item.data || []));if(item.contentItem)children.push(item.contentItem);for(const child of children)result=result.concat(walk(child,seen));return result;}
 function shown(item){if(!(item instanceof Item))return false;const same=item.Window.window===overview.contentItem.Window.window;while(item && item!==overview.contentItem){if(!item.visible)return false;item=item.parent;}return item===overview.contentItem || same;}
 function rect(item,relative){const p=item.mapToItem(relative || overview.contentItem,0,0);return {x:p.x,y:p.y,width:item.width,height:item.height};}
 Component.onCompleted:overview.show("")
 Timer {interval:20;repeat:true;running:true;onTriggered:{try{
  if(audit.painting || overview.busy || overview.windows.length!==1)return;
  if(audit.index===audit.cases.length){console.log("OVERVIEW_TEXT_RESULT "+JSON.stringify(audit.records));Qt.quit();return;}
  const entry=audit.cases[audit.index];
  if(audit.stage===0){D.Theme.colors=audit.palettes[entry.mode];D.Theme.preferences={textScale:entry.scale,reduceMotion:true};overview.implicitWidth=entry.width;overview.implicitHeight=entry.height;audit.stage=1;return;}
  if(audit.stage===1){overview.contentItem.Window.window.width=entry.width;overview.contentItem.Window.window.height=entry.height;audit.stage=2;return;}
  if(overview.width!==entry.width || overview.height!==entry.height)return;
  if(entry.scale===1 && audit.phase===0)console.log("OVERVIEW_BASE_METRICS "+JSON.stringify(audit.walk(overview.contentItem).filter(child=>child.placeholderText!==undefined || child.text!==undefined && child.implicitHeight!==undefined).map(child=>({text:child.text,placeholder:child.placeholderText,font:child.font?child.font.pixelSize:null,height:child.height,implicitHeight:child.implicitHeight,width:child.width,implicitWidth:child.implicitWidth}))));
  if(audit.stage===2){const card=audit.walk(overview.contentItem).find(child=>child instanceof Item && child.Accessible && child.Accessible.name.startsWith('A private document title with several words'+" · "));audit.check(card,"Real window card");card.forceActiveFocus();audit.stage=3;return;}
  if(audit.stage===4){const action=audit.walk(overview.contentItem).find(child=>child instanceof Item && child.Accessible && child.Accessible.name==="Assign Application");audit.check(action,"Real assignment action");action.forceActiveFocus();audit.stage=5;return;}
  if(audit.stage===6){const groups=audit.walk(overview.contentItem).find(child=>child.objectName==="window-groups-open");audit.check(groups && groups.enabled,"Real group dialog route");groups.clicked();audit.stage=7;return;}
  if(audit.stage===7){const done=audit.walk(overview.contentItem).find(child=>child instanceof Item && child.Accessible && child.Accessible.name==="Close window groups" && audit.shown(child));audit.check(done,"Opened real dialog content must be observable: "+JSON.stringify(audit.walk(overview.contentItem).filter(child=>child instanceof Popup).map(child=>({type:String(child),visible:child.visible,content:child.contentItem?String(child.contentItem):""}))));done.forceActiveFocus();audit.stage=8;return;}
  const labels=audit.walk(overview.contentItem).filter(child=>audit.shown(child) && child.font && child.text),title=labels.find(child=>child.text==='''+json.dumps(title)+'''),metadata=labels.find(child=>child.text==="Private desktop" && child.font.pixelSize===Math.round(11*entry.scale));audit.check(title,"Actual title");audit.check(title.fontInfo.family===audit.palettes.font,"Public family");audit.check(title.font.pixelSize===Math.round(12*entry.scale),"Title label role");audit.check(metadata,"Desktop metadata role");audit.check(title.height+0.01>=title.implicitHeight,"Title lines fit");const key=entry.mode+entry.width;if(audit.phase===0){if(entry.scale===1)audit.titleCapacity[key]=title.width/title.implicitWidth;else audit.check((title.width+1)/title.implicitWidth>=audit.titleCapacity[key],"Title capacity");}const r=audit.rect(title),m=audit.rect(metadata);audit.check(r.y+r.height<=m.y+1,"Title and desktop do not overlap");
  const focus=overview.contentItem.Window.window.activeFocusItem;audit.check(focus,"Actual focused control");let target=focus;while(target && !target.Accessible.name)target=target.parent;audit.check(target,"Named focused control");const focused=audit.rect(target);audit.check(focused.x>=-1 && focused.y>=-1 && focused.x+focused.width<=overview.width+1 && focused.y+focused.height<=overview.height+1,"Focused control inside viewport");for(let parent=target.parent;parent && parent!==overview.contentItem;parent=parent.parent)if(parent.clip){const clip=audit.rect(parent);audit.check(focused.y>=clip.y-1 && focused.y+focused.height<=clip.y+clip.height+1,"Focused control revealed through clip");}
  if(audit.phase===2){const description=labels.find(child=>child.text.startsWith("Groups keep window placement intact."));audit.check(description && description.font.pixelSize===Math.round(11*entry.scale),"Dialog helper metadata role");audit.check(description.height>=description.implicitHeight,"Dialog helper wraps");}
  const paintedLabel=audit.phase===0?title:audit.walk(target).find(child=>child.text && child.font && child.color && audit.shown(child));audit.check(paintedLabel,"Actual focused action text");
  const paint=audit.phase===2?target.parent:Array.from(overview.contentItem.children).find(child=>child.visible && child.width===overview.width && child.height===overview.height);audit.check(paint,"Production paint root");audit.painting=true;
  audit.check(paint.grabToImage(result=>{try{const path="'''+str(out)+'''/"+audit.index+"-"+audit.phase+".png";audit.check(result.saveToFile(path),"Capture");audit.records=audit.records.concat([{entry:entry,title:r,color:title.color.toString(),font:title.font.pixelSize,metadata:m,capture:path,paintedText:audit.rect(paintedLabel,paint),paintedColor:paintedLabel.color.toString(),phase:audit.phase,focused:focused,focusName:target.Accessible.name}]);if(audit.phase===0){audit.phase=1;audit.stage=4;}else if(audit.phase===1){audit.phase=2;audit.stage=6;}else{const done=audit.walk(overview.contentItem).find(child=>child instanceof Item && child.Accessible && child.Accessible.name==="Close window groups" && audit.shown(child));done.clicked();audit.phase=0;audit.index++;audit.stage=0;}audit.painting=false;}catch(error){console.error("OVERVIEW_TEXT_FAILURE "+error);Qt.quit();}},Qt.size(paint.width,paint.height)),"Grab accepted");
 }catch(error){console.error("OVERVIEW_TEXT_FAILURE "+error);Qt.quit();}}}
}''')
processes=[]
try:
 start_private_bus(env,out,processes)
 with (out/'qt.log').open('w')as log:child=sp.Popen(['quickshell','-p',str(qml)],env=env,stdout=log,stderr=sp.STDOUT,start_new_session=True)
 processes.append(child);child.wait(timeout=25);text=(out/'qt.log').read_text();assert child.returncode==0 and 'OVERVIEW_TEXT_FAILURE'not in text,text
 records=json.loads(next(line.split('OVERVIEW_TEXT_RESULT ',1)[1]for line in text.splitlines()if 'OVERVIEW_TEXT_RESULT 'in line));assert len(records)==48
 from PIL import Image
 for row in records:
  with Image.open(row['capture'])as image:
   r=row['paintedText'];crop=image.convert('RGB').crop(tuple(round(v)for v in [r['x'],r['y'],r['x']+r['width'],r['y']+r['height']]));color=tuple(int(row['paintedColor'][i:i+2],16)for i in [1,3,5]);ink=sum(max(abs(a-b)for a,b in zip(pixel,color))<=32 for pixel in crop.get_flattened_data());
   assert 4<=ink<crop.width*crop.height*.6,row
   row['ink']=ink
 (out/'result.json').write_text(json.dumps(dict(passed=True,cases=records),indent=2))
finally:
 for child in reversed(processes):
  if child.poll()is None:
   os.killpg(child.pid,signal.SIGTERM)
   try:child.wait(timeout=5)
   except sp.TimeoutExpired:os.killpg(child.pid,signal.SIGKILL);child.wait(timeout=5)
 close_reaped_pipes(processes)
