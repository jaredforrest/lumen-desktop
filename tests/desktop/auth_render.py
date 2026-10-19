"""Isolated greeter IPC, session-lock authentication and shared visual QA.

Never authenticates against the real user's PAM database and never locks the
live desktop. A private headless compositor hosts all test surfaces.
"""
import argparse, json, os, re, shutil, signal, socket, struct, subprocess as sp, sys, tempfile, threading, time
from pathlib import Path

p=argparse.ArgumentParser();p.add_argument('--source',type=Path,required=True);p.add_argument('--assets',type=Path);p.add_argument('--font-config',type=Path);p.add_argument('--lock-wrapper',type=Path);p.add_argument('--output',type=Path,required=True);p.add_argument('--inside',action='store_true');a=p.parse_args()
if not a.inside:
    a.output.mkdir(parents=True,exist_ok=False)
    runtime=Path(tempfile.mkdtemp(prefix='auth-qa-'))
    env=dict(os.environ,XDG_RUNTIME_DIR=str(runtime),XDG_CONFIG_HOME=str(a.output/'config'),XDG_STATE_HOME=str(a.output/'state'),XDG_CACHE_HOME=str(a.output/'cache'),XDG_DATA_HOME=str(a.output/'data'))
    for k in ('DISPLAY','WAYLAND_DISPLAY','SWAYSOCK','DBUS_SESSION_BUS_ADDRESS'):env.pop(k,None)
    env.update(QT_ACCESSIBILITY='0',QT_LINUX_ACCESSIBILITY_ALWAYS_ON='0',NO_AT_BRIDGE='1',GTK_A11Y='none')
    env.pop('AT_SPI_BUS_ADDRESS',None)
    try:raise SystemExit(sp.call(['dbus-run-session','--',sys.executable,__file__,*sys.argv[1:],'--inside'],env=env))
    finally:shutil.rmtree(runtime,ignore_errors=True)
out=a.output.resolve();runtime=Path(os.environ['XDG_RUNTIME_DIR']);assets=out/'assets'
shutil.copytree(a.assets or a.source/'res/lumen-desktop/auth',assets/'auth')
for path in [assets/'auth', *(assets/'auth').rglob('*')]:
    path.chmod(0o755 if path.is_dir() else 0o644)
(assets/'shell').mkdir();shutil.copy(a.source/'res/lumen-desktop/shell/wallpaper.png',assets/'shell/wallpaper.png');shutil.copy(a.source/'res/lumen-desktop/shell/wallpaper.png',assets/'auth/wallpaper.png')
prefix=Path(shutil.which('lumen-desktop-shell')).read_text().split('\nexec quickshell ')[0]
raw=sp.check_output(['bash','-c',prefix+'\nexec env -0'],stderr=sp.DEVNULL)
env=dict(os.environ);env.update(dict(v.decode().split('=',1) for v in raw.split(b'\0') if b'=' in v))
env.update(WLR_BACKENDS='headless',WLR_RENDERER='pixman',WLR_LIBINPUT_NO_DEVICES='1',QT_QPA_PLATFORM='wayland',GDK_BACKEND='wayland',LUMEN_AUTH_SESSION='/bin/true',LUMEN_AUTH_USER='test-user',LUMEN_AUTH_NAME='Test User')
env.update(QT_ACCESSIBILITY='0',QT_LINUX_ACCESSIBILITY_ALWAYS_ON='0',NO_AT_BRIDGE='1',GTK_A11Y='none')
env.pop('AT_SPI_BUS_ADDRESS',None)
if a.font_config:env['FONTCONFIG_FILE']=str(a.font_config)
for k in ('DISPLAY','WAYLAND_DISPLAY','SWAYSOCK'):env.pop(k,None)
procs=[];checks=[];requests=[]
def launch(cmd,name):
    proc=sp.Popen(cmd,env=env,stdout=(out/(name+'.log')).open('w'),stderr=sp.STDOUT,start_new_session=True);procs.append(proc);return proc
def call(*cmd):return sp.check_output(cmd,env=env,text=True,timeout=12).strip()
def wait(fn):
    end=time.monotonic()+20
    while time.monotonic()<end:
        try:
            if fn():return
        except (OSError,ValueError,sp.SubprocessError):pass
        time.sleep(.15)
    raise AssertionError('Timed out: '+str(out))
def stop(proc):
    if proc.poll() is None:
        os.killpg(proc.pid,signal.SIGTERM)
        try:proc.wait(timeout=3)
        except sp.TimeoutExpired:os.killpg(proc.pid,signal.SIGKILL);proc.wait()
def exact(conn,n):
    data=b''
    while len(data)<n:
        b=conn.recv(n-len(data))
        if not b:raise EOFError
        data+=b
    return data
