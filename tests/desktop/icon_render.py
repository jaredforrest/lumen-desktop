"""Paint public application icons and symbols through the standalone private entry."""
import argparse,hashlib,json,os,re,shutil,signal,subprocess as sp,sys,tempfile,time
from pathlib import Path
import xml.etree.ElementTree as ET
from PIL import Image,ImageStat
from fixture_providers import isolate_providers
from fixture_resources import close_reaped_pipes

p=argparse.ArgumentParser();p.add_argument('--assets',type=Path,required=True);p.add_argument('--output',type=Path,required=True);p.add_argument('--launcher',type=Path);p.add_argument('--sway',default='sway');p.add_argument('--inside',action='store_true');a=p.parse_args()
run=a.output.resolve()
if not a.inside:
 run.mkdir(parents=True,exist_ok=False)
 with tempfile.TemporaryDirectory(prefix='lumen-icons-') as private:
  root=Path(private);home=root/'home';home.mkdir()
  outer_env=dict(os.environ,HOME=str(home),XDG_RUNTIME_DIR=private,QT_ACCESSIBILITY='0',QT_LINUX_ACCESSIBILITY_ALWAYS_ON='0',NO_AT_BRIDGE='1',GTK_A11Y='none',DBUS_SYSTEM_BUS_ADDRESS='unix:path='+private+'/no-system-bus',PULSE_SERVER='unix:'+private+'/no-pulse',PIPEWIRE_REMOTE='no-pipewire')
  for key in ('CONFIG','STATE','CACHE','DATA'):
   directory=root/key.lower();directory.mkdir();outer_env['XDG_'+key+'_HOME']=str(directory)
  for key in ('DISPLAY','WAYLAND_DISPLAY','SWAYSOCK','DBUS_SESSION_BUS_ADDRESS','AT_SPI_BUS_ADDRESS'):outer_env.pop(key,None)
  bus=run/'bus.conf';bus.write_text('<busconfig><type>session</type><listen>unix:tmpdir='+private+'</listen><auth>EXTERNAL</auth><policy context="default"><allow own="*"/><allow send_destination="*"/><allow receive_sender="*"/></policy></busconfig>')
  (run/'bootstrap.json').write_text(json.dumps(dict(privateHome=True,privateXdg=True,ambientSessionRemoved=True,activationDirectories=False,systemBus='inert',scope='Disposable nested session only'),indent=2))
  raise SystemExit(sp.call(['dbus-run-session','--config-file='+str(bus),'--',sys.executable,__file__,*sys.argv[1:],'--inside'],env=outer_env))
assets=a.assets.resolve();runtime=Path(os.environ['XDG_RUNTIME_DIR']);launcher=a.launcher or Path(shutil.which('lumen-desktop-shell'))
prefix=re.split(r'\n[ \t]*exec quickshell ',launcher.read_text(),maxsplit=1)[0]
env=dict(value.decode().split('=',1) for value in sp.check_output(['bash','-c',prefix+'\nexec env -0']).split(b'\0') if b'=' in value)
env.update(WLR_BACKENDS='headless',WLR_RENDERER='pixman',WLR_LIBINPUT_NO_DEVICES='1',QT_QPA_PLATFORM='wayland',QT_QPA_PLATFORMTHEME='generic',QT_QUICK_BACKEND='software',LUMEN_FILE_SERVICE='0',LUMEN_LAUNCH_SCOPES='0',GLOBAL_MENU_ENABLED='0',LUMEN_SEARCH_ROOTS='[]',LUMEN_APPEARANCE_STATE=str(run/'appearance.json'),XDG_DATA_DIRS=env['XDG_DATA_HOME'],QT_ACCESSIBILITY='0',QT_LINUX_ACCESSIBILITY_ALWAYS_ON='0',NO_AT_BRIDGE='1',GTK_A11Y='none')
for key in ('DISPLAY','WAYLAND_DISPLAY','SWAYSOCK','AT_SPI_BUS_ADDRESS'):env.pop(key,None)
fixture=run/'assets/shell';shutil.copytree(assets.parent,fixture.parent);providerRecords=isolate_providers(fixture,run)
fakebin=run/'bin';fakebin.mkdir();calendar=fakebin/'lumen-calendar';calendar.write_text('#!'+sys.executable+'\nimport json,sys\nr=json.loads(sys.stdin.readline());assert r["action"] in ("list","load")\nprint(json.dumps(dict(configured=False,events=[],monthEvents=[],days=[],providers=[],total=0)))\n');calendar.chmod(0o700);env['PATH']=str(fakebin)+':'+env['PATH']
applications=Path(env['XDG_DATA_HOME'])/'applications';applications.mkdir()
for name,icon in [('Calendar','calendar'),('Notes','notes'),('Reminders','reminders')]:
 (applications/('org.example.'+name+'.desktop')).write_text('[Desktop Entry]\nType=Application\nName='+name+'\nExec=false\nIcon='+str(fixture/'reference-icons'/(icon+'.svg'))+'\n')
