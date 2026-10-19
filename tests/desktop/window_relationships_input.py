"""Disposable native parent/modal focus and actual keyboard-recipient regression."""
import argparse,json,os,signal,subprocess as sp,sys,tempfile,time
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--sway',required=True);p.add_argument('--client',required=True);p.add_argument('--keyboard',required=True);p.add_argument('--output',required=True,type=Path);a=p.parse_args()
a.output.mkdir(parents=True,exist_ok=False)
sys.path.insert(0,str(Path(__file__).resolve().parents[2]/'res/lumen-desktop/runtime'))
from window_context import contexts,tree_context
processes=[]
with tempfile.TemporaryDirectory(prefix='lumen-window-relations-') as private:
 env=dict(os.environ,HOME=private,XDG_RUNTIME_DIR=private,WLR_BACKENDS='headless',WLR_RENDERER='pixman',WLR_LIBINPUT_NO_DEVICES='1')
 for key in ('WAYLAND_DISPLAY','DISPLAY','SWAYSOCK','DBUS_SESSION_BUS_ADDRESS'):env.pop(key,None)
 def launch(cmd,log,**kw):
  with (a.output/log).open('w') as stream:process=sp.Popen(cmd,env=env,stdout=stream,stderr=sp.STDOUT,start_new_session=True,**kw)
  processes.append(process);return process
 def wait(predicate):
  deadline=time.monotonic()+12
  while time.monotonic()<deadline:
   value=predicate()
   if value:return value
   time.sleep(.03)
  raise AssertionError('Native relationship fixture timeout: '+str(a.output))
 swaymsg=str(Path(a.sway).parent/'swaymsg')
 def ipc(*args):return json.loads(sp.check_output([swaymsg,'-r',*args],env=env,timeout=4))
 def nodes(node):
  yield node
  for child in node.get('nodes',[])+node.get('floating_nodes',[]):yield from nodes(child)
 def tree():return ipc('-t','get_tree')
 def observed():return {int(n['name'].split()[-1]):n for n in nodes(tree()) if (n.get('name') or '').startswith('Relationship ')}
 def focus(n,expect=None):
  before=(a.output/'client.log').read_text();response=ipc('[con_id=%d] focus'%observed()[n]['id']);assert all(row['success'] for row in response)
  target=n if expect is None else expect
  wait(lambda:next((row for row in observed().values() if row.get('focused')),{}).get('id')==observed()[target]['id'])
  sp.run([a.keyboard,'x'],env=env,check=True,timeout=4)
  def received():
   after=(a.output/'client.log').read_text()[len(before):]
   keys=[line for line in after.splitlines() if line.startswith('key ') and line.endswith(' 1')]
   if not keys:return False
   assert all(line.split()[1]==str(target) for line in keys),keys
   return True
  wait(received)
 def command(text):
  before=(a.output/'client.log').read_text();client.stdin.write(text+'\n');client.stdin.flush();wait(lambda:'ack '+text in (a.output/'client.log').read_text()[len(before):])
 try:
  config=a.output/'sway.conf';config.write_text('output HEADLESS-1 resolution 1440x1000\nxwayland disable\nseat seat0 fallback true\nfocus_follows_mouse no\n')
  launch([a.sway,'--unsupported-gpu','-c',str(config)],'sway.log')
  wait(lambda:list(Path(private).glob('sway-ipc*.sock')))
  env['SWAYSOCK']=str(next(Path(private).glob('sway-ipc*.sock')))
  env['WAYLAND_DISPLAY']=next(path.name for path in Path(private).glob('wayland-*') if not path.name.endswith('.lock'))
  assert ipc('-t','get_version')['native_window_relationships'] is True
  client=launch([a.client],'client.log',stdin=sp.PIPE,text=True);wait(lambda:len(observed())==4)
  launch([a.keyboard,'-s','60000'],'keyboard.log')
  first=observed();(a.output/'initial-tree.json').write_text(json.dumps(first,indent=2));parent=first[0]['id'];panel=first[2]['id'];other=first[3]['id']
  assert first[2]['lumen_relationships']==dict(version=1,parent_supported=True,parent=parent,modal=None)
  initial=tree_context(tree());assert initial[panel]['application_hint']=='org.example.Editor';assert initial[panel]['context_window']==parent
  assert all(row['main_window'] is None for row in initial.values())
  checks=[]
  for target in (2,0,3,1):focus(target)
  checks.append('Utility, document and other application receive their own actual keyboard input')
  adapted=contexts([dict(id=k,app_hint=v['application_hint'],parent=v['parent']) for k,v in initial.items()],{'org.example.Editor':parent})
  assert adapted[panel]['main_window']==parent and adapted[other]['main_window'] is None
  checks.append('Independent adapter document association survives utility key focus without inventing an external main document')
  focus(0);command('modal');wait(lambda:observed()[2]['lumen_relationships']['modal'] is True and observed()[2]['focused'])
  focus(0,2);focus(1);focus(3)
  checks.append('Real xdg-dialog modal blocks only its parent branch; sibling document and other app remain independently key')
  command('unmodal');wait(lambda:observed()[2]['lumen_relationships']['modal'] is False);focus(0)
  command('reparent');wait(lambda:observed()[2]['lumen_relationships']['parent']==observed()[1]['id'])
  command('modal');focus(0);focus(1,2)
  checks.append('Dynamic unset/reparent/set modal updates actual focus scope')
  command('unparent');wait(lambda:observed()[2]['lumen_relationships']['parent'] is None);focus(0);focus(1);focus(2)
  assert tree_context(tree())[panel]['application_hint']=='org.example.Utility'
  command('destroy-dialog');wait(lambda:observed()[2]['lumen_relationships']['modal'] is None);focus(0)
  command('reparent');command('modal');focus(1,2)
  command('close-second');wait(lambda:1 not in observed());assert observed()[2]['lumen_relationships']['parent'] is None;focus(2);focus(3)
  checks.append('Unparenting, dialog object destruction, parentless modal and live orphan panel retain exact protocol scope')
  command('close-panel');wait(lambda:2 not in observed());focus(0)
  command('close-parent');wait(lambda:0 not in observed());focus(3)
  checks.append('Destroyed modal and parent leave no stale focus target or application context')
  (a.output/'result.json').write_text(json.dumps(dict(passed=True,checks=checks,scope='Isolated native compositor, disposable Wayland clients and virtual keyboard only'),indent=2)+'\n')
 finally:
  if 'client' in locals() and client.stdin:client.stdin.close()
  for process in reversed(processes):
   if process.poll() is None:os.killpg(process.pid,signal.SIGTERM)
  for process in reversed(processes):
   try:process.wait(timeout=3)
   except sp.TimeoutExpired:os.killpg(process.pid,signal.SIGKILL);process.wait(timeout=3)
print('PASS native parent/modal/key-window regression')