def serve():
    session_count=0
    listener=socket.socket(socket.AF_UNIX);listener.bind(str(runtime/'greetd.sock'));listener.listen()
    with listener.accept()[0] as conn:
        try:
            while True:
                request=json.loads(exact(conn,struct.unpack('<I',exact(conn,4))[0]));kind=request['type']
                # Keep only request types and launch command, never responses.
                requests.append({'type':kind,'cmd':request.get('cmd')})
                if kind=='create_session':
                    session_count+=1
                    reply=dict(type='auth_message',auth_message_type='visible' if session_count==1 else 'secret',auth_message='Fixture challenge:')
                elif kind=='post_auth_message_response':
                    reply=dict(type='success') if request.get('response')=='fixture-password' else dict(type='error',error_type='auth_error',description='Incorrect password')
                else:reply=dict(type='success')
                data=json.dumps(reply).encode();conn.sendall(struct.pack('<I',len(data))+data)
        except EOFError:pass
try:
    conf=out/'sway.conf';conf.write_text('output HEADLESS-1 resolution 1920x1080\nxwayland disable\ninput * xkb_layout gb\n')
    launch(['sway','--unsupported-gpu','-c',str(conf)],'sway');wait(lambda:list(runtime.glob('sway-ipc*.sock')))
    env['SWAYSOCK']=str(next(runtime.glob('sway-ipc*.sock')));env['WAYLAND_DISPLAY']=next(x.name for x in runtime.glob('wayland-*') if not x.name.endswith('.lock'))
    thread=threading.Thread(target=serve,daemon=True);thread.start();wait(lambda:(runtime/'greetd.sock').exists());env['GREETD_SOCK']=str(runtime/'greetd.sock')
    greeterfile=assets/'auth/greeter.qml'
    text=greeterfile.read_text();end=text.rfind('}')
    greeterfile.write_text(text[:end]+'IpcHandler {target:"fixture";function passwordMasked():bool{return !root.echoResponse && !root.responseReady;}}'+text[end:])
    greeter=launch(['quickshell','-p',str(greeterfile)],'greeter');time.sleep(2)
    assert greeter.poll() is None,(out/'greeter.log').read_text()
    call('grim',str(out/'login.png'))
    call('wtype','-s','300','wrong-password','-k','Return','-s','100');wait(lambda:len(requests)>=2);time.sleep(.5)
    assert not any(r['type']=='start_session' for r in requests)
    assert call('quickshell','-p',str(greeterfile),'ipc','call','fixture','passwordMasked')=='true'
    checks.append('visible challenge failure resets password masking')
    call('grim',str(out/'login-error.png'));checks.append('incorrect greetd response rejected')
    call('wtype','-s','300','fixture-password','-k','Return','-s','100');wait(lambda:any(r['type']=='start_session' for r in requests));wait(lambda:greeter.poll() is not None)
    assert [r['cmd'] for r in requests if r['type']=='start_session']==[['/bin/true']]
    checks.append('correct greetd response launches configured session')
    env.pop('GREETD_SOCK',None)
    preview=assets/'auth/preview.qml';preview.write_text('''import QtQuick
import Quickshell
import Quickshell.Wayland
import Quickshell.Io
PanelWindow {
 anchors {top:true;bottom:true;left:true;right:true}
 WlrLayershell.keyboardFocus: WlrKeyboardFocus.Exclusive
 AuthView { id:view; anchors.fill:parent }
 IpcHandler { target:"preview"
  function power(): void { view.openPowerMenu(); }
  function secretRetention(): bool {
   function walk(item){let result=[item];for(const child of item.children || [])result=result.concat(walk(child));return result;}
   const field=walk(view).find(n=>n.objectName==="password");
   field.text="synthetic";view.busy=true;let cleared=field.text.length===0;view.busy=false;
   field.text="synthetic";view.echoResponse=true;cleared=cleared && field.text.length===0;
   field.text="synthetic";view.echoResponse=false;cleared=cleared && field.text.length===0;
   field.text="synthetic";view.visible=false;cleared=cleared && field.text.length===0;view.visible=true;
   return cleared;
  }
  function passwordReveal(): bool {
   function walk(item){let result=[item];for(const child of item.children || [])result=result.concat(walk(child));return result;}
   const field=walk(view).find(n=>n.objectName==="password");
   const toggle=walk(view).find(n=>n.objectName==="passwordVisibility");
   field.text="synthetic";toggle.checked=true;
   const shown=field.echoMode===TextInput.Normal && field.text==="synthetic";
   toggle.checked=false;const hidden=field.echoMode===TextInput.Password && field.text==="synthetic";
   toggle.checked=true;view.submit();
   return shown && hidden && field.text.length===0 && !toggle.checked;
  }
  function bounds(): string {
   function walk(item){let result=[item];for(const child of item.children || [])result=result.concat(walk(child));return result;}
   const field=walk(view).find(n=>n.objectName==="password");const position=field.mapToItem(view,0,0);
   return JSON.stringify({width:view.width,height:view.height,x:position.x,y:position.y,fieldWidth:field.width,fieldHeight:field.height});
  }
 }
}''')
    proc=launch(['quickshell','-p',str(preview)],'preview');time.sleep(1);assert proc.poll() is None
    call('grim',str(out/'lock-preview.png'))
    # Same view means both modes must render the same pixels at the same time.
    from PIL import Image,ImageChops
    diff=ImageChops.difference(Image.open(out/'login.png').convert('RGB'),Image.open(out/'lock-preview.png').convert('RGB'))
    # Cursor blinking can differ; tolerate its tiny footprint, never layout differences.
    changed=sum(1 for px in diff.get_flattened_data() if max(px)>8)
    assert changed<1500,changed
    checks.append('shared login/lock layout pixel comparison')
    assert call('quickshell','-p',str(preview),'ipc','call','preview','secretRetention')=='true'
    checks.append('authentication view clears responses across busy masking and visibility transitions')
    assert call('quickshell','-p',str(preview),'ipc','call','preview','passwordReveal')=='true'
    checks.append('show-password control reveals and conceals input and resets after submission')
    call('quickshell','-p',str(preview),'ipc','call','preview','power');time.sleep(.3)
    call('grim',str(out/'power-menu.png'))
    call('wtype','-s','300','-k','Escape');time.sleep(.2)
    for width,height,scale in ((800,600,1),(1280,720,1),(2560,1440,2),(600,400,1)):
        call('swaymsg',f'output HEADLESS-1 resolution {width}x{height} scale {scale}');time.sleep(.5)
        bounds=json.loads(call('quickshell','-p',str(preview),'ipc','call','preview','bounds'))
        assert bounds['y']>=0 and bounds['y']+bounds['fieldHeight']<=bounds['height'],bounds
        assert bounds['x']>=0 and bounds['x']+bounds['fieldWidth']<=bounds['width'],bounds
        call('grim',str(out/f'preview-{width}x{height}-{scale}.png'))
    checks.append('password controls remain visible at four display sizes/scales')
    call('swaymsg','output HEADLESS-1 resolution 1920x1080 scale 1');stop(proc)
    call('swaymsg','create_output');checks.append('authentication lock tested with two outputs')
    pamdir=out/'pam';pamdir.mkdir();pamroot=next(Path('/nix/store').glob('*-linux-pam-*/lib/security/pam_deny.so')).parent
    lockfile=assets/'auth/lock.qml'
    text=lockfile.read_text().replace('config: "swaylock"','config: "fixture"\n        configDirectory: "'+str(pamdir)+'"')
    text=text.replace('onCompleted: result => {','onCompleted: result => {\n            console.log("fixture-auth-result", result);')
    lockfile.write_text(text)
    def secure():return call('quickshell','-p',str(lockfile),'ipc','call','auth','secure')=='true'
    (pamdir/'fixture').write_text('auth required '+str(pamroot/'pam_deny.so')+'\naccount required '+str(pamroot/'pam_permit.so')+'\n')
    proc=launch(['quickshell','-p',str(lockfile)],'lock');wait(secure)
    call('wtype','-s','300','fixture-password','-k','Return','-s','100');wait(lambda:'fixture-auth-result' in (out/'lock.log').read_text())
    assert secure();checks.append('failed PAM authentication keeps compositor locked')
    (pamdir/'fixture').write_text('auth required '+str(pamroot/'pam_permit.so')+'\naccount required '+str(pamroot/'pam_permit.so')+'\n')
    call('wtype','-s','300','fixture-password','-k','Return','-s','100');wait(lambda:proc.poll() is not None);checks.append('successful PAM authentication releases lock')
    if a.lock_wrapper:
        wrapper=out/'lock-wrapper'
        wrapper.write_text(re.sub(r'/nix/store/[a-z0-9]+-lumen-auth-assets/lock.qml',str(lockfile),a.lock_wrapper.read_text()))
        call('bash',str(wrapper));assert secure()
        start=time.monotonic();call('bash',str(wrapper));assert secure() and time.monotonic()-start<3
        checks.append('packaged wrapper waits for coverage and handles duplicate locking')
        call('wtype','-s','300','fixture-password','-k','Return','-s','100')
        wait(lambda:sp.run(['quickshell','-p',str(lockfile),'ipc','call','auth','secure'],env=env,stdout=sp.DEVNULL,stderr=sp.DEVNULL).returncode!=0)
    proc=launch(['quickshell','-p',str(lockfile)],'lock-crash');wait(secure);stop(proc)
    call('grim','-o','HEADLESS-1',str(out/'crash-closed.png'))
    image=Image.open(out/'crash-closed.png').convert('RGB');assert len(set(image.get_flattened_data()))==1
    call('grim','-o','HEADLESS-2',str(out/'crash-closed-secondary.png'))
    image=Image.open(out/'crash-closed-secondary.png').convert('RGB');assert len(set(image.get_flattened_data()))==1
    checks.append('locker process death leaves compositor covered')
    logs='\n'.join((out/(n+'.log')).read_text() for n in ('greeter','preview','lock','lock-crash'))
    assert not any(s in logs for s in ('Failed to load configuration','ReferenceError:','TypeError:','Binding loop','Cannot assign [undefined]')),logs
    (out/'result.json').write_text(json.dumps(dict(passed=True,checks=checks,requests=requests),indent=2));print(out/'result.json')
finally:
    for proc in reversed(procs):stop(proc)
