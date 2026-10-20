"""Real per-display membership, future windows and policy-owned stickiness only."""
import argparse,hashlib,json,os,re,shutil,signal,subprocess as sp,sys,tempfile,time
from pathlib import Path
from private_native_bus import start_private_bus
p=argparse.ArgumentParser();p.add_argument('--launcher',type=Path,required=True);p.add_argument('--sway',required=True);p.add_argument('--client',type=Path,required=True);p.add_argument('--output',type=Path,required=True);p.add_argument('--effects',action='store_true');a=p.parse_args();a.output.mkdir(parents=True,exist_ok=False)
repo=Path(__file__).resolve().parents[2];processes=[];original_socket=os.environ.get('SWAYSOCK')
with tempfile.TemporaryDirectory(prefix='lumen-membership-') as temporary:
 private=Path(temporary);prefix=re.split(r'\n[ \t]*exec quickshell ',a.launcher.read_text(),maxsplit=1)[0]
 env=dict(v.decode().split('=',1) for v in sp.check_output(['bash','-c',prefix+'\nexec env -0']).split(b'\0') if b'=' in v)
 for name in ('HOME','XDG_RUNTIME_DIR','XDG_STATE_HOME','XDG_DATA_HOME','XDG_CONFIG_HOME','XDG_CACHE_HOME'):env[name]=str(private/name);Path(env[name]).mkdir(mode=0o700)
 for name in ('SWAYSOCK','WAYLAND_DISPLAY','DISPLAY','DBUS_SESSION_BUS_ADDRESS','AT_SPI_BUS_ADDRESS'):env.pop(name,None)
 env.update(WLR_BACKENDS='headless',WLR_HEADLESS_OUTPUTS='2',WLR_RENDERER='gles2' if a.effects else 'pixman',WLR_RENDERER_FORCE_SOFTWARE='1' if a.effects else '0',WLR_RENDERER_ALLOW_SOFTWARE='1',LIBGL_ALWAYS_SOFTWARE='1',WLR_LIBINPUT_NO_DEVICES='1',QT_QPA_PLATFORM='wayland',QT_QUICK_BACKEND='software',QT_QPA_PLATFORMTHEME='generic',QT_NO_XDG_DESKTOP_PORTAL='1',QT_ACCESSIBILITY='0',NO_AT_BRIDGE='1',GTK_A11Y='none')
 python=env.get('GLOBAL_MENU_PYTHON') or sys.executable;backend=repo/'res/lumen-desktop/shell/window_overview.py';runtime=repo/'res/lumen-desktop/runtime/spaces.py'
 def launch(args,name,**kw):
  with (a.output/name).open('w') as log:proc=sp.Popen(args,env=env,stdout=log,stderr=sp.STDOUT,start_new_session=True,**kw)
  processes.append(proc);return proc
 def call(*args):return sp.check_output(args,env=env,text=True,timeout=6).strip()
 def sway(*args):return json.loads(call(str(Path(a.sway).parent/'swaymsg'),'-r',*args))
 def checked(command):
  result=sway(command);assert result and all(r.get('success') for r in result),(command,result)
 def request(action='status',**kw):return json.loads(sp.check_output([python,str(backend)],input=json.dumps(dict(action=action,**kw)),env=env,text=True,timeout=8))
 def status():
  result=request();assert not result.get('error'),result;return result
 def assign(node,kind,target=None):
  focused=next((n['id'] for n in nodes(tree()) if n.get('pid') and n.get('focused')),None)
  state=status();result=request('assign',id=node['id'],pid=node['pid'],kind=kind,target=target,sessionEpoch=state['sessionEpoch']);assert not result.get('error'),result
  assert next((n['id'] for n in nodes(tree()) if n.get('pid') and n.get('focused')),None)==focused
  return result
 def wait(fn):
  deadline=time.monotonic()+15
  while time.monotonic()<deadline:
   try:
    value=fn()
    if value:return value
   except (sp.SubprocessError,OSError,ValueError):pass
   time.sleep(.04)
  (a.output/'timeout-tree.json').write_text(json.dumps(tree(),indent=2))
  (a.output/'timeout-status.json').write_text(json.dumps(status(),indent=2))
  raise AssertionError('Membership fixture timeout: '+str(a.output))
 def nodes(node):
  yield node
  for child in node.get('nodes',[])+node.get('floating_nodes',[]):yield from nodes(child)
 def tree():return sway('-t','get_tree')
 def app(name):return [n for n in nodes(tree()) if n.get('app_id')==name]
 def find(cid):return next((n for n in nodes(tree()) if n.get('id')==cid),None)
 def row(cid):return next(w for w in status()['windows'] if w['id']==cid)
 def foot(name,label):
  before={n['id'] for n in app(name)}
  launch(['foot','--app-id='+name,'sleep','180'],label+'.log');return wait(lambda:next((n for n in app(name) if n['id'] not in before),None))
 def all_sticky(name,count):return len(app(name))==count and all(n.get('sticky') for n in app(name))
 def watcher(name):return launch([python,str(runtime),'watch'],name+'.log')
 checks=[]
 try:
  start_private_bus(env,a.output,processes)
  config=private/'sway.conf';config.write_text('output HEADLESS-1 resolution 1280x900 position 0 0\noutput HEADLESS-2 resolution 1024x768 position 1280 0\nseat seat0 fallback true\nxwayland disable\nfocus_follows_mouse no\nfor_window [all] floating enable, border pixel 0\n')
  launch([a.sway,'--unsupported-gpu','-c',str(config)],'sway.log')
  env['SWAYSOCK']=str(wait(lambda:next(Path(env['XDG_RUNTIME_DIR']).glob('sway-ipc*.sock'),None)));env['WAYLAND_DISPLAY']=next(p.name for p in Path(env['XDG_RUNTIME_DIR']).glob('wayland-*') if not p.name.endswith('.lock'))
  checked('focus output HEADLESS-1; workspace "1"; move workspace to output HEADLESS-1')
  first=foot('membership-editor','first');preexisting=foot('membership-editor','preexisting');checked('[con_id=%d] sticky enable'%preexisting['id'])
  watch=watcher('watch');wait(lambda:(a.output/'watch.log').exists() and 'ready' in (a.output/'watch.log').read_text())
  # Actual production Overview chooses the new typed mode with real keyboard input.
  palette=json.loads((repo/'res/lumen-desktop/appearance/palette.json').read_text());appearance=private/'appearance.json';appearance.write_text(json.dumps(dict(font=palette['font'],palette=palette['dark'],preferences={})));env['LUMEN_APPEARANCE_STATE']=str(appearance)
  shutil.copytree(repo/'res/lumen-desktop',private/'assets');qml=private/'shell.qml';qml.write_text('''import QtQuick
import QtQuick.Window
import Quickshell
import Quickshell.Io
import "assets/shell" as D
ShellRoot {
 QtObject {id:stub;function claimPopup(owner){}}
 D.WindowOverview {id:overview;shell:stub}
 function find(item,name){if(item.Accessible && item.Accessible.name===name)return item;for(const child of item.children || []){const found=find(child,name);if(found)return found;}if(item.contentItem && item.contentItem!==item){const found=find(item.contentItem,name);if(found)return found;}return null;}
 IpcHandler {target:"membershipAudit"
  function open(id:string):void{overview.showForWindow(Number(id));}
  function status():string{return JSON.stringify({visible:overview.visible,busy:overview.busy,error:overview.error,current:overview.current,choices:overview.assignmentChoices});}
  function prepare():bool{const box=find(overview.contentItem,"Persistent application desktop assignment");const button=find(overview.contentItem,"Assign Application");if(!box || !button)return false;box.currentIndex=overview.assignmentChoices.findIndex(item=>item.kind==="all");button.forceActiveFocus();return button.activeFocus;}
  function refresh():void{overview.send({action:"status"});}
  function close():void{overview.visible=false;}
 }
}''')
  launch(['wtype','-s','120000'],'keyboard.log')
  launch(['quickshell','-p',str(qml)],'overview.log')
  def ipc(method,*args):return call('quickshell','-p',str(qml),'ipc','call','membershipAudit',method,*map(str,args))
  def ui():
   value=json.loads(ipc('status'));(a.output/'last-ui.json').write_text(json.dumps(value,indent=2));return value
  wait(lambda:ipc('status'));ipc('open',first['id']);wait(lambda:ui()['visible'] and not ui()['busy'] and ui()['current'])
  checked('[con_id=%d] focus'%wait(lambda:next((n for n in nodes(tree()) if n.get('name')=='Window Overview'),None))['id'])
  assert ipc('prepare')=='true';call('wtype','-k','Return');wait(lambda:all_sticky('membership-editor',2));wait(lambda:not ui()['busy']);ipc('refresh');wait(lambda:not ui()['busy'] and ui()['current']['allDesktopMembership']['status']=='active');assert not ui()['error'];call('grim',str(a.output/'overview.png'));ipc('close')
  checks.append('Production Overview exposes one typed all-desktop choice and real Return assigns all current eligible instances without replacing application identity')
  checked('workspace "2"; move workspace to output HEADLESS-1');wait(lambda:row(first['id'])['workspace']=='2');assert row(first['id'])['output']=='HEADLESS-1'
  third=foot('membership-editor','future');wait(lambda:all_sticky('membership-editor',3))
  checked('focus output HEADLESS-2; workspace "3"; move workspace to output HEADLESS-2');fourth=foot('membership-editor','other-output');wait(lambda:all_sticky('membership-editor',4))
  assert row(first['id'])['output']=='HEADLESS-1';checked('workspace "4"; move workspace to output HEADLESS-2');wait(lambda:row(fourth['id'])['workspace']=='4');assert row(first['id'])['workspace']=='2'
  checks.append('Future windows and separate application instances follow every desktop only on their own output, without physical-display cloning')
  watch.terminate();watch.wait(timeout=5);watch=watcher('watch-restarted');wait(lambda:'ready' in (a.output/'watch-restarted.log').read_text())
  assign(first,'none');wait(lambda:all(not find(n['id']).get('sticky') for n in (first,third,fourth)));assert find(preexisting['id'])['sticky']
  checks.append('Watcher restart retains exact session/container/process-birth ownership; clearing removes only managed stickiness and preserves preexisting sticky')
  assign(first,'all');wait(lambda:all_sticky('membership-editor',4));target=next(item for item in status()['workspaces'] if item['name']=='2')
  assign(first,'desktop',target['logicalId']);wait(lambda:not find(first['id'])['sticky']);assert find(preexisting['id'])['sticky'];assert status()['assignments']['wayland:membership-editor']==dict(kind='desktop',target_id=target['logicalId'])
  fresh=foot('membership-editor','desktop-future');wait(lambda:row(fresh['id'])['workspace']=='2')
  checks.append('Changing to an explicit desktop preserves its logical target and future-window placement, removes only policy stickiness, and preserves native windows')
  tiled=foot('membership-tiled','tiled');checked('[con_id=%d] floating disable'%tiled['id']);assign(tiled,'all');assert row(tiled['id'])['allDesktopMembership']['status']=='pending';assert 'Tiled' in row(tiled['id'])['allDesktopMembership']['reason'];assert not find(tiled['id'])['sticky']
  ipc('open',tiled['id']);wait(lambda:ui()['visible'] and not ui()['busy'] and ui()['current'] and ui()['current']['id']==tiled['id']);assert ui()['current']['allDesktopMembership']['status']=='pending';call('grim',str(a.output/'pending.png'));ipc('close')
  checked('[con_id=%d] floating enable'%tiled['id']);wait(lambda:find(tiled['id'])['sticky']);checked('[con_id=%d] fullscreen enable'%tiled['id']);wait(lambda:row(tiled['id'])['allDesktopMembership']['status']=='pending');assert find(tiled['id'])['fullscreen_mode']==1;checked('[con_id=%d] fullscreen disable'%tiled['id']);wait(lambda:find(tiled['id'])['sticky'])
  checks.append('Tiled and fullscreen instances report pending without forced layout conversion; leaving unsupported state reconciles the persistent application policy')
  relation=launch([a.client],'relationships.log',stdin=sp.PIPE,text=True);owned=wait(lambda:app('org.example.Utility'))[0];assign(owned,'all');assert not find(owned['id'])['sticky'];assert 'Owned' in row(owned['id'])['allDesktopMembership']['reason']
  relation.stdin.write('modal\n');relation.stdin.flush();wait(lambda:find(owned['id'])['lumen_relationships']['modal'] is True);assert not find(owned['id'])['sticky']
  relation.stdin.write('unmodal\n');relation.stdin.flush();wait(lambda:find(owned['id'])['lumen_relationships']['modal'] is False);relation.stdin.write('unparent\n');relation.stdin.flush();wait(lambda:find(owned['id'])['sticky'])
  (a.output/'unparented-window.json').write_text(json.dumps(find(owned['id']),indent=2))
  checks.append('Protocol-owned and modal windows remain with their parent with an explicit reason; role changes without window events reconcile only after the native parent/modal constraint clears')
  relation.stdin.write('unparent-unknown\n');relation.stdin.flush();assert relation.wait(timeout=5)==8;assert 'unknown command' in (a.output/'relationships.log').read_text();assert 'ack unparent-unknown' not in (a.output/'relationships.log').read_text()
  checks.append('Exact native helper rejects an unknown command with nonzero exit and no false acknowledgement')
  grouped=foot('membership-grouped','grouped')
  groups=tree()['window_groups'];checked('lumen_window groups %d on'%groups['revision']);wait(lambda:any(m['id']==grouped['id'] for g in tree()['window_groups']['groups'] for m in g['members']))
  before=status()['assignments'].get('wayland:membership-grouped');state=status();rejected=request('assign',id=grouped['id'],pid=grouped['pid'],kind='all',sessionEpoch=state['sessionEpoch']);assert 'groups' in rejected['error'];assert status()['assignments'].get('wayland:membership-grouped')==before;assert not find(grouped['id'])['sticky']
  groups=tree()['window_groups'];checked('lumen_window groups %d off'%groups['revision'])
  checks.append('Existing group membership rejects the complete assignment transaction before persistence; no implicit ungroup or partial application update')
  assign(first,'all');wait(lambda:all_sticky('membership-editor',5));checked('[con_id=%d] move container to output HEADLESS-2'%first['id']);wait(lambda:row(first['id'])['output']=='HEADLESS-2');assert find(first['id'])['sticky']
  checked('output HEADLESS-2 disable');wait(lambda:row(first['id'])['output']=='HEADLESS-1');assert find(first['id'])['sticky'];checked('output HEADLESS-2 enable');wait(lambda:any(o['name']=='HEADLESS-2' for o in status()['outputs']));assert find(first['id'])['sticky']
  checked('create_output');wait(lambda:any(o['name']=='HEADLESS-3' for o in status()['outputs']));assert len(app('membership-editor'))==5
  checks.append('Explicit output migration, disconnect fallback, reconnect and output addition retain exactly the existing instances and sticky policy, never clone windows or steal focus')
  for n in (first,preexisting,third,fourth,fresh):os.kill(n['pid'],0)
  (a.output/'final-tree.json').write_text(json.dumps(tree(),indent=2))
  for log in a.output.glob('*.log'):
   text=log.read_text();assert not any(message in text for message in ('ResourceWarning','Exception ignored','Binding loop','ReferenceError','TypeError')), (log,text)
  (a.output/'result.json').write_text(json.dumps(dict(passed=True,checks=checks,liveSocketUnchanged=os.environ.get('SWAYSOCK')==original_socket,effects=a.effects,helper=dict(path=str(a.client.resolve()),sha256=hashlib.sha256(a.client.read_bytes()).hexdigest(),sourceSha256=hashlib.sha256((repo/'tests/desktop/window_relationship_client.c').read_bytes()).hexdigest()),sourceHashes={str(path.relative_to(repo)):hashlib.sha256(path.read_bytes()).hexdigest() for path in [runtime,backend,repo/'res/lumen-desktop/runtime/application_desktops.py',repo/'res/lumen-desktop/shell/WindowOverview.qml']}),indent=2));print('PASS',len(checks))
 finally:
  if 'relation' in locals() and relation.stdin:relation.stdin.close()
  for proc in reversed(processes):
   if proc.poll() is None:os.killpg(proc.pid,signal.SIGTERM)
  for proc in reversed(processes):
   try:proc.wait(timeout=5)
   except sp.TimeoutExpired:os.killpg(proc.pid,signal.SIGKILL);proc.wait(timeout=5)
   if proc.stdout:proc.stdout.close()
