"""Actual switch indicator boundary contrast across resolved accents and states."""
import argparse,importlib.util,json,os,signal,subprocess as sp,sys
from pathlib import Path
from application_fixture import offscreen
from private_native_bus import start_private_bus
from fixture_resources import close_reaped_pipes
p=argparse.ArgumentParser();p.add_argument('--launcher',type=Path,required=True);p.add_argument('--font-config',type=Path,required=True);p.add_argument('--output',type=Path,required=True);p.add_argument('--inside',action='store_true');a=p.parse_args();repo=Path(__file__).resolve().parents[2]
if not a.inside:
 raise SystemExit(offscreen.run(repo,a.output,a.launcher,a.font_config,[sys.executable,'/source/tests/desktop/switch_indicator_paint.py','--inside','--launcher',str(a.launcher.resolve()),'--font-config',str(a.font_config.resolve()),'--output','/artifacts/result']))
env=offscreen.environment();out=a.output;out.mkdir(parents=True,exist_ok=False);children=[];(out/'desktop').symlink_to(repo/'res/lumen-desktop/shell',target_is_directory=True)
spec=importlib.util.spec_from_file_location('appearance',repo/'res/lumen-desktop/appearance/appearance.py');appearance=importlib.util.module_from_spec(spec);spec.loader.exec_module(appearance);palette=json.loads((repo/'res/lumen-desktop/appearance/palette.json').read_text());cases=[dict(mode=mode,accent=accent,contrast=contrast,checked=checked,colors=appearance.resolve_palette(palette,mode,{**appearance.DEFAULTS,'accent':accent,'increaseContrast':contrast})) for mode in ('light','dark') for accent in appearance.ACCENTS for contrast in (False,True) for checked in (False,True)]
state=Path(env['XDG_STATE_HOME'])/'lumen-desktop';state.mkdir(parents=True,exist_ok=True);(state/'appearance.json').write_text(json.dumps(dict(font=palette['font'],palette=cases[0]['colors'],preferences=dict(reduceMotion=True))))
qml=out/'switch.qml';qml.write_text('''import QtQuick
import Quickshell
import "desktop" as D
ShellRoot {id:root;property var cases:'''+json.dumps(cases)+''';property var records:[];property int step:0;property int stage:0;property bool painting:false
 FloatingWindow {id:window;visible:true;implicitWidth:500;implicitHeight:90;color:D.Theme.colors.background
  D.SettingsToggle {id:toggle;x:10;y:15;width:480;text:"Private permission"}
 }
 function walk(item){let rows=[item];for(const child of item.children || [])rows=rows.concat(walk(child));return rows;}
 function rgba(c){return {r:c.r,g:c.g,b:c.b,a:c.a};}
 Timer {interval:20;repeat:true;running:true;onTriggered:{try {
  if(root.painting)return;if(root.step===root.cases.length){console.log("SWITCH_PAINT_RESULT "+JSON.stringify(root.records));Qt.quit();return;}
  const entry=root.cases[root.step];if(root.stage===0){D.Theme.colors=entry.colors;D.Theme.preferences={reduceMotion:true,increaseContrast:entry.contrast};toggle.checked=entry.checked;root.stage=1;return;}
  const track=root.walk(toggle).find(item=>item.width===32 && item.height===20),thumb=track?Array.from(track.children).find(item=>item.width===16):null;if(!thumb)throw Error("Missing actual switch indicator");const at=thumb.mapToItem(toggle,0,0),indicator=thumb.border.width>0?thumb.border.color:thumb.color;const observed={mode:entry.mode,accent:entry.accent,increaseContrast:entry.contrast,checked:entry.checked,position:thumb.x,fill:root.rgba(track.color),boundary:root.rgba(indicator),thumb:root.rgba(thumb.color),border:thumb.border.width,rect:{x:at.x,y:at.y,width:thumb.width,height:thumb.height},capture:"'''+str(out)+'''/switch-"+root.step+".png"};if(thumb.x!==(entry.checked?14:2))throw Error("Incorrect physical state position");root.painting=true;
  const item=toggle;if(!item.grabToImage(result=>{try {if(!result.saveToFile(observed.capture))throw Error("Actual switch capture failed");root.records=root.records.concat([observed]);root.step++;root.stage=0;root.painting=false;}catch(error){console.error("SWITCH_PAINT_FAILURE "+error);Qt.quit();}},Qt.size(toggle.width*3,toggle.height*3)))throw Error("Capture refused");
 }catch(error){console.error("SWITCH_PAINT_FAILURE "+error);Qt.quit();}}}
}''')
try:
 start_private_bus(env,out,children)
 with (out/'qt.log').open('w') as log:proc=sp.Popen(['quickshell','-p',str(qml)],env=env,stdout=log,stderr=sp.STDOUT,start_new_session=True)
 children.append(proc);proc.wait(timeout=30);log=(out/'qt.log').read_text();assert proc.returncode==0 and not any(word in log for word in ('SWITCH_PAINT_FAILURE','ReferenceError:','TypeError:','Binding loop','Failed to load configuration')),log
 rows=json.loads(next(line.split('SWITCH_PAINT_RESULT ',1)[1] for line in log.splitlines() if 'SWITCH_PAINT_RESULT ' in line));assert len(rows)==len(cases)
 from PIL import Image
 def lum(c):return sum((v/12.92 if v<=.04045 else ((v+.055)/1.055)**2.4)*w for v,w in zip((c[k] for k in ('r','g','b')),(.2126,.7152,.0722)))
 for row in rows:
  with Image.open(row['capture']) as source:
   rect=row['rect'];crop=source.convert('RGB').crop(tuple(round(v*3) for v in (rect['x'],rect['y'],rect['x']+rect['width'],rect['y']+rect['height'])))
   candidates=[]
   for color in (row['thumb'],row['boundary']):
    rgb=tuple(round(color[k]*255) for k in ('r','g','b'));samples=[pixel for pixel in crop.get_flattened_data() if max(abs(pixel[i]-rgb[i]) for i in range(3))<=16];ratios=[]
    for pixel in samples:
     actual=dict(zip(('r','g','b'),(value/255 for value in pixel)));low,high=sorted((lum(actual),lum(row['fill'])));ratios.append((high+.05)/(low+.05))
    candidates.append(dict(color=color,pixels=len(samples),contrast=min(ratios) if ratios else 0))
   # Rectangle border getters have defaults even when no border is painted.
   painted=[candidate for candidate in candidates if candidate['pixels']>=5];assert painted,row
   actual=max(painted,key=lambda candidate:candidate['contrast']);row['contrast']=actual['contrast'];row['paintedBoundaryPixels']=actual['pixels'];row['actualBoundary']=actual['color']
 (out/'observed.json').write_text(json.dumps(rows,indent=2));assert all(row['contrast']>=3 and row['paintedBoundaryPixels']>=5 for row in rows),[row for row in rows if row['contrast']<3 or row['paintedBoundaryPixels']<5]
 (out/'result.json').write_text(json.dumps(dict(passed=True,checks=len(rows),rows=rows,scope='Enabled production switch indicator boundary and on/off position in actual software Qt frames; all resolved accents/modes/contrast settings, no disabled contrast or human vision claim.'),indent=2));print('PASS switch boundary '+str(len(rows)))
finally:
 for proc in reversed(children):
  if proc.poll() is None:
   os.killpg(proc.pid,signal.SIGTERM)
   try:proc.wait(timeout=5)
   except sp.TimeoutExpired:os.killpg(proc.pid,signal.SIGKILL);proc.wait(timeout=5)
 close_reaped_pipes(children)
