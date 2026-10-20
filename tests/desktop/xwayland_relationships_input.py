"""Actual private X11 ancestry/modal transitions and exact keyboard recipients."""
import argparse,hashlib,json,os,select,shutil,subprocess as sp,sys,time
from pathlib import Path
from application_fixture.isolation import run as isolated_run
from application_fixture.keyboard import PhysicalKeyboard
from private_native_bus import start_private_bus
from fixture_resources import close_reaped_pipes
p=argparse.ArgumentParser();p.add_argument('--sway',type=Path,required=True);p.add_argument('--client',type=Path,required=True);p.add_argument('--keyboard',type=Path,required=True);p.add_argument('--xwayland',type=Path,required=True);p.add_argument('--effects',action='store_true');p.add_argument('--without-xres',action='store_true');p.add_argument('--output',type=Path);p.add_argument('--python-runtime',default=shutil.which('desktop-test-python'));p.add_argument('--egl-vendor',type=Path,default=os.environ.get('__EGL_VENDOR_LIBRARY_FILENAMES'));p.add_argument('--inside',action='store_true');p.add_argument('--parent-net');p.add_argument('--revision');a=p.parse_args()
if not a.inside:
 if not a.output or not a.python_runtime:p.error('Pinned Python and fresh output required')
 out=a.output.resolve();out.mkdir(mode=0o700,parents=True,exist_ok=False)
 for name in ('sway','client','keyboard','xwayland','python_runtime'):
  value=Path(getattr(a,name)).resolve();assert str(value).startswith('/nix/store/') and value.is_file();setattr(a,name,value)
 args=[str(a.python_runtime),'/source/tests/desktop/xwayland_relationships_input.py','--inside','--sway',str(a.sway),'--client',str(a.client),'--keyboard',str(a.keyboard),'--xwayland',str(a.xwayland),'--parent-net',os.readlink('/proc/self/ns/net'),'--revision',sp.check_output(['git','rev-parse','HEAD'],text=True).strip()]
 if a.without_xres:args.append('--without-xres')
 if a.effects:
  assert a.egl_vendor and str(a.egl_vendor.resolve()).startswith('/nix/store/') and a.egl_vendor.is_file();args.extend(['--effects','--egl-vendor',str(a.egl_vendor.resolve())])
 raise SystemExit(isolated_run(Path(__file__).resolve().parents[2],out,args,software_graphics=a.effects))
assert os.getuid()!=0 and os.readlink('/proc/self/ns/net')!=a.parent_net
assert not any(Path('/dev',name).exists() for name in ('dri','snd','video0')) and Path('/dev/udmabuf').exists()==a.effects
out=Path('/artifacts');runtime=Path(os.environ['XDG_RUNTIME_DIR']);env=dict(os.environ,WLR_BACKENDS='headless',WLR_RENDERER='gles2' if a.effects else 'pixman',WLR_RENDERER_FORCE_SOFTWARE='1',WLR_RENDERER_ALLOW_SOFTWARE='1',LIBGL_ALWAYS_SOFTWARE='1',WLR_LIBINPUT_NO_DEVICES='1');processes=[];keyboard=None;checks=[]
if a.effects:env['__EGL_VENDOR_LIBRARY_FILENAMES']=str(a.egl_vendor)
def launch(command,name,**kwargs):
 with (out/name).open('w') as stream:child=sp.Popen(command,env=env,stderr=stream,stdout=kwargs.pop('stdout',stream),**kwargs)
 processes.append(child);return child

def call(*command):return sp.check_output(command,env=env,text=True,stderr=sp.PIPE,timeout=5).strip()
def tree():return json.loads(call('swaymsg','-r','-t','get_tree'))
def nodes(node):
 yield node
 for child in node.get('nodes',[])+node.get('floating_nodes',[]):yield from nodes(child)
def wait(predicate,label):
 deadline=time.monotonic()+10
 while time.monotonic()<deadline:
  try:
   value=predicate()
   if value:return value
  except (OSError,ValueError,sp.SubprocessError):pass
  time.sleep(.02)
 try:(out/'failure-tree.json').write_text(json.dumps(tree(),indent=2))
 except (OSError,ValueError,sp.SubprocessError):pass
 raise AssertionError('Timed out: '+label)
def location(client,index):
 xid=client.xid(index)
 def visit(node,workspace=None):
  if node.get('type')=='workspace':workspace=node['name']
  if node.get('window')==xid:return workspace
  for child in node.get('nodes',[])+node.get('floating_nodes',[]):
   found=visit(child,workspace)
   if found is not None:return found
  return None
 return visit(tree())
