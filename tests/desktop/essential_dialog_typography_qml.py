"""Actual constrained dialog/settings text roles with private software presentation."""
import argparse,hashlib,json,os,re,shutil,signal,subprocess as sp,sys
from pathlib import Path
from application_fixture import offscreen
from private_native_bus import start_private_bus
from fixture_resources import close_reaped_pipes
p=argparse.ArgumentParser()
for name in ('launcher','font-config','output'):p.add_argument('--'+name,type=Path,required=True)
p.add_argument('--dbus-daemon',type=Path,required=True);p.add_argument('--inside',action='store_true');a=p.parse_args();repo=Path(__file__).resolve().parents[2]
if not a.inside:
 raise SystemExit(offscreen.run(repo,a.output,a.launcher,a.font_config,[sys.executable,'/source/tests/desktop/essential_dialog_typography_qml.py','--inside','--launcher',str(a.launcher.resolve()),'--font-config',str(a.font_config.resolve()),'--output','/artifacts/result','--dbus-daemon',str(offscreen.immutable_file(a.dbus_daemon))]))
env=offscreen.environment();env['PATH']=str(a.dbus_daemon.parent)+':'+env['PATH'];out=a.output;out.mkdir(parents=True,exist_ok=False);assets=out/'desktop';shutil.copytree(repo/'res/lumen-desktop/shell',assets)
palette=json.loads((repo/'res/lumen-desktop/appearance/palette.json').read_text());state=Path(env['XDG_STATE_HOME'])/'lumen-desktop';state.mkdir(parents=True,exist_ok=True);(state/'appearance.json').write_text(json.dumps(dict(font=palette['font'],palette=palette['light'],preferences=dict(reduceMotion=True))))
seams=[]
# Offscreen Qt has no layer-shell backend. Replace only the transport window type
# and its layer protocol properties; all production body/control/action code stays.
for filename in ('ForceQuitDialog.qml','WindowPairPicker.qml'):
 helper=assets/filename;before=helper.read_text();after=before.replace('PanelWindow {','Window {',1);after='import QtQuick.Window\n'+after
 after='\n'.join(line for line in after.split('\n') if not line.strip().startswith(('anchors {top:true;bottom:true;left:true;right:true}','exclusionMode:','WlrLayershell.')))
 helper.write_text(after);seams.append(dict(path=filename,beforeSha256=hashlib.sha256(before.encode()).hexdigest(),afterSha256=hashlib.sha256(after.encode()).hexdigest(),scope='Offscreen transport adapter only; native layer-shell remains outside this proof'))
for filename,payload in [('display_settings.py',dict(outputs=[dict(name='HEADLESS-1',label='Private display',mode='1280x720@60',modes=['1280x720@60'],scale=1,x=0,y=0)],pending='',seconds=0)),('window_pair.py',dict(source=None,candidates=[]))]:
 helper=assets/filename;before=helper.read_bytes();helper.write_text('import json,sys\nrequest=json.loads(sys.stdin.readline());assert request["action"]=="status","Mutation refused"\nprint('+repr(json.dumps(payload))+')\n');seams.append(dict(path=filename,beforeSha256=hashlib.sha256(before).hexdigest(),afterSha256=hashlib.sha256(helper.read_bytes()).hexdigest()))
