"""Actual owned semantic states and non-hue status labels in disposable Qt frames."""
import argparse,hashlib,importlib.util,json,os,signal,subprocess as sp,sys
from pathlib import Path
from application_fixture import offscreen
from private_native_bus import start_private_bus
from fixture_resources import close_reaped_pipes
p=argparse.ArgumentParser();p.add_argument('--launcher',type=Path,required=True);p.add_argument('--font-config',type=Path,required=True);p.add_argument('--output',type=Path,required=True);p.add_argument('--inside',action='store_true');a=p.parse_args();repo=Path(__file__).resolve().parents[2]
if not a.inside:
 command=[sys.executable,'/source/tests/desktop/semantic_state_paint.py','--inside','--launcher',str(a.launcher.resolve()),'--font-config',str(a.font_config.resolve()),'--output','/artifacts/result']
 raise SystemExit(offscreen.run(repo,a.output,a.launcher,a.font_config,command))
env=offscreen.environment();out=a.output;out.mkdir(parents=True,exist_ok=False);children=[];(out/'desktop').symlink_to(repo/'res/lumen-desktop/shell',target_is_directory=True)
spec=importlib.util.spec_from_file_location('appearance',repo/'res/lumen-desktop/appearance/appearance.py');appearance=importlib.util.module_from_spec(spec);spec.loader.exec_module(appearance);palette=json.loads((repo/'res/lumen-desktop/appearance/palette.json').read_text());cases=[dict(mode=mode,contrast=contrast,width=width,colors=appearance.resolve_palette(palette,mode,{**appearance.DEFAULTS,'increaseContrast':contrast})) for mode in ('light','dark') for contrast in (False,True) for width in (580,220)]
state=Path(env['XDG_STATE_HOME'])/'lumen-desktop';state.mkdir(parents=True,exist_ok=True);(state/'appearance.json').write_text(json.dumps(dict(font=palette['font'],palette=cases[0]['colors'],preferences=dict(reduceMotion=True))))
qml=out/'states.qml';qml.write_text('''import QtQuick
import Quickshell
import "desktop" as D
ShellRoot {id:root
 property var cases:'''+json.dumps(cases)+'''
 property int step:0
 property int stage:0
 property int messageWidth:580
 property bool painting:false
 property var records:[]
 FloatingWindow {id:window;visible:true;implicitWidth:620;implicitHeight:460;color:D.Theme.colors.background
  Rectangle {id:frame;anchors.fill:parent;color:D.Theme.colors.background
   Column {x:20;y:20;width:580;spacing:12
    Repeater {id:statuses;model:["error","success","warning","info"]
     D.StatusMessage {required property string modelData;width:root.messageWidth;text:"The request returned a result";severity:modelData}
    }
    D.SettingsToggle {id:toggle;width:580;text:"Private permission";detail:"Switch position also conveys state";checked:true}
    D.ControlTile {id:radio;width:580;label:"Private connection";symbol:"wifi";toggle:true;unavailable:false;active:true;subtitle:active?"Connected":"Disconnected"}
    D.DialogButton {id:disabled;width:580;text:"Delete private item";destructive:true;enabled:false;onClicked:{throw Error("Disabled action dispatched");}}
   }
  }
 }
 function walk(item){let rows=[item];for(const child of item.children || [])rows=rows.concat(walk(child));return rows;}
 function rgba(c){return {r:c.r,g:c.g,b:c.b,a:c.a};}
 Timer {interval:30;running:true;repeat:true;onTriggered:{try {
  if(root.painting)return;
  if(root.step===root.cases.length){console.log("SEMANTIC_PAINT_RESULT "+JSON.stringify(root.records));Qt.quit();return;}
  const entry=root.cases[root.step];
  if(root.stage===0){root.messageWidth=entry.width;D.Theme.colors=entry.colors;D.Theme.preferences={reduceMotion:true,increaseContrast:entry.contrast};root.stage=1;return;}
  const rows=[];
  for(let i=0;i<statuses.count;i++){const item=statuses.itemAt(i);if(!item)return;const at=item.mapToItem(frame,0,0);rows.push({severity:item.severity,text:item.text,accessible:item.Accessible.name,visibleTexts:root.walk(item).filter(child=>child.visible && child.text!==undefined).map(child=>String(child.text)),padding:item.leftPadding,foreground:root.rgba(item.color),background:root.rgba(frame.color),rect:{x:at.x,y:at.y,width:item.width,height:item.height}});}
  const track=root.walk(toggle).find(item=>item.width===32 && item.height===20),thumb=track?Array.from(track.children).find(item=>item.width===16):null;if(!thumb || thumb.x!==14 || !toggle.checked)throw Error("Checked switch lacks positional state "+JSON.stringify({thumb:thumb?thumb.x:null,checked:toggle.checked}));
  if(!radio.Accessible.name.includes("Connected") || !radio.Accessible.checked || disabled.enabled)throw Error("State metadata lost");
  root.painting=true;const file="'''+str(out)+'''/"+entry.mode+"-"+entry.contrast+"-"+entry.width+".png";
  if(!frame.grabToImage(result=>{try {if(!result.saveToFile(file))throw Error("Actual frame capture failed");root.records=root.records.concat([{mode:entry.mode,increaseContrast:entry.contrast,width:entry.width,statuses:rows,capture:file,checkedThumb:thumb.x,radio:radio.Accessible.name,disabled:!disabled.enabled}]);root.step++;root.stage=0;root.painting=false;}catch(error){console.error("SEMANTIC_PAINT_FAILURE "+error);Qt.quit();}},Qt.size(frame.width*2,frame.height*2)))throw Error("Frame capture rejected");
 }catch(error){console.error("SEMANTIC_PAINT_FAILURE "+error);Qt.quit();}}}
}''')
try:
 start_private_bus(env,out,children)
 with (out/'qt.log').open('w') as log:proc=sp.Popen(['quickshell','-p',str(qml)],env=env,stdout=log,stderr=sp.STDOUT,start_new_session=True)
 children.append(proc);proc.wait(timeout=20);log=(out/'qt.log').read_text();assert proc.returncode==0 and not any(word in log for word in ('SEMANTIC_PAINT_FAILURE','ReferenceError:','TypeError:','Binding loop','Failed to load configuration')),log
 records=json.loads(next(line.split('SEMANTIC_PAINT_RESULT ',1)[1] for line in log.splitlines() if 'SEMANTIC_PAINT_RESULT ' in line));assert len(records)==8
 from PIL import Image,ImageOps
 def lum(c):return sum((v/12.92 if v<=.04045 else ((v+.055)/1.055)**2.4)*weight for v,weight in zip((c[k] for k in ('r','g','b')),(.2126,.7152,.0722)))
 matrices={'protanopia':(.152286,1.052583,-.204868,.114503,.786281,.099216,-.003882,-.048116,1.051998),'deuteranopia':(.367322,.860646,-.227968,.280085,.672501,.047413,-.011820,.042940,.968881)}
 for case in records:
  with Image.open(case['capture']) as source:image=source.convert('RGB')
  ImageOps.grayscale(image).save(out/(case['mode']+'-'+str(case['increaseContrast'])+'-'+str(case['width'])+'-grayscale.png'))
  for name,matrix in matrices.items():image.convert('RGB',tuple(value for i in range(3) for value in (*matrix[i*3:i*3+3],0))).save(out/(case['mode']+'-'+str(case['increaseContrast'])+'-'+str(case['width'])+'-'+name+'.png'))
  for row in case['statuses']:
   low,high=sorted((lum(row['foreground']),lum(row['background'])));row['contrast']=(high+.05)/(low+.05);assert row['contrast']>=4.5,row
   rect=row['rect'];crop=image.crop(tuple(round(v*2) for v in (rect['x'],rect['y'],rect['x']+rect['width'],rect['y']+rect['height'])));expected=tuple(round(row['foreground'][key]*255) for key in ('r','g','b'));row['paintedInk']=sum(max(abs(pixel[i]-expected[i]) for i in range(3))<=3 for pixel in crop.get_flattened_data());assert row['paintedInk']>=10,row
 (out/'observed.json').write_text(json.dumps(records,indent=2))
 for case in records:
  for row in case['statuses']:
   if row['severity']!='info':
    assert 0<row['padding']<row['rect']['width']/2,row
    assert any(text.startswith(row['severity'].capitalize()+':') for text in row['visibleTexts']),(row['severity'],'Visible status meaning depends only on hue',row)
 (out/'result.json').write_text(json.dumps(dict(passed=True,cases=records,scope='Actual production semantic controls/messages in eight mode/contrast/width cases, painted role contrast and visible non-hue state; software colour-perception transforms are diagnostics, not human usability validation.'),indent=2));print('PASS semantic painted states8')
finally:
 for proc in reversed(children):
  if proc.poll() is None:
   os.killpg(proc.pid,signal.SIGTERM)
   try:proc.wait(timeout=5)
   except sp.TimeoutExpired:os.killpg(proc.pid,signal.SIGKILL);proc.wait(timeout=5)
 close_reaped_pipes(children)
