"""Calendar editor rendering with fake backend and fully isolated session."""
import argparse,json,os,re,shutil,signal,subprocess as sp,sys,tempfile,time
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--assets',type=Path,required=True);p.add_argument('--output',type=Path,required=True);p.add_argument('--inside',action='store_true');a=p.parse_args()
if not a.inside:
    out=a.output.resolve();out.mkdir(parents=True,exist_ok=False);runtime=Path(tempfile.mkdtemp(prefix='calendar-qa-'))
    env=dict(os.environ,XDG_RUNTIME_DIR=str(runtime),XDG_CONFIG_HOME=str(out/'config'),XDG_STATE_HOME=str(out/'state'),XDG_CACHE_HOME=str(out/'cache'),XDG_DATA_HOME=str(out/'data'))
    for key in ('DISPLAY','WAYLAND_DISPLAY','SWAYSOCK','DBUS_SESSION_BUS_ADDRESS'):env.pop(key,None)
    env.update(QT_ACCESSIBILITY='0',QT_LINUX_ACCESSIBILITY_ALWAYS_ON='0',NO_AT_BRIDGE='1',GTK_A11Y='none')
    env.pop('AT_SPI_BUS_ADDRESS',None)
    try:raise SystemExit(sp.call(['dbus-run-session','--',sys.executable,__file__,*sys.argv[1:],'--inside'],env=env))
    finally:shutil.rmtree(runtime,ignore_errors=True)
run=a.output.resolve();runtime=Path(os.environ['XDG_RUNTIME_DIR']);assets=run/'source';shutil.copytree(a.assets.resolve(),assets)
prefix=re.split(r'\n[ \t]*exec quickshell ',Path(shutil.which('lumen-desktop-shell')).read_text(),maxsplit=1)[0]
raw=sp.check_output(['bash','-c',prefix+'\nexec env -0'],stderr=sp.DEVNULL)
env=dict(os.environ);env.update(dict(v.decode().split('=',1) for v in raw.split(b'\0') if b'=' in v))
env.update(WLR_BACKENDS='headless',WLR_RENDERER='pixman',WLR_LIBINPUT_NO_DEVICES='1',QT_QPA_PLATFORM='wayland',GDK_BACKEND='wayland',LUMEN_APPEARANCE_STATE=str(run/'appearance.json'))
env.update(QT_ACCESSIBILITY='0',QT_LINUX_ACCESSIBILITY_ALWAYS_ON='0',NO_AT_BRIDGE='1',GTK_A11Y='none')
env.pop('AT_SPI_BUS_ADDRESS',None)
for key in ('DISPLAY','WAYLAND_DISPLAY','SWAYSOCK'):env.pop(key,None)
(run/'bin').mkdir();env['PATH']=str(run/'bin')+':'+env['PATH']
event=dict(accountGeneration='fixture-generation',id='fixture-event',title='Planning lunch',location='Local fixture',notes='First line\nSecond line',allDay=False,editStart='2026-10-08T12:00',editEnd='2026-10-08T13:00',calendarId='fixture',eventId='fixture-event',etag='etag',provider='Google',calendar='Personal',editable=True,recurring=False,date='2026-10-08',endDate='2026-10-08',startTime='12:00',endTime='13:00')
data=dict(events=[event],days=['2026-10-08'],total=1,configured=True,providers=[dict(id='google',name='Google',generation='fixture-generation',configured=True,lastSuccess='2026-10-08T12:00:00Z',calendars=[dict(id='fixture',name='Personal',accessRole='owner')])],lastSuccess='2026-10-08T12:00:00Z',syncError='',warning='')
(run/'fixture.json').write_text(json.dumps(data))
backend=run/'bin/lumen-calendar';backend.write_text('#!'+sys.executable+'\nimport json,sys,pathlib\nr=json.loads(sys.stdin.readline())\nprint(json.dumps({"error":"Fixture save failed: reconnect and try again."}) if r.get("action") in ("create","update") else pathlib.Path('+repr(str(run/'fixture.json'))+').read_text())\n');backend.chmod(0o755)
qml='''import QtQuick
import Quickshell
import Quickshell.Io
import "ASSETS" as Desktop
ShellRoot {
 QtObject {id:stub;property date currentDate:new Date(2026,9,8,12,30);function claimPopup(owner){}function focusOwnedWindow(title){}}
 Desktop.CalendarWindow {id:window;shell:stub;visible:true}
 IpcHandler {target:"calendarTest"
  function status():string{return JSON.stringify({loaded:window.calendarState.loaded,visible:window.visible,view:window.view,month:window.calendarState.iso(window.month),selected:window.calendarState.iso(window.selected),events:window.calendarState.monthEvents.length,editing:window.calendarState.editing});}
  function view(name:string):void{window.view=name;}
  function step(delta:int):void{window.step(delta);}
  function edit():void{window.create(window.selected);}
  function closeEditor():void{window.inspector=false;window.calendarState.editing=false;}
 }
}'''.replace('ASSETS','source')
ui=run/'fixture.qml';ui.write_text(qml);procs=[]
def call(*cmd):return sp.check_output(cmd,env=env,text=True,timeout=12)
def launch(cmd,name):
    process=sp.Popen(cmd,env=env,stdout=(run/(name+'.log')).open('w'),stderr=sp.STDOUT,start_new_session=True);procs.append(process)