catalogue=sorted(list((fixture/'icons').glob('*.svg'))+list((fixture/'reference-icons').glob('*.svg')))
assert catalogue and all(path.is_file() for path in catalogue)
symbols=sorted(set(re.findall(r'name==="([^"]+)"',(fixture/'SystemGlyph.qml').read_text())));assert symbols
qml=run/'grid.qml';qml.write_text('''import QtQuick
import Quickshell
import Quickshell.Io
import "assets/shell" as D
ShellRoot {id:root
 property bool symbolMode:false
 property var symbols:'''+json.dumps(symbols)+'''
 property var icons:'''+json.dumps([str(path) for path in catalogue])+'''
 FloatingWindow {id:window;title:"Icon catalogue";visible:false;implicitWidth:1260;implicitHeight:840;color:D.Theme.colors.background
  Grid {id:appGrid;visible:!root.symbolMode;anchors.fill:parent;columns:8;spacing:0
   Repeater {model:root.icons
    Item {id:cell;required property string modelData;required property int index;width:157;height:168
     Text {x:6;y:4;width:145;elide:Text.ElideMiddle;text:cell.modelData.split('/').slice(-2).join('/');font.pixelSize:11;color:D.Theme.colors.text}
     Row {x:8;y:40;spacing:12
      Repeater {model:[16,24,64]
       D.AppIcon {required property int modelData;property color fixtureBackground:cell.modelData.indexOf("/icons/")>=0?"#777777":D.Theme.colors.background;width:modelData;height:modelData;iconSource:cell.modelData;calendarDate:new Date(2026,9,20);Rectangle {anchors.fill:parent;color:parent.fixtureBackground;z:-1}}
      }
     }
    }
   }
  }
  Grid {id:symbolGrid;visible:root.symbolMode;anchors.fill:parent;columns:8
   Repeater {model:root.symbols
    Item {id:symbolCell;required property string modelData;width:157;height:140
     Text {x:6;y:4;text:symbolCell.modelData;font.pixelSize:11;color:D.Theme.colors.text}
     Row {x:8;y:35;spacing:12
      Repeater {model:[16,24,64]
       D.SystemGlyph {required property int modelData;property bool fixtureSymbol:true;width:modelData;height:modelData;name:symbolCell.modelData;ink:D.Theme.colors.text}
      }
     }
    }
   }
  }
 }
 function visit(item,fn){if(fn(item))return item;for(const child of item.children || []){const result=visit(child,fn);if(result)return result;}return null;}
 IpcHandler {target:"icons"
  function showGrid(value:bool):void{root.symbolMode=false;window.visible=value;}
  function showSymbols():void{root.symbolMode=true;window.visible=true;}
  function state():string {const values=[];function collect(item){if(item.iconSource!==undefined || item.fixtureSymbol===true){const at=item.mapToItem(window.contentItem,0,0);values.push({kind:item.fixtureSymbol===true?"symbol":"asset",ink:item.fixtureSymbol===true?String(item.ink):"",background:item.fixtureSymbol===true?String(D.Theme.colors.background):String(item.fixtureBackground),source:item.fixtureSymbol===true?item.name:String(item.iconSource),desired:item.fixtureSymbol===true?item.name:String(item.desiredSource),ready:item.fixtureSymbol===true?item.available:item.status===Image.Ready,failed:item.fixtureSymbol===true?false:item.failed,x:at.x,y:at.y,width:item.width,height:item.height,rasterWidth:item.fixtureSymbol===true?0:item.sourceSize.width,rasterHeight:item.fixtureSymbol===true?0:item.sourceSize.height});}for(const child of item.children || [])collect(child);}collect(root.symbolMode?symbolGrid:appGrid);return JSON.stringify({visible:window.visible,dark:D.Theme.dark,style:D.Theme.iconAppearance,background:String(D.Theme.colors.background),items:values});}
 }
}''')
processes=[]
def launch(argv,name):
 with (run/name).open('w') as log:process=sp.Popen(argv,env=env,stdout=log,stderr=sp.STDOUT,start_new_session=True)
 processes.append(process);return process