(out/'provider-seams.json').write_text(json.dumps(dict(scope='Synthetic read-only providers; every mutation refused',seams=seams),indent=2))
cases=[dict(consumer=consumer,mode=mode,scale=scale,width=width,height=height)for consumer in ['Force','Pair','Displays']for mode in ['light','dark']for scale in [1,1.15,1.3,1.5]for width,height in ([(340,480),(700,800)]if consumer=='Displays'else[(700,480),(1100,800)])]
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
 property var records:[]
 property bool painting:false
 property var owner:null
 property var surface:null
 property var labels:[]
 QtObject {id:stub;property string popupOwner:"";signal dismissPopups(string owner);function preferredOutput(){return "";}function claimPopup(name){popupOwner=name;}}
 D.ForceQuitDialog {id:force;shell:stub;applicationName:"Private document application with a long name";phase:"error";error:"The captured application changed while preparing the request. Close this decision and select the intended application again. No application has been terminated."}
 D.WindowPairPicker {id:pair;shell:stub;error:"The selected partner is no longer available. Choose another window to continue."}
 Window {id:displayWindow;visible:false;color:D.Theme.colors.background
  Rectangle {anchors.fill:parent;color:D.Theme.colors.background}
  D.FocusFlickable {id:displayScroll;anchors.fill:parent;anchors.margins:16;contentHeight:displayPane.height;clip:true;flickableDirection:Flickable.VerticalFlick
   D.DisplaySettingsPane {id:displayPane;width:displayScroll.width}
   ScrollBar.vertical:D.DesktopScrollBar {}
  }
 }
 function check(value,message){if(!value)throw Error(message+" "+JSON.stringify(cases[index]));}
 function walk(item,seen){seen=seen || [];if(!item || seen.includes(item))return [];seen.push(item);let result=[item],children=Array.from(item.children || []).concat(Array.from(item.data || []));if(item.contentItem)children.push(item.contentItem);for(const child of children)result=result.concat(walk(child,seen));return result;}
 function shown(item){if(!(item instanceof Item))return false;while(item && item!==audit.surface){if(!item.visible)return false;item=item.parent;}return item===audit.surface;}
 function rect(item){const p=item.mapToItem(audit.surface,0,0);return {x:p.x,y:p.y,width:item.width,height:item.height};}
 Timer {interval:30;repeat:true;running:true;onTriggered:{try{
  if(audit.painting)return;
  if(audit.index===audit.cases.length){console.log("ESSENTIAL_DIALOG_RESULT "+JSON.stringify(audit.records));Qt.quit();return;}
  const entry=audit.cases[audit.index];
  if(audit.stage===0){force.visible=entry.consumer==="Force";pair.visible=entry.consumer==="Pair";displayWindow.visible=entry.consumer==="Displays";D.Theme.colors=audit.palettes[entry.mode];D.Theme.preferences={textScale:entry.scale,reduceMotion:true};audit.owner=entry.consumer==="Force"?force:entry.consumer==="Pair"?pair:displayWindow;audit.surface=entry.consumer==="Displays"?displayWindow.contentItem:audit.owner.contentItem;audit.owner.width=entry.width;audit.owner.height=entry.height;audit.stage=1;return;}
  if(audit.stage===1){audit.stage=2;return;}
  const labels=audit.walk(audit.surface).filter(item=>audit.shown(item) && item.text && item.font && item.fontInfo);audit.labels=labels;
  const expected=entry.consumer==="Force"?[["Force Quit ",17],["The captured application changed",13]]:entry.consumer==="Pair"?[["Choose a compatible partner",12],["The selected partner",12]]:[["Select a display, then drag",11],["Resolution and refresh rate",12],["Confirmed settings update",11]];
  for(const [prefix,base]of expected){const label=labels.find(item=>item.text.startsWith(prefix));audit.check(label,"Actual production label "+prefix);audit.check(label.font.pixelSize===Math.round(base*entry.scale),"Role scale "+prefix+" actual "+label.font.pixelSize);audit.check(label.height+1>=label.implicitHeight,"Wrapped label fits "+prefix);audit.check(label.width>0,"Actual available text width");}
  const actions=audit.walk(audit.surface).filter(item=>audit.shown(item) && item instanceof AbstractButton && item.enabled && item.text);audit.check(actions.length,"Actual production actions");const target=actions[actions.length-1];
  if(audit.stage===2){target.forceActiveFocus();audit.stage=3;return;}
  audit.check(target.activeFocus,"Focus final action");const focused=audit.rect(target);audit.check(focused.y>=-1 && focused.y+focused.height<=audit.surface.height+1,"Focused final action revealed");
  for(let parent=target.parent;parent && parent!==audit.surface;parent=parent.parent)if(parent.clip){const clipped=audit.rect(parent);audit.check(focused.y>=clipped.y-1 && focused.y+focused.height<=clipped.y+clipped.height+1,"Focus through clip");}
  const painted=audit.walk(target).find(item=>item.text && item.font && item.color);audit.check(painted,"Actual final-action label");
  audit.painting=true;audit.check(audit.surface.grabToImage(result=>{try{const capture="'''+str(out)+'''/"+audit.index+".png";audit.check(result.saveToFile(capture),"Painted capture");audit.records=audit.records.concat([{entry:entry,capture:capture,focused:focused,paintedText:audit.rect(painted),paintedColor:painted.color.toString(),labels:expected.map(([prefix,base])=>{const item=labels.find(item=>item.text.startsWith(prefix));return {text:item.text,size:item.font.pixelSize,font:item.fontInfo.family,rect:audit.rect(item),implicitHeight:item.implicitHeight};})}]);audit.index++;audit.stage=0;audit.painting=false;}catch(error){console.error("ESSENTIAL_DIALOG_FAILURE "+error);Qt.quit();}},Qt.size(audit.surface.width,audit.surface.height)),"Grab accepted");
 }catch(error){console.error("ESSENTIAL_DIALOG_FAILURE "+error);Qt.quit();}}}
}''')
processes=[]
try:
 start_private_bus(env,out,processes)
 with (out/'qt.log').open('w')as log:child=sp.Popen(['quickshell','-p',str(qml)],env=env,stdout=log,stderr=sp.STDOUT,start_new_session=True)
 processes.append(child);child.wait(timeout=40);text=(out/'qt.log').read_text();assert child.returncode==0 and 'ESSENTIAL_DIALOG_FAILURE'not in text,text
 records=json.loads(next(line.split('ESSENTIAL_DIALOG_RESULT ',1)[1]for line in text.splitlines()if 'ESSENTIAL_DIALOG_RESULT 'in line));assert len(records)==48
finally:
 for child in reversed(processes):
  if child.poll()is None:
   os.killpg(child.pid,signal.SIGTERM)
   try:child.wait(timeout=5)
   except sp.TimeoutExpired:os.killpg(child.pid,signal.SIGKILL);child.wait(timeout=5)
 close_reaped_pipes(processes)

# Evaluate warning/artifact acceptance only after every owned child has been reaped.
warnings={path.name:re.findall(r"^.*(?:WARN|WARNING|ResourceWarning|Exception ignored|TypeError|ReferenceError|ERROR).*$",path.read_text(),re.M)for path in out.glob('*.log')}
warnings={key:value for key,value in warnings.items()if value};(out/'warning-scan.json').write_text(json.dumps(warnings,indent=2));assert not warnings,warnings
from PIL import Image
for row in records:
 with Image.open(row['capture'])as image:
  r=row['paintedText'];crop=image.convert('RGB').crop(tuple(round(v)for v in [r['x'],r['y'],r['x']+r['width'],r['y']+r['height']]));color=tuple(int(row['paintedColor'][i:i+2],16)for i in [1,3,5]);ink=sum(max(abs(a-b)for a,b in zip(pixel,color))<=32 for pixel in crop.get_flattened_data());assert 20<=ink<crop.width*crop.height*.65,row;row['ink']=ink
(out/'result.json').write_text(json.dumps(dict(passed=True,cases=records,scope='Production QML software presentation, actual painted action labels, text roles and programmatic focus reveal; layer transport adapted and no native physical action acceptance'),indent=2))