def wait(fn):
    end=time.monotonic()+25
    while time.monotonic()<end:
        try:
            if fn():return
        except (ValueError,OSError,sp.SubprocessError):pass
        time.sleep(.15)
    raise AssertionError('Timed out: '+str(run))
def ipc(method,*args):return call('quickshell','-p',str(ui),'ipc','call','calendarTest',method,*args)
def state():return json.loads(ipc('status'))
def appearance(dark):
    colors=dict(background='#28282b' if dark else '#f2f2f4',surface='#343437' if dark else '#ffffff',field='#1e1e21' if dark else '#e5e5e8',text='#f5f5f7' if dark else '#202023',muted='#aeaeb5' if dark else '#6b6b70',border='#55555b' if dark else '#c8c8ce',hover='#45454b' if dark else '#dedee3',accent='#0a84ff' if dark else '#007aff',selectedText='#ffffff')
    (run/'appearance.json').write_text(json.dumps(dict(font='Interlude Variable',palette=colors,preferences={})))
try:
    appearance(False);conf=run/'sway.conf';conf.write_text('output HEADLESS-1 resolution 1280x900\nxwayland disable\nfor_window [title="Calendar"] floating enable, border none, move position center\n')
    launch(['sway','-c',str(conf)],'sway');wait(lambda:list(runtime.glob('sway-ipc*.sock')))
    env['SWAYSOCK']=str(next(runtime.glob('sway-ipc*.sock')));env['WAYLAND_DISPLAY']=next(x.name for x in runtime.glob('wayland-*') if not x.name.endswith('.lock'))
    launch(['quickshell','-p',str(ui)],'shell');wait(lambda:state()['loaded'])
    checks=[]
    for scale in (1,1.25,2):
        call('swaymsg',f'output HEADLESS-1 resolution {int(1280*scale)}x{int(900*scale)} scale {scale}')
        for dark in (False,True):
            appearance(dark)
            for view in ('Month','Week','Day','Year'):
                ipc('view',view);wait(lambda:state()['view']==view);time.sleep(.5)
                name=f'{scale}-'+('dark' if dark else 'light')+'-'+view
                call('grim',str(run/(name+'.png')));checks.append(dict(name=name,state=state()))
            ipc('view','Month');ipc('step','1');wait(lambda:state()['month']=='2026-11-01');ipc('step','-1');wait(lambda:state()['month']=='2026-10-01')
            ipc('edit');wait(lambda:state()['editing']);ipc('closeEditor')
    log=(run/'shell.log').read_text();assert not any(x in log for x in ('Failed to load configuration','ReferenceError:','TypeError:','Binding loop','Cannot assign [undefined]','Error: left-hand')),log
    (run/'result.json').write_text(json.dumps(dict(passed=True,checks=checks),indent=2));print(run/'result.json')

finally:
    for process in reversed(procs):
        if process.poll() is None:
            os.killpg(process.pid,signal.SIGTERM)
            try:process.wait(timeout=5)
            except sp.TimeoutExpired:os.killpg(process.pid,signal.SIGKILL)
