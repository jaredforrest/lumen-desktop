"""Real Qt provider contention in a detached private software namespace."""
import argparse,json,shutil,subprocess as sp,sys
from pathlib import Path
from application_fixture import offscreen
from private_native_bus import start_private_bus
from fixture_resources import close_reaped_pipes
p=argparse.ArgumentParser();p.add_argument('--launcher',type=Path,required=True);p.add_argument('--fontconfig',type=Path,required=True);p.add_argument('--output',type=Path,required=True);p.add_argument('--inside',action='store_true');a=p.parse_args();repo=Path(__file__).resolve().parents[2]
if not a.inside:
 raise SystemExit(offscreen.run(repo,a.output,a.launcher,a.fontconfig,[sys.executable,'/source/tests/desktop/screen_saver_queue_qml.py','--inside','--launcher',str(a.launcher.resolve()),'--fontconfig',str(a.fontconfig.resolve()),'--output','/artifacts']))
env=offscreen.environment();out=a.output
shutil.copy(repo/'res/lumen-desktop/shell/ScreenSaverState.qml',out/'ScreenSaverState.qml')
# Controlled provider holds each status request until the fixture releases it.
(out/'screen_saver_settings.py').write_text('''import json,time
from pathlib import Path
root=Path(__file__).parent
request=json.loads(input());log=root/'requests.jsonl'
with log.open('a') as stream:stream.write(json.dumps(request)+'\\n')
if request['action']=='status':
 (root/'blocked').touch()
 while not (root/'release').exists():time.sleep(.01)
settings=root/'settings.json';values=json.loads(settings.read_text()) if settings.exists() else dict(enabled=True,idleSeconds=300,motion=True)
if request['action']=='set':values[request['key']]=request['value'];settings.write_text(json.dumps(values))
print(json.dumps(dict(preferences=values,lock=dict(known=True,locked=False))))
''')
# Process seams only coordinate fixture barriers; the state component is unchanged.
(out/'barrier.py').write_text("from pathlib import Path\nroot=Path(__file__).parent\n(root/'release').touch()\n")
(out/'fixture.qml').write_text('''import QtQuick
import Quickshell
import Quickshell.Io
import "."
ShellRoot {
 id:test
 property int phase:0
 property int ticks:0
 ScreenSaverState {id:service}
 Process {id:release;command:[Quickshell.env("GLOBAL_MENU_PYTHON"),Qt.resolvedUrl("barrier.py").toString().replace(/^file:\\/\\//,"")]}
 Timer {interval:50;running:true;repeat:true;onTriggered:{try{
 if(++test.ticks>120)throw Error("provider contention deadline");
 if(test.phase===0){service.refresh();test.phase=1;return;}
 if(test.phase===1){if(!service.busy)return;service.setValue("enabled",false);service.setValue("idleSeconds",60);service.setValue("motion",false);service.setValue("idleSeconds",90);service.refresh();release.running=true;test.phase=2;return;}
 if(test.phase===2){if(service.busy)return;if(service.preferences.enabled!==false || service.preferences.idleSeconds!==90 || service.preferences.motion!==false)throw Error("explicit settings lost behind status poll: "+JSON.stringify(service.preferences));console.log("QUEUE_PASS");Qt.quit();}
 }catch(error){console.error("QUEUE_FAIL "+error);Qt.exit(1);}}}
}''')
processes=[]
try:
 start_private_bus(env,out,processes)
 result=sp.run(['quickshell','-p',str(out/'fixture.qml')],env=env,text=True,stdout=sp.PIPE,stderr=sp.STDOUT,timeout=15);(out/'qml.log').write_text(result.stdout)
finally:
 for process in reversed(processes):process.terminate();process.wait(timeout=5)
 close_reaped_pipes(processes)
assert result.returncode==0 and 'QUEUE_PASS' in result.stdout,result.stdout
requests=[json.loads(line) for line in (out/'requests.jsonl').read_text().splitlines()]
assert requests==[dict(action='status'),dict(action='set',key='enabled',value=False),dict(action='set',key='idleSeconds',value=90),dict(action='set',key='motion',value=False)],requests
(out/'result.json').write_text(json.dumps(dict(passed=True,requests=requests,scope='Actual Qt state component; controlled provider contention, not native idle/lock behavior'),indent=2)+'\n')
