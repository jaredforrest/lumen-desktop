"""Private native dirty-document prompts with a final-action stub, never host logout."""
import argparse,json,os,re,shutil,signal,subprocess as sp,sys,tempfile,time
from pathlib import Path
from private_native_bus import start_private_bus
p=argparse.ArgumentParser();p.add_argument('--sway',required=True);p.add_argument('--effects',action='store_true');p.add_argument('--launcher',type=Path,required=True);p.add_argument('--output',type=Path,required=True);a=p.parse_args();a.output.mkdir(parents=True,exist_ok=False)
processes=[]
with tempfile.TemporaryDirectory(prefix='lumen-end-') as private:
 root=Path(private);env=dict(os.environ,HOME=private,XDG_RUNTIME_DIR=private)
 for key in ('WAYLAND_DISPLAY','DISPLAY','SWAYSOCK','DBUS_SESSION_BUS_ADDRESS','AT_SPI_BUS_ADDRESS'):env.pop(key,None)
 for key in ('CONFIG','STATE','CACHE','DATA'):env['XDG_'+key+'_HOME']=str(root/key.lower())
 prefix=re.split(r'\n[ \t]*exec quickshell ',a.launcher.read_text(),maxsplit=1)[0]
 installed=dict(v.decode().split('=',1) for v in sp.check_output(['bash','-c',prefix+'\nexec env -0'],env=env,stderr=sp.DEVNULL).split(b'\0') if b'=' in v)
 for key in ('PATH','GLOBAL_MENU_PYTHON','QT_PLUGIN_PATH','QML_IMPORT_PATH'):
  if key in installed:env[key]=installed[key]
 source=Path(installed['LUMEN_NOTIFICATION_HELPER']).parent.parent
 assert (source/'runtime/session_end.py').is_file(), 'Build a shell containing cooperative session preflight first'
 env.update(WLR_BACKENDS='headless',WLR_RENDERER='gles2' if a.effects else 'pixman',WLR_RENDERER_FORCE_SOFTWARE='1' if a.effects else '0',WLR_RENDERER_ALLOW_SOFTWARE='1',LIBGL_ALWAYS_SOFTWARE='1',WLR_LIBINPUT_NO_DEVICES='1',QT_QPA_PLATFORM='wayland',QT_QUICK_BACKEND='software',QT_QPA_PLATFORMTHEME='generic',QT_ACCESSIBILITY='0',QT_LINUX_ACCESSIBILITY_ALWAYS_ON='0',NO_AT_BRIDGE='1',GTK_A11Y='none',GDK_BACKEND='wayland')
 binary=root/'bin';binary.mkdir();env['PATH']=str(binary)+':'+env['PATH']
 # The final host command is intercepted inside this disposable fixture only.
 marker=a.output/'host-actions';stub=binary/'systemctl';stub.write_text('#!/bin/sh\nprintf "%s\\n" "$*" >> "'+str(marker)+'"\n');stub.chmod(0o700)
 palette=json.loads((source/'appearance/palette.json').read_text());appearance=Path(env['XDG_STATE_HOME'])/'lumen-desktop/appearance.json';appearance.parent.mkdir(parents=True);appearance.write_text(json.dumps({'font':palette['font'],'palette':palette['light'],'preferences':{}}))
 fixture=root/'fixture.qml';fixture.write_text('''import QtQuick
import Quickshell
import Quickshell.Io
import "'''+(source/'shell').as_uri()+'''" as D
ShellRoot {
 D.ApplicationLifecycleState {id:lifecycle}
 D.SessionOptions {id:options;lifecycle:lifecycle}
 IpcHandler {target:"sessionAudit"
  function begin():void{options.show("reboot");options.confirm();}
  function confirm():void{options.confirm();}
  function cancel():void{options.dismiss();}
  function status():string{return JSON.stringify({available:lifecycle.available,session:lifecycle.session,applications:lifecycle.applications,visible:options.visible,error:options.error});}
 }
}''')
 client=root/'client.py';client.write_text('''import gi,sys,json
from pathlib import Path
gi.require_version('Gtk','3.0')
from gi.repository import Gtk,Gio,GLib,Gdk
root=Path(sys.argv[1]);app=Gtk.Application(application_id='org.example.SessionDocument');dialog=None;requests=0
GLib.set_prgname('org.example.SessionDocument')
def status(choice=''):(root/'client-state.json').write_text(json.dumps(dict(requests=requests,prompt=dialog is not None,choice=choice)))
def activate(app):
 global window
 window=Gtk.ApplicationWindow(application=app,title='Unsaved session document');window.set_default_size(400,240);window.add(Gtk.Label(label='Private unsaved fixture text'));window.show_all();status()
def response(prompt,code):
 global dialog
 if code==1:(root/'saved.txt').write_text('Private unsaved fixture text')
 dialog=None;prompt.destroy();status({1:'save',2:'discard'}.get(code,'cancel'))
 if code in (1,2):app.quit()
def keypress(widget,event):
 if event.keyval==Gdk.KEY_Escape:widget.response(Gtk.ResponseType.CANCEL);return True
 if event.state & Gdk.ModifierType.MOD1_MASK:
  choice={Gdk.KEY_s:1,Gdk.KEY_d:2,Gdk.KEY_c:Gtk.ResponseType.CANCEL}.get(event.keyval)
  if choice is not None:widget.response(choice);return True
 return False
def quit_action(action,parameter):
 global requests,dialog
 requests+=1
 if dialog is None:
  dialog=Gtk.Dialog(title='Save session document?',transient_for=window,modal=True);dialog.add_button('_Cancel',Gtk.ResponseType.CANCEL);dialog.add_button('_Discard',2);dialog.add_button('_Save',1);dialog.set_default_response(Gtk.ResponseType.CANCEL);dialog.connect('response',response);dialog.connect('key-press-event',keypress);dialog.show_all()
 status()
def startup(app):
 action=Gio.SimpleAction.new('quit',None);action.connect('activate',quit_action);app.add_action(action)
app.connect('startup',startup);app.connect('activate',activate);app.run([])
''')
 def launch(cmd,name,**kwargs):
  with (a.output/name).open('w') as log:proc=sp.Popen(cmd,env=env,stdout=log,stderr=sp.STDOUT,start_new_session=True,**kwargs)
  processes.append(proc);return proc
 def call(*cmd):return sp.check_output(cmd,env=env,text=True,timeout=5).strip()
 def wait(fn):
  until=time.monotonic()+20
  while time.monotonic()<until:
   try:
    value=fn()
    if value:return value
   except (OSError,ValueError,sp.SubprocessError):pass
   time.sleep(.05)
  raise AssertionError('Session fixture timeout: '+str(a.output))
 def ipc(method):return call('quickshell','-p',str(fixture),'ipc','call','sessionAudit',method)
 def state():
  value=json.loads(ipc('status'));(a.output/'last-state.json').write_text(json.dumps(value,indent=2));return value
 def app_state():return json.loads((a.output/'client-state.json').read_text())
 def nodes(tree):
  yield tree
  for child in tree.get('nodes',[])+tree.get('floating_nodes',[]):yield from nodes(child)
 def focus_prompt():
  dialog=wait(lambda:next((n for n in nodes(json.loads(call('swaymsg','-r','-t','get_tree'))) if n.get('name')=='Save session document?'),None));call('swaymsg','[con_id='+str(dialog['id'])+'] focus');time.sleep(.15)
 def choose(letter):focus_prompt();call('wtype','-s','200','-M','alt',letter,'-m','alt')
 try:
  start_private_bus(env,a.output,processes)
  config=a.output/'sway.conf';config.write_text('output HEADLESS-1 resolution 1280x800\nxwayland disable\nseat seat0 fallback true\nfocus_follows_mouse no\n')
  launch([a.sway,'--unsupported-gpu','-c',str(config)],'sway.log');env['SWAYSOCK']=str(wait(lambda:next(root.glob('sway-ipc*.sock'),None)));env['WAYLAND_DISPLAY']=wait(lambda:next((v.name for v in root.glob('wayland-*') if not v.name.endswith('.lock')),None))
  desktop=launch(['quickshell','-p',str(fixture)],'shell.log');wait(lambda:state()['available'])
  app=launch([env['GLOBAL_MENU_PYTHON'],str(client),str(a.output)],'client.log');wait(lambda:(a.output/'client-state.json').exists());wait(lambda:any(app['appId']=='org.example.SessionDocument' for app in state()['applications']))
  ipc('begin');wait(lambda:state()['session']['phase'] in ('waiting','blocked'));assert state()['session']['phase']=='waiting',state();wait(lambda:app_state()['prompt'])
  dialog=next(n for n in nodes(json.loads(call('swaymsg','-r','-t','get_tree'))) if n.get('name')=='Save session document?');assert dialog['lumen_relationships']['parent'] is not None
  call('grim',str(a.output/'app-prompt.png'));focus_prompt();call('wtype','-s','200','-k','Escape');wait(lambda:app_state()['choice']=='cancel');time.sleep(1.2);assert state()['session']['phase']=='waiting' and not marker.exists();ipc('cancel');wait(lambda:state()['session']['phase']=='idle');assert app.poll() is None and not marker.exists()
  # Re-request and save from the application's actual modal controls.
  ipc('begin');wait(lambda:app_state()['prompt']);choose('s');app.wait(timeout=5);wait(lambda:state()['session']['phase']=='ready');assert (a.output/'saved.txt').read_text()=='Private unsaved fixture text';assert not marker.exists()
  ipc('cancel');wait(lambda:state()['session']['phase']=='idle');time.sleep(1.2);assert not marker.exists()
  # A fresh dirty app discards through its own dialog; final host stub needs another click.
  (a.output/'client-state.json').unlink();app=launch([env['GLOBAL_MENU_PYTHON'],str(client),str(a.output)],'client-discard.log');wait(lambda:(a.output/'client-state.json').exists());wait(lambda:any(app['appId']=='org.example.SessionDocument' for app in state()['applications']));ipc('begin');wait(lambda:app_state()['prompt']);choose('d');app.wait(timeout=5);wait(lambda:state()['session']['phase']=='ready');assert not marker.exists();ipc('confirm');wait(marker.exists);assert marker.read_text().splitlines()==['reboot'];time.sleep(.3);assert marker.read_text().splitlines()==['reboot']
  (a.output/'result.json').write_text(json.dumps(dict(passed=True,checks=['native app-owned modal save/cancel/discard','cancel preserves live dirty application','save persists document','no implicit final action after ready or cancellation','explicit fresh confirmation invokes injected final-action stub once']),indent=2))
 finally:
  for proc in reversed(processes):
   if proc.poll() is None:os.killpg(proc.pid,signal.SIGTERM)
  for proc in reversed(processes):
   try:proc.wait(timeout=5)
   except sp.TimeoutExpired:os.killpg(proc.pid,signal.SIGKILL);proc.wait()
