"""Full installed shell switching through native keys and service-PID routing."""
import argparse,hashlib,json,os,re,shutil,signal,subprocess as sp,sys,tempfile,time
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--launcher',required=True);p.add_argument('--action',required=True);p.add_argument('--sway',required=True);p.add_argument('--effects',action='store_true');p.add_argument('--output',type=Path,required=True);p.add_argument('--inside',action='store_true');a=p.parse_args()
if not a.inside:
    private=tempfile.mkdtemp(prefix='lumen-switch-');env=dict(os.environ,HOME=private,XDG_RUNTIME_DIR=private)
    for key in ('CONFIG','STATE','CACHE','DATA'):env['XDG_'+key+'_HOME']=str(Path(private)/key.lower())
    for key in ('SWAYSOCK','WAYLAND_DISPLAY','DISPLAY','DBUS_SESSION_BUS_ADDRESS','AT_SPI_BUS_ADDRESS'):env.pop(key,None)
    env['DBUS_SYSTEM_BUS_ADDRESS']='unix:path='+private+'/no-system-bus'
    conf=Path(private)/'bus.conf';conf.write_text('<busconfig><type>session</type><listen>unix:tmpdir='+private+'</listen><policy context="default"><allow own="*"/><allow send_destination="*"/><allow receive_sender="*"/></policy></busconfig>')
    try:raise SystemExit(sp.call(['dbus-run-session','--config-file='+str(conf),'--',sys.executable,__file__,*sys.argv[1:],'--inside'],env=env))
    finally:shutil.rmtree(private,ignore_errors=True)
out=a.output.resolve();out.mkdir(parents=True,exist_ok=False)
prefix=re.split(r'\n[ \t]*exec quickshell ',Path(a.launcher).read_text(),maxsplit=1)[0]
env=dict(v.decode().split('=',1) for v in sp.check_output(['bash','-c',prefix+'\nexec env -0']).split(b'\0') if b'=' in v)
env.update(QT_QPA_PLATFORM='wayland',QT_QPA_PLATFORMTHEME='generic',QT_QUICK_BACKEND='software',QT_ACCESSIBILITY='0',NO_AT_BRIDGE='1',GTK_A11Y='none',WLR_BACKENDS='headless',WLR_RENDERER='gles2' if a.effects else 'pixman',WLR_RENDERER_FORCE_SOFTWARE='1' if a.effects else '0',WLR_RENDERER_ALLOW_SOFTWARE='1',LIBGL_ALWAYS_SOFTWARE='1',WLR_LIBINPUT_NO_DEVICES='1',GLOBAL_MENU_ENABLED='0',LUMEN_FILE_SERVICE='0',LUMEN_LAUNCH_SCOPES='0')
for key in ('WAYLAND_DISPLAY','SWAYSOCK','DISPLAY'):env.pop(key,None)
processes=[]
def launch(command,name):
    with (out/name).open('w') as stream:proc=sp.Popen(command,env=env,stdout=stream,stderr=sp.STDOUT,start_new_session=True)
    processes.append(proc);return proc

def wait(callback,description):
    end=time.monotonic()+15
    while time.monotonic()<end:
        try:
            value=callback()
            if value:return value
        except (OSError,ValueError,sp.SubprocessError):pass
        time.sleep(.05)
    raise AssertionError('Timed out: '+description)

def call(*parts):return sp.check_output(parts,env=env,text=True,timeout=8).strip()
assets=out/'assets';shutil.copytree(Path(env['LUMEN_NOTIFICATION_HELPER']).parent.parent,assets);source=assets/'shell'
text=(source/'shell.qml').read_text();assert text.count('        window.activate();')==1
# Fault injection disables only the first activation request, exercising the
# actual installed fallback. It never supplies windows, grouping or targets.
text=text.replace('        window.activate();','        if(!switchAuditSuppressActivation)window.activate();',1);head,tail=text.rsplit('}',1)
(source/'shell.qml').chmod(0o600)
(source/'shell.qml').write_text(head+'''\n property bool switchAuditSuppressActivation:false
 IpcHandler {target:"switchAudit"
 function suppressActivation(value:bool):void{root.switchAuditSuppressActivation=value;}
 function focusCaptured(title:string):void{const target=root.windows.find(w=>w.title===title);if(target)root.focusWindow(target);}
 function status():string{return JSON.stringify({picker:JSON.parse(applicationSwitcher.status()),active:root.active?root.active.title:"",windows:root.windows.map(w=>({title:w.title,app:w.appId,minimized:w.minimized})),creation:root.windowCreationOrder.map(w=>w.title)});}
 }\n}'''+tail)
