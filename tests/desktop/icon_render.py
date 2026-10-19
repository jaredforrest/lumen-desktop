"""Render Dock/launcher in isolated Sway at fixed logical size and physical scales."""
import argparse, json, os, re, shlex, shutil, signal, subprocess as sp, sys, tempfile, time
from pathlib import Path

p=argparse.ArgumentParser()
p.add_argument('--assets',type=Path,required=True)
p.add_argument('--output',type=Path,required=True)
p.add_argument('--inside',action='store_true')
a=p.parse_args()
if not a.inside:
    output=a.output.resolve();output.mkdir(parents=True,exist_ok=False)
    private_runtime=Path(tempfile.mkdtemp(prefix='icon-render-'))
    outer_env=dict(os.environ,XDG_RUNTIME_DIR=str(private_runtime),XDG_CONFIG_HOME=str(output/'config'),XDG_STATE_HOME=str(output/'state'),XDG_CACHE_HOME=str(output/'cache'),XDG_DATA_HOME=str(output/'data'))
    for key in ('DISPLAY','WAYLAND_DISPLAY','SWAYSOCK','DBUS_SESSION_BUS_ADDRESS'):outer_env.pop(key,None)
    try:
        env.update(QT_ACCESSIBILITY='0',QT_LINUX_ACCESSIBILITY_ALWAYS_ON='0',NO_AT_BRIDGE='1',GTK_A11Y='none')
        env.pop('AT_SPI_BUS_ADDRESS',None)
        raise SystemExit(sp.call(['dbus-run-session','--',sys.executable,__file__,*sys.argv[1:],'--inside'],env=outer_env))
    finally:shutil.rmtree(private_runtime,ignore_errors=True)
run=a.output.resolve()
assets=a.assets.resolve()
runtime=Path(os.environ['XDG_RUNTIME_DIR'])
prefix=re.split(r'\n[ \t]*exec quickshell ',Path(shutil.which('lumen-desktop-shell')).read_text(),maxsplit=1)[0]
raw=sp.check_output(['bash','-c',prefix+'\nexec env -0'],stderr=sp.DEVNULL)
env=dict(os.environ)
env.update(dict(v.decode().split('=',1) for v in raw.split(b'\0') if b'=' in v))
env.update(XDG_RUNTIME_DIR=str(runtime),XDG_CONFIG_HOME=str(run/'config'),XDG_STATE_HOME=str(run/'state'),XDG_CACHE_HOME=str(run/'cache'),XDG_DATA_HOME=str(run/'data'),WLR_BACKENDS='headless',WLR_RENDERER='pixman',WLR_LIBINPUT_NO_DEVICES='1',QT_QPA_PLATFORM='wayland',GDK_BACKEND='wayland',LUMEN_FILE_SERVICE='0',LUMEN_LAUNCH_SCOPES='0',GLOBAL_MENU_ENABLED='0',LUMEN_SEARCH_ROOTS='[]',LUMEN_APPEARANCE_STATE=str(run/'appearance.json'))
env.update(QT_ACCESSIBILITY='0',QT_LINUX_ACCESSIBILITY_ALWAYS_ON='0',NO_AT_BRIDGE='1',GTK_A11Y='none')
env.pop('AT_SPI_BUS_ADDRESS',None)
for key in ('DISPLAY','WAYLAND_DISPLAY','SWAYSOCK'):env.pop(key,None)
procs=[]
def call(*args):return sp.check_output(args,env=env,text=True,timeout=12)
def launch(args,name):
    process=sp.Popen(args,env=env,stdout=(run/(name+'.log')).open('w'),stderr=sp.STDOUT,start_new_session=True);procs.append(process)
def wait(fn):
    end=time.monotonic()+25
    while time.monotonic()<end:
        try:
            if fn():return
        except (OSError,sp.SubprocessError,ValueError):pass
        time.sleep(.15)
    raise AssertionError('UI readiness timed out; inspect '+str(run))
def ipc(method,*args):return call('quickshell','-p',str(assets),'ipc','call','desktop',method,*args)
def appearance(dark,icon_style):
    palette=dict(background='#28282b' if dark else '#f2f2f4',surface='#343437' if dark else '#ffffff',field='#1e1e21' if dark else '#e5e5e8',text='#f5f5f7' if dark else '#202023',muted='#aeaeb5' if dark else '#6b6b70',border='#55555b' if dark else '#c8c8ce',hover='#45454b' if dark else '#dedee3',accent='#0a84ff' if dark else '#007aff',selectedText='#ffffff')
    (run/'appearance.json').write_text(json.dumps(dict(font='Interlude Variable',palette=palette,preferences=dict(iconAppearance=icon_style,dockMagnification=True,reduceMotion=False))))
try:
    appearance(False,'auto')
    conf=run/'sway.conf';conf.write_text('output HEADLESS-1 resolution 1440x1000\nxwayland disable\n')
    launch(['sway','-c',str(conf)],'sway');wait(lambda:list(runtime.glob('sway-ipc*.sock')))
    env['SWAYSOCK']=str(next(runtime.glob('sway-ipc*.sock')));env['WAYLAND_DISPLAY']=next(x.name for x in runtime.glob('wayland-*') if not x.name.endswith('.lock'))
    launch(['quickshell','-p',str(assets)],'shell');wait(lambda:json.loads(ipc('status')))
    for scale in (1,1.25,2):
        call('swaymsg',f'output HEADLESS-1 resolution {int(1440*scale)}x{int(1000*scale)} scale {scale}')
        time.sleep(.6)
        for dark,style in ((False,'auto'),(True,'auto'),(True,'default')):
            name=f'{scale}-'+('dark' if dark else 'light')+'-'+style
            appearance(dark,style);time.sleep(.6)
            ipc('menu','');time.sleep(.3)
            call('grim',str(run/(name+'-dock.png')))
            ipc('launcher');time.sleep(.8)
            call('wtype','-s','150','-M','ctrl','-k','1','-m','ctrl');time.sleep(.6)
            call('grim',str(run/(name+'-launcher.png')))
            ipc('menu','');time.sleep(.2)
    log=(run/'shell.log').read_text()
    assert not any(x in log for x in ('Failed to load configuration','ReferenceError:','TypeError:')),log
    (run/'result.json').write_text(json.dumps(dict(passed=True,assets=str(assets),scales=[1,1.25,2],appearances=['light-auto','dark-auto','dark-default']),indent=2))
    print(run/'result.json')
finally:
    for process in reversed(procs):
        if process.poll() is None:
            os.killpg(process.pid,signal.SIGTERM)
            try:process.wait(timeout=5)
            except sp.TimeoutExpired:os.killpg(process.pid,signal.SIGKILL)
