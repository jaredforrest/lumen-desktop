"""Native paired-full-output identity, divider and restore lifecycle in isolation."""
import argparse,json,os,signal,subprocess as sp,tempfile,time,sys
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--sway',required=True);p.add_argument('--pointer',required=True);p.add_argument('--keyboard',required=True);p.add_argument('--constraint-client',required=True);p.add_argument('--effects',action='store_true');p.add_argument('--output',type=Path,required=True);a=p.parse_args();a.output.mkdir(parents=True,exist_ok=False);processes=[]
def wait(fn):
 end=time.monotonic()+12
 while time.monotonic()<end:
  value=fn()
  if value:return value
  time.sleep(.04)
 raise AssertionError('Paired layout timed out; see '+str(a.output))
with tempfile.TemporaryDirectory(prefix='lumen-pair-') as private:
 env=dict(os.environ,HOME=private,XDG_RUNTIME_DIR=private,WLR_BACKENDS='headless',WLR_HEADLESS_OUTPUTS='2',WLR_RENDERER='gles2' if a.effects else 'pixman',WLR_RENDERER_ALLOW_SOFTWARE='1',WLR_LIBINPUT_NO_DEVICES='1',LIBGL_ALWAYS_SOFTWARE='1',QT_ACCESSIBILITY='0',NO_AT_BRIDGE='1',GTK_A11Y='none')
 for key in ('SWAYSOCK','WAYLAND_DISPLAY','DISPLAY','DBUS_SESSION_BUS_ADDRESS','AT_SPI_BUS_ADDRESS'):env.pop(key,None)
 for kind in ('CONFIG','STATE','CACHE','DATA'):env['XDG_'+kind+'_HOME']=str(Path(private)/kind.lower())
 def launch(args,name,**kwargs):
  proc=sp.Popen(args,env=env,stdout=(a.output/name).open('w'),stderr=sp.STDOUT,start_new_session=True,**kwargs);processes.append(proc);return proc
 def call(*args):return sp.check_output(args,env=env,text=True,timeout=5)
 def query(kind):return json.loads(call('swaymsg','-r','-t',kind))
 def ipc(command,success=True):
  response=sp.run(['swaymsg','-r',command],env=env,text=True,capture_output=True,timeout=5);answer=json.loads(response.stdout);assert all(x.get('success') is success for x in answer),(command,answer);time.sleep(.12);return answer
 def nodes(tree):
  yield tree
  for child in tree.get('nodes',[])+tree.get('floating_nodes',[]):yield from nodes(child)
 def find(cid):return next((n for n in nodes(query('get_tree')) if n['id']==cid),None)
 def app(name):return next((n for n in nodes(query('get_tree')) if n.get('app_id')==name),None)
 def box(cid):
  n=find(cid);r=dict(n['rect']);r['y']-=n['deco_rect']['height'];r['height']+=n['deco_rect']['height'];return r
 def full_parent(cid):
  n=find(cid);return next((n for n in nodes(query('get_tree')) if n.get('fullscreen_mode')==1 and any(child.get('id')==cid for child in n.get('nodes',[]))),None)
 def workspace(cid):
  def walk(n,ws=''):
   if n.get('type')=='workspace':ws=n['name']
   if n['id']==cid:return ws
   for child in n.get('nodes',[])+n.get('floating_nodes',[]):
    value=walk(child,ws)
    if value is not None:return value
  return walk(query('get_tree'))
 def at(x,y):ipc(f'seat seat0 cursor set {x} {y}')
 def motion(dx,dy):pointer.stdin.write(f'm {dx} {dy}\n');pointer.stdin.flush();time.sleep(.15)
 def button(v):pointer.stdin.write(v+'\n');pointer.stdin.flush();time.sleep(.12)
 def ratio(cid):return find(cid)['lumen_pair']['ratio']
 def begin_divider(cid):
  parent=full_parent(cid);x=parent['rect']['x']+parent['rect']['width']*ratio(cid);y=parent['rect']['y']+200;at(x,y);motion(.1,0);button('p')
 try:
  config=a.output/'sway.conf';config.write_text('output HEADLESS-1 resolution 1280x720 position 0 0\noutput HEADLESS-2 resolution 1000x800 position 1280 0\nseat seat0 fallback true\ndefault_border normal 2\n')
  launch([a.sway,'--unsupported-gpu','-c',str(config)],'sway.log');env['SWAYSOCK']=str(wait(lambda:next(Path(private).glob('sway-ipc*.sock'),None)));env['WAYLAND_DISPLAY']=wait(lambda:next((p.name for p in Path(private).glob('wayland-*') if not p.name.endswith('.lock')),None));assert query('get_version')['native_paired_layouts']
  pointer=launch([a.pointer,'pointer','move','0','0'],'pointer.log',stdin=sp.PIPE,text=True);motion(0,0)
  for name in ('pair-a','pair-b','pair-c'):
   launch(['foot','-o','resize-by-cells=no','--app-id='+name,'sleep','180'],name+'.log');wait(lambda:app(name))
  aa=app('pair-a')['id'];bb=app('pair-b')['id'];cc=app('pair-c')['id'];ids=(aa,bb,cc);original={};original_ws={}
  for index,cid in enumerate(ids):
   ipc(f'[con_id={cid}] floating enable, resize set 360 px 300 px, move absolute position {100+index*180} px {130+index*35} px');original[cid]=box(cid);original_ws[cid]=workspace(cid)
  ipc(f'lumen_pair create {aa} {bb}');parent=full_parent(aa);assert parent and full_parent(bb)['id']==parent['id'];assert workspace(aa).startswith('paired-') and workspace(aa)!=original_ws[aa]
  assert find(aa)['lumen_pair']['partner']==bb and find(bb)['lumen_pair']['side']=='right';assert abs(ratio(aa)-.5)<.01
  sys.path.insert(0,str(Path(__file__).resolve().parents[2]/'res/lumen-desktop/runtime'));import state as observer
  observed=observer.snapshot(query('get_tree'),query('get_outputs'),'disposable-pair');assert all(w['placement']=='paired' for w in observed['windows'] if w['id'] in ('w-'+str(aa),'w-'+str(bb)))
  # Reveal hides the native pair as one group, preserving identity and ratio.
  parent_id=parent['id'];ipc('lumen_window show-desktop-on');assert workspace(aa)=='__i3_scratch' and workspace(bb)=='__i3_scratch';assert find(aa)['lumen_pair']['partner']==bb
  ipc('lumen_window show-desktop-off');assert full_parent(aa)['id']==parent_id and abs(ratio(aa)-.5)<.01
  ipc('lumen_window show-desktop-on');ipc(f'[con_id={aa}] scratchpad show');assert full_parent(aa)['id']==parent_id and workspace(aa).startswith('paired-')
  ipc('lumen_window show-desktop-on');ipc(f'lumen_pair ratio {aa} 0.5');assert full_parent(aa)['id']==parent_id and abs(ratio(aa)-.5)<.01
  # Full-screen yellow controls cannot create an un-restorable half-window snapshot.
  ipc(f'[con_id={aa}] lumen_window minimize');assert full_parent(aa)['id']==parent_id and not find(aa)['lumen_state']['minimised']
  # A new unrelated app gets an ordinary desktop, never a third paired child.
  paired_space=workspace(aa);launch(['foot','-o','resize-by-cells=no','--app-id=pair-new','sleep','180'],'new-app.log');wait(lambda:app('pair-new'));new_id=app('pair-new')['id'];assert workspace(new_id)!=paired_space and len(full_parent(aa)['nodes'])==2
  assert app('pair-new')['focused'];ipc(f'[con_id={new_id}] kill');ipc('workspace '+json.dumps(paired_space))
  # Actual divider drag changes only the captured pair and Escape restores ratio.
  other=box(cc);begin_divider(aa);motion(180,0);button('r');assert ratio(aa)>.6 and box(cc)==other;changed=ratio(aa)
  begin_divider(aa);motion(-140,0);sp.run([a.keyboard,'-k','Escape'],env=env,check=True);button('r');assert abs(ratio(aa)-changed)<.01
  time.sleep(.35);begin_divider(aa);button('r');button('p');button('r');assert abs(ratio(aa)-.5)<.01
  call('grim','-o','HEADLESS-1',str(a.output/'paired.png'))
  ipc('lumen_window show-desktop-on');ipc(f'lumen_pair swap {aa}');assert find(aa)['lumen_pair']['side']=='right' and find(bb)['lumen_pair']['side']=='left'
  ipc('lumen_window show-desktop-on');ipc(f'lumen_pair replace {aa} {cc}');assert find(aa)['lumen_pair']['partner']==cc;assert box(bb)==original[bb] and workspace(bb)==original_ws[bb]
  ipc('lumen_window show-desktop-on');ipc(f'lumen_pair leave {aa}');assert box(aa)==original[aa] and workspace(aa)==original_ws[aa];assert find(cc)['fullscreen_mode']==1 and find(cc)['lumen_pair']['survivor'];assert workspace(cc).startswith('paired-')
  ipc('workspace '+json.dumps(workspace(cc)));ipc('lumen_window show-desktop-on');ipc('lumen_window show-desktop-off');assert find(cc)['lumen_pair']['survivor'] and find(cc)['fullscreen_mode']==1
  ipc('lumen_window show-desktop-on');ipc(f'lumen_pair leave {cc}');assert box(cc)==original[cc] and workspace(cc)==original_ws[cc]
  # Closing a partner during a physical divider operation keeps the survivor safe.
  ipc(f'lumen_pair create {aa} {bb}');begin_divider(aa);motion(30,0);ipc(f'[con_id={bb}] kill');wait(lambda:find(bb) is None);button('r');assert find(aa)['fullscreen_mode']==1 and find(aa)['lumen_pair']['survivor'];ipc(f'lumen_pair leave {aa}');assert box(aa)==original[aa]
  # Native external removal is an exit, not a dangling pair or retarget.
  ipc(f'lumen_pair create {aa} {cc}');ipc(f'[con_id={cc}] move container to workspace 3');assert workspace(cc)=='3';assert find(aa)['fullscreen_mode']==1;ipc(f'lumen_pair leave {aa}')
  # A source that was full screen exits to its saved ordinary desktop geometry.
  ipc(f'[con_id={aa}] fullscreen enable');ipc(f'lumen_pair create {aa} {cc}');ipc(f'lumen_pair leave {aa}');assert find(aa)['fullscreen_mode']==0 and box(aa)==original[aa];ipc(f'lumen_pair leave {cc}')
  # Retiling/hiding first exits pairing, so normal app restore remains well-defined.
  ipc(f'lumen_pair create {aa} {cc}');ipc(f'[con_id={aa}] lumen_window hide');assert find(aa)['lumen_state']['hidden'] and find(cc)['lumen_pair']['survivor'];ipc(f'[con_id={aa}] lumen_window unhide');assert box(aa)==original[aa];ipc(f'lumen_pair leave {cc}')
  ipc(f'lumen_pair create {aa} {cc}');ipc(f'[con_id={aa}] floating toggle');assert box(aa)==original[aa] and workspace(aa)==original_ws[aa];assert find(cc)['lumen_pair']['survivor'];ipc(f'lumen_pair leave {cc}')
  # Invalid identities and fixed-size clients reject atomically.
  fixed=launch([a.constraint_client,'fixed'],'fixed.log',stdin=sp.PIPE,text=True);wait(lambda:app('constraint-test'));ff=app('constraint-test')['id'];before_a=box(aa);before_f=box(ff)
  ipc(f'lumen_pair create {aa} {ff}',False);assert box(aa)==before_a and box(ff)==before_f
  ipc(f'lumen_pair create {aa} 999999999',False);assert box(aa)==before_a
  fixed.stdin.write('q\n');fixed.stdin.flush();fixed.wait(timeout=3)
  # A bounded compatible client constrains both command and physical ratios.
  bounded=launch([a.constraint_client,'bounded'],'bounded.log',stdin=sp.PIPE,text=True);wait(lambda:app('constraint-test'));bound=app('constraint-test')['id'];bound_ws=workspace(bound)
  ipc(f'lumen_pair create {aa} {bound}');assert box(bound)['width']<=504
  ipc(f'lumen_pair ratio {aa} 0.1');assert box(bound)['width']<=504 and ratio(aa)>.5
  time.sleep(.35);begin_divider(aa);motion(-1000,0);button('r');assert box(bound)['width']<=504
  ipc(f'lumen_pair leave {bound}');assert workspace(bound)==bound_ws and not find(bound)['lumen_pair'].get('id');assert find(aa)['fullscreen_mode']==1;ipc(f'lumen_pair leave {aa}')
  bounded.stdin.write('q\n');bounded.stdin.flush();bounded.wait(timeout=3)
  # A real protocol transient stays visible above the pair without becoming a pane.
  parent_client=launch([a.constraint_client,'free'],'transient-parent.log',stdin=sp.PIPE,text=True);wait(lambda:app('constraint-test'));parent_id=app('constraint-test')['id']
  parent_client.stdin.write('f\n');parent_client.stdin.flush();wait(lambda:app('constraint-dialog'));dialog=app('constraint-dialog');before_a=box(aa)
  assert not dialog['lumen_pair']['eligible'];ipc(f"lumen_pair create {aa} {dialog['id']}",False);assert box(aa)==before_a
  ipc(f'lumen_pair create {aa} {parent_id}',False);assert box(aa)==before_a
  parent_client.stdin.write('d\n');parent_client.stdin.flush();wait(lambda:app('constraint-dialog') is None)
  ipc(f'lumen_pair create {aa} {parent_id}');pair_space=workspace(aa)
  parent_client.stdin.write('f\n');parent_client.stdin.flush();wait(lambda:app('constraint-dialog'));dialog=app('constraint-dialog');assert workspace(dialog['id'])==pair_space and dialog['visible'] and dialog['focused'];assert len(full_parent(aa)['nodes'])==2
  call('grim',str(a.output/'transient-dialog.png'))
  # Exact protocol ownership follows leave, replacement and external movement.
  ipc(f'lumen_pair leave {parent_id}');assert workspace(dialog['id'])==workspace(parent_id) and find(dialog['id'])['visible'];ipc(f'lumen_pair leave {aa}')
  ipc(f'lumen_pair create {aa} {parent_id}');assert workspace(dialog['id'])==workspace(parent_id)
  ipc(f'lumen_pair replace {aa} {cc}');assert workspace(dialog['id'])==workspace(parent_id) and workspace(dialog['id'])!=workspace(aa)
  ipc(f'lumen_pair leave {aa}');ipc(f'lumen_pair leave {cc}')
  ipc(f'lumen_pair create {aa} {parent_id}');ipc(f'[con_id={parent_id}] move container to workspace 7');assert workspace(dialog['id'])=='7' and workspace(parent_id)=='7';ipc(f'lumen_pair leave {aa}')
  # Explicit protocol parent removal clears captured ownership at arrange time.
  ipc(f'lumen_pair create {aa} {parent_id}');parent_client.stdin.write('n\n');parent_client.stdin.flush();time.sleep(.15)
  ipc('output HEADLESS-1 resolution 1280x720');assert workspace(dialog['id'])!=workspace(parent_id) and find(dialog['id'])['lumen_pair']['eligible']
  ipc(f'lumen_pair leave {parent_id}');ipc(f'lumen_pair leave {aa}')
  parent_client.stdin.write('d\n');parent_client.stdin.flush();wait(lambda:app('constraint-dialog') is None)
  ipc(f'lumen_pair create {aa} {parent_id}');parent_client.stdin.write('f\n');parent_client.stdin.flush();wait(lambda:app('constraint-dialog'));dialog=app('constraint-dialog')
  # Parent destruction does not strand its surviving transient behind the partner.
  parent_client.stdin.write('p\n');parent_client.stdin.flush();wait(lambda:find(parent_id) is None);assert workspace(dialog['id'])!=workspace(aa);assert find(aa)['lumen_pair']['survivor'];ipc(f'lumen_pair leave {aa}')
  ipc('workspace '+workspace(dialog['id']));assert find(dialog['id'])['visible'];parent_client.stdin.write('d\n');parent_client.stdin.flush();wait(lambda:app('constraint-dialog') is None)
  parent_client.stdin.write('q\n');parent_client.stdin.flush();parent_client.wait(timeout=3)
  parent_client=launch([a.constraint_client,'free'],'survivor-dialog-parent.log',stdin=sp.PIPE,text=True);wait(lambda:app('constraint-test'));parent_id=app('constraint-test')['id'];parent_ws=workspace(parent_id);ipc(f'lumen_pair create {aa} {parent_id}')
  parent_client.stdin.write('f\n');parent_client.stdin.flush();wait(lambda:app('constraint-dialog'));dialog=app('constraint-dialog');ipc(f'lumen_pair leave {aa}');assert find(parent_id)['lumen_pair']['survivor']
  parent_client.stdin.write('p\n');parent_client.stdin.flush();wait(lambda:find(parent_id) is None);assert workspace(dialog['id'])==parent_ws
  ipc('workspace '+parent_ws);assert find(dialog['id'])['visible'];parent_client.stdin.write('q\n');parent_client.stdin.flush();parent_client.wait(timeout=3);wait(lambda:app('constraint-dialog') is None)
  parent_client=launch([a.constraint_client,'free'],'revealed-parent.log',stdin=sp.PIPE,text=True);wait(lambda:app('constraint-test'));parent_id=app('constraint-test')['id'];ipc(f'lumen_pair create {aa} {parent_id}')
  ipc('lumen_window show-desktop-on');parent_client.stdin.write('q\n');parent_client.stdin.flush();parent_client.wait(timeout=3);wait(lambda:find(parent_id) is None);assert find(aa)['fullscreen_mode']==1 and find(aa)['lumen_pair']['survivor'];ipc(f'lumen_pair leave {aa}')
  # Output loss remaps the dedicated space; both exits restore reachable placements.
  ipc(f'lumen_pair create {aa} {cc}');ipc('output HEADLESS-1 disable');assert full_parent(aa) and full_parent(cc);assert box(aa)['x']>=1280 and box(cc)['x']>=1280
  ipc(f'lumen_pair leave {aa}');assert find(cc)['fullscreen_mode']==1;ipc(f'lumen_pair leave {cc}')
  for cid in (aa,cc):
   r=box(cid);assert 1280<=r['x']<2280 and r['x']+r['width']<=2280 and 0<=r['y'] and r['y']+r['height']<=800,r
  assert query('get_version')['native_paired_layouts']
  log=(a.output/'sway.log').read_text();assert 'Assertion' not in log and 'Segmentation fault' not in log and 'Expected a fullscreen container' not in log,log
  (a.output/'result.json').write_text(json.dumps({'passed':True,'ids':ids,'original':original,'checks':['dedicated shared full-output space','physical divider drag','Escape restores ratio','double-click reset','swap exact identities','replace restores old partner','exit leaves full-screen survivor','survivor exit restores original','partner close during divider','external partner removal','fixed size and stale IDs reject','bounded client limits constrain physical and command ratios','group reveal and child activation preserve pair','hidden pair management normalizes group before actions','full-screen source exits to ordinary saved geometry','hide exits without stranding restore state','new app routes outside pair','protocol transient remains visible and focused','transient follows leave replace external move and owner close','explicit parent removal clears captured dialog owner','output loss preserves reachability']},indent=2));print('PASS native paired placement, divider, identity, exit and output loss')
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
