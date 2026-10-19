"""Exercise the common compositor frame without touching the login session."""
import os,json,subprocess as sp,time,tempfile,signal,sys,argparse,re,shutil
from pathlib import Path
parser=argparse.ArgumentParser()
parser.add_argument('--sway',required=True)
parser.add_argument('--launcher')
parser.add_argument('--core-only',action='store_true')
parser.add_argument('--output',type=Path,required=True)
parser.add_argument('--pointer-tool',default=os.environ.get('LUMEN_TEST_POINTER'))
parser.add_argument('--inside',action='store_true')
parser.add_argument('--effects',action='store_true')
a=parser.parse_args();out=a.output.resolve()
if not a.inside:
 out.mkdir(parents=True,exist_ok=False)
 env=dict(os.environ);env.pop('DBUS_SESSION_BUS_ADDRESS',None)
 env['XDG_RUNTIME_DIR']=tempfile.mkdtemp(prefix='sway-frame-')
 env.update(QT_ACCESSIBILITY='0',QT_LINUX_ACCESSIBILITY_ALWAYS_ON='0',NO_AT_BRIDGE='1',GTK_A11Y='none')
 env.pop('AT_SPI_BUS_ADDRESS',None)
 try:raise SystemExit(sp.call(['dbus-run-session','--',sys.executable,__file__,*sys.argv[1:],'--inside'],env=env))
 finally:shutil.rmtree(env['XDG_RUNTIME_DIR'],ignore_errors=True)
env=dict(os.environ,GSETTINGS_BACKEND='keyfile',GTK_THEME='WhiteSur-Light',WLR_BACKENDS='headless',WLR_RENDERER='gles2' if a.effects else 'pixman',WLR_RENDERER_ALLOW_SOFTWARE='1',WLR_LIBINPUT_NO_DEVICES='1',GDK_BACKEND='wayland',QT_QPA_PLATFORM='wayland',QT_ACCESSIBILITY='0',QT_LINUX_ACCESSIBILITY_ALWAYS_ON='0',NO_AT_BRIDGE='1',GTK_A11Y='none')
for k in ['SWAYSOCK','WAYLAND_DISPLAY','DISPLAY']:env.pop(k,None)
for k in ['CONFIG','STATE','CACHE','DATA']:env['XDG_'+k+'_HOME']=str(out/('test-'+k.lower()))
if a.launcher:
 prefix=re.split(r'\n[ \t]*exec quickshell ',Path(a.launcher).read_text(),maxsplit=1)[0]
 raw=sp.check_output(['bash','-c',prefix+'\nexec env -0'],env=env)
 env.update(dict(v.decode().split('=',1) for v in raw.split(b'\0') if b'=' in v))
 env.update(QT_ACCESSIBILITY='0',QT_LINUX_ACCESSIBILITY_ALWAYS_ON='0',NO_AT_BRIDGE='1',GTK_A11Y='none')
appearance=Path(shutil.which('lumen-appearance',path=env.get('PATH'))).read_text()
prefix=re.split(r'\n[ \t]*exec ',appearance,maxsplit=1)[0]
raw=sp.check_output(['bash','-c',prefix+'\nexec env -0'],env=env)
env.update(dict(v.decode().split('=',1) for v in raw.split(b'\0') if b'=' in v))
settings=Path(env['XDG_CONFIG_HOME'])/'gtk-3.0/settings.ini';settings.parent.mkdir(parents=True,exist_ok=True)
profile=out/'dconf-profile';profile.write_text('user-db:frame-test\n');env['DCONF_PROFILE']=str(profile)
settings.write_text('[Settings]\ngtk-decoration-layout=:\ngtk-theme-name=WhiteSur-Light\n')
config=out/'test.conf';config.write_text('title_align center\nfont Interlude Variable Medium 10\noutput * resolution 1280x900\ndefault_border normal 1\ndefault_floating_border normal 1\nclient.focused #b8b8bd #f5f5f7 #202024 #b8b8bd #b8b8bd\nfor_window [all] border normal 1\n')
if a.effects:
 with config.open('a') as stream:stream.write('corner_radius 12\nshadows enable\nshadow_blur_radius 24\nshadow_color #00000050\nanimation_duration_ms 0\n')
swaypath=a.sway
procs=[]
def launch(cmd,log):
 p=sp.Popen(cmd,env=env,stdout=(out/log).open('w'),stderr=sp.STDOUT,start_new_session=True);procs.append(p);return p
