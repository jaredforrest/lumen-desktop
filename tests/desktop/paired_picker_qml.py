"""Real menu and paired-window picker with exact compositor targets."""
from fixture_resources import close_reaped_pipes
import argparse,json,os,re,select,shutil,signal,subprocess as sp,tempfile,time
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--launcher',required=True);p.add_argument('--sway',required=True);p.add_argument('--output',type=Path,required=True);p.add_argument('--keyboard',default='wtype');a=p.parse_args();a.output.mkdir(parents=True,exist_ok=False)
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
  with (a.output/name).open('w') as _fixture_log0:
      proc=sp.Popen(args,env=env,stdout=_fixture_log0,stderr=sp.STDOUT,start_new_session=True,**kwargs);processes.append(proc);return proc
 def call(*args):return sp.check_output(args,env=env,text=True,timeout=5)
 def sway(command):
  response=json.loads(call('swaymsg','-r',command));assert all(x.get('success') for x in response),response
 def nodes(tree):
  yield tree
  for child in tree.get('nodes',[])+tree.get('floating_nodes',[]):yield from nodes(child)
 def windows():return {n.get('app_id'):n for n in nodes(json.loads(call('swaymsg','-r','-t','get_tree'))) if n.get('app_id') in ('overview-source','overview-bystander','overview-replacement')}
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
  with (a.output/'bus.log').open('w') as _fixture_log0:
      bus=sp.Popen(['dbus-daemon','--nofork','--print-address=1','--config-file='+str(bus_config)],env=env,stdout=sp.PIPE,stderr=_fixture_log0,text=True,start_new_session=True);processes.append(bus)
  assert select.select([bus.stdout],[],[],5)[0],'Private bus did not start';env['DBUS_SESSION_BUS_ADDRESS']=bus.stdout.readline().strip();assert env['DBUS_SESSION_BUS_ADDRESS'].startswith('unix:')
  config=private/'sway.conf';config.write_text('output * resolution 1200x800\nseat seat0 fallback true\n')
  launch([a.sway,'--unsupported-gpu','-c',str(config)],'sway.log');env['SWAYSOCK']=str(wait(lambda:next(Path(env['XDG_RUNTIME_DIR']).glob('sway-ipc*.sock'),None)));env['WAYLAND_DISPLAY']=wait(lambda:next((p.name for p in Path(env['XDG_RUNTIME_DIR']).glob('wayland-*') if not p.name.endswith('.lock')),None))
  launch(['foot','-o','resize-by-cells=no','--app-id=overview-source','sleep','180'],'source.log');launch(['foot','-o','resize-by-cells=no','--app-id=overview-bystander','sleep','180'],'bystander.log');wait(lambda:len(windows())==2)
  source=windows()['overview-source'];other=windows()['overview-bystander'];cid=source['id'];bid=other['id'];bws=workspace(bid);sway(f'[con_id={cid}] move container to workspace 2')
  launch(['foot','-o','resize-by-cells=no','--app-id=overview-replacement','sleep','180'],'replacement.log');wait(lambda:len(windows())==3);rid=windows()['overview-replacement']['id']
  shutil.copytree(root/'res/lumen-desktop',private/'lumen-desktop');assets=private/'lumen-desktop/shell'
  appearance=Path(env['XDG_STATE_HOME'])/'lumen-desktop/appearance.json';appearance.parent.mkdir(parents=True);appearance.write_text(json.dumps({'palette':{'background':'#f2f2f4','surface':'#ffffff','field':'#e5e5e8','text':'#202023','muted':'#6b6b70','border':'#c8c8ce','hover':'#dedee3','accent':'#007aff','selectedText':'#ffffff'},'font':'sans-serif','preferences':{}}))
  fixture=private/'shell.qml'
  fixture.write_text('''import QtQuick
import Quickshell
import Quickshell.Io
import "lumen-desktop/shell" as D
ShellRoot {
 QtObject {id:stub;property string popupOwner:"";signal dismissPopups(string owner);function claimPopup(owner){popupOwner=owner;dismissPopups(owner);}function pairWindow(id,side){picker.open(id,side);}}
 D.WindowLayoutMenu {id:menu;shell:stub}
 D.WindowPairPicker {id:picker;shell:stub}
 function findText(item,text){if(item && item.text===text)return item;for(const child of (item && item.children || [])){const found=findText(child,text);if(found)return found;}return null;}
 IpcHandler {target:"pairTest"
  function route(id:string,side:string):void{menu.target=Number(id);menu.updateCapabilities(JSON.stringify({native_window_layouts:true,native_paired_layouts:true}));menu.apply(menu.choices.findIndex(choice=>choice.action==="pair-"+side));}
  function select(id:string):void{picker.selected=picker.candidates.findIndex(item=>item.id===Number(id));}
  function click(text:string):bool{const button=findText(picker.contentItem,text);if(button && button.enabled){button.clicked();return true;}return false;}
  function status():string{return JSON.stringify({visible:picker.visible,busy:picker.busy,source:picker.source,current:picker.current,side:picker.side,error:picker.error});}
 }
}
''')
  launch([a.keyboard,'-s','60000'],'keyboard.log')
  launch(['quickshell','-p',str(fixture)],'picker.log')
  def ipc(method,*args):return call('quickshell','-p',str(fixture),'ipc','call','pairTest',method,*map(str,args))
  def state():
   try:return json.loads(ipc('status'))
   except (sp.SubprocessError,ValueError):return {}
  def node(cid):return next((n for n in nodes(json.loads(call('swaymsg','-r','-t','get_tree'))) if n['id']==cid),None)
  def loaded(cid):
   value=state();return value if (value.get('source') or {}).get('id')==cid and not value.get('busy',True) else None
  def click(text):assert ipc('click',text).strip()=='true',(text,state())
  wait(lambda:state().get('visible') is False);ipc('route',cid,'left');wait(lambda:loaded(cid));ipc('select',bid);assert state()['current']['id']==bid;sp.run([a.keyboard,'-k','Return'],env=env,check=True);wait(lambda:not state()['visible'])
  assert node(cid)['lumen_pair']['partner']==bid and node(cid)['lumen_pair']['side']=='left'
  # The management overlay remains visible above the real full-screen pair.
  ipc('route',cid,'manage');wait(lambda:loaded(cid));call('grim',str(a.output/'pair-management.png'));click('Swap Sides');wait(lambda:loaded(cid) and state()['source']['pair']['side']=='right')
  sway(f'lumen_pair ratio {cid} 0.65');click('Reset Division');wait(lambda:abs(node(cid)['lumen_pair']['ratio']-.5)<.01 and loaded(cid))
  ipc('select',rid);click('Replace Partner');wait(lambda:not state()['visible']);assert node(cid)['lumen_pair']['partner']==rid and not node(bid)['lumen_pair'].get('id')
  ipc('route',cid,'manage');wait(lambda:loaded(cid));click('Return to Desktop');wait(lambda:not state()['visible']);assert not node(cid)['lumen_pair'].get('id') and node(rid)['lumen_pair']['survivor']
  ipc('route',rid,'manage');wait(lambda:loaded(rid));click('Return to Desktop');wait(lambda:not state()['visible']);assert node(rid)['fullscreen_mode']==0 and not node(rid)['lumen_pair']['survivor']
  # A closed selected partner produces an error, never another candidate action.
  ipc('route',cid,'right');wait(lambda:loaded(cid));ipc('select',bid);sway(f'[con_id={bid}] kill');wait(lambda:node(bid) is None);click('Create Pair');wait(lambda:'no longer available' in state().get('error',''))
  time.sleep(.3);assert not node(cid)['lumen_pair'].get('id') and not node(rid)['lumen_pair'].get('id');assert state()['current'] is None
  assert not any(word in (a.output/'picker.log').read_text() for word in (' WARN ',' ERROR ','TypeError:','ReferenceError:')),(a.output/'picker.log').read_text()
  (a.output/'result.json').write_text(json.dumps({'passed':True,'source':cid,'partner':bid,'replacement':rid,'final':state(),'checks':['real layout menu captured source routing','native create via production picker and physical Return','overlay usable above full-screen pair','swap exact identities','reset division','replace and restore old partner','exit and survivor restore','closed selected partner rejected without fallback']},indent=2));print('PASS real paired-window menu, picker and native lifecycle')
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
  for _fixture_process in processes:
      if _fixture_process.poll() is None:_fixture_process.wait(timeout=5)
  close_reaped_pipes(processes)
