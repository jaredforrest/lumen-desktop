"""Real overview UI/backend target authority with a delayed disposable snapshot."""
import argparse,json,os,re,select,shutil,signal,subprocess as sp,tempfile,time
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--launcher',required=True);p.add_argument('--sway',required=True);p.add_argument('--output',type=Path,required=True);a=p.parse_args();a.output.mkdir(parents=True,exist_ok=False)
root=Path(__file__).resolve().parents[2];processes=[]
def wait(fn):
 end=time.monotonic()+15
 while time.monotonic()<end:
  value=fn()
  if value:return value
  time.sleep(.04)
 raise AssertionError('Overview lifecycle timed out; see '+str(a.output))
with tempfile.TemporaryDirectory(prefix='lumen-overview-') as temporary:
 private=Path(temporary);prefix=re.split(r'\n[ \t]*exec quickshell ',Path(a.launcher).read_text(),maxsplit=1)[0]
 raw=sp.check_output(['bash','-c',prefix+'\nexec env -0']);env=dict(v.decode().split('=',1) for v in raw.split(b'\0') if b'=' in v)
 for key in ('HOME','XDG_RUNTIME_DIR','XDG_CONFIG_HOME','XDG_STATE_HOME','XDG_DATA_HOME','XDG_CACHE_HOME'):
  env[key]=str(private/key);Path(env[key]).mkdir(mode=0o700)
 for key in ('SWAYSOCK','WAYLAND_DISPLAY','DISPLAY','DBUS_SESSION_BUS_ADDRESS','AT_SPI_BUS_ADDRESS'):env.pop(key,None)
 env.update(WLR_BACKENDS='headless',WLR_HEADLESS_OUTPUTS='1',WLR_RENDERER='pixman',WLR_LIBINPUT_NO_DEVICES='1',QT_QPA_PLATFORM='wayland',QT_QUICK_BACKEND='software',QT_QPA_PLATFORMTHEME='generic',QT_NO_XDG_DESKTOP_PORTAL='1',QT_ACCESSIBILITY='0',QT_LINUX_ACCESSIBILITY_ALWAYS_ON='0',NO_AT_BRIDGE='1',GTK_A11Y='none')
 def launch(args,name,**kwargs):
  proc=sp.Popen(args,env=env,stdout=(a.output/name).open('w'),stderr=sp.STDOUT,start_new_session=True,**kwargs);processes.append(proc);return proc
 def call(*args):return sp.check_output(args,env=env,text=True,timeout=5)
 def sway(command):
  response=json.loads(call('swaymsg','-r',command));assert all(x.get('success') for x in response),response
 def nodes(tree):
  yield tree
  for child in tree.get('nodes',[])+tree.get('floating_nodes',[]):yield from nodes(child)
 def windows():return {n.get('app_id'):n for n in nodes(json.loads(call('swaymsg','-r','-t','get_tree'))) if n.get('app_id') in ('overview-source','overview-bystander')}
 def workspace(cid):
  def walk(node,ws=''):
   if node.get('type')=='workspace':ws=node['name']
   if node.get('id')==cid:return ws
   for c in node.get('nodes',[])+node.get('floating_nodes',[]):
    result=walk(c,ws)
    if result is not None:return result
  return walk(json.loads(call('swaymsg','-r','-t','get_tree')))
 try:
  bus_config=private/'bus.conf';bus_config.write_text('<busconfig><type>session</type><listen>unix:tmpdir='+env['XDG_RUNTIME_DIR']+'</listen><policy context="default"><allow own="*"/><allow send_destination="*"/><allow receive_sender="*"/></policy></busconfig>')
  bus=sp.Popen(['dbus-daemon','--nofork','--print-address=1','--config-file='+str(bus_config)],env=env,stdout=sp.PIPE,stderr=(a.output/'bus.log').open('w'),text=True,start_new_session=True);processes.append(bus)
  assert select.select([bus.stdout],[],[],5)[0],'Private bus did not start';env['DBUS_SESSION_BUS_ADDRESS']=bus.stdout.readline().strip();assert env['DBUS_SESSION_BUS_ADDRESS'].startswith('unix:')
  config=private/'sway.conf';config.write_text('output * resolution 1200x800\nseat seat0 fallback true\n')
  launch([a.sway,'--unsupported-gpu','-c',str(config)],'sway.log');env['SWAYSOCK']=str(wait(lambda:next(Path(env['XDG_RUNTIME_DIR']).glob('sway-ipc*.sock'),None)));env['WAYLAND_DISPLAY']=wait(lambda:next((p.name for p in Path(env['XDG_RUNTIME_DIR']).glob('wayland-*') if not p.name.endswith('.lock')),None))
  launch(['foot','-o','resize-by-cells=no','--app-id=overview-source','sleep','180'],'source.log');launch(['foot','-o','resize-by-cells=no','--app-id=overview-bystander','sleep','180'],'bystander.log');wait(lambda:len(windows())==2)
  source=windows()['overview-source'];other=windows()['overview-bystander'];cid=source['id'];bid=other['id'];bws=workspace(bid);sway(f'[con_id={cid}] move container to workspace 2')
  # Gate only status snapshots; their data still comes from the real backend.
  gate=private/'gate.py';hold=private/'hold';release=private/'release';held=private/'held';requests=private/'requests.jsonl';responses=private/'responses.jsonl';realpython=env.get('GLOBAL_MENU_PYTHON') or shutil.which('python3',path=env['PATH'])
  gate.write_text('import json,subprocess,sys,time\nfrom pathlib import Path\nreal='+repr(realpython)+'\nhold=Path('+repr(str(hold))+');release=Path('+repr(str(release))+');held=Path('+repr(str(held))+');requests=Path('+repr(str(requests))+');responses=Path('+repr(str(responses))+')\npayload=sys.stdin.readline() if sys.argv[1].endswith("window_overview.py") else None\nif payload is None:raise SystemExit(subprocess.call([real,*sys.argv[1:]]))\nwith requests.open("a") as f:f.write(payload)\nresult=subprocess.run([real,*sys.argv[1:]],input=payload,text=True,capture_output=True,timeout=8)\nwith responses.open("a") as f:f.write(json.dumps({"request":json.loads(payload),"reply":json.loads(result.stdout)})+"\\n")\nif hold.exists() and json.loads(payload).get("action")=="status":\n held.write_text("snapshot-ready")\n deadline=time.monotonic()+10\n while not release.exists() and time.monotonic()<deadline:time.sleep(.02)\n if not release.exists():raise SystemExit(8)\nsys.stdout.write(result.stdout);sys.stderr.write(result.stderr);raise SystemExit(result.returncode)\n')
  wrapper=private/'python-gate';wrapper.write_text('#!/bin/sh\nexec '+realpython+' '+str(gate)+' "$@"\n');wrapper.chmod(0o700);env['GLOBAL_MENU_PYTHON']=str(wrapper)
  fixture=private/'shell.qml';shutil.copytree(root/'res/lumen-desktop',private/'lumen-desktop');assets=private/'lumen-desktop/shell'
  appearance=Path(env['XDG_STATE_HOME'])/'lumen-desktop/appearance.json';appearance.parent.mkdir(parents=True);appearance.write_text(json.dumps({'palette':{'background':'#f2f2f4','surface':'#ffffff','field':'#e5e5e8','text':'#202023','muted':'#6b6b70','border':'#c8c8ce','hover':'#dedee3','accent':'#007aff','selectedText':'#ffffff'},'font':'sans-serif','preferences':{}}))
  fixture.write_text('''import QtQuick
import Quickshell
import Quickshell.Io
import "'''+os.path.relpath(assets,private)+'''" as D
ShellRoot {
 QtObject {id:stub;function claimPopup(owner){}}
 D.WindowOverview {id:overview;shell:stub}
 function findText(item,text){if(item && item.text===text)return item;for(const child of (item && item.children || [])){const found=findText(child,text);if(found)return found;}return null;}
 function findName(item,name){if(item && item.Accessible && item.Accessible.name===name)return item;for(const child of (item && item.children || [])){const found=findName(child,name);if(found)return found;}return null;}
 function moveButton(item){if(item && item.text==="Move Window")return item;for(const child of (item && item.children || [])){const found=moveButton(child);if(found)return found;}return null;}
 IpcHandler {target:"overviewTest"
  function capture(id:string):void{overview.showForWindow(Number(id));}
  function refresh():void{overview.send({action:"status"});}
  function status():string{return JSON.stringify({visible:overview.visible,busy:overview.busy,dragged:overview.draggedId,selected:overview.selected,current:overview.current?overview.current.id:null,error:overview.error,moveEnabled:!!moveButton(overview.contentItem) && moveButton(overview.contentItem).enabled,assignEnabled:findText(overview.contentItem,"Assign Application").enabled,orderEnabled:findText(overview.contentItem,"Recent desktop order").enabled,outputEnabled:findName(overview.contentItem,"Destination display").enabled,desktopEnabled:findText(overview.contentItem,"Desktop 1").enabled});}
  function move():bool{const button=moveButton(overview.contentItem);if(button && button.enabled){button.clicked();return true;}return false;}
  function activate():void{overview.activate();}
  function activateAndRefresh():void{overview.activate();overview.send({action:"status"});}
 }
}
''')
  launch(['quickshell','-p',str(fixture)],'overview.log')
  def ipc(method,*args):return call('quickshell','-p',str(fixture),'ipc','call','overviewTest',method,*map(str,args))
  def status():
   try:return json.loads(ipc('status'))
   except (sp.SubprocessError,ValueError):return {}
  wait(lambda:status().get('visible') is False);ipc('capture',cid);wait(lambda:status().get('current')==cid and not status().get('busy',True))
  initial=status();assert initial['moveEnabled'],initial
  # Production Move Window button targets the captured source, not current focus.
  sway(f'[con_id={bid}] focus');assert ipc('move').strip()=='true';wait(lambda:workspace(cid)=='1');assert workspace(bid)==bws
  # A delayed focus request waits for a busy status worker and keeps its ID.
  wait(lambda:not status().get('busy',True));sway(f'[con_id={bid}] focus');hold.touch();ipc('activateAndRefresh');wait(held.exists)
  delayed=status();assert delayed['busy'] and not delayed['visible'],delayed
  time.sleep(.2);assert not any(json.loads(line).get('action')=='focus' for line in requests.read_text().splitlines())
  release.touch();wait(lambda:any(reply['request'].get('action')=='focus' and reply['request'].get('id')==cid and reply['reply'].get('ok') for reply in [json.loads(line) for line in responses.read_text().splitlines()]))
  assert next(node for node in nodes(json.loads(call('swaymsg','-r','-t','get_tree'))) if node.get('id')==cid)['focused']
  wait(lambda:not status().get('busy',True));hold.unlink();release.unlink();held.unlink();ipc('capture',cid);wait(lambda:status().get('current')==cid and not status().get('busy',True))
  # Hold a genuine old snapshot while its captured source disappears.
  hold.touch();ipc('refresh');wait(held.exists);ipc('capture',cid);busy=status();assert busy['busy'] and not any(busy[key] for key in ('moveEnabled','assignEnabled','orderEnabled','outputEnabled','desktopEnabled')),busy;assert ipc('move').strip()=='false';ipc('activate');assert status()['visible'];sway(f'[con_id={cid}] kill');wait(lambda:'overview-source' not in windows());release.touch();wait(lambda:status().get('current')==cid and not status().get('busy',True));time.sleep(.15)
  assert ipc('move').strip()=='true';wait(lambda:any(reply['request'].get('action')=='move' and reply['reply'].get('error')=='This window is no longer available' for reply in [json.loads(line) for line in responses.read_text().splitlines()]));assert workspace(bid)==bws
  stale=status();hold.unlink();ipc('refresh');wait(lambda:status().get('current') is None and status().get('error')=='The dragged window is no longer available')
  absent=status();assert absent['dragged']==cid and absent['selected']==-1 and not absent['moveEnabled'],absent
  wait(lambda:not status().get('busy',True));before=[line for line in requests.read_text().splitlines() if json.loads(line).get('action') in ('move','focus')];assert ipc('move').strip()=='false';ipc('activate');time.sleep(.2);assert [line for line in requests.read_text().splitlines() if json.loads(line).get('action') in ('move','focus')]==before;assert workspace(bid)==bws
  call('grim',str(a.output/'missing-captured-window.png'))
  assert not any(x in (a.output/'overview.log').read_text() for x in (' WARN ',' ERROR ','TypeError:','ReferenceError:','Failed to load'))
  (a.output/'result.json').write_text(json.dumps({'passed':True,'capturedID':cid,'bystanderID':bid,'initial':initial,'busyDisabled':busy,'delayedActivation':delayed,'staleRejected':stale,'absent':absent,'requests':[json.loads(x) for x in requests.read_text().splitlines()],'responses':[json.loads(x) for x in responses.read_text().splitlines()]},indent=2));print('PASS real overview captured identity, delayed stale snapshot and missing-target refusal')
 finally:
  for proc in reversed(processes):
   try:os.killpg(proc.pid,signal.SIGTERM)
   except ProcessLookupError:pass
  for proc in reversed(processes):
   try:proc.wait(timeout=3)
   except sp.TimeoutExpired:
    try:os.killpg(proc.pid,signal.SIGKILL)
    except ProcessLookupError:pass
    proc.wait()