def wait(fn):
 until=time.monotonic()+20
 while time.monotonic()<until:
  try:
   if fn():return
  except (OSError,ValueError,sp.SubprocessError):pass
  time.sleep(.1)
 raise AssertionError('Timed out')
def call(*args):return sp.check_output(args,env=env,text=True,timeout=10)
def ipc(cmd):
 r=json.loads(call('swaymsg',cmd));assert all(x['success'] for x in r),r
 time.sleep(.2)
def assert_buttons(image,node,scale=1):
 r=node['deco_rect'];y=(r['y']+16)*scale
 coords=[((r['x']+20+i*20)*scale,y) for i in range(3)]
 fmt=' '.join('%[pixel:p{'+str(x)+','+str(y)+'}]' for x,y in coords)
 colors=call('magick',str(image),'-format',fmt,'info:')
 assert colors=='srgb(255,95,87) srgb(254,188,46) srgb(40,200,64)',colors
def tree():return json.loads(call('swaymsg','-t','get_tree','-r'))
def leaves(n=None):
 n=tree() if n is None else n
 return ([n] if n.get('app_id') or n.get('window') else [])+sum([leaves(x) for x in n.get('nodes',[])+n.get('floating_nodes',[])],[])
def click(x,y):
 ipc(f'seat seat0 cursor set {x} {y}');ipc('seat seat0 cursor press button1');ipc('seat seat0 cursor release button1')