class Client:
 def __init__(self,label):
  self.child=launch([str(a.client),label],label+'.log',stdin=sp.PIPE,stdout=sp.PIPE,text=True,bufsize=1);self.buffer=b'';self.latest={};self.serial=0;self.drain(True);assert self.latest['pid']==self.child.pid
 def drain(self,required=False):
  deadline=time.monotonic()+3 if required else time.monotonic()
  while True:
   ready=select.select([self.child.stdout],[],[],max(0,deadline-time.monotonic()) if required and not self.latest else 0)[0]
   if not ready:break
   part=os.read(self.child.stdout.fileno(),65536);assert part,'Owned X11 client exited';self.buffer+=part
   while b'\n' in self.buffer:
    line,self.buffer=self.buffer.split(b'\n',1);self.latest=json.loads(line);(out/(str(self.child.pid)+'-client-state.json')).write_text(json.dumps(self.latest,indent=2))
  if required:assert self.latest,'Missing client acknowledgement'
  return self.latest
 def command(self,action,index=0,value=0):
  self.serial+=1;self.child.stdin.write(f'{action} {index} {value}\n');self.child.stdin.flush();wait(lambda:self.drain()['serial']>=self.serial,'exact client request '+action)
 def xid(self,index):return self.drain()['windows'][index]['id']
 def node(self,index):return next((node for node in nodes(tree()) if node.get('window')==self.xid(index)),None)
 def mapped(self,index):return wait(lambda:self.node(index),'actual mapped X11 surface')
 def relation(self,index):return (self.node(index) or {}).get('lumen_relationships',{})
 def key_count(self,index):return self.drain()['windows'][index]['keys']
 def map(self,index):self.command('map',index);return self.mapped(index)
 def focus(self,index,target=None):
  node=self.mapped(index);call('swaymsg',f'[con_id={node["id"]}] focus');wanted=self if target is None else target[0];slot=index if target is None else target[1];wait(lambda:wanted.node(slot) and wanted.node(slot).get('focused'),'exact native focus')
 def type_only(self,index,others):
  before=[(client,slot,client.key_count(slot)) for client,slot in [(self,index),*others]];keyboard.key('-k','a');wait(lambda:self.key_count(index)==before[0][2]+1,'actual X11 key event')
  assert all(client.key_count(slot)==count for client,slot,count in before[1:]),'Keyboard reached bystander'
