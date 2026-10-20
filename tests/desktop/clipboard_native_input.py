"""Private real Wayland clipboard, actual launcher controls and document paste observer."""
from fixture_resources import close_reaped_pipes
import argparse,json,os,re,shutil,signal,subprocess as sp,sys,tempfile,time
from pathlib import Path
from private_native_bus import start_private_bus
p=argparse.ArgumentParser();p.add_argument('--launcher',type=Path,required=True);p.add_argument('--sway',required=True);p.add_argument('--producer',required=True);p.add_argument('--observer',required=True);p.add_argument('--pointer-tool',required=True);p.add_argument('--effects',action='store_true');p.add_argument('--output',type=Path,required=True);a=p.parse_args();a.output.mkdir(parents=True,exist_ok=False)
repo=Path(__file__).resolve().parents[2];assets=repo/'res/lumen-desktop/shell';processes=[]
with tempfile.TemporaryDirectory(prefix='lumen-clipboard-native-') as temporary:
 private=Path(temporary);env=dict(os.environ,HOME=temporary,XDG_RUNTIME_DIR=temporary,XDG_STATE_HOME=str(private/'state'),XDG_CONFIG_HOME=str(private/'config'),XDG_CACHE_HOME=str(private/'cache'),XDG_DATA_HOME=str(private/'data'),WLR_BACKENDS='headless',WLR_RENDERER='gles2' if a.effects else 'pixman',WLR_RENDERER_FORCE_SOFTWARE='1' if a.effects else '0',WLR_RENDERER_ALLOW_SOFTWARE='1',LIBGL_ALWAYS_SOFTWARE='1',WLR_LIBINPUT_NO_DEVICES='1',QT_QPA_PLATFORM='wayland',QT_QPA_PLATFORMTHEME='generic',QT_QUICK_BACKEND='software',QT_ACCESSIBILITY='0',QT_LINUX_ACCESSIBILITY_ALWAYS_ON='0',NO_AT_BRIDGE='1',GTK_A11Y='none',QT_NO_XDG_DESKTOP_PORTAL='1',LUMEN_FILE_SERVICE='0',LUMEN_LAUNCH_SCOPES='0')
 for key in ('WAYLAND_DISPLAY','DISPLAY','SWAYSOCK','DBUS_SESSION_BUS_ADDRESS','AT_SPI_BUS_ADDRESS'):env.pop(key,None)
 prefix=re.split(r'\n[ \t]*exec quickshell ',a.launcher.read_text(),maxsplit=1)[0]
 raw=sp.check_output(['bash','-c',prefix+'\nexec env -0'],env=env);env.update(dict(v.decode().split('=',1) for v in raw.split(b'\0') if b'=' in v));env.update(QT_QPA_PLATFORM='wayland',QT_QPA_PLATFORMTHEME='generic',QT_QUICK_BACKEND='software',QT_ACCESSIBILITY='0',QT_LINUX_ACCESSIBILITY_ALWAYS_ON='0',NO_AT_BRIDGE='1',GTK_A11Y='none',LUMEN_FILE_SERVICE='0',LUMEN_LAUNCH_SCOPES='0',GLOBAL_MENU_ENABLED='0',LUMEN_SEARCH_ROOTS=json.dumps([str(private/'Documents')]))
 binary=private/'bin';binary.mkdir();env['PATH']=str(binary)+':'+env['PATH'];env['CLIPBOARD_NATIVE_OUTPUT']=str(a.output.resolve());env['CLIPBOARD_NATIVE_CLOCK']=str(a.output/'clock-offset');env['LUMEN_CLIPBOARD_RETENTION_SECONDS']='60'
 cli=binary/'lumen-clipboard-history';cli.write_text('#!'+sys.executable+'\nimport os,runpy,time\nfrom pathlib import Path\nclock=time.clock_gettime\ndef virtual_clock(kind):\n value=clock(kind)\n offset=Path(os.environ["CLIPBOARD_NATIVE_CLOCK"])\n return value+(float(offset.read_text()) if kind==time.CLOCK_BOOTTIME and offset.exists() else 0)\ntime.clock_gettime=virtual_clock\nrunpy.run_path('+repr(str(repo/'res/lumen-desktop/runtime/clipboard_history.py'))+',run_name="__main__")\n');cli.chmod(0o700);env['LUMEN_CLIPBOARD_COMMAND']=str(cli);env['LUMEN_CLIPBOARD_OBSERVER']=a.observer
 # Only service supervision is adapted. All offers, readers, writers and history code are real.
 ctl=binary/'systemctl';ctl.write_text('#!'+sys.executable+'''\nimport os,signal,subprocess as sp,sys,time
from pathlib import Path
out=Path(os.environ['CLIPBOARD_NATIVE_OUTPUT']);pidfile=out/'watcher.pid'
assert sys.argv[1:] in (['--user','start','lumen-clipboard-history.service'],['--user','stop','lumen-clipboard-history.service'])
failure=out/'control-failure'
if failure.exists() and failure.read_text()==sys.argv[2]:raise SystemExit(1)
if sys.argv[2]=='start':
 if pidfile.exists():
  try:
   old=int(pidfile.read_text());os.kill(old,0)
   if not Path('/proc/'+str(old)+'/stat').read_text().split(') ',1)[1].startswith('Z'):raise SystemExit(0)
  except (ProcessLookupError,FileNotFoundError):pass
 unavailable=out/'empty-provider'
 if unavailable.exists():os.environ['WAYLAND_DISPLAY']=unavailable.read_text()
 pid=os.fork()
 if pid==0:
  os.setsid();fd=os.open(out/'watcher.log',os.O_WRONLY|os.O_CREAT|os.O_APPEND,0o600);os.dup2(fd,1);os.dup2(fd,2);os.close(fd)
  fd=os.open('/dev/null',os.O_RDONLY);os.dup2(fd,0);os.close(fd)
  os.execv(os.environ['LUMEN_CLIPBOARD_COMMAND'],[os.environ['LUMEN_CLIPBOARD_COMMAND'],'watch'])
 pidfile.write_text(str(pid))
else:
 if pidfile.exists():
  pid=int(pidfile.read_text())
  try:os.killpg(pid,signal.SIGTERM)
  except ProcessLookupError:pass
  until=time.monotonic()+3
  while time.monotonic()<until:
   try:
    if Path('/proc/'+str(pid)+'/stat').read_text().split(') ',1)[1].startswith('Z'):break
   except FileNotFoundError:break
   time.sleep(.02)
  else:raise RuntimeError('Private watcher did not stop')
  pidfile.unlink()
''');ctl.chmod(0o700)
 def launch(cmd,name,**kw):
  with (a.output/name).open('w') as log:proc=sp.Popen(cmd,env=env,stdout=log,stderr=sp.STDOUT,start_new_session=True,**kw)
  processes.append(proc);return proc
 def call(*cmd):return sp.check_output(cmd,env=env,text=True,timeout=5)
 def wait(fn):
  deadline=time.monotonic()+12
  while time.monotonic()<deadline:
   try:value=fn()
   except (sp.CalledProcessError,ValueError):value=None
   if value:return value
   time.sleep(.04)
  raise AssertionError('Clipboard native readiness failed; inspect '+str(a.output))
 def ipc(method,*args):return call('quickshell','-p',str(a.output/'shell.qml'),'ipc','call','fixture',method,*args).strip()
 def focus(title):
  rows=json.loads(call(str(Path(a.sway).parent/'swaymsg'),'-r','[title="^'+title+'$"] focus'));assert all(row['success'] for row in rows),rows
 def state():return json.loads(ipc('status'))
 def history():return json.loads(call(str(cli),'list'))
 def pointer(text):
  def ready():
   position=json.loads(ipc('button',text));return position if position.get('visible') and position.get('enabled') else None
  pos=wait(ready)
  for command in ('seat seat0 cursor set '+str(round(pos['x']))+' '+str(round(pos['y'])),):
   rows=json.loads(call(str(Path(a.sway).parent/'swaymsg'),'-r',command));assert all(row['success'] for row in rows),rows
  (a.output/'last-button.json').write_text(json.dumps(pos));native_pointer.stdin.write('m 0.5 0.5\n');native_pointer.stdin.flush();time.sleep(.12);native_pointer.stdin.write('p\n');native_pointer.stdin.flush();time.sleep(.12);native_pointer.stdin.write('r\n');native_pointer.stdin.flush()
 def producer(text,marker=None,delay=0):
  index=len(processes);proc=launch([a.producer,text,*([marker or '-',str(delay)] if marker or delay else [])],f'producer-{index}.log');wait(lambda:'ready' in (a.output/f'producer-{index}.log').read_text());return a.output/f'producer-{index}.log'
 def refresh(query=''):
  ipc('query',query);time.sleep(.18);wait(lambda:not state()['busy'] and not state()['refreshPending'])
 def watcher_stop():
  call(str(ctl),'--user','stop','lumen-clipboard-history.service');time.sleep(.2)
 try:
  start_private_bus(env,a.output,processes)
  conf=a.output/'sway.conf';conf.write_text('output HEADLESS-1 resolution 1200x900\nxwayland disable\nseat seat0 fallback true\nfocus_follows_mouse no\ndefault_border none\nfor_window [app_id="org.quickshell"] floating enable, move position 0 0\n')
  launch([a.sway,'--unsupported-gpu','-c',str(conf)],'sway.log');wait(lambda:list(private.glob('sway-ipc*.sock')))
  env['SWAYSOCK']=str(next(private.glob('sway-ipc*.sock')));env['WAYLAND_DISPLAY']=next(f.name for f in private.glob('wayland-*') if not f.name.endswith('.lock'))
  launch(['wtype','-s','600000'],'keyboard.log')
  native_pointer=launch([a.pointer_tool,'pointer','move','0','0'],'pointer.log',stdin=sp.PIPE,text=True)
  shutil.copytree(assets,a.output/'assets/shell');shutil.copytree(assets.parent/'appearance',a.output/'assets/appearance')
  palettes=json.loads((assets.parent/'appearance/palette.json').read_text());appearance=private/'state/lumen-desktop/appearance.json';appearance.parent.mkdir(parents=True,exist_ok=True);appearance.write_text(json.dumps(dict(font=palettes['font'],palette=palettes['light'],preferences=dict(mode='light',reduceMotion=True))))
  qml=a.output/'shell.qml';qml.write_text('''import QtQuick
import QtQuick.Controls
import QtQuick.Window
import Quickshell
import Quickshell.Io
import "assets/shell" as D
ShellRoot {
 id:root
 D.SearchState {id:search}
 QtObject {id:notifications;property bool dnd:false;property bool available:false}
 QtObject {id:audio;property bool muted:false;property bool available:false}
 QtObject {id:exported;property var state:({menu:null})}
 QtObject {id:stub;property var search:search;property var notifications:notifications;property var audioState:audio;property var exportedMenu:exported;property var entries:[];property date currentDate:new Date();function appLabel(e){return e.name;}}
 FloatingWindow {id:stage;visible:true;implicitWidth:1000;implicitHeight:850;title:"Clipboard Launcher Fixture";color:"#202024"
  D.AppLauncher {id:launcher;shell:stub;x:80;y:50;onCloseRequested:stage.visible=false}
 }
 FloatingWindow {id:document;visible:false;implicitWidth:800;implicitHeight:600;title:"Clipboard Document Observer";color:"white"
  TextArea {id:editor;anchors.fill:parent;text:"";selectByMouse:true}
 }
 function find(item,predicate){if(predicate(item))return item;for(const child of item.children || []){const match=find(child,predicate);if(match)return match;}return null;}
 function clipboard(){return find(launcher,item=>item.objectName==="clipboard-history-state");}
 IpcHandler {target:"fixture"
  function status():string {const c=root.clipboard();return JSON.stringify({provider:launcher.provider,enabled:c.enabled,collecting:c.collecting,busy:c.busy,refreshPending:c.refreshPending,available:c.available,error:c.error,message:c.message,entries:c.entries,results:launcher.results,visible:stage.visible,document:editor.text});}
  function open():void {stage.visible=true;launcher.visible=true;launcher.setProvider("Clipboard");launcher.focusSearch();}
  function query(text:string):void {const c=root.clipboard();c.search(text);}
  function refresh():void {root.clipboard().request("list");}
  function button(text:string):string {const b=root.find(launcher,item=>item.text===text && item.visible);if(!b)return JSON.stringify({visible:false});const point=b.mapToItem(stage.contentItem,b.width/2,b.height/2);return JSON.stringify({x:point.x,y:point.y,visible:b.visible,enabled:b.enabled});}
  function select(id:string):void {const index=launcher.results.findIndex(row=>row.id===id);if(index>=0){launcher.selectedIndex=index;launcher.focusSearch();}}
  function capturedCopy(key:string):void {launcher.launchKey(key,false);}
  function documentFocus():void {stage.visible=false;document.visible=true;editor.forceActiveFocus();}
 }
 Component.onCompleted:Qt.callLater(()=>launcher.setProvider("Clipboard"))
}
''')
  launch(['quickshell','-p',str(qml)],'shell.log');wait(lambda:state()['available'] and not state()['busy'])
  assert not state()['enabled'] and not history()['entries']
  producer('Synthetic before consent');wait(lambda:call('wl-paste','--no-newline')=='Synthetic before consent')
  focus('Clipboard Launcher Fixture');ipc('open');time.sleep(.15);call('wtype','-k','Escape');wait(lambda:not state()['visible']);assert not history()['enabled'] and history()['entries']==[]
  assert call('wl-paste','--no-newline')=='Synthetic before consent','Declining changed ordinary clipboard'
  ipc('open');wait(lambda:state()['visible']);focus('Clipboard Launcher Fixture');pointer('Applications');wait(lambda:state()['provider']=='Applications');pointer('Clipboard');wait(lambda:state()['provider']=='Clipboard');pointer('Enable Clipboard History');wait(lambda:state()['enabled'] and not state()['busy'])
  wait(lambda:(a.output/'watcher.log').exists() and 'watching' in (a.output/'watcher.log').read_text());assert history()['entries']==[],'Initial pre-consent selection was imported'
  producer('Synthetic alpha');wait(lambda:any(e['text']=='Synthetic alpha' for e in history()['entries']))
  producer('Synthetic beta');wait(lambda:any(e['text']=='Synthetic beta' for e in history()['entries']))
  refresh();rows=state()['results'];alpha=next(e for e in rows if e['title']=='Synthetic alpha')
  ipc('select',alpha['id']);call('wtype','-k','Return');wait(lambda:'Paste' in state()['message'])
  assert call('wl-paste','--no-newline')=='Synthetic alpha'
  assert state()['document']=='','Selecting history silently inserted document text'
  ipc('documentFocus');time.sleep(.2);focus('Clipboard Document Observer');call('wtype','-M','ctrl','v','-m','ctrl');wait(lambda:state()['document']=='Synthetic alpha')
  ipc('open');wait(lambda:state()['visible']);refresh();pointer('Clear History');wait(lambda:not state()['busy'] and history()['entries']==[])
  watcher_stop();call(str(ctl),'--user','start','lumen-clipboard-history.service');time.sleep(.5);refresh()
  assert history()['entries']==[],'Clear followed by watcher restart replayed a retired current selection'
  # A new same-byte offer is a new event, distinct from the skipped startup offer.
  producer('Synthetic alpha');wait(lambda:any(e['text']=='Synthetic alpha' for e in history()['entries']))
  pointer('Clear History');wait(lambda:not state()['busy'] and history()['entries']==[])
  watcher_stop();call('wl-copy','--clear');call(str(ctl),'--user','start','lumen-clipboard-history.service');time.sleep(.2)
  producer('Synthetic first after empty');wait(lambda:any(e['text']=='Synthetic first after empty' for e in history()['entries']))
  ipc('capturedCopy','clipboard:'+alpha['id']+':');time.sleep(.15)
  assert call('wl-paste','--no-newline')=='Synthetic first after empty','Retired history identity copied a replacement row'
  pointer('Clear History');wait(lambda:not state()['busy'] and history()['entries']==[])
  producer('@html-first');wait(lambda:any(e['text']=='Synthetic plain preferred' for e in history()['entries']));assert not any('<b>' in e['text'] for e in history()['entries']),'HTML ordering displaced the offered plaintext'
  pointer('Clear History');wait(lambda:not state()['busy'] and history()['entries']==[])
  producer('@html-only');time.sleep(.25);assert history()['entries']==[],'Rich-only offer was treated as plain text'
  for marker in ('x-kde-passwordManagerHint','application/x-keepassxc'):
   marked=producer('Synthetic excluded '+marker,marker);time.sleep(.2);producer('Synthetic replacement '+marker);wait(lambda:any(e['text']=='Synthetic replacement '+marker for e in history()['entries']));refresh();assert not any('Synthetic excluded' in e['text'] for e in history()['entries'])
  before_clear=producer('Synthetic pending before clear',delay=1000);wait(lambda:'send text/' in before_clear.read_text());pointer('Clear History');wait(lambda:not state()['busy'] and history()['entries']==[]);time.sleep(1.1);assert history()['entries']==[],'In-flight pre-clear offer repopulated history'
  pending=producer('Synthetic delayed retired',delay=500);wait(lambda:'send text/' in pending.read_text());producer('Synthetic replacement after pending');wait(lambda:any(e['text']=='Synthetic replacement after pending' for e in history()['entries']));time.sleep(.55);assert not any(e['text']=='Synthetic delayed retired' for e in history()['entries'])
  producer('@oversize');time.sleep(.25);assert not any(len(e['text'])>4096 or e['text'].startswith('XXX') for e in history()['entries'])
  slow=producer('Synthetic timed out',delay=3500);wait(lambda:'send text/' in slow.read_text());time.sleep(3.2);assert not any(e['text']=='Synthetic timed out' for e in history()['entries'])
  producer('Synthetic allowed after markers');wait(lambda:any(e['text']=='Synthetic allowed after markers' for e in history()['entries']))
  pointer('Disable History');wait(lambda:not state()['busy'] and not state()['enabled']);assert history()['entries']==[]
  producer('Synthetic after disable');time.sleep(.3);assert history()['entries']==[] and call('wl-paste','--no-newline')=='Synthetic after disable'
  # Even a failed service command cannot leave the owned collector retaining data.
  pointer('Enable Clipboard History');wait(lambda:state()['enabled'] and not state()['busy']);time.sleep(.15)
  producer('Synthetic before stop failure');wait(lambda:any(e['text']=='Synthetic before stop failure' for e in history()['entries']));refresh()
  watcher_pid=int((a.output/'watcher.pid').read_text());children=Path('/proc/'+str(watcher_pid)+'/task/'+str(watcher_pid)+'/children').read_text().split();assert len(children)==1,children
  observer_pid=int(children[0]);assert os.path.samefile('/proc/'+str(observer_pid)+'/exe',a.observer)
  failure=a.output/'control-failure';failure.write_text('stop');pointer('Clear History');wait(lambda:not state()['busy'] and not state()['enabled']);wait(lambda:not Path('/proc/'+str(observer_pid)).exists());assert history()['entries']==[];assert call('wl-paste','--no-newline')=='Synthetic before stop failure';failure.unlink()
  pointer('Enable Clipboard History');wait(lambda:state()['enabled'] and not state()['busy']);time.sleep(.15)
  producer('Synthetic before start failure');wait(lambda:any(e['text']=='Synthetic before start failure' for e in history()['entries']));refresh()
  failure.write_text('start');pointer('Clear History');wait(lambda:not state()['busy'] and not state()['enabled']);assert history()['entries']==[];assert call('wl-paste','--no-newline')=='Synthetic before start failure';failure.unlink()
  empty=launch([str(Path(a.producer).parent/'lumen-test-clipboard-empty-provider')],'empty-provider.log');wait(lambda:(a.output/'empty-provider.log').read_text().strip());unavailable=a.output/'empty-provider';unavailable.write_text((a.output/'empty-provider.log').read_text().strip())
  pointer('Enable Clipboard History');wait(lambda:not state()['busy'] and not state()['enabled'] and bool(state()['error']));assert not history()['enabled'] and not history()['collecting'],'Unsupported provider reported collection ready';unavailable.unlink()
  pointer('Enable Clipboard History');wait(lambda:not state()['busy'] and state()['enabled'] and state()['collecting']);watcher_stop();unavailable.write_text((a.output/'empty-provider.log').read_text().strip());call(str(ctl),'--user','start','lumen-clipboard-history.service');time.sleep(.2);refresh();assert state()['enabled'] and not state()['collecting'] and 'unavailable' in state()['error'].lower(),'Transient provider failure discarded consent or claimed collection';assert history()['enabled'] and not history()['collecting'];unavailable.unlink()
  watcher_stop();call(str(ctl),'--user','start','lumen-clipboard-history.service');wait(lambda:history()['collecting'])
  watcher_pid=int((a.output/'watcher.pid').read_text());identity=Path('/proc/'+str(watcher_pid)+'/stat').read_text().rsplit(') ',1)[1].split()[19];start=len((a.output/'watcher.log').read_text());os.kill(watcher_pid,signal.SIGSTOP)
  try:
   queued=producer('Synthetic queued past retention');wait(lambda:'send text/' in queued.read_text());time.sleep(.1);(a.output/'clock-offset').write_text('61')
  finally:
   if Path('/proc/'+str(watcher_pid)+'/stat').exists():
    assert Path('/proc/'+str(watcher_pid)+'/stat').read_text().rsplit(') ',1)[1].split()[19]==identity
    os.kill(watcher_pid,signal.SIGCONT)
  wait(lambda:'"recorded": false' in (a.output/'watcher.log').read_text()[start:]);assert history()['entries']==[],'Paused supervisor refreshed expired queued data'
  pointer('Disable History');wait(lambda:not state()['busy'] and not state()['enabled'])
  assert not list((private/'state/lumen-desktop/clipboard').iterdir()),'Clipboard content or consent persisted after revocation'
  log=(a.output/'shell.log').read_text();assert not any(error in log for error in ('ReferenceError:','TypeError:','Failed to load configuration')),log
  result=dict(passed=True,checks=['New-home consent decline preserves ordinary clipboard without history','Actual launcher enable starts private real Wayland watcher','Selected captured history ID copies exact real clipboard without insertion','Explicit native document paste inserts selected text','Clear and watcher restart leave history empty','Initial nil and same-byte future selections preserve event identity','Retired history ID cannot copy replacement row','Plaintext preferred despite HTML-first order; rich-only offers excluded','Both supported sensitive MIME markers excluded from history despite replacement','Pending pre-clear/replaced/oversize/timed-out transfers excluded','Disable clears history and stops collection while ordinary clipboard works','Clear stop failure revokes consent and reaps actual idle native observer','Clear start failure leaves storage empty and ordinary clipboard intact','Unsupported real Wayland provider cannot complete initial enablement','Transient provider failure retains consent and explicitly reports unavailable collection','Actual paused supervisor rejects queued data after simulated BOOTTIME expiry'])
  (a.output/'result.json').write_text(json.dumps(result,indent=2));print(a.output/'result.json')
 finally:
  try:call('grim',str(a.output/'last-screen.png'))
  except (OSError,sp.SubprocessError):pass
  try:(a.output/'last-history.json').write_text(json.dumps(history()))
  except (OSError,sp.SubprocessError,ValueError):pass
  try:(a.output/'last-state.json').write_text(json.dumps(state()));(a.output/'last-tree.json').write_text(call(str(Path(a.sway).parent/'swaymsg'),'-r','-t','get_tree'))
  except (OSError,sp.SubprocessError,ValueError):pass
  try:watcher_stop()
  except (OSError,sp.SubprocessError):pass
  for proc in reversed(processes):
   if proc.poll() is None:
    os.killpg(proc.pid,signal.SIGTERM)
    try:proc.wait(timeout=5)
    except sp.TimeoutExpired:os.killpg(proc.pid,signal.SIGKILL);proc.wait(timeout=5)
   if proc.stdout is not None:proc.stdout.close()
   if proc.stdin is not None:proc.stdin.close()
  for _fixture_process in processes:
      if _fixture_process.poll() is None:_fixture_process.wait(timeout=5)
  close_reaped_pipes(processes)
