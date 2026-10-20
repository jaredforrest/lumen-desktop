"""Real empty/live-dialog compositor shutdown and restart in a private session."""
from fixture_resources import close_reaped_pipes
import argparse,json,os,resource,select,signal,subprocess as sp,tempfile,time
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--sway',required=True);p.add_argument('--keyboard',required=True);p.add_argument('--locker',required=True);p.add_argument('--pointer-tool',required=True);p.add_argument('--client',required=True);p.add_argument('--output',type=Path,required=True);p.add_argument('--effects',action='store_true');a=p.parse_args();a.output.mkdir(parents=True,exist_ok=False)
processes=[]
with tempfile.TemporaryDirectory(prefix='lumen-window-groups-') as temporary:
 private=Path(temporary);runtime=private/'runtime';runtime.mkdir(mode=0o700)
 env=dict(os.environ,HOME=str(private),XDG_RUNTIME_DIR=str(runtime),WLR_BACKENDS='headless',WLR_HEADLESS_OUTPUTS='2',WLR_RENDERER='gles2' if a.effects else 'pixman',WLR_RENDERER_FORCE_SOFTWARE='1' if a.effects else '0',WLR_RENDERER_ALLOW_SOFTWARE='1',LIBGL_ALWAYS_SOFTWARE='1',WLR_LIBINPUT_NO_DEVICES='1')
 for key in ('XDG_CONFIG_HOME','XDG_STATE_HOME','XDG_CACHE_HOME','XDG_DATA_HOME'):
  env[key]=str(private/key);Path(env[key]).mkdir(mode=0o700)
 for key in ('SWAYSOCK','WAYLAND_DISPLAY','DISPLAY','DBUS_SESSION_BUS_ADDRESS','AT_SPI_BUS_ADDRESS'):env.pop(key,None)
 bus_config=private/'bus.conf';bus_config.write_text('<busconfig><type>session</type><listen>unix:tmpdir='+str(runtime)+'</listen><policy context="default"><allow own="*"/><allow send_destination="*"/><allow receive_sender="*"/></policy></busconfig>')
 with (a.output/'bus.log').open('w') as stream:bus=sp.Popen(['dbus-daemon','--nofork','--print-address=1','--config-file='+str(bus_config)],env=env,stdout=sp.PIPE,stderr=stream,text=True,start_new_session=True)
 processes.append(bus);assert select.select([bus.stdout],[],[],5)[0];env['DBUS_SESSION_BUS_ADDRESS']=bus.stdout.readline().strip();assert env['DBUS_SESSION_BUS_ADDRESS'].startswith('unix:')
 swaymsg=str(Path(a.sway).parent/'swaymsg')
 config=private/'sway.conf';config.write_text('output HEADLESS-1 resolution 800x600\nxwayland disable\nseat seat0 fallback true\n')
 def no_core():resource.setrlimit(resource.RLIMIT_CORE,(0,0))
 def launch(command,name,**kwargs):
  with (a.output/name).open('w') as stream:proc=sp.Popen(command,env=env,stdout=stream,stderr=sp.STDOUT,start_new_session=True,preexec_fn=no_core,**kwargs)
  processes.append(proc);return proc
 def wait(predicate):
  deadline=time.monotonic()+12
  while time.monotonic()<deadline:
   value=predicate()
   if value:return value
   time.sleep(.03)
  raise AssertionError('Window groups timeout: '+str(a.output)+'; compositor logs: '+ '\n'.join(path.name+': '+path.read_text() for path in a.output.glob('*sway*.log')))
 def query(kind):return json.loads(sp.check_output([swaymsg,'-r','-t',kind],env=env,text=True,timeout=4))
 def nodes(node):
  yield node
  for child in node.get('nodes',[])+node.get('floating_nodes',[]):yield from nodes(child)
 checks=[]
 try:
  compositor=launch([a.sway,'--unsupported-gpu','-c',str(config)],'sway.log')
  env['SWAYSOCK']=str(wait(lambda:next(runtime.glob('sway-ipc*.sock'),None)));env['WAYLAND_DISPLAY']=wait(lambda:next((path for path in runtime.glob('wayland-*') if not path.name.endswith('.lock')),None)).name
  def command(value,success=True):
   result=json.loads(sp.run([swaymsg,'-r',value],env=env,text=True,timeout=4,stdout=sp.PIPE,check=False).stdout);assert all(row.get('success') is success for row in result),(value,result)
  def state():return query('get_tree')['window_groups']
  def window(n):return next((row for row in nodes(query('get_tree')) if row.get('name')=='Relationship '+str(n)),None)
  def group(n):return next(g for g in state()['groups'] if any(m['id']==window(n)['id'] for m in g['members']))
  def act(action,tail='',success=True,revision=None):
   value=state();command('lumen_window groups '+str(value['revision'] if revision is None else revision)+' '+action+(' '+tail if tail else ''),success)
  assert query('get_version')['native_window_groups'] is True;assert state()['enabled'] is False
  launch([a.keyboard,'-s','60000'],'keyboard.log')
  client=launch([a.client],'client.log',stdin=sp.PIPE,text=True)
  wait(lambda:sum(1 for row in nodes(query('get_tree')) if (row.get('name') or '').startswith('Relationship '))==4)
  command('[app_id="org.example.*"] floating enable')
  captured={i:window(i) for i in range(4)}
  def placement(n):
   row=window(n)
   def locate(node,workspace=None):
    if node.get('type')=='workspace':workspace=node['name']
    if node.get('id')==row['id']:return workspace
    for child in node.get('nodes',[])+node.get('floating_nodes',[]):
     value=locate(child,workspace)
     if value is not None:return value
   return dict(workspace=locate(query('get_tree')),rect=row['rect'],floating=row.get('floating'))
  before={i:placement(i) for i in range(4)}
  act('on');wait(lambda:len(state()['groups'])==3)
  assert group(0)['id']==group(2)['id'];assert group(0)['id']!=group(1)['id']
  checks.append('Native parent follows owner; independent roots receive distinct groups')
  stale=state()['revision'];g=group(0)['id'];act('select',str(g));act('add',f"{g} {captured[1]['id']} {captured[1]['pid']}")
  assert group(0)['id']==group(1)['id'];assert len(group(0)['members'])==3
  act('off',success=False,revision=stale);assert state()['enabled']
  for wrong_pid in (captured[3]['pid']+1,captured[3]['pid']+2**32):
   act('add',f"{g} {captured[3]['id']} {wrong_pid}",False)
  act('remove',f"{g} {captured[1]['id']} {captured[1]['pid']}")
  assert group(1)['id']!=group(0)['id'];assert group(1)['selected']
  assert all(m['group_hidden'] for m in group(0)['members'])
  command('[con_id='+str(captured[0]['id'])+'] focus');wait(lambda:group(0)['selected'])
  assert all(not m['group_hidden'] for m in group(0)['members'])
  def keyboard_recipient(target):
   before=(a.output/'client.log').read_text()
   sp.run([a.keyboard,'x'],env=env,check=True,timeout=4)
   def received():
    keys=[line for line in (a.output/'client.log').read_text()[len(before):].splitlines() if line.startswith('key ') and line.endswith(' 1')]
    if not keys:return False
    assert all(line.split()[1]==str(target) for line in keys),keys
    return True
   try:wait(received)
   except AssertionError:
    (a.output/'failed-tree.json').write_text(json.dumps(query('get_tree'),indent=2));raise
  keyboard_recipient(0)
  # Put a hidden outgoing window directly under a real private pointer.
  for i,x in ((0,400),(1,40),(2,400),(3,400)):
   command('[con_id='+str(captured[i]['id'])+'] move position '+str(x)+' 40')
  before={i:placement(i) for i in range(4)}
  pointer=launch([a.pointer_tool,'pointer','move','0','0'],'pointer.log',stdin=sp.PIPE,text=True)
  command('seat seat0 cursor set 100 100')
  for event in ('m 0.5 0.5','p','r'):
   pointer.stdin.write(event+'\n');pointer.stdin.flush();time.sleep(.07)
  time.sleep(.08)
  assert not group(1)['selected'],'Hidden group received pointer click'
  command('[con_id='+str(captured[0]['id'])+'] focus');keyboard_recipient(0)
  command('[con_id='+str(captured[1]['id'])+'] focus');wait(lambda:group(1)['selected']);keyboard_recipient(1)
  client.stdin.write('modal\n');client.stdin.flush()
  wait(lambda:window(2)['lumen_relationships']['modal'] is True)
  command('[con_id='+str(captured[0]['id'])+'] focus');keyboard_recipient(2)
  assert group(0)['selected'] and group(2)['id']==group(0)['id']
  client.stdin.write('unmodal\n');client.stdin.flush();wait(lambda:window(2)['lumen_relationships']['modal'] is False)
  checks.append('Atomic add/remove/select, stale revision/PID rejection; real keyboard recipients and hidden-window pointer exclusion')
  act('select',str(group(1)['id']))
  original_owner_group=group(0)['id']
  command('[con_id='+str(captured[0]['id'])+'] lumen_window minimize')
  assert group(2)['id']==original_owner_group and all(m['group_hidden'] for m in group(2)['members'])
  command('[con_id='+str(captured[0]['id'])+'] scratchpad show')
  assert group(0)['id']==original_owner_group
  command('[con_id='+str(captured[1]['id'])+'] lumen_window minimize')
  command('[con_id='+str(captured[3]['id'])+'] lumen_window hide')
  hidden={i:{key:value for key,value in window(i)['lumen_state'].items() if key!='group_hidden'} for i in (1,3)}
  act('off');assert state()['enabled'] is False
  assert all({key:value for key,value in window(i)['lumen_state'].items() if key!='group_hidden'}==hidden[i] for i in (1,3))
  for i in (0,2):assert placement(i)==before[i],(i,placement(i),before[i])
  command('[con_id='+str(captured[1]['id'])+'] scratchpad show');command('[con_id='+str(captured[3]['id'])+'] lumen_window unhide')
  checks.append('Disabled mode preserves ordinary geometry and independent minimized/application-hidden reasons')
  act('on');command('lumen_window show-desktop-on');assert all(g['suspended'] for g in state()['groups']);act('off');command('lumen_window show-desktop-off')
  assert all(window(i)['pid']==captured[i]['pid'] for i in range(4))
  checks.append('Show Desktop suspension and disable/restore retain all exact live document identities')
  act('on')
  command('[con_id='+str(captured[3]['id'])+'] sticky enable')
  assert not any(m['id']==captured[3]['id'] for g in state()['groups'] for m in g['members'])
  assert not window(3)['lumen_state']['group_hidden']
  command('[con_id='+str(captured[3]['id'])+'] sticky disable');wait(lambda:any(m['id']==captured[3]['id'] for g in state()['groups'] for m in g['members']))
  checks.append('Sticky windows stay outside grouping; removing sticky status creates a fresh ordinary group')
  command('[con_id='+str(captured[0]['id'])+'] fullscreen enable')
  assert group(0)['suspended'];act('select',str(group(0)['id']),False)
  command('[con_id='+str(captured[0]['id'])+'] fullscreen disable')
  command('lumen_pair create '+str(captured[1]['id'])+' '+str(captured[3]['id']))
  assert group(1)['suspended'];act('select',str(group(1)['id']),False)
  command('lumen_pair leave '+str(captured[1]['id']))
  locker=launch([a.locker],'locker.log',stdin=sp.PIPE,text=True)
  wait(lambda:state()['locked']);act('off',success=False);assert state()['enabled']
  locker.stdin.write('u\n');locker.stdin.flush();assert locker.wait(timeout=4)==0
  wait(lambda:not state()['locked']);assert state()['enabled']
  checks.append('Actual fullscreen/pair contexts suspend grouping; actual session lock rejects mutations until unlock')
  command('[con_id='+str(captured[0]['id'])+'] move container to workspace 2')
  wait(lambda:group(0)['workspace']!=group(1)['workspace'])
  g0=group(0);act('add',f"{g0['id']} {captured[3]['id']} {captured[3]['pid']}",False)
  command('move workspace to output HEADLESS-2')
  retained=group(0)['id'];command('output HEADLESS-2 disable');wait(lambda:any(g['id']==retained for g in state()['groups']))
  assert window(0)['pid']==captured[0]['pid']
  client.stdin.write('close-second\n');client.stdin.flush();wait(lambda:window(1) is None)
  assert all(m['id']!=captured[1]['id'] for g in state()['groups'] for m in g['members'])
  incoming=launch(['foot','--app-id=org.example.GroupIncoming','sleep','60'],'incoming.log')
  new_window=wait(lambda:next((row for row in nodes(query('get_tree')) if row.get('app_id')=='org.example.GroupIncoming'),None))
  new_group=wait(lambda:next((g for g in state()['groups'] if any(m['id']==new_window['id'] for m in g['members'])),None))
  assert new_group['selected']
  incoming.send_signal(signal.SIGTERM);incoming.wait(timeout=4)
  wait(lambda:not any(m['id']==new_window['id'] for g in state()['groups'] for m in g['members']))
  checks.append('Incoming mapped window receives selected independent group; actual process exit removes membership')
  checks.append('Native workspace movement and destruction reconcile exact membership')
  act('off');compositor.send_signal(signal.SIGTERM);assert compositor.wait(timeout=8)==0
  (a.output/'result.json').write_text(json.dumps(dict(passed=True,checks=checks),indent=2)+'\n')
 finally:
  for proc in reversed(processes):
   if proc.poll() is None:os.killpg(proc.pid,signal.SIGTERM)
  for proc in reversed(processes):
   try:proc.wait(timeout=3)
   except sp.TimeoutExpired:os.killpg(proc.pid,signal.SIGKILL);proc.wait(timeout=3)
   if proc.stdin and not proc.stdin.closed:proc.stdin.close()
  if bus.stdout:bus.stdout.close()
  for _fixture_process in processes:
      if _fixture_process.poll() is None:_fixture_process.wait(timeout=5)
  close_reaped_pipes(processes)
print('PASS native window groups')