try:
 start_private_bus(env,out,processes)
 shell=shutil.which('sh');assert shell and str(Path(shell).resolve()).startswith('/nix/store/')
 wrapper=out/'xwayland-server';wrapper.write_text('#!'+shell+'\nexec '+str(a.xwayland)+(' -extension X-Resource' if a.without_xres else '')+' "$@"\n');wrapper.chmod(0o700);env['WLR_XWAYLAND']=str(wrapper)
 display_helper=out/'display-helper';display_helper.write_text('#!'+sys.executable+'\nimport os\nfrom pathlib import Path\nPath("/artifacts/display").write_text(os.environ["DISPLAY"])\n');display_helper.chmod(0o700)
 config=out/'sway.conf';config.write_text('output HEADLESS-1 resolution 1280x800\nseat seat0 fallback true\nfocus_follows_mouse no\nxwayland force\nfor_window [all] floating enable\nexec '+str(display_helper)+'\n')
 compositor=launch([str(a.sway),'-d','--unsupported-gpu','-c',str(config)],'sway.log')
 env['SWAYSOCK']=str(wait(lambda:next(runtime.glob('sway-ipc.*.sock'),None),'private compositor'));env['WAYLAND_DISPLAY']=wait(lambda:next((v.name for v in runtime.glob('wayland-*') if not v.name.endswith('.lock')),None),'private Wayland socket');env['DISPLAY']=wait(lambda:(out/'display').read_text() if (out/'display').exists() else None,'private Xwayland DISPLAY');assert env['DISPLAY'].startswith(':')
 with (out/'keyboard.log').open('w') as stream:keyboard=PhysicalKeyboard(a.keyboard,env,stream)
 processes.append(keyboard.process)
 first=Client('first');second=Client('second');first.map(0);second.map(0)
 if a.without_xres:
  assert first.drain()['xres'] is False and second.drain()['xres'] is False
  assert first.node(0).get('pid',0)==0 and second.node(0).get('pid',0)==0
  first.command('parent',1,first.xid(0));first.command('property',1,1);first.map(1)
  assert first.relation(1)==dict(version=1,parent_supported=False,parent=None,modal=None),first.relation(1)
  first.focus(0);first.type_only(0,[(first,1),(second,0)]);checks.append('Actual XRes-disabled server leaves unknown-PID ancestry/modal unavailable and preserves explicit key recipient')
 else:
  assert first.drain()['xres'] is True and second.drain()['xres'] is True
  assert first.node(0)['pid']==first.child.pid and second.node(0)['pid']==second.child.pid
  first.command('parent',1,first.xid(0));first.map(1)
  wait(lambda:first.relation(1).get('parent')==first.node(0)['id'],'live same-XRes-process ancestry');assert first.relation(1)['parent_supported'] and first.relation(1)['modal'] is False
  checks.append('Actual XRes process identities and live WM_TRANSIENT_FOR map to exact parent conId')
  first.focus(1);first.type_only(1,[(first,0),(second,0)]);first.focus(0);first.type_only(0,[(first,1),(second,0)]);second.focus(0);second.type_only(0,[(first,0),(first,1)]);checks.append('Utility/document/other-process focus sequence delivers each physical key only to the captured native recipient')
  first.command('parent',2,first.xid(0));first.command('property',2,1);first.map(2);first.focus(0,(first,2));first.type_only(2,[(first,0),(first,1),(second,0)]);checks.append('Initially modal child receives parent-targeted focus and typing without redirecting unrelated app')
  first.command('modal',2,0);wait(lambda:first.relation(2).get('modal') is False,'client-message modal removal');first.focus(0);first.command('modal',2,1);wait(lambda:first.node(2).get('focused') and first.relation(2).get('modal') is True,'live modal add immediate focus reconciliation');first.type_only(2,[(first,0),(second,0)])
  first.command('modal',2,0);wait(lambda:first.relation(2).get('modal') is False,'live modal remove');first.focus(0);first.command('property',2,1);wait(lambda:first.node(2).get('focused'),'property modal add');first.command('property',2,0);wait(lambda:first.relation(2).get('modal') is False,'property deletion resets modal');first.focus(0);checks.append('Actual EWMH client-message and property add/remove/delete reconcile modality live with no sticky retired state')
  first.command('parent',2,first.xid(1));wait(lambda:first.relation(2).get('parent')==first.node(1)['id'],'actual reparent');first.command('parent',2,0);wait(lambda:first.relation(2).get('parent') is None and first.relation(2).get('parent_supported'),'actual detach');checks.append('Live reparent/detach updates exact parent metadata')
  first.command('parent',1,first.xid(0));first.command('parent',0,first.xid(1));wait(lambda:first.relation(0).get('parent') is None and first.relation(1).get('parent')==first.node(0)['id'],'upstream rejects cyclic parent hint');first.focus(0);first.type_only(0,[(first,1),(first,2),(second,0)]);first.command('parent',0,0);checks.append('Actual cycle attempt is rejected by pinned XWM and preserves acyclic ancestry without hangs or arbitrary focus redirection')
  first.command('parent',2,first.xid(0));first.command('modal',1,1);first.command('modal',2,1);wait(lambda:first.relation(1).get('modal') and first.relation(2).get('modal'),'two actual modal siblings');second.focus(0);call('swaymsg',f'[con_id={first.node(0)["id"]}] focus');assert second.node(0).get('focused');second.type_only(0,[(first,0),(first,1),(first,2)]);first.command('modal',1,0);first.command('modal',2,0);checks.append('Ambiguous live modal siblings reject parent activation and never select arbitrary sibling')
  first.command('parent',2,0);first.command('modal',2,1);wait(lambda:first.relation(2).get('modal') and first.relation(2).get('parent') is None,'parentless modal');assert first.node(2)['lumen_pair']['eligible'] is False
  rejected=sp.run(['swaymsg','-r',f'lumen_pair create {first.node(0)["id"]} {first.node(2)["id"]}'],env=env,text=True,capture_output=True,timeout=5);result=json.loads(rejected.stdout);assert rejected.returncode==2 and result and result[0]['success'] is False;assert not first.node(0)['lumen_pair'].get('id');first.command('modal',2,0);checks.append('Actual parentless modal is ineligible for paired primary membership and create fails without changing partner')
  first.command('parent',1,second.xid(0));first.command('pid',1,second.child.pid);wait(lambda:first.relation(1).get('parent_supported') is False,'crossprocess hint unknown');assert first.node(1)['pid']==first.child.pid and first.relation(1).get('parent') is None and first.relation(1).get('modal') is None;first.command('property',1,1);second.focus(0);second.type_only(0,[(first,0),(first,1),(first,2)]);first.command('property',1,0);checks.append('Cross-process parent and forged _NET_WM_PID cannot change XRes PID, confer ancestry or steal unrelated keyboard input')
  first.command('parent',1,first.xid(0));first.command('unmap',0);wait(lambda:first.node(0) is None,'parent actual unmap');assert first.relation(1).get('parent') is None;first.map(0);wait(lambda:first.relation(1).get('parent')==first.node(0)['id'],'parent actual remap');old=first.node(0)['id'];first.command('destroy',0);wait(lambda:first.node(0) is None,'parent actual destroy');wait(lambda:first.relation(1).get('parent') is None,'destroy clears child link');first.command('new',0);first.map(0);assert first.node(0)['id']!=old and first.relation(1).get('parent') is None;checks.append('Parent unmap/remap/destroy and replacement retire stale conIds without retargeting child')
  first.command('override',3,1);first.command('map',3);wait(lambda:first.drain()['windows'][3]['mapState']==2 and first.drain()['windows'][3]['overrideRedirect']==1 and first.node(3) is None,'actual mapped override-redirect surface')
  first.command('parent',1,first.xid(3));wait(lambda:first.relation(1).get('parent_supported') is False and first.relation(1).get('parent') is None,'managed child rejects unmanaged parent');first.focus(1);first.type_only(1,[(first,0),(second,0)])
  first.command('override',3,0);managed=first.mapped(3);wait(lambda:first.relation(1).get('parent')==managed['id'],'unmanaged parent becomes managed');first.command('override',3,1);wait(lambda:first.node(3) is None and first.relation(1).get('parent_supported') is False,'managed parent becomes unmanaged');first.focus(1);first.type_only(1,[(first,0),(second,0)]);first.command('parent',1,0);first.command('unmap',3);first.command('override',3,0);checks.append('Actual managed child rejects same-PID override-redirect parent safely through managed/unmanaged transitions without wrong casts or key targets')
  owner=first.node(0)['id'];partner=second.node(0)['id'];call('swaymsg','-r',f'lumen_pair create {owner} {partner}');wait(lambda:first.node(0)['lumen_pair'].get('partner')==partner,'actual X11 primary pair')
  first.command('parent',3,first.xid(0));first.command('property',3,1);first.map(3);wait(lambda:first.relation(3).get('parent')==owner and location(first,3)==location(first,0),'actual mapped modal follows paired owner');first.focus(0,(first,3));first.type_only(3,[(first,0),(second,0)]);assert first.node(3)['lumen_pair']['eligible'] is False
  call('swaymsg','-r',f'lumen_pair leave {owner}');wait(lambda:not first.node(0)['lumen_pair'].get('id') and location(first,3)==location(first,0),'paired owner exit restores modal association');call('swaymsg','-r',f'lumen_pair leave {partner}');first.command('parent',3,0);first.command('property',3,0);wait(lambda:first.relation(3).get('parent') is None and first.node(3)['lumen_pair']['eligible'],'actual dialog detaches without stale pair ownership');checks.append('Actual X11 primary pair admits a new same-process modal, routes exact input and restores/detaches its owner without stranding it')

 (out/'final-tree.json').write_text(json.dumps(tree(),indent=2))
 first.command('parent',1,first.xid(0));first.command('property',1,1)
 compositor.terminate();compositor.wait(timeout=5);first.child.wait(timeout=5);second.child.wait(timeout=5);assert compositor.returncode==first.child.returncode==second.child.returncode==0 and not Path(env['SWAYSOCK']).exists();checks.append('Actual compositor SIGTERM exit with live X11 parent/modal clients detaches listeners cleanly and removes private socket')
 if a.effects:
  log=(out/'sway.log').read_text();assert all(v in log for v in ('Creating scenefx FX renderer','GL renderer: llvmpipe','FX RENDERER: Shaders Initialized Successfully','Trying udmabuf allocator','Created GL FBO for buffer'))
 (out/'result.json').write_text(json.dumps(dict(passed=True,checks=checks,sourceRevision=a.revision,effects=a.effects,withoutXRes=a.without_xres,fixtureSha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),packages={k:dict(path=str(v),sha256=hashlib.sha256(v.read_bytes()).hexdigest()) for k,v in dict(compositor=a.sway,client=a.client,keyboard=a.keyboard,xwayland=a.xwayland).items()},scope='Private actual Xwayland server/XRes/WM hints and physical key recipients; hints are not authenticated application or document identity'),indent=2)+'\n');print('PASS actual Xwayland relationships',len(checks),'checks')
finally:
 for child in reversed(processes):
  if child.poll() is None:
   child.terminate()
   try:child.wait(timeout=5)
   except sp.TimeoutExpired:child.kill();child.wait(timeout=5)
 close_reaped_pipes(processes)