# Only the systemctl endpoint is substituted. The installed action dispatch and
# installed routing code remain unchanged; all selection/focus uses full shell.
pidfile=out/'service.pid';locator=out/'systemctl-private';locator.write_text('#!/usr/bin/env bash\ncat '+str(pidfile)+'\n');locator.chmod(0o700)
action_text=Path(a.action).read_text();ipc_path=re.search(r'(\/nix/store/[^\s]+/bin/lumen-shell-ipc)',action_text).group(1)
ipc_text=Path(ipc_path).read_text();assert ipc_text.count('--systemctl ')==1
original_ipc=ipc_text
match=re.search(r'--systemctl (/nix/store/[^ ]+/bin/systemctl)',ipc_text);assert match
ipc_text=ipc_text[:match.start(1)]+str(locator)+ipc_text[match.end(1):]
assert ipc_text.replace(str(locator),match.group(1),1)==original_ipc
(out/'routing-manifest.json').write_text(json.dumps(dict(action=a.action,originalRouter=ipc_path,systemctl=match.group(1),privateLocator=str(locator),originalSha256=hashlib.sha256(original_ipc.encode()).hexdigest(),modifiedSha256=hashlib.sha256(ipc_text.encode()).hexdigest(),actionDispatchPreserved=action_text.replace(ipc_path,str(out/'shell-ipc')).replace(str(out/'shell-ipc'),ipc_path)==action_text),indent=2))
ipc=out/'shell-ipc';ipc.write_text(ipc_text);ipc.chmod(0o700)
action=out/'desktop-action';action.write_text(action_text.replace(ipc_path,str(ipc)));action.chmod(0o700)
config=out/'sway.conf';config.write_text('output HEADLESS-1 resolution 1280x900\nseat seat0 fallback true\nxwayland disable\nfocus_follows_mouse no\nfor_window [all] floating enable\n'+''.join('bindsym '+key+' exec '+str(action)+' '+cmd+'\n' for key,cmd in [('Mod4+Tab','app-switcher'),('Mod4+Shift+Tab','app-switcher-reverse'),('--release Super_L','app-switcher-commit'),('--release Super_R','app-switcher-commit'),('Mod4+grave','window-next'),('Mod4+Shift+grave','window-previous')]))
results={}
try:
    launch([a.sway,'--unsupported-gpu','-c',str(config)],'sway.log');runtime=Path(env['XDG_RUNTIME_DIR'])
    env['SWAYSOCK']=str(wait(lambda:next(runtime.glob('sway-ipc*.sock'),None),'compositor socket; see sway.log'))
    env['WAYLAND_DISPLAY']=wait(lambda:next((v.name for v in runtime.glob('wayland-*') if not v.name.endswith('.lock')),None),'Wayland display')
    launch(['swaymsg','-m','-r','-t','subscribe','["window"]'],'native-events.jsonl')
    launch(['wtype','-s','60000'],'keyboard-seat.log')
    shell=launch(['quickshell','-p',str(source)],'shell.log');pidfile.write_text(str(shell.pid))
    def audit(*parts):return call('quickshell','ipc','--pid',str(shell.pid),'call','switchAudit',*parts)
    def status():return json.loads(audit('status'))
    def leaves(n):return ([n] if n.get('app_id') else [])+sum((leaves(c) for c in n.get('nodes',[])+n.get('floating_nodes',[])),[])
    def windows():return leaves(json.loads(call('swaymsg','-t','get_tree','-r')))
    def find(title):return next((w for w in windows() if w['name']==title),None)
    identities={}
    def focused():
        node=next((w for w in windows() if w.get('focused')),None)
        if not node:return None
        expected=identities.get(node['name'])
        assert expected is None or expected==[node['id'],node['pid']],('wrong native identity',node,expected)
        return node['name']
    def focus(title):call('swaymsg','[con_id='+str(find(title)['id'])+'] focus');wait(lambda:status()['active']==title,'shell focus '+title)
    def app(title,appid):
        proc=launch(['foot','--app-id='+appid,'--title='+title,'sh','-c','sleep 600'],title+'.log');wait(lambda:find(title),'window '+title);wait(lambda:any(w['title']==title for w in status()['windows']),'shell window '+title);node=find(title);assert node['pid']==proc.pid;identities[title]=[node['id'],node['pid']];return proc
    wait(lambda:status()['picker']['phase']=='closed','full shell')
    app('A1','lumen-fixture-a');app('A2','lumen-fixture-a');app('A3','lumen-fixture-a');app('B','lumen-fixture-b');app('C','lumen-fixture-c')
    focus('C');focus('B');focus('A1')
    def held(reverse=False,repeat=1):
        preview.commits=status()['picker']['commits']
        preview.eventOffset=(out/'native-events.jsonl').stat().st_size
        keys=['wtype','-M','logo','-P','Super_L']+(['-M','shift'] if reverse else [])
        for _ in range(repeat):keys+=['-k','Tab','-s','200']
        keys+=['-s','1600']+(['-m','shift'] if reverse else [])+['-p','Super_L','-m','logo']
        return launch(keys,'keys-'+str(len(processes))+'.log')
    def preview(proc):
        state=wait(lambda:(s if (s:=status())['picker']['visible'] else None),'switcher preview')
        assert focused() in (None,'A1'),('preview changed document focus',focused(),state)
        assert state['picker']['commits']==preview.commits,('preview committed',state)
        time.sleep(.15)
        for line in (out/'native-events.jsonl').read_text()[preview.eventOffset:].splitlines():
            event=json.loads(line)
            if event.get('change')=='focus':assert event['container']['id']==identities['A1'][0],('preview focused another native window',event)
        return state
    proc=held();state=preview(proc);assert state['picker']['key']=='lumen-fixture-b',state;proc.wait(timeout=5);wait(lambda:focused()=='B','release B');results['forward']=status()
    focus('C');focus('B');focus('A1');proc=held(True);state=preview(proc);assert state['picker']['key']=='lumen-fixture-c',state;proc.wait(timeout=5);wait(lambda:focused()=='C','reverse release C');results['reverse']=status()
    focus('C');focus('B');focus('A1');proc=held();preview(proc);call('wtype','-k','Escape');proc.wait(timeout=5);wait(lambda:status()['picker']['phase']=='cancelled','cancel');assert focused()=='A1';results['cancel']=status()
    # Cycling uses actual creation order, independent of app MRU.
    focus('A1')
    for key,target in [('grave','A2'),('grave','A3'),('grave','A1')]:
        call('wtype','-M','logo','-k',key,'-m','logo');wait(lambda:focused()==target,'same-app next '+target)
    call('wtype','-M','logo','-M','shift','-k','grave','-m','shift','-m','logo');wait(lambda:focused()=='A3','same-app previous');results['cycle']=status()
    # Delayed fallback must not undo a newer native focus decision.
    audit('suppressActivation','true');focus('A1');audit('focusCaptured','A2');focus('C');time.sleep(.3);assert focused()=='C';results['fallbackCancelled']=status()
    audit('focusCaptured','A2');wait(lambda:focused()=='A2','legitimate fallback exact A2');results['fallbackLegitimate']=status()
    audit('focusCaptured','A3');call('swaymsg','[con_id='+str(find('A3')['id'])+'] kill');wait(lambda:find('A3') is None,'fallback target removed');focus('C');time.sleep(.3);assert focused()=='C';results['fallbackDestroyed']=status()
    audit('focusCaptured','A2');call('swaymsg','[con_id='+str(find('A2')['id'])+'] kill');wait(lambda:find('A2') is None,'fallback replacement target removed');app('A2replacement','lumen-fixture-a');focus('C');time.sleep(.3);assert focused()=='C';results['fallbackReplacement']=status()
    audit('suppressActivation','false')
    # Minimized documents retain app membership and same-app cycling restores
    # the exact captured existing window instead of creating a new instance.
    call('swaymsg','[con_id='+str(find('A2replacement')['id'])+'] lumen_window minimize');wait(lambda:bool(find('A2replacement')['lumen_state']['minimised']),'A2 minimized');focus('A1');call('wtype','-M','logo','-k','grave','-m','logo');wait(lambda:focused()=='A2replacement','cycle restores minimized A2replacement');assert not find('A2replacement')['lumen_state']['minimised'];results['minimizedCycle']=status()
    # Removing an app during held preview cannot give its slot to another app.
    focus('C');focus('B');focus('A1');proc=held();preview(proc);call('swaymsg','[con_id='+str(find('B')['id'])+'] kill');wait(lambda:find('B') is None,'B destroyed');proc.wait(timeout=5);wait(lambda:status()['picker']['phase']=='cancelled','removed app cancels');wait(lambda:focused()=='A1','cancel restores original exact window');results['destroyed']=status()
    # Same-app replacement is permitted, but never a different application.
    app('B2','lumen-fixture-b');focus('C');focus('B2');focus('A1');proc=held();preview(proc);call('swaymsg','[con_id='+str(find('B2')['id'])+'] kill');app('B3','lumen-fixture-b');proc.wait(timeout=5);wait(lambda:focused()=='B3','same-app replacement');results['replacement']=status()
    call('swaymsg','[con_id='+str(find('B3')['id'])+'] lumen_window minimize');wait(lambda:bool(find('B3')['lumen_state']['minimised']),'B3 minimized');focus('C');focus('A1');proc=held();state=preview(proc);assert state['picker']['key']=='lumen-fixture-c',state
    # Advance to the real minimized app, retaining the held modifier.
    call('wtype','-k','Tab');wait(lambda:status()['picker']['key']=='lumen-fixture-b','minimized app selection');assert status()['picker']['commits']==preview.commits;proc.wait(timeout=5);wait(lambda:focused()=='B3','switcher restores minimized B3');assert not find('B3')['lumen_state']['minimised'];results['minimizedApp']=status()
    results['nativeIdentities']=identities
    (out/'result.json').write_text(json.dumps(results,indent=2)+'\n');print('PASS installed native switcher and same-app cycling')
finally:
    try:(out/'last-state.json').write_text(json.dumps(dict(status=status(),native=windows()),indent=2))
    except Exception:pass
    for proc in reversed(processes):
        try:os.killpg(proc.pid,signal.SIGTERM)
        except ProcessLookupError:pass
    for proc in processes:
        try:proc.wait(timeout=3)
        except sp.TimeoutExpired:os.killpg(proc.pid,signal.SIGKILL)
