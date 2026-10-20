"""Real Focus expiry through the notification daemon, transport and painted centre."""
import argparse,hashlib,json,os,re,shutil,subprocess as sp,sys
from pathlib import Path
from application_fixture import offscreen
from fixture_resources import close_reaped_pipes
from private_native_bus import start_private_bus
p=argparse.ArgumentParser();p.add_argument('--launcher',type=Path,required=True);p.add_argument('--font-config',type=Path,required=True);p.add_argument('--output',type=Path,required=True);p.add_argument('--bus',type=Path,required=True);p.add_argument('--inside',action='store_true');a=p.parse_args();a.bus=offscreen.immutable_file(a.bus);assert os.access(a.bus,os.X_OK);repo=Path(__file__).resolve().parents[2]
if not a.inside:
 bus_hash=hashlib.sha256(a.bus.read_bytes()).hexdigest()
 code=offscreen.run(repo,a.output,a.launcher,a.font_config,[sys.executable,'/source/tests/desktop/focus_expiry_presentation.py','--inside','--launcher',str(a.launcher.resolve()),'--font-config',str(a.font_config.resolve()),'--output','/artifacts','--bus',str(a.bus)])
 assert hashlib.sha256(a.bus.read_bytes()).hexdigest()==bus_hash
 (a.output/'bus-dependency.json').write_text(json.dumps(dict(path=str(a.bus),sha256=bus_hash,unchanged=True),indent=2)+'\n')
 raise SystemExit(code)
env=offscreen.environment();env['PATH']=str(a.bus.parent)+':'+env['PATH'];out=a.output;env['LUMEN_NOTIFICATION_HELPER']=str(repo/'res/lumen-desktop/notifications/service.py');processes=[]
settings=out/'settings.json';settings.write_text(json.dumps(dict(notifications=dict(preview='when_unlocked',policies={}))))
env['LUMEN_NIX_SETTINGS_ACTIVE']=str(settings)
palette=json.loads((repo/'res/lumen-desktop/appearance/palette.json').read_text());appearance=out/'appearance.json';appearance.write_text(json.dumps(dict(font=palette['font'],palette=palette['light'],preferences={})));env['LUMEN_APPEARANCE_STATE']=str(appearance)
shutil.copytree(repo/'res/lumen-desktop/shell',out/'shell')
qml='''import QtQuick
import QtQuick.Window
import Quickshell
import "shell" as D
ShellRoot {
 id:test
 property int phase:0
 property int ticks:0
 property bool capturePending:false
 QtObject {id:testShell;property var bars:({});function preferredOutput(){return "";}function claimPopup(value){}}
 D.NotificationState {id:service;shell:testShell}
 Window {id:window;visible:true;width:360;height:110;color:D.Theme.colors.background
  Rectangle {anchors.fill:parent;color:D.Theme.colors.background}
  D.NotificationCentre {id:centre;width:320;x:20;y:20;service:service}
 }
 function snapshot(name,next){
  const labels=centre.children.filter(child=>child.visible && child.text!==undefined).map(child=>String(child.text));
  const expected=name==="active"?"Do Not Disturb is on\\nNo notifications":"No notifications";
  if(labels.length!==1 || labels[0]!==expected)throw Error("painted centre label mismatch "+JSON.stringify(labels));
  capturePending=true;
  if(!window.contentItem.grabToImage(result=>{
   if(!result.saveToFile("/artifacts/"+name+".png")){console.error("EXPIRY_FAIL capture write");Qt.exit(1);return;}
   console.log("EXPIRY_FRAME "+JSON.stringify({name:name,label:labels[0],focus:service.focusState,dnd:service.dnd,revision:service.revision}));
   test.capturePending=false;test.phase=next;
  }))throw Error("capture refused");
 }
 Timer {interval:50;repeat:true;running:true;onTriggered:{try{
  if(++test.ticks>160)throw Error("Focus expiry presentation deadline");
  if(test.capturePending)return;
  if(test.phase===0 && service.available){if(service.dnd || service.focusState.active!=="")throw Error("fresh daemon unexpectedly focused");service.command({action:"focus",profile:"quiet",seconds:2});test.phase=1;return;}
  if(test.phase===1 && service.dnd && service.focusState.active==="quiet"){test.snapshot("active",2);return;}
  if(test.phase===2 && !service.dnd && service.focusState.active==="" && service.focusState.scheduled===true){test.snapshot("expired",3);return;}
  if(test.phase===3){console.log("EXPIRY_PASS");Qt.quit();}
 }catch(error){console.error("EXPIRY_FAIL "+error);Qt.exit(1);}}}
}'''
(out/'fixture.qml').write_text(qml)
try:
 start_private_bus(env,out,processes)
 with (out/'daemon.log').open('w') as log:
  processes.append(sp.Popen([sys.executable,env['LUMEN_NOTIFICATION_HELPER'],'serve'],env=env,stdout=log,stderr=sp.STDOUT))
 result=sp.run(['quickshell','-p',str(out/'fixture.qml')],env=env,text=True,stdout=sp.PIPE,stderr=sp.STDOUT,timeout=12);(out/'qml.log').write_text(result.stdout)
finally:
 for process in reversed(processes):
  if process.poll()is None:process.terminate()
 for process in reversed(processes):
  try:process.wait(timeout=5)
  except sp.TimeoutExpired:process.kill();process.wait(timeout=5)
 close_reaped_pipes(processes)
assert result.returncode==0 and 'EXPIRY_PASS' in result.stdout,result.stdout
for path in (out/'qml.log',out/'daemon.log',out/'bus.log'):
 text=path.read_text();assert not re.search(r'(?:\bWARN\b|\bERROR\b|Warning:|Fontconfig error|Traceback|ReferenceError|TypeError)',text),text
frames=[json.loads(line.split('EXPIRY_FRAME ',1)[1])for line in result.stdout.splitlines()if 'EXPIRY_FRAME 'in line];assert [row['name']for row in frames]==['active','expired'] and frames[1]['revision']>frames[0]['revision'],frames
from PIL import Image,ImageChops
with Image.open(out/'active.png')as active,Image.open(out/'expired.png')as expired:
 assert active.convert('RGB').getpixel((10,10))==expired.convert('RGB').getpixel((10,10))==(242,242,244)
 for image in (active,expired):
  pixels=image.convert('RGB').tobytes();assert sum(pixels[i:i+3]!=bytes((242,242,244))for i in range(0,len(pixels),3))>64
 assert active.size==expired.size==(360,110) and ImageChops.difference(active.convert('RGB'),expired.convert('RGB')).getbbox()is not None
(out/'result.json').write_text(json.dumps(dict(passed=True,frames=frames,scope='Real private daemon/transport/Qt centre and painted timed expiry; clock/suspend boundaries separately covered by production Focus unit tests; no host clock or lock state changed'),indent=2)+'\n')
