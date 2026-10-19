"""Real desktop lifecycle, saved window evacuation and cancel-safe Overview order."""
import argparse,json,os,re,select,shutil,signal,subprocess as sp,sys,tempfile,time
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--launcher',type=Path,required=True);p.add_argument('--sway',required=True);p.add_argument('--pointer-tool',required=True);p.add_argument('--output',type=Path,required=True);p.add_argument('--effects',action='store_true');a=p.parse_args();a.output.mkdir(parents=True,exist_ok=False)
repo=Path(__file__).resolve().parents[2];processes=[]
with tempfile.TemporaryDirectory(prefix='lumen-desktop-lifecycle-') as temporary:
 private=Path(temporary);prefix=re.split(r'\n[ \t]*exec quickshell ',a.launcher.read_text(),maxsplit=1)[0]
 env=dict(value.decode().split('=',1) for value in sp.check_output(['bash','-c',prefix+'\nexec env -0']).split(b'\0') if b'=' in value)
 for name in ('HOME','XDG_RUNTIME_DIR','XDG_STATE_HOME','XDG_DATA_HOME','XDG_CONFIG_HOME','XDG_CACHE_HOME'):env[name]=str(private/name);Path(env[name]).mkdir(mode=0o700)
 for name in ('SWAYSOCK','WAYLAND_DISPLAY','DISPLAY','DBUS_SESSION_BUS_ADDRESS','AT_SPI_BUS_ADDRESS'):env.pop(name,None)
 env.update(WLR_BACKENDS='headless',WLR_HEADLESS_OUTPUTS='2',WLR_RENDERER='gles2' if a.effects else 'pixman',WLR_RENDERER_FORCE_SOFTWARE='1' if a.effects else '0',WLR_RENDERER_ALLOW_SOFTWARE='1',LIBGL_ALWAYS_SOFTWARE='1',WLR_LIBINPUT_NO_DEVICES='1',QT_QPA_PLATFORM='wayland',QT_QUICK_BACKEND='software',QT_QPA_PLATFORMTHEME='generic',QT_NO_XDG_DESKTOP_PORTAL='1',QT_ACCESSIBILITY='0',QT_LINUX_ACCESSIBILITY_ALWAYS_ON='0',NO_AT_BRIDGE='1',GTK_A11Y='none')
 backend=repo/'res/lumen-desktop/shell/window_overview.py';python=env.get('GLOBAL_MENU_PYTHON') or sys.executable
 def launch(args,name,**kwargs):
  with (a.output/name).open('w') as stream:proc=sp.Popen(args,env=env,stdout=stream,stderr=sp.STDOUT,start_new_session=True,**kwargs)
  processes.append(proc);return proc
 def call(*args):return sp.check_output(args,env=env,text=True,timeout=6).strip()
 def sway(*args):return json.loads(call('swaymsg','-r',*args))
 def checked(command):
  result=sway(command);assert result and all(item.get('success') for item in result),result
 def request(action,**values):
  result=json.loads(sp.check_output([python,str(backend)],input=json.dumps(dict(action=action,**values)),env=env,text=True,timeout=8));assert not result.get('error'),result;return result
 def status():return request('status')
 def mutate(action,**values):
  state=status();return request(action,revision=state['revision'],sessionEpoch=state['sessionEpoch'],**values)
 def wait(predicate):
  deadline=time.monotonic()+15
  while time.monotonic()<deadline:
   try:
    value=predicate()
    if value:return value
   except (ValueError,OSError,sp.SubprocessError):pass
   time.sleep(.04)
  raise AssertionError('Desktop lifecycle timeout: '+str(a.output)+'; compositor logs: '+ '\n'.join(path.name+': '+path.read_text() for path in a.output.glob('*sway*.log')))
 def nodes(node):
  yield node
  for child in node.get('nodes',[])+node.get('floating_nodes',[]):yield from nodes(child)
 def native(app):return next((node for node in nodes(sway('-t','get_tree')) if node.get('app_id')==app),None)
 def logical(name):return next(item for item in status()['workspaces'] if item['name']==name)
 try:
  busconf=private/'bus.conf';busconf.write_text('<busconfig><type>session</type><listen>unix:tmpdir='+env['XDG_RUNTIME_DIR']+'</listen><policy context="default"><allow own="*"/><allow send_destination="*"/><allow receive_sender="*"/></policy></busconfig>')
  with (a.output/'bus.log').open('w') as log:bus=sp.Popen(['dbus-daemon','--nofork','--print-address=1','--config-file='+str(busconf)],env=env,stdout=sp.PIPE,stderr=log,text=True,start_new_session=True)
  processes.append(bus);assert select.select([bus.stdout],[],[],5)[0];env['DBUS_SESSION_BUS_ADDRESS']=bus.stdout.readline().strip()
  config=private/'sway.conf';config.write_text('output HEADLESS-1 resolution 1440x1000 position 0 0\noutput HEADLESS-2 resolution 1280x900 position 1440 0\nseat seat0 fallback true\nfocus_follows_mouse no\nxwayland disable\nfor_window [all] floating enable, border pixel 0\n')
  launch([a.sway,'--unsupported-gpu','-c',str(config)],'sway.log');env['SWAYSOCK']=str(wait(lambda:next(Path(env['XDG_RUNTIME_DIR']).glob('sway-ipc*.sock'),None)));env['WAYLAND_DISPLAY']=wait(lambda:next((item.name for item in Path(env['XDG_RUNTIME_DIR']).glob('wayland-*') if not item.name.endswith('.lock')),None))
  checked('focus output HEADLESS-1; workspace "1"; move workspace to output HEADLESS-1')
  launch(['foot','--app-id=desktop-visible','sleep','180'],'visible.log');first=wait(lambda:native('desktop-visible'));visible_pid=first['pid']
  launch(['foot','--app-id=desktop-minimized','sleep','180'],'minimized.log');mini=wait(lambda:native('desktop-minimized'));mini_pid=mini['pid']
  launch(['foot','--app-id=desktop-hidden','sleep','180'],'hidden.log');hidden=wait(lambda:native('desktop-hidden'));hidden_pid=hidden['pid']
  checked('[con_id=%d] lumen_window minimize'%mini['id']);checked('[con_id=%d] lumen_window hide'%hidden['id'])
  original=status();source=logical('1');output=source['outputId'];focused=next(item['name'] for item in sway('-t','get_workspaces') if item.get('focused'))
  created=mutate('desktop-create',outputId=output)['created'];assert created['id'] is None and created['name'] not in {item['name'] for item in sway('-t','get_workspaces')}
  assert next(item['name'] for item in sway('-t','get_workspaces') if item.get('focused'))==focused
  assert next(item for item in status()['workspaces'] if item['logicalId']==created['logicalId'])['name']==created['name']
  # A fresh backend process reloads the same empty logical desktop.
  assert status()['workspaces'] and logical(created['name'])['logicalId']==created['logicalId']
  before={name:native(name)['id'] for name in ('desktop-visible','desktop-minimized','desktop-hidden')}
  mutate('desktop-remove',source=source['logicalId'],destination=created['logicalId'])
  state=status();assert source['logicalId'] not in {item['logicalId'] for item in state['workspaces']}
  assert {name:native(name)['id'] for name in before}==before
  assert native('desktop-minimized')['lumen_state']['minimised'] and native('desktop-hidden')['lumen_state']['hidden']
  assert native('desktop-minimized')['lumen_state']['restore_workspace']==created['name']
  assert native('desktop-hidden')['lumen_state']['restore_workspace']==created['name']
  for pid in (visible_pid,mini_pid,hidden_pid):os.kill(pid,0)
  checked('[con_id=%d] scratchpad show'%mini['id']);checked('[con_id=%d] lumen_window unhide'%hidden['id'])
  wait(lambda:all(window['workspace']==created['name'] for window in status()['windows'] if window['id'] in before.values()))
  checks=['Empty logical desktop survives separate backend processes without native focus change','Populated removal migrates visible, minimized and hidden saved placements without closing or changing PID/container identity','Restoring minimized and hidden windows uses surviving desktop']
  for identifier in before.values():checked('[con_id=%d] lumen_window minimize'%identifier)
  checked('focus output HEADLESS-2')
  dormant=mutate('desktop-create',outputId=output)['created']
  mutate('desktop-remove',source=created['logicalId'],destination=dormant['logicalId'])
  assert next(item['output'] for item in sway('-t','get_workspaces') if item.get('focused'))=='HEADLESS-2'
  checked('focus output HEADLESS-1; workspace "dormant-test-away"; focus output HEADLESS-2')
  assert dormant['name'] not in {item['name'] for item in sway('-t','get_workspaces')}
  checked('[con_id=%d] scratchpad show'%mini['id'])
  wait(lambda:any(window['id']==mini['id'] and window['workspace']==dormant['name'] and window['output']=='HEADLESS-1' for window in status()['windows']))
  for identifier in (first['id'],hidden['id']):checked('[con_id=%d] scratchpad show'%identifier)
  created=dormant;checks.append('Removing a fully minimized desktop preserves hidden state; restore recreates dormant destination on its saved output even when another display was focused')
  second=mutate('desktop-create',outputId=output)['created'];third=mutate('desktop-create',outputId=output)['created']
  order=[item['logicalId'] for item in status()['workspaces'] if item['outputId']==output];mutate('desktop-reorder',outputId=output,order=list(reversed(order)))
  assert not status()['auto_reorder'];assert [item['logicalId'] for item in status()['workspaces'] if item['outputId']==output]==list(reversed(order))
  # Dormant identity on a disconnected output moves to a reachable output.
  output_b=next(item['logicalId'] for item in status()['outputs'] if item['name']=='HEADLESS-2')
  remote=mutate('desktop-create',outputId=output_b)['created'];checked('output HEADLESS-2 disable')
  wait(lambda:logical(remote['name'])['output']=='HEADLESS-1');assert logical(remote['name'])['available']
  checks.append('Manual order is identity-preserving and disables recent order; dormant desktop remains reachable after output disconnect')
  (a.output/'backend-checks.json').write_text(json.dumps(dict(passed=True,checks=checks),indent=2))
  palette=json.loads((repo/'res/lumen-desktop/appearance/palette.json').read_text());appearance=private/'appearance.json';appearance.write_text(json.dumps(dict(font=palette['font'],palette=palette['dark'],preferences={})));env['LUMEN_APPEARANCE_STATE']=str(appearance)
  # Real Overview, production widgets and native pointer/keyboard.
  shutil.copytree(repo/'res/lumen-desktop',private/'assets');qml=private/'shell.qml'
  qml.write_text('''import QtQuick
import QtQuick.Window
import Quickshell
import Quickshell.Io
import "assets/shell" as D
ShellRoot {
 QtObject {id:stub;function claimPopup(owner){}}
 D.WindowOverview {id:overview;shell:stub}
 function find(item,name){if(item.objectName===name || (item.Accessible && item.Accessible.name===name))return item;for(const child of item.children || []){const found=find(child,name);if(found)return found;}if(item.contentItem && item.contentItem!==item){const found=find(item.contentItem,name);if(found)return found;}return null;}
 IpcHandler {target:"desktopAudit"
  function openOverview(id:string):bool{overview.showForWindow(Number(id));return overview.visible;}
  function refreshOverview():void{overview.send({action:"status"});}
  function status():string{return JSON.stringify({visible:overview.visible,busy:overview.busy,error:overview.error,workspaces:overview.workspaces,revision:overview.revision,editing:find(overview.contentItem,"desktop-order-pointer").parent.editing,groups:overview.windowGroups,managed:overview.managedDesktopId});}
  function geometry(name:string):string{const item=find(overview.contentItem.Window.window.contentItem,name);if(!item)return "null";const at=item.mapToItem(overview.contentItem,0,0);return JSON.stringify({x:at.x,y:at.y,width:item.width,height:item.height,title:overview.title});}
  function selectWindow(id:string):bool{const index=overview.matches.findIndex(w=>w.id===Number(id));if(index<0)return false;overview.selected=index;return true;}
  function focusControl(name:string):bool{const item=find(overview.contentItem.Window.window.contentItem,name);if(!item)return false;item.forceActiveFocus();return true;}
  function click(name:string):bool{const item=find(overview.contentItem.Window.window.contentItem,name);if(!item || !item.enabled)return false;item.clicked();return true;}
 }
}''')
  probe=private/'assets/shell/WindowOverview.qml';probe.write_text(probe.read_text().replace('Process {id:worker;stdinEnabled:true','Process {id:worker;stdinEnabled:true\n        stderr:StdioCollector {onStreamFinished:console.log("WORKER_STDERR:"+text)}').replace('stdout:StdioCollector {onStreamFinished:{try{','stdout:StdioCollector {onStreamFinished:{console.log("WORKER_STDOUT:"+text);try{'))
  launch(['quickshell','-p',str(qml)],'overview.log')
  def ipc(method,*args):return call('quickshell','-p',str(qml),'ipc','call','desktopAudit',method,*map(str,args))
  def ui():
   value=json.loads(ipc('status'));(a.output/'last-ui.json').write_text(json.dumps(value,indent=2));return value
  wait(lambda:ui().get('visible') is False);assert ipc('openOverview',first['id'])=='true';wait(lambda:ui()['visible'] and not ui()['busy'] and ui()['workspaces']);assert not ui()['error'];call('grim',str(a.output/'overview.png'))
  launch(['wtype','-s','60000'],'keyboard.log')
  pointer=sp.Popen([a.pointer_tool,'pointer','move','0','0'],env=env,stdin=sp.PIPE,text=True,start_new_session=True);processes.append(pointer)
  def point(name,ratio=.5):
   geometry=json.loads(ipc('geometry',name));overview=next(node for node in nodes(sway('-t','get_tree')) if node.get('name')=='Window Overview');rect=overview['rect'];return rect['x']+geometry['x']+geometry['width']*ratio,rect['y']+geometry['y']+geometry['height']*.5
  def pointer_command(text):pointer.stdin.write(text+'\n');pointer.stdin.flush();time.sleep(.07)
  pointer_state={'pressed':False,'position':(0,0)}
  def move(point):
   if pointer_state['pressed']:
    previous=pointer_state['position'];pointer_command('m %f %f'%(point[0]-previous[0],point[1]-previous[1]))
   else:
    checked('seat seat0 cursor set %d %d'%point);pointer_command('m 0.5 0.5')
   pointer_state['position']=point
  def press():pointer_command('p');pointer_state['pressed']=True
  def release():pointer_command('r');pointer_state['pressed']=False
  def key(*args):sp.run(['wtype',*args],env=env,check=True,timeout=5)
  def order_now():return [item['logicalId'] for item in status()['workspaces'] if item['outputId']==output]
  original_order=order_now();first_id=original_order[0];last_id=original_order[-1]
  move(point('desktop-'+first_id));press();move(point('desktop-'+last_id));wait(lambda:ui()['editing']);key('-k','Escape');release();assert order_now()==original_order and ui()['visible']
  checks.append('Escape during genuine held desktop drag cancels order and leaves Overview open')
  move(point('desktop-'+first_id));press();end=point('desktop-'+last_id);move(end);move((end[0],end[1]+70));release();assert order_now()==original_order and ui()['visible']
  checks.append('Releasing a genuine reorder outside the desktop strip cancels without policy writes')
  # Pointer reorder commits only on release.
  move(point('desktop-'+first_id));press();move(point('desktop-'+last_id));assert order_now()==original_order;release();wait(lambda:order_now()!=original_order);wait(lambda:not ui()['busy'])
  reordered=order_now();checks.append('Native pointer release commits exact same-output order; no preview writes policy')
  assert ipc('click','Move desktop left')=='true';wait(lambda:ui()['editing']);key('-k','Escape');assert order_now()==reordered
  assert ipc('click','Move desktop left')=='true';key('-k','Return');wait(lambda:not ui()['editing']);wait(lambda:not ui()['busy']);wait(lambda:order_now()!=reordered);checks.append('Keyboard order preview cancels on Escape and commits on Return')
  # The optional group workflow uses real production dialog controls.
  move(point('window-groups-open'));press();release()
  call('grim',str(a.output/'groups-open.png'))
  wait(lambda:json.loads(ipc('geometry','window-groups-toggle')) is not None)
  wait(lambda:not ui()['busy'])
  move(point('window-groups-toggle'));press();release();wait(lambda:status()['windowGroups']['enabled']);wait(lambda:not ui()['busy'])
  assert not ui()['error'];call('grim',str(a.output/'window-groups.png'))
  groups=status()['windowGroups'];member=groups['groups'][0]['members'][0]
  assert ipc('selectWindow',member['id'])=='true'
  initial_revision=groups['revision'];start=point('window-group-create');move(start);press();move((start[0]+260,start[1]+70));release()
  assert status()['windowGroups']['revision']==initial_revision
  move(point('window-group-create'));press();release();wait(lambda:status()['windowGroups']['revision']>initial_revision);wait(lambda:not ui()['busy'])
  assert any(m['id']==member['id'] and m['pid']==member['pid'] for g in status()['windowGroups']['groups'] for m in g['members'])
  checks.append('Production New Group commits on release; releasing outside cancels without native membership writes')
  assert ipc('focusControl','window-groups-toggle')=='true';key('-k','space');wait(lambda:not status()['windowGroups']['enabled']);wait(lambda:not ui()['busy'])
  key('-k','Escape');assert ui()['visible']
  checks.append('Production Window Groups dialog enables by native pointer and disables by native keyboard without closing Overview')
  # Production New and removal Cancel controls.
  count=len(status()['workspaces']);assert ipc('click','Create desktop')=='true';wait(lambda:len(status()['workspaces'])==count+1);wait(lambda:not ui()['busy']);ipc('refreshOverview');wait(lambda:not ui()['busy'])
  assert ipc('click','Remove desktop')=='true';key('-k','Escape');assert len(status()['workspaces'])==count+1
  checks.append('Overview creates a durable desktop; Escape from removal confirmation preserves every desktop')
  (a.output/'result.json').write_text(json.dumps(dict(passed=True,checks=checks,final=status()),indent=2)+'\n')
 finally:
  if 'pointer' in locals() and pointer.stdin:pointer.stdin.close()
  for process in reversed(processes):
   if process.poll() is None:os.killpg(process.pid,signal.SIGTERM)
  for process in reversed(processes):
   try:process.wait(timeout=4)
   except sp.TimeoutExpired:os.killpg(process.pid,signal.SIGKILL);process.wait(timeout=4)
  if 'bus' in locals() and bus.stdout:bus.stdout.close()
print('PASS native desktop lifecycle and Overview order')
