"""Production recent-group panels with real private pointer/keyboard and native scope."""
import argparse,json,os,re,select,shlex,shutil,signal,subprocess as sp,tempfile,time
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--effects',action='store_true');p.add_argument('--launcher',type=Path,required=True);p.add_argument('--sway',required=True);p.add_argument('--client',required=True);p.add_argument('--pointer-tool',required=True);p.add_argument('--locker',required=True);p.add_argument('--output',type=Path,required=True);a=p.parse_args();a.output.mkdir(parents=True,exist_ok=False)
repo=Path(__file__).resolve().parents[2];processes=[]
with tempfile.TemporaryDirectory(prefix='lumen-group-selector-') as temporary:
 private=Path(temporary);prefix=re.split(r'\n[ \t]*exec quickshell ',a.launcher.read_text(),maxsplit=1)[0]
 env=dict(value.decode().split('=',1) for value in sp.check_output(['bash','-c',prefix+'\nexec env -0']).split(b'\0') if b'=' in value)
 for name in ('HOME','XDG_RUNTIME_DIR','XDG_STATE_HOME','XDG_DATA_HOME','XDG_CONFIG_HOME','XDG_CACHE_HOME'):env[name]=str(private/name);Path(env[name]).mkdir(mode=0o700)
 for name in ('SWAYSOCK','WAYLAND_DISPLAY','DISPLAY','DBUS_SESSION_BUS_ADDRESS','AT_SPI_BUS_ADDRESS'):env.pop(name,None)
 env.update(WLR_BACKENDS='headless',WLR_HEADLESS_OUTPUTS='2',WLR_RENDERER='gles2' if a.effects else 'pixman',WLR_RENDERER_ALLOW_SOFTWARE='1',LIBGL_ALWAYS_SOFTWARE='1',WLR_LIBINPUT_NO_DEVICES='1',QT_QPA_PLATFORM='wayland',QT_QUICK_BACKEND='software',QT_QPA_PLATFORMTHEME='generic',QT_NO_XDG_DESKTOP_PORTAL='1',QT_ACCESSIBILITY='0',QT_LINUX_ACCESSIBILITY_ALWAYS_ON='0',NO_AT_BRIDGE='1')
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
  qml.write_text('''import QtQuick
import Quickshell
import Quickshell.Io
import "assets/shell" as D
ShellRoot {
 QtObject {id:stub;property var groupSurfaces:({});property string popupOwner:"";property int focusEpoch:0;signal requestGroupSelector(string name);signal dismissPopups(string owner);function claimPopup(owner){focusEpoch++;popupOwner=owner;dismissPopups(owner);}}
 D.WindowGroupState {id:groupState}
 Variants {model:Quickshell.screens;D.WindowGroupSelector {required property var modelData;screen:modelData;outputName:modelData.name;shell:stub;service:groupState}}
 function find(item,name){if(item.objectName===name)return item;for(const child of item.children || []){const found=find(child,name);if(found)return found;}return null;}
 IpcHandler {target:"groupAudit"
  function status():string{return JSON.stringify({state:groupState.state,surfaces:Object.values(stub.groupSurfaces).map(s=>({output:s.outputName,visible:s.visible,revealed:s.revealed,keyboard:s.keyboardMode,rows:s.rows,capture:s.capture,selected:s.selected,commits:s.commits,error:s.error}))});}
  function beginFocused():void{stub.requestGroupSelector(groupState.state.focusedOutput || "");}
  function begin(name:string):void{stub.requestGroupSelector(name);}
  function stopObserver():void{groupState.observer.running=false;}
  function geometry(output:string,name:string):string{const panel=stub.groupSurfaces[output];if(!panel)return "null";const item=find(panel.contentItem,name);if(!item)return "null";const point=item.mapToItem(panel.contentItem,0,0);return JSON.stringify({x:panel.screen.x+point.x,y:panel.screen.y+point.y,width:item.width,height:item.height});}
 }
}''')
  shell=launch(['quickshell','-p',str(qml)],'selector.log')
  def ipc(method,*args):return call('quickshell','-p',str(qml),'ipc','call','groupAudit',method,*map(str,args))
  def ui():
   value=json.loads(ipc('status'));(a.output/'last-ui.json').write_text(json.dumps(value,indent=2));return value
  def surface(output):return next((s for s in ui()['surfaces'] if s['output']==output),None)
  wait(lambda:ui()['state'].get('available') is True and len(ui()['surfaces'])==2)
  assert not ui()['state']['enabled'] and all(not s['visible'] for s in ui()['surfaces'])
  group_action('on');wait(lambda:all(s['visible'] and not s['revealed'] for s in ui()['surfaces']))
  initial=groups()['revision'];initial_h2=next(g['id'] for g in groups()['groups'] if any(m['id']==captured[3]['id'] for m in g['members']))
  pointer=launch([a.pointer_tool,'pointer','move','0','0'],'pointer.log',stdin=sp.PIPE,text=True)
  held=False;position=(0,0)
  def pointer_event(text):pointer.stdin.write(text+'\n');pointer.stdin.flush();time.sleep(.07)
  def move(x,y):
   global position
   if held:pointer_event('m %f %f'%(x-position[0],y-position[1]))
   else:command('seat seat0 cursor set %d %d'%(x,y));pointer_event('m 0.5 0.5')
   position=(x,y)
  def point(output,name):
   value=json.loads(ipc('geometry',output,name));return value['x']+value['width']*.5,value['y']+value['height']*.5
  def press():
   global held
   pointer_event('p');held=True
  def release():
   global held
   pointer_event('r');held=False
  def key(*args):sp.run(['wtype',*args],env=env,check=True,timeout=4)
  def shortcut():key('-M','logo','-M','ctrl','-k','g','-m','ctrl','-m','logo')
  move(2,300);wait(lambda:surface('HEADLESS-1')['revealed']);assert not surface('HEADLESS-1')['keyboard'];assert groups()['revision']==initial
  expected={g['id'] for g in groups()['groups'] if g['workspace']==surface('HEADLESS-1')['capture']['workspace']}
  assert {row['id'] for row in surface('HEADLESS-1')['rows']}==expected and initial_h2 not in expected
  call('grim',str(a.output/'recent-groups.png'))
  move(700,700);wait(lambda:not surface('HEADLESS-1')['revealed']);assert groups()['revision']==initial
  checks.append('Mode off has no selector; enabled mode starts concealed; pointer edge reveal/leave changes no native membership and is output/visible-desktop scoped')
  command('[con_id='+str(captured[0]['id'])+'] focus');wait(lambda:ui()['state']['focusedOutput']=='HEADLESS-1');shortcut();wait(lambda:surface('HEADLESS-1')['keyboard'])
  key('-k','Down');key('-k','Escape');wait(lambda:not surface('HEADLESS-1')['revealed']);assert groups()['revision']==initial
  shortcut();wait(lambda:surface('HEADLESS-1')['keyboard']);move(700,700);press();release();wait(lambda:not surface('HEADLESS-1')['revealed']);assert groups()['revision']==initial
  shortcut();wait(lambda:surface('HEADLESS-1')['keyboard']);key('-k','Down');value=surface('HEADLESS-1');target=value['rows'][value['selected']]['id'];key('-k','Return')
  wait(lambda:surface('HEADLESS-1')['commits']==1);assert next(g for g in groups()['groups'] if g['id']==target)['selected'];assert next(g for g in groups()['groups'] if g['id']==initial_h2)['selected']
  checks.append('Real configured keyboard shortcut previews, Escape cancels and Return selects an exact group without touching another output')
  move(2,300);wait(lambda:surface('HEADLESS-1')['revealed']);value=surface('HEADLESS-1');target=next(row['id'] for row in value['rows'] if not row['selected']);before=groups()['revision'];move(*point('HEADLESS-1','window-group-row-'+str(target)));press();key('-k','Escape');release();assert groups()['revision']==before
  move(700,700);move(2,300);wait(lambda:surface('HEADLESS-1')['revealed']);move(*point('HEADLESS-1','window-group-row-'+str(target)));press();assert groups()['revision']==before;release();wait(lambda:surface('HEADLESS-1')['commits']==2);assert next(g for g in groups()['groups'] if g['id']==target)['selected']
  checks.append('Real held row press/Escape cancels; pointer release commits captured group only')
  move(700,700);move(2,300);wait(lambda:surface('HEADLESS-1')['revealed']);old=surface('HEADLESS-1')['capture'];gid=surface('HEADLESS-1')['rows'][0]['id'];command('workspace 2');wait(lambda:not surface('HEADLESS-1')['visible'])
  command('lumen_window groups '+str(groups()['revision'])+' select-scoped '+str(gid)+' '+str(old['workspace'])+' "HEADLESS-1"',False)
  assert next(ws for ws in native('get_workspaces') if ws['focused'])['name']=='2'
  command('workspace 1');wait(lambda:surface('HEADLESS-1')['visible']);ipc('begin','HEADLESS-2');wait(lambda:surface('HEADLESS-2')['keyboard']);assert all(row['id']==initial_h2 for row in surface('HEADLESS-2')['rows']);command('output HEADLESS-2 disable');wait(lambda:all(s['output']!='HEADLESS-2' for s in ui()['surfaces']))
  checks.append('Visible-workspace change cancels and native scope guard rejects stale cross-desktop activation; output disconnect removes its keyboard surface')
  ipc('begin','HEADLESS-1');wait(lambda:surface('HEADLESS-1')['keyboard']);locker=launch([a.locker],'locker.log',stdin=sp.PIPE,text=True);wait(lambda:ui()['state']['locked'] and not surface('HEADLESS-1')['visible']);assert not surface('HEADLESS-1')['revealed']
  locker.stdin.write('u\n');locker.stdin.flush();assert locker.wait(timeout=4)==0;wait(lambda:surface('HEADLESS-1')['visible']);assert not surface('HEADLESS-1')['revealed']
  ipc('begin','HEADLESS-1');wait(lambda:surface('HEADLESS-1')['keyboard']);ipc('stopObserver');wait(lambda:not ui()['state']['available'] and not surface('HEADLESS-1')['visible']);assert not surface('HEADLESS-1')['revealed']
  checks.append('Actual native lock and observer disconnect close panels and release keyboard; unlock remains concealed')
  warnings=(a.output/'selector.log').read_text();assert ' WARN' not in warnings and ' ERROR' not in warnings,warnings
  (a.output/'result.json').write_text(json.dumps(dict(passed=True,checks=checks),indent=2)+'\n')
 finally:
  for process in reversed(processes):
   if process.poll() is None:os.killpg(process.pid,signal.SIGTERM)
  for process in reversed(processes):
   try:process.wait(timeout=4)
   except sp.TimeoutExpired:os.killpg(process.pid,signal.SIGKILL);process.wait(timeout=4)
   if process.stdin and not process.stdin.closed:process.stdin.close()
  if 'bus' in locals() and bus.stdout:bus.stdout.close()
print('PASS recent-group selector native UI')
