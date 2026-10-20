"""Production recent-group panels with real private pointer/keyboard and native scope."""
from fixture_resources import close_reaped_pipes
import argparse,json,os,re,select,shlex,shutil,signal,subprocess as sp,tempfile,time
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--effects',action='store_true');p.add_argument('--launcher',type=Path,required=True);p.add_argument('--sway',required=True);p.add_argument('--client',required=True);p.add_argument('--pointer-tool',required=True);p.add_argument('--locker',required=True);p.add_argument('--output',type=Path,required=True);a=p.parse_args();a.output.mkdir(parents=True,exist_ok=False)
repo=Path(__file__).resolve().parents[2];processes=[]
with tempfile.TemporaryDirectory(prefix='lumen-group-selector-') as temporary:
 private=Path(temporary);prefix=re.split(r'\n[ \t]*exec quickshell ',a.launcher.read_text(),maxsplit=1)[0]
 env=dict(value.decode().split('=',1) for value in sp.check_output(['bash','-c',prefix+'\nexec env -0']).split(b'\0') if b'=' in value)
 for name in ('HOME','XDG_RUNTIME_DIR','XDG_STATE_HOME','XDG_DATA_HOME','XDG_CONFIG_HOME','XDG_CACHE_HOME'):env[name]=str(private/name);Path(env[name]).mkdir(mode=0o700)
 for name in ('SWAYSOCK','WAYLAND_DISPLAY','DISPLAY','DBUS_SESSION_BUS_ADDRESS','AT_SPI_BUS_ADDRESS'):env.pop(name,None)
 env.update(WLR_BACKENDS='headless',WLR_HEADLESS_OUTPUTS='2',WLR_RENDERER='gles2' if a.effects else 'pixman',WLR_RENDERER_FORCE_SOFTWARE='1' if a.effects else '0',WLR_RENDERER_ALLOW_SOFTWARE='1',LIBGL_ALWAYS_SOFTWARE='1',WLR_LIBINPUT_NO_DEVICES='1',QT_QPA_PLATFORM='wayland',QT_QUICK_BACKEND='software',QT_QPA_PLATFORMTHEME='generic',QT_NO_XDG_DESKTOP_PORTAL='1',QT_ACCESSIBILITY='0',QT_LINUX_ACCESSIBILITY_ALWAYS_ON='0',NO_AT_BRIDGE='1')
 def launch(args,name,**kwargs):
  with (a.output/name).open('w') as stream:proc=sp.Popen(args,env=env,stdout=stream,stderr=sp.STDOUT,start_new_session=True,**kwargs)
  processes.append(proc);return proc
 def call(*args):return sp.check_output(args,env=env,text=True,timeout=5).strip()
 swaymsg=str(Path(a.sway).parent/'swaymsg')
 def native(kind):return json.loads(call(swaymsg,'-r','-t',kind))
 def command(value,success=True):
  result=json.loads(sp.run([swaymsg,'-r',value],env=env,text=True,stdout=sp.PIPE,timeout=4,check=False).stdout);assert result and all(item.get('success') is success for item in result),(value,result)
 def wait(predicate):
  deadline=time.monotonic()+15
  while time.monotonic()<deadline:
   try:value=predicate()
   except (sp.CalledProcessError,sp.TimeoutExpired,json.JSONDecodeError):value=None
   if value:return value
   time.sleep(.04)
  raise AssertionError('Group selector timeout: '+str(a.output)+'; compositor log: '+(a.output/'sway.log').read_text())
 def nodes(node):
  yield node
  for child in node.get('nodes',[])+node.get('floating_nodes',[]):yield from nodes(child)
 def window(n):return next((row for row in nodes(native('get_tree')) if row.get('name')=='Relationship '+str(n)),None)
 def groups():return native('get_tree')['window_groups']
 def group_action(action):command('lumen_window groups '+str(groups()['revision'])+' '+action)
 qml=private/'shell.qml'
 checks=[]
 try:
  busconf=private/'bus.conf';busconf.write_text('<busconfig><type>session</type><listen>unix:tmpdir='+env['XDG_RUNTIME_DIR']+'</listen><policy context="default"><allow own="*"/><allow send_destination="*"/><allow receive_sender="*"/></policy></busconfig>')
  with (a.output/'bus.log').open('w') as log:bus=sp.Popen(['dbus-daemon','--nofork','--print-address=1','--config-file='+str(busconf)],env=env,stdout=sp.PIPE,stderr=log,text=True,start_new_session=True)
  processes.append(bus);assert select.select([bus.stdout],[],[],5)[0];env['DBUS_SESSION_BUS_ADDRESS']=bus.stdout.readline().strip()
  shortcut=shlex.join(['quickshell','-p',str(qml),'ipc','call','groupAudit','beginFocused'])
  config=private/'sway.conf';config.write_text('output HEADLESS-1 resolution 1200x800 position 0 0\noutput HEADLESS-2 resolution 1000x800 position 1200 0\nseat seat0 fallback true\nfocus_follows_mouse no\nxwayland disable\nfor_window [all] floating enable, border pixel 0\nbindsym Mod4+Control+g exec '+shortcut+'\n')
  compositor=launch([a.sway,'--unsupported-gpu','-c',str(config)],'sway.log');env['SWAYSOCK']=str(wait(lambda:next(Path(env['XDG_RUNTIME_DIR']).glob('sway-ipc*.sock'),None)));env['WAYLAND_DISPLAY']=wait(lambda:next((item.name for item in Path(env['XDG_RUNTIME_DIR']).glob('wayland-*') if not item.name.endswith('.lock')),None))
  launch(['wtype','-s','60000'],'keyboard.log')
  command('focus output HEADLESS-1; workspace 1; move workspace to output HEADLESS-1')
  client=launch([a.client],'client.log',stdin=sp.PIPE,text=True);wait(lambda:all(window(i) for i in range(4)))
  captured={i:window(i) for i in range(4)}
  command('[con_id='+str(captured[3]['id'])+'] move container to workspace 3');command('workspace 3; move workspace to output HEADLESS-2');command('focus output HEADLESS-1; workspace 1')
  appearance=Path(env['XDG_STATE_HOME'])/'lumen-desktop';appearance.mkdir();(appearance/'appearance.json').write_text(json.dumps(dict(font='sans-serif',palette=dict(background='#242426',surface='#303033',field='#363639',text='#eeeeef',muted='#aaaab0',border='#505054',hover='#414145',accent='#007aff',selectedText='#ffffff'),preferences=dict(mode='dark',reduceMotion=True))))
  shutil.copytree(repo/'res/lumen-desktop',private/'assets')
  qml.write_text('import QtQuick\nimport QtQuick.Window\nimport Quickshell\nimport Quickshell.Io\nimport "assets/shell" as D\nShellRoot {\n QtObject {id:stub;property var groupSurfaces:({});property var groupMembershipDrag:dragController;property string popupOwner:"";property int focusEpoch:0;signal requestGroupSelector(string name);signal dismissPopups(string owner);function claimPopup(owner){focusEpoch++;popupOwner=owner;dismissPopups(owner);}}\n D.WindowGroupState {id:groupState}\n D.GroupMembershipDrag {id:dragController;shell:stub;service:groupState}\n D.WindowOverview {id:overview;shell:stub}\n Variants {model:Quickshell.screens;D.WindowGroupSelector {required property var modelData;screen:modelData;outputName:modelData.name;shell:stub;service:groupState}}\n function find(item,name){if(item.objectName===name)return item;for(const child of item.children || []){const found=find(child,name);if(found)return found;}return null;}\n IpcHandler {target:"groupAudit"\n  function status():string{return JSON.stringify({state:groupState.state,visible:overview.visible,busy:overview.busy,revision:overview.windowGroups?overview.windowGroups.revision:-1,error:overview.error,drag:{active:dragController.active,keyboard:dragController.keyboardMode,source:dragController.source,target:dragController.target,destinations:dragController.destinations,selected:dragController.selected,commits:dragController.commits,error:dragController.error},surfaces:Object.values(stub.groupSurfaces).map(s=>({output:s.outputName,revealed:s.revealed,membership:s.membershipMode,rows:s.rows}))});}\n  function open(id:string):void{overview.showForWindow(Number(id));}\n  function refresh():void{overview.send({action:"status"});}\n  function forgedOfferAccepted():bool{return dragController.accepts({getDataAsString:function(){return "forged";}});}\n  function geometry(name:string,output:string):string{let panel=null,item=null;if(output){panel=stub.groupSurfaces[output];if(panel)item=find(panel.contentItem,name);}else{panel=overview.contentItem.Window.window;item=find(panel.contentItem,name);}if(!item || !panel)return "null";const point=item.mapToItem(panel.contentItem,0,0);return JSON.stringify({x:(output?panel.screen.x:0)+point.x,y:(output?panel.screen.y:0)+point.y,width:item.width,height:item.height,enabled:item.enabled});}\n  function focusControl(name:string):bool{const item=find(overview.contentItem.Window.window.contentItem,name);if(!item)return false;item.forceActiveFocus();return true;}\n }\n}')
  shell=launch(['quickshell','-p',str(qml)],'membership.log')
  def ipc(method,*args):return call('quickshell','-p',str(qml),'ipc','call','groupAudit',method,*map(str,args))
  def ui():
   value=json.loads(ipc('status'));(a.output/'last-ui.json').write_text(json.dumps(value,indent=2));return value
  def group(n):return next(g for g in groups()['groups'] if any(m['id']==captured[n]['id'] for m in g['members']))
  wait(lambda:ui()['state'].get('available') is True)
  group_action('on');wait(lambda:ui()['state']['enabled'])
  ipc('open',captured[1]['id']);wait(lambda:ui()['visible'] and not ui()['busy'] and ui()['revision']==ui()['state']['revision']);assert not ui()['error']
  pointer=launch([a.pointer_tool,'pointer','move','0','0'],'pointer.log',stdin=sp.PIPE,text=True)
  held=False;position=(0,0)
  def pointer_event(text):pointer.stdin.write(text+'\n');pointer.stdin.flush();time.sleep(.07)
  def move(x,y):
   global position
   if held:pointer_event('m %f %f'%(x-position[0],y-position[1]))
   else:command('seat seat0 cursor set %d %d'%(x,y));pointer_event('m 0.5 0.5')
   position=(x,y)
  def point(name,output=''):
   value=json.loads(ipc('geometry',name,output));
   if not output:
    native_window=next(n for n in nodes(native('get_tree')) if n.get('name')=='Window Overview' and n.get('pid')==shell.pid);value['x']+=native_window['rect']['x'];value['y']+=native_window['rect']['y']
   assert value and value.get('enabled',True),(name,value);return value['x']+value['width']*.5,value['y']+value['height']*.5
  def press():
   global held
   pointer_event('p');held=True
  def release():
   global held
   pointer_event('r');held=False
  def key(*args):sp.run(['wtype',*args],env=env,check=True,timeout=4)
  def ready():
   ipc('refresh');wait(lambda:not ui()['busy'] and ui()['revision']==ui()['state']['revision'])
  def start(n):
   ready();start=point('group-drag-'+str(captured[n]['id']));move(*start);press();move(start[0]+14,start[1]+2);wait(lambda:ui()['drag']['active']);return start
  original=group(1)['id'];target=group(0)['id'];revision=groups()['revision']
  start(1);assert ipc('forgedOfferAccepted')=='false';move(*point('group-drop-'+str(target)));assert groups()['revision']==revision;key('-k','Escape');release();assert not ui()['drag']['active'] and group(1)['id']==original
  checks.append('Held native membership drag previews without writes; Escape retains exact source group')
  start(1);move(*point('group-drop-'+str(target)));assert ui()['drag']['target']['groupId']==target and groups()['revision']==revision;call('grim',str(a.output/'membership-preview.png'));release();wait(lambda:ui()['drag']['commits']==1);assert group(1)['id']==target
  checks.append('Valid Overview target release atomically adds captured native window to exact group')
  ready();assert ipc('focusControl','group-drag-'+str(captured[1]['id']))=='true';key('-k','space');wait(lambda:ui()['drag']['active'] and ui()['drag']['keyboard']);assert ui()['drag']['destinations'][0]['kind']=='new';key('-k','Return');wait(lambda:ui()['drag']['commits']==2);assert group(1)['id']!=target
  checks.append('Real keyboard equivalent detaches to a new group through the same captured native intent')
  start(1);move(2,300);wait(lambda:any(s['output']=='HEADLESS-1' and s['revealed'] for s in ui()['surfaces']));move(*point('window-group-row-'+str(target),'HEADLESS-1'));assert ui()['drag']['target']['groupId']==target;release();wait(lambda:ui()['drag']['commits']==3);assert group(1)['id']==target
  start(1);move(2,300);wait(lambda:any(s['output']=='HEADLESS-1' and s['revealed'] for s in ui()['surfaces']));move(*point('selector-group-drop-new','HEADLESS-1'));assert ui()['drag']['target']['kind']=='new';release();wait(lambda:ui()['drag']['commits']==4);assert group(1)['id']!=target
  checks.append('Real cross-surface held drag adds through recent selector and detaches through its New Group target')
  original=group(1)['id'];revision=groups()['revision'];start(1);move(900,700);release();assert groups()['revision']==revision and group(1)['id']==original
  start(1);source=dict(ui()['drag']['source']);group_action('remove '+str(group(0)['id'])+' '+str(captured[0]['id'])+' '+str(captured[0]['pid']));wait(lambda:not ui()['drag']['active']);release();assert group(1)['id']==original and ui()['drag']['commits']==4
  checks.append('Outside release and an independent native revision change cancel without retargeting source')
  start(1);source=dict(ui()['drag']['source']);command('workspace 2');wait(lambda:not ui()['drag']['active']);release();assert group(1)['id']==original
  command('lumen_window groups '+str(groups()['revision'])+' remove-scoped '+str(original)+' '+str(source['workspace'])+' "HEADLESS-1" '+str(captured[1]['id'])+' '+str(captured[1]['pid']),False)
  command('workspace 1');ready()
  start(1);locker=launch([a.locker],'locker.log',stdin=sp.PIPE,text=True);wait(lambda:ui()['state']['locked'] and not ui()['drag']['active']);release();locker.stdin.write('u\n');locker.stdin.flush();assert locker.wait(timeout=4)==0;wait(lambda:not ui()['state']['locked']);assert group(1)['id']==original
  checks.append('Visible-workspace change and actual session lock cancel; native atomic guard rejects hidden-desktop membership')
  start(1);command('output HEADLESS-1 disable');wait(lambda:not ui()['drag']['active']);release();assert group(1)['id']==original
  assert all(window(i)['id']==captured[i]['id'] and window(i)['pid']==captured[i]['pid'] for i in range(4))
  checks.append('Output disconnect cancels and all original native window/PID identities survive every operation')
  warnings=(a.output/'membership.log').read_text();assert ' WARN' not in warnings and ' ERROR' not in warnings,warnings
  (a.output/'result.json').write_text(json.dumps(dict(passed=True,checks=checks),indent=2)+'\n')
 finally:
  for process in reversed(processes):
   if process.poll() is None:os.killpg(process.pid,signal.SIGTERM)
  for process in reversed(processes):
   try:process.wait(timeout=4)
   except sp.TimeoutExpired:os.killpg(process.pid,signal.SIGKILL);process.wait(timeout=4)
   if process.stdin and not process.stdin.closed:process.stdin.close()
  if 'bus' in locals() and bus.stdout:bus.stdout.close()
  for _fixture_process in processes:
      if _fixture_process.poll() is None:_fixture_process.wait(timeout=5)
  close_reaped_pipes(processes)
print('PASS captured group membership native drag')
