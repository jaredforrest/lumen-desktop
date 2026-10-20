"""Actual live calendar labels, date boundaries, locales and unavailable assets."""
import argparse,datetime as dt,hashlib,json,os,shutil,signal,subprocess as sp,sys
from pathlib import Path
from application_fixture import offscreen
from private_native_bus import start_private_bus
from fixture_resources import close_reaped_pipes
p=argparse.ArgumentParser();p.add_argument('--launcher',type=Path,required=True);p.add_argument('--font-config',type=Path,required=True);p.add_argument('--output',type=Path,required=True);p.add_argument('--inside',action='store_true');a=p.parse_args();repo=Path(__file__).resolve().parents[2]
if not a.inside:
 command=[sys.executable,'/source/tests/desktop/dock_calendar_corpus.py','--inside','--launcher',str(a.launcher.resolve()),'--font-config',str(a.font_config.resolve()),'--output','/artifacts/result']
 raise SystemExit(offscreen.run(repo,a.output,a.launcher,a.font_config,command))
env=offscreen.environment();out=a.output;out.mkdir(parents=True,exist_ok=False);children=[];palettes=json.loads((repo/'res/lumen-desktop/appearance/palette.json').read_text());assets=out/'assets';broken=out/'broken';shutil.copytree(repo/'res/lumen-desktop',assets);shutil.copytree(repo/'res/lumen-desktop',broken)
removed=['shell/reference-icons/calendar.svg','shell/reference-icons/dark/calendar.svg'];removedHashes={name:hashlib.sha256((broken/name).read_bytes()).hexdigest() for name in removed}
for name in removed:(broken/name).unlink()
state=Path(env['XDG_STATE_HOME'])/'lumen-desktop';state.mkdir(parents=True,exist_ok=True);(state/'appearance.json').write_text(json.dumps(dict(font=palettes['font'],palette=palettes['light'],preferences=dict(reduceMotion=True))))
instants=['2026-10-20T23:59:59Z','2026-10-21T00:00:00Z','2026-10-31T23:59:59Z','2026-11-01T00:00:00Z','2026-12-31T23:59:59Z','2027-01-01T00:00:00Z','2028-02-28T23:59:59Z','2028-02-29T00:00:00Z','2028-03-01T00:00:00Z']
english=['Mon','Tue','Wed','Thu','Fri','Sat','Sun'];french=['lun.','mar.','mer.','jeu.','ven.','sam.','dim.'];records=[];recoveries=[]
qml=out/'corpus.qml';qml.write_text('''import QtQuick
import Quickshell
import Quickshell.Io
import "assets/shell" as D
import "broken/shell" as B
ShellRoot {id:root
 property var palettes:'''+json.dumps(palettes)+'''
 property var dates:'''+json.dumps(instants)+'''
 property int step:0
 property int stage:0
 property bool painting:false
 property date currentDate:new Date(root.dates[0])
 property var records:[]
 property var recoveries:[]
 property int recoveryStage:0
 property bool copyComplete:false
 FloatingWindow {id:window;visible:true;implicitWidth:600;implicitHeight:190;color:D.Theme.colors.background
  Row {id:frame;x:10;y:10;spacing:20
   D.AppIcon {id:normal;width:128;height:128;calendarDate:root.currentDate;iconSource:Qt.resolvedUrl("assets/shell/reference-icons/calendar.svg")}
   B.AppIcon {id:unavailable;width:128;height:128;calendarDate:root.currentDate;iconSource:Qt.resolvedUrl("broken/shell/reference-icons/calendar.svg")}
   D.AppIcon {id:generic;width:128;height:128;iconSource:Qt.resolvedUrl("absent/application.svg")}
  }
 }
 Process {id:restore;command:["'''+sys.executable+'''","-c","import shutil,sys;shutil.copyfile(sys.argv[1],sys.argv[2]);shutil.copyfile(sys.argv[3],sys.argv[4])",'/artifacts/result/assets/shell/reference-icons/dark/calendar.svg','/artifacts/result/broken/shell/reference-icons/dark/calendar.svg','/artifacts/result/assets/shell/reference-icons/calendar.svg','/artifacts/result/broken/shell/reference-icons/calendar.svg'];onExited:(code,status)=>{if(code!==0){console.error("CALENDAR_CORPUS_FAILURE restore");Qt.quit();}root.copyComplete=true;}}
 function recovery(mode,next){root.painting=true;const observed={mode:mode,source:unavailable.source.toString(),failed:unavailable.failed,labels:root.labels(unavailable)};if(!frame.grabToImage(result=>{const path="/artifacts/result/"+Qt.locale().name+"-recovery-"+mode+".png";if(!result.saveToFile(path)){console.error("CALENDAR_CORPUS_FAILURE recovery frame");Qt.quit();return;}observed.capture=path;root.recoveries=root.recoveries.concat([observed]);root.recoveryStage=next;root.painting=false;},Qt.size(frame.width,frame.height)))throw Error("Recovery capture rejected");}
 function labels(icon){return Array.from(icon.children).filter(child=>child.text!==undefined && child.visible).map(child=>({text:child.text,width:child.implicitWidth,height:child.implicitHeight,font:child.font.pixelSize}));}
 Timer {interval:20;repeat:true;running:true;onTriggered:{try {
  if(root.painting)return;
  if(root.step===root.dates.length*2){
   if(root.recoveryStage===0){restore.running=true;root.recoveryStage=1;return;}
   if(root.recoveryStage===1){if(!root.copyComplete)return;unavailable.iconSource=Qt.resolvedUrl("assets/shell/icons/application.svg");root.recoveryStage=2;return;}
   if(root.recoveryStage===2){if(unavailable.status!==Image.Ready || unavailable.variantName!=="")return;unavailable.iconSource=Qt.resolvedUrl("broken/shell/reference-icons/calendar.svg");root.recoveryStage=3;return;}
   if(root.recoveryStage===3){if(unavailable.status!==Image.Ready || unavailable.failed)return;root.recovery("dark",4);return;}
   if(root.recoveryStage===4){B.Theme.preferences={reduceMotion:true,iconAppearance:"light"};root.recoveryStage=5;return;}
   if(root.recoveryStage===5){if(unavailable.status!==Image.Ready || unavailable.failed || unavailable.usingDarkVariant)return;root.recovery("light",6);return;}
   console.log("CALENDAR_CORPUS_RESULT "+JSON.stringify({locale:Qt.locale().name,records:root.records,recoveries:root.recoveries}));Qt.quit();return;
  }
  const mode=root.step%2?"dark":"light",index=Math.floor(root.step/2);
  if(root.stage===0){D.Theme.colors=root.palettes[mode];B.Theme.colors=root.palettes[mode];D.Theme.preferences={reduceMotion:true,iconAppearance:mode};B.Theme.preferences={reduceMotion:true,iconAppearance:mode};root.currentDate=new Date(root.dates[index]);root.stage=1;return;}
  if(normal.status!==Image.Ready || !unavailable.failed || !generic.failed || generic.status!==Image.Ready)return;
  const observed={iso:root.dates[index],mode:mode,labels:root.labels(normal),normalSource:normal.source.toString(),normalDark:normal.usingDarkVariant,unavailableLabels:root.labels(unavailable),unavailableSource:unavailable.source.toString(),unavailableFailed:unavailable.failed,genericLabels:root.labels(generic),genericSource:generic.source.toString()};
  root.painting=true;
  if(!frame.grabToImage(result=>{try {const path="'''+str(out)+'''/"+Qt.locale().name+"-"+root.step+".png";if(!result.saveToFile(path))throw Error("Frame save failed");observed.capture=path;root.records=root.records.concat([observed]);root.step++;root.stage=0;root.painting=false;}catch(error){console.error("CALENDAR_CORPUS_FAILURE "+error);Qt.quit();}},Qt.size(frame.width,frame.height)))throw Error("Frame capture rejected");
 }catch(error){console.error("CALENDAR_CORPUS_FAILURE "+error);Qt.quit();}}}
}''')
try:
 start_private_bus(env,out,children)
 for locale,weekdays in [('en_GB',english),('fr_FR',french)]:
  for name in removed:(broken/name).unlink(missing_ok=True)
  current=dict(env,TZ='UTC',LANG=locale+'.UTF-8',LC_ALL=locale+'.UTF-8');logfile=out/(locale+'.log')
  with logfile.open('w') as log:process=sp.Popen(['quickshell','-p',str(qml)],env=current,stdout=log,stderr=sp.STDOUT,start_new_session=True)
  children.append(process);process.wait(timeout=25);text=logfile.read_text();assert process.returncode==0 and 'CALENDAR_CORPUS_FAILURE' not in text,text;payload=json.loads(next(line.split('CALENDAR_CORPUS_RESULT ',1)[1] for line in text.splitlines() if 'CALENDAR_CORPUS_RESULT ' in line));assert payload['locale']==locale,payload
  assert len(payload['records'])==len(instants)*2
  assert len(payload['recoveries'])==2
  for recovery in payload['recoveries']:
   assert not recovery['failed'] and recovery['source'].endswith('/reference-icons/'+('dark/' if recovery['mode']=='dark' else '')+'calendar.svg') and Path(recovery['capture']).is_file(),recovery
   assert [label['text'] for label in recovery['labels']]==[weekdays[dt.datetime.fromisoformat(instants[-1].replace('Z','+00:00')).weekday()],'1'],recovery
   recoveries.append(dict(locale=locale,**recovery))
  for row in payload['records']:
   date=dt.datetime.fromisoformat(row['iso'].replace('Z','+00:00'));expected=[weekdays[date.weekday()],str(date.day)];actual=[label['text'] for label in row['labels']];row['expectedLabels']=expected;row['dateLabelsCorrect']=actual==expected and all(label['width']<=128 for label in row['labels']);row['unavailableLabelsCorrect']=[label['text'] for label in row['unavailableLabels']]==expected;row['fallbackSemantic']=not row['unavailableSource'].endswith('/icons/application.svg');row['genericFallbackCorrect']=row['genericSource'].endswith('/icons/application.svg') and not row['genericLabels'];assert Path(row['capture']).is_file();records.append(dict(locale=locale,**row))
 from PIL import Image
 for row in records:
  with Image.open(row['capture']) as image:
   assert image.width>380 and image.height==128;image=image.convert('RGB')
   background=(245,245,247) if row['mode']=='light' else (69,69,75);ink=(23,23,23) if row['mode']=='light' else (245,245,247);red=(185,57,49) if row['mode']=='light' else (241,109,104)
   def count(crop,color):return sum(max(abs(pixel[i]-color[i]) for i in range(3))<=4 for pixel in crop.get_flattened_data())
   row['painted']=[]
   for offset in (0,148):
    crop=image.crop((offset,0,offset+128,128));proof=dict(background=count(crop,background),dayInk=count(crop.crop((10,37,118,116)),ink),weekdayInk=count(crop.crop((5,14,123,44)),red));assert proof['background']>1000 and proof['dayInk']>30 and proof['weekdayInk']>8,(row['locale'],row['iso'],row['mode'],offset,proof);row['painted'].append(proof)
   row['frameSha256']=hashlib.sha256(Path(row['capture']).read_bytes()).hexdigest()
 result=dict(passed=all(row['dateLabelsCorrect'] and row['unavailableLabelsCorrect'] and row['fallbackSemantic'] and row['genericFallbackCorrect'] for row in records),cases=records,recoveries=recoveries,removedAssets=removedHashes,fixtureSha256=hashlib.sha256(qml.read_bytes()).hexdigest(),sourceSha256=hashlib.sha256((repo/'res/lumen-desktop/shell/AppIcon.qml').read_bytes()).hexdigest(),scope='Actual production AppIcon live date binding and software frames. Explicit UTC synthetic clock and English/French default process locales; no host clock, reference-platform or physical Dock clock claim.')
 (out/'result.json').write_text(json.dumps(result,indent=2));assert result['passed'],'Calendar date/locale/fallback contract failed; retained result.json and actual frames';print(json.dumps(dict(passed=True,cases=len(records))))
finally:
 for process in reversed(children):
  if process.poll() is None:
   os.killpg(process.pid,signal.SIGTERM)
   try:process.wait(timeout=5)
   except sp.TimeoutExpired:os.killpg(process.pid,signal.SIGKILL);process.wait(timeout=5)
 close_reaped_pipes(children)