def call(*argv):return sp.check_output(argv,env=env,text=True,timeout=12).strip()
def wait(fn):
 deadline=time.monotonic()+25
 while time.monotonic()<deadline:
  try:
   value=fn()
   if value:return value
  except (OSError,sp.SubprocessError,ValueError):pass
  time.sleep(.1)
 raise AssertionError('UI readiness timed out; inspect '+str(run))
def ipc(method,*args):return call('quickshell','-p',str(fixture),'ipc','call','desktop',method,*args)
def grid(method,*args):return call('quickshell','-p',str(qml),'ipc','call','icons',method,*args)
def state():return json.loads(grid('state'))
def nodes(node):
 yield node
 for child in node.get('nodes',[])+node.get('floating_nodes',[]):yield from nodes(child)
def appearance(dark,style):
 palettes=json.loads((fixture.parent/'appearance/palette.json').read_text());target=run/'appearance.json';temporary=run/'appearance.tmp';temporary.write_text(json.dumps(dict(font=palettes['font'],palette=palettes['dark' if dark else 'light'],preferences=dict(mode='dark' if dark else 'light',iconAppearance=style,dockMagnification=True,reduceMotion=False))));temporary.replace(target)
def painted(path,box=None):
 call('grim',str(path));image=Image.open(path).convert('RGB');crop=image.crop(box) if box else image
 stats=ImageStat.Stat(crop);return max(stats.stddev)>2 and max(stats.mean)>10
records=[]
def mismatch(reason,**detail):
 (run/'last-paint-mismatch.json').write_text(json.dumps(dict(reason=reason,**detail),indent=2));return None