try:
 launch([swaypath,'--unsupported-gpu','-c',str(config)],'nested-sway.log')
 wait(lambda:list(Path(env['XDG_RUNTIME_DIR']).glob('sway-ipc.*.sock')))
 env['SWAYSOCK']=str(next(Path(env['XDG_RUNTIME_DIR']).glob('sway-ipc.*.sock')))
 assert json.loads(call('swaymsg','-t','get_version','-r'))['native_window_controls']
 ipc('lumen_window margins disable')
 env['WAYLAND_DISPLAY']=next(x.name for x in Path(env['XDG_RUNTIME_DIR']).glob('wayland-*') if not x.name.endswith('.lock'))
 launch(['foot','--title=Frame Test','sh','-c','sleep 600'],'foot.log');wait(lambda:any(n['name']=='Frame Test' for n in leaves()))
 n=next(n for n in leaves() if n['name']=='Frame Test');assert n['deco_rect']['height']==32,n
 ipc('seat seat0 cursor set 110 16');call('grim',str(out/'native-frame.png'));assert_buttons(out/'native-frame.png',n)
 ipc('seat seat0 cursor set 20 16');ipc('seat seat0 cursor press button1');ipc('seat seat0 cursor set 110 16');ipc('seat seat0 cursor release button1');assert any(v['name']=='Frame Test' for v in leaves())
 ipc('output HEADLESS-1 resolution 2560x1800 scale 2');call('grim',str(out/'native-frame-2x.png'));assert_buttons(out/'native-frame-2x.png',next(v for v in leaves() if v['name']=='Frame Test'),2)
 ipc('output HEADLESS-1 resolution 1280x900 scale 1')
 # Green enters real fullscreen; Fill remains a separate reversible operation.
 original = next(n for n in leaves() if n['name']=='Frame Test')
 # Native layouts are explicit and idempotent; every transition retains the original placement.
 layout_boxes={'left':(0,0,640,900),'right':(640,0,640,900),'top':(0,0,1280,450),'bottom':(0,450,1280,450),
  'top-left':(0,0,640,450),'top-right':(640,0,640,450),'bottom-left':(0,450,640,450),'bottom-right':(640,450,640,450)}
 for action,box in layout_boxes.items():
  ipc('[title="^Frame Test$"] lumen_window '+action)
  placed=next(n for n in leaves() if n['name']=='Frame Test')
  outer=dict(placed['rect']);outer['y']-=placed['deco_rect']['height'];outer['height']+=placed['deco_rect']['height']
  assert tuple(outer[k] for k in ('x','y','width','height'))==box,(action,outer)
  ipc('[title="^Frame Test$"] lumen_window '+action)
  assert next(n for n in leaves() if n['name']=='Frame Test')['rect']==placed['rect']
 ipc('[title="^Frame Test$"] lumen_window restore')
 assert next(n for n in leaves() if n['name']=='Frame Test')['rect']==original['rect']
 ipc('[title="^Frame Test$"] lumen_window restore')
 assert next(n for n in leaves() if n['name']=='Frame Test')['rect']==original['rect']
 ticklog=out/'layout-events.jsonl'
 subscriber=launch(['swaymsg','-r','-m','-t','subscribe','["tick"]'],'layout-events.jsonl')
 time.sleep(.2)
 assert a.pointer_tool, 'A real virtual pointer is required for the green hover regression'
 pointer=sp.Popen([a.pointer_tool,'pointer','move','0','0'],env=env,stdin=sp.PIPE,text=True,start_new_session=True);procs.append(pointer)
 time.sleep(.2);ipc('seat seat0 cursor set 200 100')
 pointer.stdin.write('m -140 -84\n');pointer.stdin.flush()
 time.sleep(.3)
 ticks=[json.loads(line) for line in ticklog.read_text().splitlines() if line.startswith('{')]
 payloads=[]
 for event in ticks:
  try:payloads.append(json.loads(event.get('payload','')))
  except (ValueError,TypeError):pass
 assert any(p.get('lumen_layout')==1 and p.get('id')==original['id'] for p in payloads),ticks
 ipc('seat seat0 cursor set 200 100')
 print('PASS: native half/quarter layouts are idempotent, original placement restores, green hover emits scoped event.')
 ipc('lumen_window margins enable')
 ipc('[title="^Frame Test$"] lumen_window left')
 margined=next(n for n in leaves() if n['name']=='Frame Test')
 assert margined['rect']['x']==8 and margined['rect']['width']==628,margined['rect']
 ipc('[title="^Frame Test$"] lumen_window restore')
 ipc('lumen_window margins disable')
 ipc('seat seat0 cursor set 110 16')
 pointer.stdin.write('m 0 0\np\nr\np\nr\n');pointer.stdin.flush();time.sleep(.3)
 assert next(n for n in leaves() if n['name']=='Frame Test')['lumen_state']['filled']
 ipc('[title="^Frame Test$"] lumen_window restore')
 print('PASS: optional layout margins and native titlebar double-click Fill work.')
 # Exercise the real menu with the pointer stationary on its trigger, then crossing the gap.
 menu_fixture=out/'layout-menu-fixture.qml'
 menu_assets=Path(__file__).resolve().parents[2]/'res/lumen-desktop/shell'
 menu_fixture.write_text('import QtQuick\nimport Quickshell\nimport Quickshell.Io\nimport "file:'+str(menu_assets)+'" as D\nShellRoot {\n QtObject {id:stub;property string popupOwner:"";signal dismissPopups(string owner);function claimPopup(owner){popupOwner=owner;dismissPopups(owner);}}\n D.WindowLayoutMenu {id:menu;shell:stub}\n IpcHandler {target:"fixture";function status():string{return JSON.stringify({visible:menu.visible,target:menu.target,supported:menu.supported});}}\n}\n')
 menu_process=launch(['quickshell','-p',str(menu_fixture)],'layout-menu.log')
 def menu_status():return json.loads(call('quickshell','-p',str(menu_fixture),'ipc','call','fixture','status'))
 wait(lambda:menu_status().get('supported'))
 ipc('seat seat0 cursor set 200 100')
 pointer.stdin.write('m -140 -84\n');pointer.stdin.flush()
 wait(lambda:menu_status()['visible'])
 time.sleep(.7)
 assert menu_status()['visible'], 'Hover menu closed while pointer remained on green'
 pointer.stdin.write('m 30 54\n');pointer.stdin.flush();time.sleep(.5)
 assert menu_status()['visible'], 'Hover menu closed while pointer crossed into its rows'
 call('grim',str(out/'layout-menu.png'))
 pointer.stdin.write('p\nr\n');pointer.stdin.flush()
 wait(lambda:next(n for n in leaves() if n['name']=='Frame Test')['lumen_state']['filled'])
 assert not menu_status()['visible']
 ipc('[title="^Frame Test$"] lumen_window restore')
 os.killpg(menu_process.pid,signal.SIGTERM);menu_process.wait(timeout=5)
 print('PASS: real green menu stays open at trigger and across pointer gap; captured-window Fill commits.')
 click(60,16);full=next(n for n in leaves() if n['name']=='Frame Test')
 assert full['fullscreen_mode']!=0,full
 ipc('[title="^Frame Test$"] lumen_window fullscreen')
 ipc('[title="^Frame Test$"] lumen_window fill');filled=next(n for n in leaves() if n['name']=='Frame Test')
 assert filled['fullscreen_mode']==0 and filled['deco_rect']['height']==32, filled
 assert filled['type']=='floating_con', filled
 ipc('[title="^Frame Test$"] lumen_window fill')
 restored=next(n for n in leaves() if n['name']=='Frame Test')
 assert restored['type']==original['type'] and restored['rect']==original['rect'], (original,restored)
 click(40,16);n=next(n for n in leaves() if n['name']=='Frame Test');assert n['scratchpad_state']!='none',n
 ipc('[title="^Frame Test$"] scratchpad show')
 restored=next(n for n in leaves() if n['name']=='Frame Test')
 assert restored['type']==original['type'] and restored['rect']==original['rect'], (original,restored)
 n=next(n for n in leaves() if n['name']=='Frame Test');(out/'restored.json').write_text(json.dumps(n,indent=2));call('grim',str(out/'restored.png'));r=n['deco_rect'];click(r['x']+20,r['y']+16)
 wait(lambda:not any(n['name']=='Frame Test' for n in leaves()))
 print('PASS: native title bar is 32px; close, minimise/restore placement fullscreen and reversible fill work.')
 launch(['foot','--title=Floating Frame Test','sh','-c','sleep 600'],'floating-foot.log');wait(lambda:any(n['name']=='Floating Frame Test' for n in leaves()))
 ipc('[title="^Floating Frame Test$"] floating enable, resize set width 500 px height 320 px, move position 150 px 170 px')
 before=next(n for n in leaves() if n['name']=='Floating Frame Test')
 ipc('[title="^Floating Frame Test$"] lumen_window minimize')
 ipc('workspace 2');ipc('[title="^Floating Frame Test$"] scratchpad show')
 after=next(n for n in leaves() if n['name']=='Floating Frame Test')
 assert after['type']==before['type'] and after['rect']==before['rect'],(before,after)
 ipc('[title="^Floating Frame Test$"] lumen_window fill')
 assert next(n for n in leaves() if n['name']=='Floating Frame Test')['deco_rect']['height']==32
 ipc('[title="^Floating Frame Test$"] lumen_window fill')
 assert next(n for n in leaves() if n['name']=='Floating Frame Test')['rect']==before['rect']
 ipc('[title="^Floating Frame Test$"] lumen_window minimize')
 ipc('output HEADLESS-1 resolution 640x480 scale 1')
 ipc('[title="^Floating Frame Test$"] scratchpad show')
 limited=next(n for n in leaves() if n['name']=='Floating Frame Test')['rect']
 assert limited['x']>=0 and limited['y']>=0 and limited['x']+limited['width']<=640 and limited['y']+limited['height']<=480,limited
 ipc('output HEADLESS-1 resolution 1280x900 scale 1')
 print('PASS: restore constrains floating placement to a resized surviving work area.')
 ipc('[title="^Floating Frame Test$"] lumen_window close');wait(lambda:not leaves())
 print('PASS: floating minimize/fill restore geometry after a workspace change.')
 launch(['foot','--title=Tiled A','sh','-c','sleep 600'],'tile-a.log');wait(lambda:any(n['name']=='Tiled A' for n in leaves()))
 launch(['foot','--title=Tiled B','sh','-c','sleep 600'],'tile-b.log');wait(lambda:any(n['name']=='Tiled B' for n in leaves()))
 before={n['name']:n['rect'] for n in leaves()}
 ipc('[title="^Tiled A$"] lumen_window minimize');ipc('[title="^Tiled A$"] scratchpad show')
 after={n['name']:n['rect'] for n in leaves()}
 assert before==after,(before,after)
 # Hiding is independent of minimisation, including when both apply.
 ipc('[title="^Tiled A$"] lumen_window minimize')
 ipc('[title="^Tiled [AB]$"] lumen_window hide')
 hidden={n['name']:n for n in leaves()}
 assert hidden['Tiled A']['lumen_state']['minimised'] and hidden['Tiled A']['lumen_state']['hidden'],hidden
 assert not hidden['Tiled B']['lumen_state']['minimised'] and hidden['Tiled B']['lumen_state']['hidden'],hidden
 ipc('[title="^Tiled [AB]$"] lumen_window unhide')
 shown={n['name']:n for n in leaves()}
 assert shown['Tiled A']['scratchpad_state']!='none' and shown['Tiled A']['lumen_state']['minimised'],shown
 assert shown['Tiled B']['scratchpad_state']=='none' and not shown['Tiled B']['lumen_state']['hidden'],shown
 ipc('[title="^Tiled A$"] scratchpad show')
 assert {n['name']:n['rect'] for n in leaves()}==before
 ipc('[title="^Tiled [AB]$"] lumen_window close');wait(lambda:not leaves())
 print('PASS: hide/unhide keeps independent minimisation and original placements.')
 print('PASS: minimizing one tiled sibling restores both placements.')
 if a.core_only:raise SystemExit(0)


 call('gsettings','set','org.gnome.desktop.wm.preferences','button-layout',':')
 launch(['evince','--new-window'],'gtk.log');wait(lambda:any(n.get('app_id')=='org.gnome.Evince' for n in leaves()))
 call('grim',str(out/'gtk-frame.png'));(out/'gtk-tree.json').write_text(json.dumps(leaves(),indent=2))
 print('PASS: GTK header-bar application loads under the common frame.')
 ipc('[app_id="org.gnome.Evince"] kill');wait(lambda:not leaves())
 fixture=out/'qt.qml';fixture.write_text('import QtQuick\nimport Quickshell\nimport Quickshell.Wayland\nShellRoot { PanelWindow { anchors {top:true;bottom:true;left:true;right:true} exclusionMode:ExclusionMode.Ignore; WlrLayershell.layer:WlrLayer.Background;color:"#808080" } FloatingWindow { title:"Qt Frame Test"; visible:true; implicitWidth:600; implicitHeight:400; Rectangle {anchors.fill:parent;color:"#f2f2f4"} } }')
 with config.open('a') as stream:stream.write('for_window [title="^Qt Frame Test$"] floating enable, resize set width 600 px height 400 px, move position center\n')
 ipc('reload')
 launch(['quickshell','-p',str(fixture)],'qt.log');wait(lambda:any(n['name']=='Qt Frame Test' for n in leaves()))
 time.sleep(.4);call('grim',str(out/'qt-frame.png'))
 if a.effects:
  n=next(n for n in leaves() if n['name']=='Qt Frame Test');r=n['rect'];x=r['x'];y=r['y']-n['deco_rect']['height']
  corner=float(call('magick',str(out/'qt-frame.png'),'-format',f'%[fx:p{{{x},{y}}}.r]','info:'))
  shadow=float(call('magick',str(out/'qt-frame.png'),'-format',f'%[fx:p{{{x-6},{r["y"]+100}}}.r]','info:'))
  assert abs(corner-128/255)<.04,(corner,n)
  assert .1<shadow<.49,(shadow,n)
  print('PASS: rounded corner reveals the background and floating shadow darkens it.')
 ipc('[title="^Qt Frame Test$"] kill');wait(lambda:not leaves())
 utility=out/'utility.qml';utility.write_text('import QtQuick\nimport Quickshell\nShellRoot { FloatingWindow {title:"Window Overview";visible:true;implicitWidth:400;implicitHeight:300;Rectangle{anchors.fill:parent;color:"#f2f2f4"}}}')
 launch(['quickshell','-p',str(utility)],'utility.log');wait(lambda:any(n['name']=='Window Overview' for n in leaves()))
 wait(lambda:any(n['name']=='Window Overview' and n['rect']['width']==1280 and n['rect']['height']==868 for n in leaves()))
 before=next(n for n in leaves() if n['name']=='Window Overview')
 ipc('[title="^Window Overview$"] lumen_window minimize');ipc('[title="^Window Overview$"] lumen_window fill')
 after=next(n for n in leaves() if n['name']=='Window Overview')
 assert after['scratchpad_state']=='none' and after['rect']==before['rect'] and after['fullscreen_mode']==0,after
 ipc('[title="^Window Overview$"] lumen_window close');wait(lambda:not leaves())
 print('PASS: utility windows reject unsupported minimize/fill actions.')

 home=out/'test-home';home.mkdir(exist_ok=True);env['HOME']=str(home)
 obsidian=Path(env['XDG_CONFIG_HOME'])/'obsidian/obsidian.json';obsidian.parent.mkdir(parents=True,exist_ok=True);obsidian.write_text('{"frame":"native","updateDisabled":true}')
 launch(['obsidian','--ozone-platform=wayland','--disable-gpu'],'electron.log');wait(lambda:bool(leaves()))
 call('grim',str(out/'electron-frame.png'));(out/'electron-tree.json').write_text(json.dumps(leaves(),indent=2))
 assert_buttons(out/'electron-frame.png',leaves()[0]);print('PASS: Qt and Electron native-frame applications load; all three traffic lights render.')
finally:
 for p in reversed(procs):
  if p.poll() is None:
   os.killpg(p.pid,signal.SIGTERM)
   try:p.wait(timeout=3)
   except sp.TimeoutExpired:os.killpg(p.pid,signal.SIGKILL)
