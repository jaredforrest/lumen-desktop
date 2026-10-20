"""Actual private Quick Look markup gestures, file chooser and immutable copy export."""
import argparse,hashlib,json,os,re,shutil,subprocess as sp,tempfile,time
import dbus
from pathlib import Path
from PIL import Image
from application_fixture.keyboard import PhysicalKeyboard
from private_native_bus import start_private_bus
from fixture_resources import close_reaped_pipes
p=argparse.ArgumentParser();p.add_argument('--launcher',type=Path,required=True);p.add_argument('--sway',required=True);p.add_argument('--pointer-tool',required=True);p.add_argument('--keyboard',required=True);p.add_argument('--output',type=Path,required=True);p.add_argument('--at-spi-prefix',type=Path,default=os.environ.get('LUMEN_TEST_AT_SPI'));p.add_argument('--effects',action='store_true');a=p.parse_args();out=a.output.resolve();out.mkdir(parents=True,exist_ok=False);processes=[];checks=[];keyboard=None;pointer=None
with tempfile.TemporaryDirectory(prefix='lumen-markup-native-') as temporary:
 home=Path(temporary);env=dict(os.environ,HOME=temporary,XDG_RUNTIME_DIR=temporary)
 for key in ('CONFIG','CACHE','DATA','STATE'):env['XDG_'+key+'_HOME']=str(home/key.lower())
 for key in ('WAYLAND_DISPLAY','SWAYSOCK','DISPLAY','DBUS_SESSION_BUS_ADDRESS','AT_SPI_BUS_ADDRESS'):env.pop(key,None)
 prefix=re.split(r'\n[ \t]*exec quickshell ',a.launcher.read_text(),maxsplit=1)[0];env.update(dict(v.decode().split('=',1) for v in sp.check_output(['bash','-c',prefix+'\nexec env -0'],env=env).split(b'\0') if b'=' in v))
 env.update(QT_QPA_PLATFORM='wayland',QT_QPA_PLATFORMTHEME='generic',QT_QUICK_BACKEND='software',QT_ACCESSIBILITY='1',QT_LINUX_ACCESSIBILITY_ALWAYS_ON='1',NO_AT_BRIDGE='0',GTK_A11Y='atspi',WLR_BACKENDS='headless',WLR_RENDERER='gles2' if a.effects else 'pixman',WLR_RENDERER_ALLOW_SOFTWARE='1',WLR_RENDERER_FORCE_SOFTWARE='1' if a.effects else '0',LIBGL_ALWAYS_SOFTWARE='1',WLR_LIBINPUT_NO_DEVICES='1',LUMEN_LAUNCH_SCOPES='0',GLOBAL_MENU_ENABLED='0',LUMEN_FILE_SERVICE='0')
 source=home/'original.png';Image.new('RGB',(160,100),'white').save(source);original=source.read_bytes();target=home/'annotated-copy.png';collision=home/'collision.png';collision.write_bytes(b'untouched')
 assets=out/'assets';shutil.copytree(Path(env['LUMEN_NOTIFICATION_HELPER']).parent.parent,assets)
 quick=assets/'shell/QuickLookWindow.qml';text=quick.read_text();marker='    id:root\n';replacement=marker+'    readonly property var fixtureMenu:previewActions\n    readonly property var fixtureMarkup:markup\n';assert text.count(marker)==1;modified=text.replace(marker,replacement,1);assert modified.replace(replacement,marker,1)==text;quick.chmod(0o600);quick.write_text(modified)
 (out/'instrumentation.json').write_text(json.dumps({'original':hashlib.sha256(text.encode()).hexdigest(),'modified':hashlib.sha256(modified.encode()).hexdigest(),'roundtrip':True,'scope':'Read-only actual menu and markup component exposure; no provider/actions/controls changed'},indent=2))
 markup=assets/'shell/ImageMarkupDialog.qml';markup_original=markup.read_text();marker=re.search(r'function saveTo\([^)]*\)\{',markup_original).group(0);observed=marker+'console.log("MARKUP_SELECTED_DESTINATION "+value);';assert markup_original.count(marker)==1;markup_modified=markup_original.replace(marker,observed,1);image_marker='    id:root\n';image_observed=image_marker+'    readonly property var fixtureImage:image\n';assert markup_modified.count(image_marker)==1;markup_modified=markup_modified.replace(image_marker,image_observed,1);assert markup_modified.replace(observed,marker,1).replace(image_observed,image_marker,1)==markup_original;markup.chmod(0o600);markup.write_text(markup_modified);manifest=json.loads((out/'instrumentation.json').read_text());manifest['chooserObservation']={'original':hashlib.sha256(markup_original.encode()).hexdigest(),'modified':hashlib.sha256(markup_modified.encode()).hexdigest(),'roundtrip':True,'scope':'Read-only selected destination URI log and actual Image geometry exposure; dispatch and checks unchanged'};(out/'instrumentation.json').write_text(json.dumps(manifest,indent=2))

 palette=json.loads((assets/'appearance/palette.json').read_text());appearance=home/'appearance.json';appearance.write_text(json.dumps(dict(font=palette['font'],palette=palette['dark'],preferences={})));env['LUMEN_APPEARANCE_STATE']=str(appearance)
 qml=out/'shell.qml';qml.write_text('''import QtQuick
import QtQuick.Window
import Quickshell
import Quickshell.Io
import "assets/shell" as D
ShellRoot {
 id:root
 D.QuickLookWindow {id:quick}
 function find(item,name,seen){if(!item || seen.includes(item))return null;seen.push(item);if((item.text===name || item.objectName===name) && item.visible && item.enabled)return item;for(const list of [item.children,item.data,item.contentData])if(list && typeof list.length==="number")for(let i=0;i<list.length;i++){const found=find(list[i],name,seen);if(found)return found;}if(item.contentItem && item.contentItem!==item)return find(item.contentItem,name,seen);return null;}
 IpcHandler {target:"fixture"
  function startPreview(path:string):void{quick.show(path,null);}
  function imageGeometry():string{const image=quick.fixtureMarkup.fixtureImage,window=quick.contentItem.Window.window;const point=image.mapToItem(window.contentItem,0,0);return JSON.stringify({x:point.x,y:point.y,width:image.width,height:image.height,paintedWidth:image.paintedWidth,paintedHeight:image.paintedHeight,ready:image.status===Image.Ready,autoTransform:image.autoTransform});}
  function status():string{const mark=quick.fixtureMarkup;return JSON.stringify({result:quick.result,message:quick.actionMessage,markup:mark.visible,strokes:mark.strokes,current:mark.current,saving:mark.saving,drawing:mark.drawing,draft:mark.draft,error:mark.message,uri:mark.sourceUri});}
  function button(name:string):string{let item=root.find(quick.contentItem,name,[]);if(!item && quick.fixtureMarkup.visible)item=root.find(quick.fixtureMarkup.contentItem,name,[]);if(!item && quick.fixtureMenu.visible)for(let i=0;i<quick.fixtureMenu.count;i++){const candidate=quick.fixtureMenu.itemAt(i);if(candidate && candidate.text===name && candidate.enabled)item=candidate;}if(!item)return "{}";const window=quick.contentItem.Window.window;const point=item.mapToItem(window.contentItem,item.width/2,item.height/2);return JSON.stringify({x:point.x,y:point.y,width:item.width,height:item.height,title:window.title,windowWidth:window.width,windowHeight:window.height,active:window.active,hovered:item.hovered===undefined?item.containsMouse:item.hovered,down:item.down===undefined?item.pressed:item.down});}
 }
}''')
 def launch(command,name,**kwargs):
  with (out/name).open('w') as log:child=sp.Popen(command,env=env,stdout=log,stderr=sp.STDOUT,start_new_session=True,**kwargs)
  processes.append(child);return child
 def call(*command):return sp.check_output(command,env=env,text=True,timeout=8).strip()
 def wait(fn,label):
  end=time.monotonic()+12
  while time.monotonic()<end:
   try:value=fn()
   except (ValueError,OSError,sp.SubprocessError):value=None
   if value:return value
   time.sleep(.03)
  raise AssertionError('Timed out: '+label)
 def nodes(node):return [node]+sum((nodes(child) for child in node.get('nodes',[])+node.get('floating_nodes',[])),[])
 def tree():return nodes(json.loads(call('swaymsg','-r','-t','get_tree')))
 def ipc(*args):return call('quickshell','ipc','--pid',str(shell.pid),'call','fixture',*args)
 def state():return json.loads(ipc('status'))
 def geometry(name):return json.loads(ipc('button',name))
 def move(name,dx=0,dy=0):
  g=wait(lambda:geometry(name),'control '+name);node=wait(lambda:next((n for n in tree() if n.get('name')==g['title'] and n.get('pid')==shell.pid and n.get('app_id')=='org.quickshell'),None),'exact control owner');call('swaymsg','-r',f'[con_id={node["id"]}] focus')
  wait(lambda:geometry(name).get('active') and next(n for n in tree() if n['id']==node['id']).get('focused'),'focused owner')
  call('swaymsg','seat','seat0','cursor','set',str(round(node['rect']['x']+node.get('window_rect',{}).get('x',0)+g['x']+dx)),str(round(node['rect']['y']+node.get('window_rect',{}).get('y',0)+g['y']+dy)));pointer.stdin.write('m 0.5 0.5\n');pointer.stdin.flush();
  if name!='markup-drawing-surface':wait(lambda:geometry(name).get('hovered'),'actual hovered control')
  return g
 def click(name):
  move(name);pointer.stdin.write('p\n');pointer.stdin.flush();wait(lambda:geometry(name).get('down'),'actual pressed control');pointer.stdin.write('r\n');pointer.stdin.flush();time.sleep(.12)
 def begin():
  click('•••');wait(lambda:geometry('Annotate a Copy…'),'annotation command');click('Annotate a Copy…');wait(lambda:state()['markup'],'actual markup popup')
 def draw_line():
  move('markup-drawing-surface',-80,0);pointer.stdin.write('p\n');pointer.stdin.flush();wait(lambda:state()['drawing'],'actual drawing press');pointer.stdin.write('m 160 0\n');pointer.stdin.flush();wait(lambda:len(state()['draft'])>2,'actual freehand movement');pointer.stdin.write('r\n');pointer.stdin.flush();wait(lambda:len(state()['strokes'])==1,'actual freehand gesture')
 def accessible_rows():
  pending=[('org.a11y.atspi.Registry','/org/a11y/atspi/accessible/root')];seen=set();rows=[];deadline=time.monotonic()+5
  while pending and len(rows)<1500:
   if time.monotonic()>deadline:raise ValueError('Private chooser accessibility observation timed out')
   ref=tuple(map(str,pending.pop(0)))
   if ref in seen or ref[1]=='/org/a11y/atspi/null':continue
   seen.add(ref)
   try:
    obj=a11y.get_object(*ref,introspect=False);item=dbus.Interface(obj,'org.a11y.atspi.Accessible');props=dbus.Interface(obj,'org.freedesktop.DBus.Properties');words=item.GetState(timeout=.5);name=str(props.Get('org.a11y.atspi.Accessible','Name',timeout=.5));role=str(item.GetRoleName(timeout=.5));showing=bool(int(words[25//32])&(1<<(25%32)));pid=int(a11y.get_object('org.freedesktop.DBus','/org/freedesktop/DBus').GetConnectionUnixProcessID(ref[0],dbus_interface='org.freedesktop.DBus',timeout=.5));rows.append(dict(ref=ref,name=name,role=role,showing=showing,pid=pid));pending.extend(item.GetChildren(timeout=.5))
   except dbus.DBusException:continue
  (out/'chooser-accessibility.json').write_text(json.dumps(rows,indent=2));return rows
 def save_path(path,hold=False):
  click('Save Copy…');dialog=wait(lambda:next((n for n in tree() if n.get('name')=='Save Annotated Copy' and n.get('pid')==shell.pid),None),'actual Save Copy chooser');call('swaymsg','-r',f'[con_id={dialog["id"]}] focus')
  def bounds(row):return list(map(int,dbus.Interface(a11y.get_object(*row['ref'],introspect=False),'org.a11y.atspi.Component').GetExtents(dbus.UInt32(1),timeout=.5)))
  def filename_entry():
   rows=accessible_rows();labels=[row for row in rows if row['pid']==shell.pid and row['showing'] and row['role']=='label' and row['name']=='File name']
   if len(labels)!=1:return None
   label=labels[0];label_box=bounds(label);relations=dbus.Interface(a11y.get_object(*label['ref'],introspect=False),'org.a11y.atspi.Accessible').GetRelationSet(timeout=.5);label_for=[tuple(map(str,ref)) for kind,refs in relations if int(kind)==1 for ref in refs]
   candidates=[]
   for row in rows:
    if row['pid']!=shell.pid or not row['showing'] or row['role'] not in ('text','entry'):continue
    box=bounds(row)
    associated=tuple(row['ref']) in label_for if label_for else box[0]>=label_box[0]+label_box[2] and box[1]<=label_box[1]+label_box[3]/2<=box[1]+box[3]
    if associated and box[2]>0 and box[3]>0:candidates.append((row,box))
   if len(candidates)!=1:return None
   row,box=candidates[0];(out/'filename-field-proof.json').write_text(json.dumps(dict(label=label,field=row,labelBounds=label_box,fieldBounds=box,relation=bool(label_for),windowId=dialog['id'],windowPid=dialog['pid']),indent=2));return row,box
  field_row,field=wait(filename_entry,'actual visible File name entry associated with its label');assert field[2]>0 and field[3]>0 and 0<=field[0]<dialog['window_rect']['width'] and 0<=field[1]<dialog['window_rect']['height']
  call('swaymsg','seat','seat0','cursor','set',str(round(dialog['rect']['x']+dialog['window_rect']['x']+field[0]+field[2]/2)),str(round(dialog['rect']['y']+dialog['window_rect']['y']+field[1]+field[3]/2)));pointer.stdin.write('m 0.5 0.5\np\nr\n');pointer.stdin.flush()
  def focused_field():
   words=dbus.Interface(a11y.get_object(*field_row['ref'],introspect=False),'org.a11y.atspi.Accessible').GetState(timeout=.5);return bool(int(words[12//32])&(1<<(12%32)))
  wait(focused_field,'actual filename entry focus acknowledgement');keyboard.key('-M','ctrl','-k','a','-m','ctrl');assert path.parent==source.parent;keyboard.type(path.name)
  wait(lambda:str(dbus.Interface(a11y.get_object(*field_row['ref'],introspect=False),'org.a11y.atspi.Text').GetText(dbus.Int32(0),dbus.Int32(-1),timeout=.5))==path.name,'actual destination filename acknowledgement');keyboard.key('-k','Tab')
  save=wait(lambda:next((row for row in accessible_rows() if row['pid']==shell.pid and row['showing'] and row['role'] in ('push button','button') and row['name']=='Save'),None),'actual Save confirmation button');box=bounds(save)
  call('swaymsg','seat','seat0','cursor','set',str(round(dialog['rect']['x']+dialog['window_rect']['x']+box[0]+box[2]/2)),str(round(dialog['rect']['y']+dialog['window_rect']['y']+box[1]+box[3]/2)));pointer.stdin.write('m 0.5 0.5\np\n');pointer.stdin.flush();time.sleep(.08)
  if not hold:pointer.stdin.write('r\n');pointer.stdin.flush()

 try:
  start_private_bus(env,out,processes)
  assert a.at_spi_prefix
  launch([str(a.at_spi_prefix/'libexec/at-spi-bus-launcher'),'--launch-immediately','--a11y=1','--screen-reader=1'],'accessibility-bus.log');session=dbus.bus.BusConnection(env['DBUS_SESSION_BUS_ADDRESS']);wait(lambda:session.name_has_owner('org.a11y.Bus'),'private accessibility bus');address=str(session.get_object('org.a11y.Bus','/org/a11y/bus').GetAddress(dbus_interface='org.a11y.Bus'));assert address.startswith('unix:path='+str(home)+'/');env['AT_SPI_BUS_ADDRESS']=address;a11y=dbus.bus.BusConnection(address);launch([str(a.at_spi_prefix/'libexec/at-spi2-registryd')],'accessibility-registry.log');wait(lambda:a11y.name_has_owner('org.a11y.atspi.Registry'),'private accessibility registry')
  config=out/'sway.conf';config.write_text('output HEADLESS-1 resolution 1200x900\nseat seat0 fallback true\nxwayland disable\nfocus_follows_mouse no\nfor_window [all] floating enable\n')
  launch([a.sway,'--unsupported-gpu','-c',str(config)],'sway.log');env['SWAYSOCK']=str(wait(lambda:next(home.glob('sway-ipc*.sock'),None),'private compositor'));env['WAYLAND_DISPLAY']=wait(lambda:next((p.name for p in home.glob('wayland-*') if not p.name.endswith('.lock')),None),'private display');pointer=launch([a.pointer_tool,'pointer','move','0','0'],'pointer.log',stdin=sp.PIPE,text=True);
  with (out/'keyboard.log').open('w') as keyboard_log:keyboard=PhysicalKeyboard(a.keyboard,env,keyboard_log)
  processes.append(keyboard.process);shell=launch(['quickshell','-p',str(qml)],'shell.log')
  wait(lambda:ipc('startPreview',str(source))=='','owned Quick Look show');wait(lambda:state()['result'].get('type')=='image','actual immutable image preview');runtime=(Path(shutil.which('quickshell',path=env['PATH'])).resolve().parent.parent/'share/lumen-desktop/qt-wayland-runtime').read_text().strip();loaded=sorted({line.split()[-1] for line in Path('/proc/'+str(shell.pid)+'/maps').read_text().splitlines() if 'libQt6WaylandClient.so' in line});assert runtime and loaded and all(path.startswith(runtime+'/') for path in loaded),(runtime,loaded);(out/'qt-runtime.json').write_text(json.dumps(dict(root=runtime,loaded=loaded,scope='Actual owned shell process with scoped executable RPATH; no runtime environment override'),indent=2));captured=state()['result'];begin();click('Add Rectangle');wait(lambda:len(state()['strokes'])==1,'actual keyboard-equivalent rectangle button');click('Cancel');wait(lambda:not state()['markup'] and state()['uri']=='','canceled source retired');assert source.read_bytes()==original and not target.exists();checks.append('Actual Quick Look annotation command, rectangle control and Cancel produce no output or original change')
  begin();click('Add Rectangle');call('grim','-o','HEADLESS-1',str(out/'editor.png'));click('Save Copy…');wait(lambda:any(n.get('name')=='Save Annotated Copy' for n in tree()),'Save Copy chooser');keyboard.key('-k','Escape');wait(lambda:state()['markup'] and len(state()['strokes'])==1,'chooser cancel preserves edits');assert not target.exists();checks.append('Actual Save Copy chooser cancellation returns to edits without a write')
  save_path(collision);wait(lambda:state()['error']=='That filename already exists; choose a new name','collision explained');assert collision.read_bytes()==b'untouched' and state()['markup'];checks.append('Existing destination stays byte-identical and failed Save preserves annotations')
  save_path(target);wait(lambda:target.exists() and not state()['markup'],'actual confirmed copy saved');assert source.read_bytes()==original
  with Image.open(target) as image:assert image.getpixel((32,20))[:3]==(255,59,48)
  shutil.copy2(target,out/'annotated-copy.png')
  checks.append('Explicit physical chooser save publishes annotated PNG pixels and leaves original bytes unchanged')
  begin();draw_line();replacement=home/'replacement.png';Image.new('RGB',(160,100),'blue').save(replacement);replacement.replace(source);denied=home/'replaced-denied.png';save_path(denied);wait(lambda:state()['error']=='The selected file changed','changed original refused');assert not denied.exists() and not list(home.glob('.lumen-markup-*'));checks.append('Actual freehand recipe cannot export after captured original is replaced; no partial output survives')
  click('Cancel');ipc('startPreview',str(source));wait(lambda:state()['result'].get('actionCapability') not in (None,captured['actionCapability']),'fresh source capability');begin();click('Add Rectangle');stale=home/'stale-copy.png';save_path(stale,hold=True);other=home/'other.png';Image.new('RGB',(160,100),'green').save(other);ipc('startPreview',str(other));wait(lambda:state()['result'].get('path')==str(other) and not state()['markup'] and state()['uri']=='' and not any(n.get('name')=='Save Annotated Copy' for n in tree()),'new preview retires exact old chooser');pointer.stdin.write('r\n');pointer.stdin.flush();time.sleep(.15);assert not stale.exists();begin();draw_line();click('Cancel');wait(lambda:not state()['markup'],'new preview controls usable');checks.append('Held Save release cannot retarget a new preview, closes old chooser and leaves fresh drawing/click controls usable')
  for extension,format,orientation,expected in [('jpg','JPEG',6,[(255,0,0),(0,255,0),(0,0,255),(255,255,0)]),('webp','WEBP',8,[(255,0,0),(0,255,0),(0,0,255),(255,255,0)])]:
   oriented=home/('oriented.'+extension);pixels=Image.new('RGB',(80,40));pixels.paste((255,0,0),(0,0,40,20));pixels.paste((0,255,0),(40,0,80,20));pixels.paste((0,0,255),(0,20,40,40));pixels.paste((255,255,0),(40,20,80,40));exif=Image.Exif();exif[274]=orientation;pixels.save(oriented,format=format,exif=exif,**({'lossless':True} if format=='WEBP' else {'quality':95}));pixels.close();before=oriented.read_bytes();ipc('startPreview',str(oriented));wait(lambda:state()['result'].get('path')==str(oriented),'oriented source preview');begin();g=wait(lambda:(value if (value:=json.loads(ipc('imageGeometry')))['ready'] else None),'actual oriented Image ready');assert not g['autoTransform'] and abs(g['paintedWidth']/g['paintedHeight']-2)<.01;node=next(n for n in tree() if n.get('pid')==shell.pid and n.get('name')=='Quick Look');capture=out/('oriented-'+extension+'-editor.png');call('grim','-o','HEADLESS-1',str(capture))
   with Image.open(capture) as painted:
    for (x,y),colour in zip([(.1,.1),(.9,.1),(.1,.9),(.9,.9)],expected):
     point=(round(node['rect']['x']+node['window_rect']['x']+g['x']+(g['width']-g['paintedWidth'])/2+g['paintedWidth']*x),round(node['rect']['y']+node['window_rect']['y']+g['y']+(g['height']-g['paintedHeight'])/2+g['paintedHeight']*y));actual=painted.getpixel(point)[:3];assert all(abs(a-b)<24 for a,b in zip(actual,colour)),(extension,point,actual,colour,g)
   click('Add Rectangle');export=home/('oriented-'+extension+'.png');save_path(export);wait(lambda:export.exists() and not state()['markup'],'oriented PNG saved')
   with Image.open(export) as encoded:
    assert encoded.size==(80,40) and encoded.getexif().get(274,1)==1
    for (x,y),colour in zip([(8,4),(72,4),(8,36),(72,36)],expected):assert all(abs(a-b)<24 for a,b in zip(encoded.getpixel((x,y))[:3],colour))
   assert oriented.read_bytes()==before;shutil.copy2(export,out/export.name);checks.append('Actual '+format+' explicit raw orientation matches painted editor corners and saved PNG dimensions/pixels without original changes')
  assert captured['resourcePath']!=str(source);call('grim','-o','HEADLESS-1',str(out/'final.png'));result={'passed':True,'checks':checks,'source':sp.check_output(['git','rev-parse','HEAD'],text=True).strip(),'launcher':str(a.launcher),'originalSha256':hashlib.sha256(original).hexdigest(),'copySha256':hashlib.sha256(target.read_bytes()).hexdigest()};(out/'result.json').write_text(json.dumps(result,indent=2)+'\n')
 except BaseException:
  try:(out/'failure-state.json').write_text(json.dumps(state(),indent=2));(out/'failure-tree.json').write_text(call('swaymsg','-r','-t','get_tree'));call('grim','-o','HEADLESS-1',str(out/'failure.png'))
  except (OSError,ValueError,NameError,sp.SubprocessError):pass
  raise
 finally:
  if 'a11y' in locals():a11y.close()
  if 'session' in locals():session.close()
  if keyboard:keyboard.close()
  if pointer and pointer.stdin and not pointer.stdin.closed:
   try:pointer.stdin.close()
   except BrokenPipeError:pass
  for child in reversed(processes):
   if child.poll() is None:child.terminate()
  for child in reversed(processes):
   try:child.wait(timeout=5)
   except sp.TimeoutExpired:child.kill();child.wait(timeout=5)
  close_reaped_pipes(processes)