try:
 appearance(False,'auto');config=run/'sway.conf';config.write_text('output HEADLESS-1 resolution 1440x1000\noutput * bg #29445b solid_color\nxwayland disable\ndefault_border none\nfor_window [title="^Icon catalogue$"] floating enable\n')
 launch([a.sway,'--unsupported-gpu','-c',str(config)],'sway.log');wait(lambda:list(runtime.glob('sway-ipc*.sock')));env['SWAYSOCK']=str(next(runtime.glob('sway-ipc*.sock')));env['WAYLAND_DISPLAY']=next(path.name for path in runtime.glob('wayland-*') if not path.name.endswith('.lock'))
 launch(['quickshell','-p',str(fixture)],'shell.log');wait(lambda:json.loads(ipc('status')));launch(['quickshell','-p',str(qml)],'grid.log');wait(lambda:len(state()['items'])==len(catalogue)*3)
 for scale in (1,1.25,1.5,2):
  response=json.loads(call('swaymsg',f'output HEADLESS-1 resolution {int(1440*scale)}x{int(1000*scale)} scale {scale}'));assert all(row['success'] for row in response)
  for dark in (False,True):
   for style in ('auto','default','dark'):
    name=f'{scale}-'+('dark' if dark else 'light')+'-'+style;appearance(dark,style);wait(lambda:state()['dark']==dark and state()['style']==style and all(item['ready'] and not item['failed'] for item in state()['items']))
    grid('showGrid','false');ipc('menu','');wait(lambda:painted(run/(name+'-dock.png'),(0,int(820*scale),int(1440*scale),int(1000*scale))))
    ipc('launcher');time.sleep(.2);call('wtype','-M','ctrl','-k','1','-m','ctrl');wait(lambda:painted(run/(name+'-launcher.png'),(int(100*scale),int(80*scale),int(1340*scale),int(880*scale))));ipc('menu','')
    grid('showGrid','true');wait(lambda:state()['visible']);window=wait(lambda:next((node for node in nodes(json.loads(call('swaymsg','-r','-t','get_tree'))) if node.get('name')=='Icon catalogue' and node.get('visible')),None));items=state()['items'];box=window['rect'];client=window['window_rect'];assert (client['width'],client['height'])==(1260,840),window
    def gridPainted():
     call('grim',str(run/captureName));image=Image.open(run/captureName).convert('RGB');results=[]
     background=tuple(int(state()['background'][i:i+2],16) for i in (1,3,5));corner=image.getpixel((round((box['x']+client['x']+client['width']-2)*scale),round((box['y']+client['y']+client['height']-2)*scale)))
     if max(abs(corner[i]-background[i]) for i in range(3))>3:return mismatch('profile background not presented',capture=captureName,observed=corner,expected=background)
     for item in items:
      assert item['x']>=0 and item['y']>=0 and item['x']+item['width']<=client['width'] and item['y']+item['height']<=client['height'],item
      if item['kind']=='asset' and re.search(r'/reference-icons/(calendar|notes|reminders)\.svg$',item['source']):
       expectedDark=style=='dark' or style=='auto' and dark
       assert ('/reference-icons/dark/' in item['desired'])==expectedDark,item
      background=tuple(int(item['background'][i:i+2],16) for i in (1,3,5))
      x=(box['x']+client['x']+item['x'])*scale;y=(box['y']+client['y']+item['y'])*scale;w=item['width']*scale;h=item['height']*scale;crop=image.crop((round(x),round(y),round(x+w),round(y+h)));count=sum(max(abs(pixel[i]-background[i]) for i in range(3))>15 for pixel in crop.get_flattened_data())
      if item['kind']=='symbol':
       ink=tuple(int(item['ink'][i:i+2],16) for i in (1,3,5));direction=[ink[i]-background[i] for i in range(3)];norm=sum(value*value for value in direction);count=0
       for pixel in crop.get_flattened_data():
        delta=[pixel[i]-background[i] for i in range(3)];coverage=sum(delta[i]*direction[i] for i in range(3))/norm
        if .12<coverage<=1.1 and max(abs(delta[i]-coverage*direction[i]) for i in range(3))<20:count+=1
      elif item['width']==64 and re.search(r'/reference-icons/(?:dark/)?(calendar|notes|reminders)\.svg$',item['desired']):
       from urllib.parse import unquote,urlparse
       source=ET.parse(Path(unquote(urlparse(item['desired']).path)));rectangle=source.getroot().find('{http://www.w3.org/2000/svg}rect');viewBox=[float(value) for value in source.getroot().attrib['viewBox'].split()]
       expected=tuple(int(rectangle.attrib['fill'][i:i+2],16) for i in (1,3,5));sx=(float(rectangle.attrib['x'])+float(rectangle.attrib['width'])*.1)/viewBox[2];sy=(float(rectangle.attrib['y'])+float(rectangle.attrib['height'])*.5)/viewBox[3];sample=image.getpixel((round(x+w*sx),round(y+h*sy)))
       if max(abs(sample[i]-expected[i]) for i in range(3))>8:return mismatch('source variant not presented',capture=captureName,item=item,observed=sample,expected=expected)
      if count<4:return mismatch('sample not painted with current role',capture=captureName,item=item,paintedPixels=count)
      if item['kind']=='asset':assert item['rasterWidth']>=w and item['rasterHeight']>=h,item
      results.append(dict(source=item['source'],desired=item['desired'],size=item['width'],physicalBox=[x,y,w,h],paintedPixels=count))
     return results
    captureName=name+'-grid.png';pixels=wait(gridPainted)
    grid('showSymbols');wait(lambda:len(state()['items'])==len(symbols)*3 and all(item['ready'] for item in state()['items']));items=state()['items'];captureName=name+'-symbols.png';glyphs=wait(gridPainted)
    records.append(dict(scale=scale,dark=dark,style=style,icons=pixels,symbols=glyphs,window=box,captures=[name+'-grid.png',captureName]));grid('showGrid','false')
 for name in ('shell.log','grid.log'):
  log=(run/name).read_text();assert not any(text in log for text in ('Failed to load configuration','ReferenceError:','TypeError:','Binding loop','Cannot assign','Error decoding','Cannot open:')),log
 (run/'result.json').write_text(json.dumps(dict(passed=True,assets=str(assets),launcher=str(launcher),compositor=a.sway,scales=[1,1.25,1.5,2],appearances=['light-auto','light-default','light-dark','dark-auto','dark-default','dark-dark'],cases=records,catalogueCount=len(catalogue),symbolCount=len(symbols),symbolNames=symbols,symbolSourceSha256=hashlib.sha256((fixture/'SystemGlyph.qml').read_bytes()).hexdigest(),symbolSamples=sum(len(row['symbols']) for row in records),iconSamples=sum(len(row['icons']) for row in records),providerIsolation=providerRecords,fixtureSha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),catalogueSha256={str(path.relative_to(fixture)):hashlib.sha256(path.read_bytes()).hexdigest() for path in catalogue}),indent=2));print(run/'result.json')
finally:
 for process in reversed(processes):
  if process.poll() is None:
   os.killpg(process.pid,signal.SIGTERM)
   try:process.wait(timeout=5)
   except sp.TimeoutExpired:os.killpg(process.pid,signal.SIGKILL);process.wait(timeout=5)
 close_reaped_pipes(processes)
