"""Private installed launcher routing and deliberate recent-query history input."""
import argparse,ast,hashlib,json,os,re,shutil,signal,subprocess as sp,sys,tempfile,time
from pathlib import Path
from private_native_bus import start_private_bus
from fixture_resources import close_reaped_pipes
p=argparse.ArgumentParser();p.add_argument('--launcher',type=Path,required=True);p.add_argument('--app-launcher',type=Path,required=True);p.add_argument('--sway',required=True);p.add_argument('--pointer-tool',required=True);p.add_argument('--effects',action='store_true');p.add_argument('--output',type=Path,required=True);a=p.parse_args();out=a.output.resolve();out.mkdir(parents=True,exist_ok=False);processes=[];checks=[];transforms=[]
with tempfile.TemporaryDirectory(prefix='lumen-query-history-') as temporary:
 home=Path(temporary);env=dict(os.environ,HOME=temporary,XDG_RUNTIME_DIR=temporary,GSETTINGS_BACKEND='memory')
 for key in ('CONFIG','STATE','CACHE','DATA'):env['XDG_'+key+'_HOME']=str(home/key.lower())
 for key in ('SWAYSOCK','WAYLAND_DISPLAY','DISPLAY','DBUS_SESSION_BUS_ADDRESS','AT_SPI_BUS_ADDRESS'):env.pop(key,None)
 prefix=re.split(r'\n[ \t]*exec quickshell ',a.launcher.read_text(),maxsplit=1)[0]
 env.update(dict(v.decode().split('=',1) for v in sp.check_output(['bash','-c',prefix+'\nexec env -0'],env=env).split(b'\0') if b'=' in v))
 env.update(QT_QPA_PLATFORM='wayland',QT_QPA_PLATFORMTHEME='generic',QT_QUICK_BACKEND='software',QT_ACCESSIBILITY='0',QT_LINUX_ACCESSIBILITY_ALWAYS_ON='0',NO_AT_BRIDGE='1',GTK_A11Y='none',QT_NO_XDG_DESKTOP_PORTAL='1',WLR_BACKENDS='headless',WLR_RENDERER='gles2' if a.effects else 'pixman',WLR_RENDERER_FORCE_SOFTWARE='1' if a.effects else '0',WLR_RENDERER_ALLOW_SOFTWARE='1',LIBGL_ALWAYS_SOFTWARE='1',WLR_LIBINPUT_NO_DEVICES='1',GLOBAL_MENU_ENABLED='0',LUMEN_FILE_SERVICE='0',LUMEN_LAUNCH_SCOPES='0',LUMEN_NATIVE_WINDOW_CONTROLS='0',LUMEN_NATIVE_WINDOW_ACTIONS='0')
 docs=home/'Documents';docs.mkdir();env['LUMEN_SEARCH_ROOTS']=json.dumps([str(docs)])
 assets=out/'assets';shutil.copytree(Path(env['LUMEN_NOTIFICATION_HELPER']).parent.parent,assets);source=assets/'shell'
 def transform(path,old,new,reason):
  original=path.read_text();assert original.count(old)==1,(path,old)
  modified=original.replace(old,new,1);assert modified.replace(new,old,1)==original
  path.chmod(0o600);path.write_text(modified)
  transforms.append(dict(path=str(path),reason=reason,originalSha256=hashlib.sha256(original.encode()).hexdigest(),modifiedSha256=hashlib.sha256(modified.encode()).hexdigest(),originalMatchCount=1,roundtrip=True))
 # Unrelated providers must never probe real account or wireless control sockets.
 for filename,needle,result in (
  ('VpnState.qml','Qt.resolvedUrl("vpn_control.py").toString().replace("file://", "")',dict(mullvad=dict(installed=False,state='unavailable',account=False),tailscale=dict(installed=False,state='unavailable',online=False,peers=0,exitNodes=[]))),
  ('SystemState.qml','Qt.resolvedUrl("system-controls.py").toString().replace(/^file:\\/\\//, "")',dict(wifi=dict(available=False,networks=[]),bluetoothRadios={},power=dict(available=False)))):
  helper=out/(filename+'.provider.py');helper.write_text('import json\nprint('+repr(json.dumps(result))+')\n')
  transform(source/filename,needle,json.dumps(str(helper)),'Only unrelated provider command target replaced by synthetic unavailable endpoint')
 defaults=next(ast.literal_eval(node.value) for node in ast.parse((assets/'appearance/appearance.py').read_text()).body if isinstance(node,ast.Assign) and any(isinstance(target,ast.Name) and target.id=='DEFAULTS' for target in node.targets))
 preferences=dict(defaults,mode='light',reduceMotion=True,dockRecentApps=False);search_policy=dict(queryHistory=True,disabledProviders=['applications','files','actions'])
 policy=home/'settings.json';policy.write_text(json.dumps(dict(appearance=preferences,search=search_policy)));env['LUMEN_NIX_SETTINGS_ACTIVE']=str(policy)
 palettes=json.loads((assets/'appearance/palette.json').read_text());appearance=home/'state/lumen-desktop/appearance.json';appearance.parent.mkdir(parents=True);appearance.write_text(json.dumps(dict(font=palettes['font'],palette=palettes['light'],preferences=preferences,searchPreferences=search_policy)))
 # Only read-only introspection is appended. No history/query actions are mocked.
 transform(source/'MenuBar.qml','function launcherStatus() {return launcherPanel.status();}','function launcherStatus() {return launcherPanel.status();}\n    function historyAuditLauncher() {return launcherPanel;}','Read-only accessor to the actual nested popup launcher')
 original=(source/'shell.qml').read_text();head,tail=original.rsplit('}',1)
 addition='''
 function historyFind(item,predicate){if(!item)return null;if(predicate(item))return item;for(const child of item.children || []){const found=historyFind(child,predicate);if(found)return found;}return null;}
 function historyLauncher(){for(const bar of Object.values(root.bars)){const item=bar.historyAuditLauncher();if(item)return item;}return null;}
 IpcHandler {target:"historyAudit"
  function status():string {const launcher=root.historyLauncher();const recent=launcher?historyFind(launcher,node=>node.objectName==="search-history-state"):null;const settingsRecent=historyFind(settingsWindow.contentItem,node=>node.objectName==="search-history-state");return JSON.stringify({menus:Object.values(root.bars).map(bar=>bar.menu),visible:!!launcher && launcher.visible,query:launcher?launcher.queryText:"",index:launcher?launcher.historyIndex:-1,history:recent?recent.queries:[],busy:recent?recent.busy:false,enabled:recent?recent.enabled:false,provider:launcher?launcher.provider:"",settingsVisible:settingsWindow.visible,settingsPage:settingsWindow.page,settingsBusy:settingsRecent?settingsRecent.busy:false,settingsError:settingsRecent?settingsRecent.error:""});}
  function button(text:string):string {const item=historyFind(settingsWindow.contentItem,node=>node.text===text && node.visible);if(!item)return "{}";const point=item.mapToItem(settingsWindow.contentItem,item.width/2,item.height/2);return JSON.stringify({x:point.x,y:point.y,enabled:item.enabled});}
 }
'''
 (source/'shell.qml').chmod(0o600);(source/'shell.qml').write_text(head+addition+'}'+tail);transforms.append(dict(path=str(source/'shell.qml'),reason='Append read-only launcher/history/settings introspection',originalSha256=hashlib.sha256(original.encode()).hexdigest(),modifiedSha256=hashlib.sha256((head+addition+'}'+tail).encode()).hexdigest(),roundtrip=(head+addition+'}'+tail).replace(addition,'',1)==original))
 pidfile=out/'service.pid';locator=out/'systemctl-private';locator.write_text('#!/usr/bin/env bash\nprintf "%s\\n" "$*" >> '+str(out/'locator.log')+'\ncat '+str(pidfile)+'\n');locator.chmod(0o700)
 invocation=a.app_launcher.read_text();router_path=re.search(r'(/nix/store/[^\s]+/bin/lumen-shell-ipc)',invocation).group(1);router_text=Path(router_path).read_text();match=re.search(r'--systemctl (/nix/store/[^ ]+/bin/systemctl)',router_text);assert match and router_text.count('--systemctl ')==1
 router=out/'shell-ipc';router.write_text(router_text);transform(router,match.group(1),str(locator),'Exact installed router systemctl locator only');router.chmod(0o700)
 fallback=out/'fallback-observer';fallback.write_text('#!'+sys.executable+'\nimport json,sys\nfrom pathlib import Path\nwith Path('+repr(str(out/'fallback.jsonl'))+').open("a") as f:f.write(json.dumps(sys.argv[1:])+"\\n")\n');fallback.chmod(0o700)
 entry=out/'app-launcher';entry.write_text(invocation);transform(entry,router_path,str(router),'Installed launcher dispatch path points at isolated exact router');fallback_match=re.search(r'(/nix/store/[^\s]+/bin/rofi)',invocation);assert fallback_match;transform(entry,fallback_match.group(1),str(fallback),'Only fallback application replaced by argument observer');entry.chmod(0o700)
 history_hashes={name:hashlib.sha256((source/name).read_bytes()).hexdigest() for name in ('AppLauncher.qml','SearchHistoryState.qml','search_history.py')}
 for name,digest in history_hashes.items():assert digest==hashlib.sha256((Path(env['LUMEN_NOTIFICATION_HELPER']).parent.parent/'shell'/name).read_bytes()).hexdigest()
 gate=out/'hold-record';held=out/'held-record';release=out/'release-record'
 transport=out/'history-transport.py';transport.write_text('import json,subprocess,sys,time\nfrom pathlib import Path\nline=sys.stdin.readline()\ndata=json.loads(line)\ngate=Path('+repr(str(gate))+')\nif data.get("action")=="record" and gate.exists():\n Path('+repr(str(held))+').write_text(line)\n deadline=time.monotonic()+10\n while not Path('+repr(str(release))+').exists():\n  if time.monotonic()>deadline:raise RuntimeError("Private held request expired")\n  time.sleep(.02)\nchild=subprocess.run([sys.executable,'+repr(str(source/'search_history.py'))+'],input=line,text=True,stdout=subprocess.PIPE)\nprint(child.stdout,end="")\nsys.exit(child.returncode)\n')
 transform(source/'SearchHistoryState.qml','Qt.resolvedUrl("search_history.py").toString().replace(/^file:\\/\\//,"")',json.dumps(str(transport)),'Exact history transport gate delays only an original captured record; backend unmodified')
 (out/'manifest.json').write_text(json.dumps(dict(historySourceHashes=history_hashes,launcher=str(a.launcher),appLauncher=str(a.app_launcher),transforms=transforms,historyBackendUnmodified=True,noClipboardOffers=True),indent=2))
 history_path=home/'state/lumen-desktop/search/queries.json'
 def saved():return json.loads(history_path.read_text()) if history_path.exists() else []
 def launch(command,name,**kwargs):
  with (out/name).open('w') as stream:proc=sp.Popen(command,env=env,stdout=stream,stderr=sp.STDOUT,start_new_session=True,**kwargs)
  processes.append(proc);return proc
 def call(*parts):return sp.check_output(parts,env=env,text=True,timeout=8).strip()
 def wait(predicate,label):
  end=time.monotonic()+12
  while time.monotonic()<end:
   try:value=predicate()
   except (OSError,ValueError,sp.SubprocessError):value=None
   if value:return value
   time.sleep(.04)
  try:(out/'failure-state.json').write_text(audit('status'))
  except (NameError,OSError,ValueError,sp.SubprocessError):pass
  raise AssertionError('History fixture timeout: '+label)
 def audit(method,*args):return call('quickshell','ipc','--pid',str(shell.pid),'call','historyAudit',method,*args)
 def state():return json.loads(audit('status'))
 def keys(*args):call('wtype',*args)
 def toggle():keys('-M','logo','-P','Super_L','-k','space','-p','Super_L','-m','logo')
 def type_query(text):keys('-M','ctrl','a','-m','ctrl');keys(text);wait(lambda:state()['query']==text,'typed query')
 def recall(delta):keys('-M','alt','-P','Alt_L','-k','Up' if delta>0 else 'Down','-p','Alt_L','-m','alt')
 def start_shell(index):
  proc=launch(['quickshell','-p',str(source)],'shell-'+str(index)+'.log');return proc
 def stop_shell(proc):
  proc.terminate();proc.wait(timeout=5)
 def leaves(node):return ([node] if node.get('app_id') else [])+sum((leaves(child) for child in node.get('nodes',[])+node.get('floating_nodes',[])),[])
 def pointer_button(text):
  point=wait(lambda:(p if (p:=json.loads(audit('button',text))).get('enabled') else None),'enabled '+text)
  node=next(node for node in leaves(json.loads(call('swaymsg','-r','-t','get_tree'))) if node.get('name')=='System Settings')
  x=node['rect']['x']+node['window_rect']['x']+point['x'];y=node['rect']['y']+node['window_rect']['y']+point['y'];call('swaymsg','seat seat0 cursor set '+str(round(x))+' '+str(round(y)))
  pointer.stdin.write('m 0.5 0.5\n');pointer.stdin.flush();time.sleep(.12);pointer.stdin.write('p\n');pointer.stdin.flush();time.sleep(.12);pointer.stdin.write('r\n');pointer.stdin.flush()
 try:
  start_private_bus(env,out,processes)
  config=out/'sway.conf';config.write_text('output HEADLESS-1 resolution 1280x900\nxwayland disable\nseat seat0 fallback true\nfocus_follows_mouse no\ndefault_border none\nfor_window [all] floating enable\nbindsym Mod4+space exec '+str(entry)+' >'+str(out/'invocation.log')+' 2>&1\n')
  launch([a.sway,'--unsupported-gpu','-c',str(config)],'sway.log');wait(lambda:list(home.glob('sway-ipc*.sock')),'compositor')
  env['SWAYSOCK']=str(next(home.glob('sway-ipc*.sock')));env['WAYLAND_DISPLAY']=next(path.name for path in home.glob('wayland-*') if not path.name.endswith('.lock'))
  launch(['wtype','-s','600000'],'keyboard.log');pointer=launch([a.pointer_tool,'pointer','move','0','0'],'pointer.log',stdin=sp.PIPE,text=True)
  shell=start_shell(1);pidfile.write_text(str(shell.pid));wait(lambda:len(state()['menus'])==1,'shell')
  assert not state()['visible'] and saved()==[];toggle();wait(lambda:state()['visible'] and state()['enabled'] and not state()['busy'],'installed invocation');type_query('Synthetic first query');keys('-k','Escape');wait(lambda:not state()['visible'] and saved()==['Synthetic first query'],'first deliberate record')
  toggle();wait(lambda:state()['visible'] and not state()['busy'],'reopen');type_query('Synthetic second query');keys('-k','Escape');wait(lambda:saved()==['Synthetic second query','Synthetic first query'],'second deliberate record');checks.append('native installed invocation records ordinary dismissed queries with explicit policy')
  toggle();wait(lambda:state()['visible'] and not state()['busy'],'history loaded');type_query('Synthetic draft');recall(1);wait(lambda:state()['query']=='Synthetic second query','latest history');recall(1);wait(lambda:state()['query']=='Synthetic first query','older history');recall(1);assert state()['query']=='Synthetic first query';recall(-1);wait(lambda:state()['query']=='Synthetic second query','newer history');recall(-1);wait(lambda:state()['query']=='Synthetic draft','restored draft');recall(-1);assert state()['query']=='Synthetic draft';checks.append('actual Alt Up Down bounds retain and restore typed draft')
  keys('-k','Escape');wait(lambda:saved()[0]=='Synthetic draft','record restored draft');expected=saved();toggle();wait(lambda:state()['visible'] and not state()['busy'],'clipboard category');keys('-M','ctrl','4','-m','ctrl');wait(lambda:state()['provider']=='Clipboard','clipboard category');type_query('Synthetic excluded clipboard query');keys('-k','Escape');wait(lambda:not state()['visible'],'clipboard dismissed');assert saved()==expected;checks.append('clipboard-category query excluded without reading any clipboard offer')
  mode=history_path.stat().st_mode&0o777;assert mode==0o600 and history_path.parent.stat().st_mode&0o777==0o700;first_pid=shell.pid;stop_shell(shell);shell=start_shell(2);wait(lambda:len(state()['menus'])==1,'replacement shell')
  toggle();wait(lambda:(out/'fallback.jsonl').exists(),'stale PID fallback');assert not state()['visible'];assert json.loads((out/'fallback.jsonl').read_text().splitlines()[-1])==['-show','drun'];checks.append('stale captured service PID uses observed fallback without retargeting replacement shell')
  pidfile.write_text(str(shell.pid));toggle();wait(lambda:state()['visible'] and not state()['busy'],'replacement invocation');assert state()['history']==expected;recall(1);wait(lambda:state()['query']==expected[0],'persisted latest query');checks.append('new service PID invocation restores persisted history after actual shell restart')
  for _ in range(len(expected)):recall(-1)
  wait(lambda:state()['query']=='','restore empty new draft');toggle();wait(lambda:not state()['visible'],'toggle close');toggle();wait(lambda:state()['visible'],'toggle open');assert state()['menus'].count('Launcher')==1;checks.append('repeated installed native invocation toggles a single search surface')
  type_query('Synthetic pre-disable pending intent');gate.touch();call(str(router),'call','desktop','settings','Search');wait(lambda:held.exists(),'held before disable')
  def preference(enabled):
   data=json.loads(appearance.read_text());data['searchPreferences']['queryHistory']=enabled;appearance.write_text(json.dumps(data))
  preference(False);wait(lambda:not state()['enabled'],'observed disable while worker busy');preference(True);wait(lambda:state()['enabled'],'observed reenable before worker resumes');release.touch();wait(lambda:not state()['busy'] and saved()==[],'captured disable Clear completes');assert not history_path.exists();checks.append('observed rapid disable reenable preserves queued Clear intent while original worker is held')
  for path in (gate,held,release):path.unlink()
  toggle();wait(lambda:state()['visible'] and not state()['busy'],'new post-disable invocation')
  type_query('Synthetic pre-clear pending intent');gate.touch();call(str(router),'call','desktop','settings','Search');wait(lambda:held.exists(),'held original record');wait(lambda:state()['settingsVisible'] and state()['settingsPage']=='Search' and not state()['settingsBusy'],'Search settings');pointer_button('Clear Search History');wait(lambda:not state()['settingsBusy'] and not history_path.exists(),'actual clear');release.touch();wait(lambda:not state()['busy'],'retired record completed');assert not history_path.exists();assert json.loads(held.read_text()).get('generation');checks.append('actual settings Clear Search History rejects an in-flight earlier captured record')
  second_pid=shell.pid;stop_shell(shell);shell=start_shell(3);pidfile.write_text(str(shell.pid));wait(lambda:len(state()['menus'])==1,'post-clear replacement');toggle();wait(lambda:state()['visible'] and not state()['busy'],'post-clear invocation');assert state()['history']==[] and saved()==[];recall(1);assert state()['query']=='';checks.append('clear survives actual shell replacement and cannot recall retired queries')
  result=dict(passed=True,checks=checks,launcher=str(a.launcher),appLauncher=str(a.app_launcher),effects=a.effects,shellPids=[first_pid,second_pid,shell.pid],privateHistoryMode=mode,clipboardOffersRead=False,hostServicesUsed=False)
  (out/'result.json').write_text(json.dumps(result,indent=2));print(json.dumps(result))
 finally:
  for proc in reversed(processes):
   if proc.poll() is None:
    try:os.killpg(proc.pid,signal.SIGTERM)
    except ProcessLookupError:pass
  for proc in reversed(processes):
   try:proc.wait(timeout=5)
   except sp.TimeoutExpired:os.killpg(proc.pid,signal.SIGKILL);proc.wait(timeout=3)
  close_reaped_pipes(processes)
