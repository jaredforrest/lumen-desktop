"""Private installed office windows with real compositor focus and physical typing."""
import argparse,hashlib,json,os,re,signal,socket,subprocess,sys,time,traceback
from pathlib import Path
from application_fixture.isolation import run as isolate
from application_fixture.keyboard import PhysicalKeyboard
from fixture_resources import close_reaped_pipes
from office_fixture import corpus,formats,live
from office_fixture.control import Control,ControlError
p=argparse.ArgumentParser();p.add_argument('--writer',required=True);p.add_argument('--sheet',required=True);p.add_argument('--slides',required=True);p.add_argument('--software-driver',required=True);p.add_argument('--sway',required=True);p.add_argument('--keyboard-tool',required=True);p.add_argument('--pointer-tool',required=True);p.add_argument('--close-probe',action='store_true');p.add_argument('--document-probe',action='store_true');p.add_argument('--pdf-tools',default='');p.add_argument('--pdf-viewer',default='');p.add_argument('--xdg-tools',default='');p.add_argument('--mime-data',default='');p.add_argument('--at-spi-prefix',default=os.environ.get('LUMEN_TEST_AT_SPI',''));p.add_argument('--dialog-tools',default='');p.add_argument('--new-saveas',action='store_true');p.add_argument('--sheet-close-outcome',choices=['cancel','discard','save','error'],default='');p.add_argument('--slides-close-outcome',choices=['cancel','discard','save'],default='');p.add_argument('--effects',action='store_true');p.add_argument('--writer-close-outcome',choices=['cancel','discard','save'],default='cancel');p.add_argument('--app',choices=['writer','sheet','slides'],default='writer');p.add_argument('--output',type=Path,required=True);p.add_argument('--inside',action='store_true');p.add_argument('--profiles-json',default='');p.add_argument('--source-sha',default='');a=p.parse_args()
if a.new_saveas and (not a.dialog_tools or a.close_probe or a.sheet_close_outcome or a.slides_close_outcome or a.writer_close_outcome!='cancel'):p.error('New SaveAs requires private dialog tools and a separate lifecycle scenario')
if a.sheet_close_outcome and a.app!='sheet':p.error('Spreadsheet close outcome requires the spreadsheet provider')
if a.slides_close_outcome and a.app!='slides':p.error('Presentation close outcome requires the presentation provider')
if not a.inside:
 source=Path(__file__).resolve().parents[2];out=a.output.resolve();out.mkdir(mode=0o700,parents=True,exist_ok=False)
 for binary in [a.writer,a.sheet,a.slides,a.sway,a.pointer_tool,a.keyboard_tool]:
  path=Path(binary).resolve();assert str(path).startswith('/nix/store/') and path.is_file() and os.access(path,os.X_OK),'Exact installed application/test binaries required'
 policies=subprocess.check_output(['nix','eval','--json','--file',str(source/'config/application-profiles.nix')],text=True)
 sha=subprocess.check_output(['git','-C',str(source),'rev-parse','HEAD'],text=True).strip()
 args=[sys.executable,'/source/tests/desktop/office_native_workflows.py','--inside','--writer',a.writer,'--sheet',a.sheet,'--slides',a.slides,'--software-driver',a.software_driver,'--sway',a.sway,'--keyboard-tool',a.keyboard_tool,'--pointer-tool',a.pointer_tool,'--writer-close-outcome',a.writer_close_outcome,'--app',a.app,'--profiles-json',policies,'--source-sha',sha,'--output','/artifacts']
 if a.effects:args.append('--effects')
 if a.close_probe:args.append('--close-probe')
 if a.document_probe:args.append('--document-probe')
 if a.pdf_tools:args.extend(['--pdf-tools',a.pdf_tools])
 if a.pdf_viewer:args.extend(['--pdf-viewer',a.pdf_viewer,'--xdg-tools',a.xdg_tools,'--mime-data',a.mime_data,'--at-spi-prefix',a.at_spi_prefix])
 if a.dialog_tools:args.extend(['--dialog-tools',a.dialog_tools])
 if a.new_saveas:args.append('--new-saveas')
 if a.sheet_close_outcome:args.extend(['--sheet-close-outcome',a.sheet_close_outcome])
 if a.slides_close_outcome:args.extend(['--slides-close-outcome',a.slides_close_outcome])
 raise SystemExit(isolate(source,out,args,software_graphics=a.effects))
assert os.getpid()==1 and os.getuid()!=0 and os.uname().nodename=='fixture' and Path.home()==Path('/home/fixture')
assert not any((Path('/dev')/name).exists() for name in ('dri','snd','video0')) and Path('/dev/udmabuf').exists()==a.effects
out=Path('/artifacts');processes=[];control=None;keyboard=None;accessibility=None;checks=[];facts={};env=dict(os.environ)
facts['testSources']={str(path.relative_to('/source')):hashlib.sha256(path.read_bytes()).hexdigest() for path in [Path('/source/tests/desktop/office_native_workflows.py'),*sorted(Path('/source/tests/desktop/office_fixture').glob('*.py')),*sorted(Path('/source/tests/desktop/application_fixture').glob('*'))] if path.is_file()}
facts['installedBinaries']={binary:hashlib.sha256(Path(binary).read_bytes()).hexdigest() for binary in [a.writer,a.sheet,a.slides,a.sway,a.pointer_tool,a.keyboard_tool]}
settings=Path('/etc/nix');settings.mkdir();(settings/'desktop-settings.json').write_text(json.dumps(dict(applicationPreferences=json.loads(a.profiles_json))))
mesa=Path(a.software_driver);vulkan=mesa/'share/vulkan/icd.d/lvp_icd.x86_64.json';egl=mesa/'share/glvnd/egl_vendor.d/50_mesa.json';assert vulkan.is_file() and egl.is_file()
env.update(WLR_BACKENDS='headless',WLR_RENDERER='pixman',WLR_LIBINPUT_NO_DEVICES='1',LIBGL_ALWAYS_SOFTWARE='1',MESA_LOADER_DRIVER_OVERRIDE='llvmpipe',VK_DRIVER_FILES=str(vulkan),VK_ICD_FILENAMES=str(vulkan),__EGL_VENDOR_LIBRARY_FILENAMES=str(egl),LIBGL_DRIVERS_PATH=str(mesa/'lib/dri'),WGPU_BACKEND='vulkan',SWAYSOCK='/run/fixture/sway.sock',DECKCRAFT_WAYLAND='1',DECKCRAFT_NO_AUDIO='1')
if a.effects:env.update(WLR_RENDERER='gles2',WLR_RENDERER_ALLOW_SOFTWARE='1',WLR_RENDERER_FORCE_SOFTWARE='1')
facts['boundary']=dict(uid=os.getuid(),gid=os.getgid(),pid=os.getpid(),hostname=os.uname().nodename,home=str(Path.home()),hardwarePathsAbsent=True,softwareAllocatorExposed=a.effects,renderer=env['WLR_RENDERER'])
app={'writer':a.writer,'sheet':a.sheet,'slides':a.slides}[a.app];name={'writer':'wordcraft','sheet':'gridcraft','slides':'deckcraft'}[a.app];cli=str(Path(app).with_name(name+'-cli'));port=34217;env[name.upper()+'_CONTROL_PORT']=str(port)
fixture=corpus.create();extension={'writer':'docx','sheet':'xlsx','slides':'pptx'}[a.app];source=out/(a.app+'-working.'+extension)
def launch(command,label,**options):
 with (out/(label+'.log')).open('w') as log:process=subprocess.Popen(command,env=env,stdout=log,stderr=subprocess.STDOUT,start_new_session=True,**options)
 processes.append(process);return process

def command(args):return subprocess.check_output(args,env=env,text=True,timeout=10).strip()
def tree():return json.loads(command(['swaymsg','-r','-t','get_tree']))
def walk(node):
 yield node
 for child in node.get('nodes',[])+node.get('floating_nodes',[]):yield from walk(child)
def wait(probe,label,owner=None):
 deadline=time.monotonic()+30
 while time.monotonic()<deadline:
  if owner is not None and owner.poll() is not None:raise AssertionError(label+': owned process exited '+str(owner.returncode))
  try:
   value=probe()
   if value:return value
  except (OSError,ValueError,subprocess.SubprocessError):pass
  time.sleep(.05)
 diagnostics=dict(label=label,profileFiles={str(p):p.read_text() for p in Path('/run/fixture').glob('lumen-profile-*.json')})
 if control is not None:
  try:diagnostics['liveDocument']=control.command(a.app,'file.info' if a.app=='writer' else 'document.inspect')
  except Exception as error:diagnostics['liveDocumentError']=type(error).__name__
 try:diagnostics['tree']=tree();command(['grim',str(out/'failure.png')])
 except (OSError,ValueError,subprocess.SubprocessError):pass
 (out/'timeout.json').write_text(json.dumps(diagnostics,indent=2))
 raise AssertionError('Office readiness condition timed out: '+label)
def live_pid():
 path=Path('/run/fixture')/('lumen-profile-'+name+'.json')
 if not path.exists():return None
 values=json.loads(path.read_text()).get('pids',[])
 return values[0] if len(values)==1 else None
def native():
 pid=live_pid()
 candidates=[node for node in walk(tree()) if pid and node.get('pid')==pid and node.get('app_id')=='ai.storyteller.'+name and node.get('type') in ('con','floating_con') and node.get('rect',{}).get('width',0)>0]
 return candidates[0] if len(candidates)==1 else None
def listener():
 with socket.socket(socket.AF_INET,socket.SOCK_STREAM) as client:client.settimeout(.1);return client.connect_ex(('127.0.0.1',port))==0

def key(*args):
 if '--' in args:
  for char in args[-1]:keyboard.type(char);time.sleep(.04)
  time.sleep(.1)
 else:keyboard.key(*args)
def pointer_events(x,y,events):
 command(['swaymsg','seat','seat0','cursor','set',str(x),str(y)])
 for event in ['m 0.5 0.5',*events]:
  pointer.stdin.write(event+'\n');pointer.stdin.flush();time.sleep(.12)

def inspect_dirty_close():
 expected_hash=formats.digest(source);marker='Unsaved provider close fixture'
 assert native()['id']==window['id'],'Native close target changed'
 if a.app=='sheet':
  pointer_events(156,289,['p','r']);wait(lambda:control.command('sheet','document.inspect')['activeCell']=='C2','captured dirty-close cell selected',provider)
  key('--',marker);key('-k','Return');facts['closeProbeDirtyCell']=wait(lambda:(value if marker in json.dumps(value:=control.tool('get_cell',dict(cell='Summary!C2'))) else None),'dirty close cell acknowledged',provider)
 else:
  pointer_events(520,320,['p','r','p','r']);key('-k','End');key('--',marker)
  facts['closeProbeDirtySlide']=wait(lambda:(value if marker in json.dumps(value:=control.tool('inspect_slide',dict(index=0))) else None),'dirty close slide acknowledged',provider)
 facts['closeProbeBefore']=control.command(a.app,'document.inspect');assert facts['closeProbeBefore']['dirty'] and formats.digest(source)==expected_hash
 command(['swaymsg','[con_id='+str(window['id'])+'] kill']);time.sleep(.3)
 facts['closeProbeProcessState']=dict(wrapperExit=provider.poll(),capturedNativePidPresent=Path('/proc/'+str(pid)).exists(),capturedWindowPresent=any(node.get('id')==window['id'] for node in walk(tree())))
 try:facts['closeProbeAfter']=control.command(a.app,'document.inspect')
 except ControlError:
  facts['closeProbeDocumentInspection']='refused_after_close_request_cause_unconfirmed'
  facts['closeProbeRefusal']=control.frames[-1]
 if a.app=='slides':facts['closeProbeUi']=control.tool('call_app',dict(method='ui.inspect'))
 else:facts['closeProbeUi']=json.loads(command([cli,'send',str(port),'ui.inspect','{}']))
 facts['closeProbeTree']=tree();facts['exploratoryCloseProbe']=True
 assert formats.digest(source)==expected_hash
 command(['grim',str(out/'close-probe.png')])



def sheet_ui():return json.loads(command([cli,'send',str(port),'ui.inspect','{}']))

def finish_sheet_close():
 expected_hash=formats.digest(source);marker='Unsaved provider close fixture'
 first=facts['closeProbeUi']['dialog'];assert first['name']=='saveChanges' and type(first['values']['uid']) is int
 key('-k','Escape')
 wait(lambda:not sheet_ui().get('dialog'),'physical Escape cancels spreadsheet native close',provider)
 def retained():
  assert native() and control.command('sheet','document.inspect')['dirty'] and marker in json.dumps(control.tool('get_cell',dict(cell='Summary!C2'))) and formats.digest(source)==expected_hash
 retained()
 checks.append('Physical Escape preserves complete unsaved spreadsheet content and saved source after native close')
 key('-M','ctrl','-k','f','-m','ctrl')
 wait(lambda:(sheet_ui().get('dialog') or {}).get('name')=='find','physical Find opens unrelated spreadsheet dialog',provider)
 command(['swaymsg','[con_id='+str(window['id'])+'] kill']);time.sleep(.2)
 assert native() and (sheet_ui().get('dialog') or {}).get('name')=='find'
 key('-k','Escape');wait(lambda:not sheet_ui().get('dialog'),'physical Escape dismisses unrelated dialog',provider);retained()
 checks.append('Native close during unrelated dialog is canceled without changing captured document')
 command(['swaymsg','[con_id='+str(window['id'])+'] kill'])
 dialog=wait(lambda:(value if (value:=sheet_ui()).get('dialog') else None),'second captured spreadsheet native close prompts',provider)
 assert dialog['dialog']['values']['uid']==first['values']['uid']
 outcome=a.sheet_close_outcome
 if outcome=='error':out.chmod(0o500)
 try:
  pointer_events({'cancel':642,'discard':574,'save':693,'error':693}[outcome],494,['p','r'])
  if outcome=='error':
   message=wait(lambda:sheet_ui().get('message'),'failed physical spreadsheet Save reports error',provider)
   facts['failedCloseSaveMessage']=message
 finally:
  if outcome=='error':out.chmod(0o700)
 if outcome in ('cancel','error'):
  wait(lambda:not sheet_ui().get('dialog'),'spreadsheet close confirmation retires without closing dirty document',provider);retained()
  if outcome=='error':
   command(['swaymsg','[con_id='+str(window['id'])+'] kill']);time.sleep(.2)
   assert native() and sheet_ui().get('message')==message
   key('-k','Escape');wait(lambda:not sheet_ui().get('message'),'physical Escape dismisses failed-save message',provider);retained()
 else:
  def released():
   if provider.poll() is not None or not any(node.get('id')==window['id'] for node in walk(tree())):return True
   return control.command('sheet','document.inspect').get('path')!=str(source)
  wait(released,'physical spreadsheet close releases captured document')
  persisted=formats.spreadsheet(source);facts['nativeCloseCompleted']=True
  if outcome=='discard':assert formats.digest(source)==expected_hash and marker not in json.dumps(persisted)
  else:assert formats.digest(source)!=expected_hash and marker in json.dumps(persisted)
 facts['spreadsheetCloseOutcome']=dict(choice=outcome,savedSha256=formats.digest(source),tree=tree())
 checks.append('Physical spreadsheet '+outcome+' has independently validated saved-file and captured-document outcome')

def finish_slides_close():
 expected_hash=formats.digest(source);marker='Unsaved provider close fixture'
 def ui():return control.tool('call_app',dict(method='ui.inspect'))
 assert facts['closeProbeUi'].get('dialog')=='quit'
 key('-k','Escape')
 wait(lambda:not ui().get('dialog'),'physical Escape cancels presentation dirty close',provider)
 assert native() and marker in json.dumps(control.tool('inspect_slide',dict(index=0))) and formats.digest(source)==expected_hash
 checks.append('Physical Escape cancels presentation native close and preserves complete unsaved text and saved file')
 command(['swaymsg','[con_id='+str(window['id'])+'] kill'])
 wait(lambda:ui().get('dialog')=='quit','repeated native presentation close prompts again',provider)
 pointer_events({'cancel':777,'discard':592,'save':854}[a.slides_close_outcome],510,['p','r'])
 if a.slides_close_outcome=='cancel':
  wait(lambda:not ui().get('dialog'),'physical presentation Cancel preserves its document',provider)
  assert native() and control.command('slides','document.inspect')['dirty'] and marker in json.dumps(control.tool('inspect_slide',dict(index=0))) and formats.digest(source)==expected_hash
 else:
  def released():
   if provider.poll() is not None or not any(node.get('id')==window['id'] for node in walk(tree())):return True
   return control.command('slides','document.inspect').get('path')!=str(source)
  wait(released,'physical presentation close releases captured document')
  persisted=formats.presentation(source);facts['nativeCloseCompleted']=True
  if a.slides_close_outcome=='discard':assert formats.digest(source)==expected_hash and marker not in json.dumps(persisted)
  else:assert formats.digest(source)!=expected_hash and marker in json.dumps(persisted)
 facts['presentationCloseOutcome']=dict(choice=a.slides_close_outcome,savedSha256=formats.digest(source),tree=tree())
 checks.append('Physical presentation '+a.slides_close_outcome+' has independently validated saved-file and native-document outcome')


def finish_new_saveas():
 assert a.dialog_tools
 expected_hash=formats.digest(source);marker='Unsaved new document fixture'
 control.tool({'writer':'new_document','sheet':'new_workbook','slides':'new_presentation'}[a.app])
 if a.app=='writer':key('--',marker)
 elif a.app=='sheet':key('--',marker);key('-k','Return')
 else:
  slide=control.tool('inspect_slide',dict(index=0));ui=control.tool('call_app',dict(method='ui.inspect'));info=control.command('slides','document.inspect')
  titles=[shape for shape in slide['shapes'] if shape.get('placeholder') in ('title','ctrTitle')];assert len(titles)==1
  box=titles[0]['box'];rect=ui['slideRect'];owner=native()['rect'];scale=rect[2]/info['slideSize'][0]
  x=owner['x']+rect[0]+(box[0]+box[2]/2)*scale;y=owner['y']+rect[1]+(box[1]+box[3]/2)*scale
  facts['newPresentationTitleTarget']=dict(shape=titles[0],point=[x,y],slideRect=rect,scale=scale)
  pointer_events(x,y,['p','r','p','r']);key('-k','End');key('--',marker)
 def text():
  if a.app=='writer':return control.tool('get_text')
  if a.app=='sheet':return control.tool('get_cell',dict(cell='Sheet1!A1'))
  return control.tool('inspect_slide',dict(index=0))
 facts['newUnsavedContent']=wait(lambda:(value if marker in json.dumps(value:=text()) else None),'new document physical text acknowledged',provider)
 def retained():
  info=control.command(a.app,'file.info' if a.app=='writer' else 'document.inspect')
  assert info['dirty'] and not info.get('path') and marker in json.dumps(text()) and formats.digest(source)==expected_hash
 retained()
 def owned_choosers():return [node for node in walk(tree()) if node.get('pid')==facts['privateChooser']['backendPid'] and node.get('type') in ('con','floating_con') and node.get('rect',{}).get('width',0)>0]
 def chooser():
  return wait(lambda:next((node for node in owned_choosers() if node.get('focused')),None),'exact owned standard chooser maps and gains focus',provider)
 def absent(node):wait(lambda:not any(item.get('id')==node['id'] for item in walk(tree())),'captured chooser retires',provider)
 if a.app=='sheet':
  command(['swaymsg','[con_id='+str(window['id'])+'] kill'])
  wait(lambda:(sheet_ui().get('dialog') or {}).get('name')=='saveChanges','new unsaved workbook native close decision',provider)
  pointer_events(693,494,['p','r'])
 else:key('-M','ctrl','-k','s','-m','ctrl')
 first=chooser();facts['newSaveAsChooser']=first;command(['grim',str(out/'unsaved-chooser.png')])
 key('-k','Escape');absent(first);retained()
 checks.append('New document actual standard SaveAs cancels physically while complete draft and original saved file remain intact')
 # A nonexistent location must remain a chooser error, never a saved document.
 key('-M','ctrl','-k','s','-m','ctrl');second=chooser();before_ids={node['id'] for node in owned_choosers()}
 key('-M','ctrl','-k','l','-m','ctrl');key('-M','ctrl','-k','a','-m','ctrl');key('--','/source/missing-fixture-save-folder/book.'+extension);key('-k','Return')
 error=wait(lambda:next((node for node in owned_choosers() if node['id'] not in before_ids),None),'standard chooser reports invalid location in an owned dialog',provider)
 facts['newSaveAsInvalidLocation']=error;command(['grim',str(out/'unsaved-chooser-error.png')])
 key('-k','Escape');absent(error)
 key('-k','Escape');absent(second);retained()
 checks.append('New document invalid-location chooser error and physical cancellation preserve complete unsaved content')
 key('-M','ctrl','-k','s','-m','ctrl');third=chooser();filename='new-unsaved.'+extension;saved=out/filename
 key('-M','ctrl','-k','l','-m','ctrl');key('-M','ctrl','-k','a','-m','ctrl');key('--',str(saved));key('-k','Return')
 absent(third)
 wait(lambda:saved.is_file(),'explicit standard SaveAs writes generated native document',provider)
 parse={'writer':formats.writer,'sheet':formats.spreadsheet,'slides':formats.presentation}[a.app]
 persisted=parse(saved);assert marker in json.dumps(persisted) and formats.digest(source)==expected_hash
 info=wait(lambda:(value if (value:=control.command(a.app,'file.info' if a.app=='writer' else 'document.inspect')).get('path')==str(saved) and not value['dirty'] else None),'new SaveAs document acknowledged clean at exact destination',provider)
 facts['newSaveAsResult']=dict(path=str(saved),sha256=formats.digest(saved),document=persisted,live=info)
 command(['grim',str(out/'unsaved-saved.png')]);facts['completedPaint']='unsaved-saved.png'
 command(['swaymsg','[con_id='+str(window['id'])+'] kill'])
 wait(lambda:not any(node.get('id')==window['id'] for node in walk(tree())),'explicit later clean native close succeeds')
 facts['nativeCloseCompleted']=True
 checks.append('Later explicit SaveAs persists complete new document, preserves original source and permits clean native close')

def reopen_saved():
 saved_hash=formats.digest(source);parse={'writer':formats.writer,'sheet':formats.spreadsheet,'slides':formats.presentation}[a.app];saved_facts=parse(source)
 before_live=live.snapshot(control,a.app);facts['beforeReopen']=before_live
 control.tool({'writer':'new_document','sheet':'new_workbook','slides':'new_presentation'}[a.app])
 cleared=control.command(a.app,'file.info' if a.app=='writer' else 'document.inspect')
 assert cleared.get('path')!=str(source),'New document did not detach the saved document'
 control.tool({'writer':'open_document','sheet':'open_workbook','slides':'open_presentation'}[a.app],dict(path=str(source)))
 wait(lambda:(value if (value:=control.command(a.app,'file.info' if a.app=='writer' else 'document.inspect')).get('path')==str(source) and not value.get('dirty') else None),'GUI reopens exact saved path cleanly',provider)
 after_live=live.snapshot(control,a.app);facts['afterReopen']=after_live
 diff=formats.differences(before_live,after_live);facts['nativeReopenDifferences']=diff
 if diff:(out/'reopen-differences.json').write_text(json.dumps(dict(before=before_live,after=after_live,differences=diff),indent=2))
 assert not diff,'Native reopened document differs from saved live content; inspect retained differences'
 assert formats.digest(source)==saved_hash and parse(source)==saved_facts,'Reopen modified saved file'
 command(['grim',str(out/'reopened.png')])
 checks.append('Native app clears then reopens exact saved file; full text/structured provider projection and independent file facts agree')

try:
 data=fixture[{'writer':'writer','sheet':'grid','slides':'deck'}[a.app]];args=['run',*[value for id,params in data for value in ('--cmd',id+'='+json.dumps(params,separators=(',',':')))],'--out' if a.app=='sheet' else '--save',str(source)]
 with (out/'create.log').open('w') as log:subprocess.run([cli,*args],env=env,stdout=log,stderr=subprocess.STDOUT,check=True,timeout=60)
 assert formats.package_kind(source)==extension;before=formats.digest(source);facts['originalSha256']=before
 config=out/'sway.conf';config.write_text('output HEADLESS-1 resolution 1440x1000\nxwayland disable\nfor_window [all] floating enable, border pixel 0\n')
 compositor=launch([a.sway,'-d','--unsupported-gpu','-c',str(config)],'sway');wait(lambda:Path(env['SWAYSOCK']).is_socket(),'private compositor IPC',compositor)
 env['WAYLAND_DISPLAY']=wait(lambda:next((p.name for p in Path('/run/fixture').glob('wayland-*') if p.is_socket()),None),'private display',compositor)
 if a.dialog_tools:
  from office_fixture import dialogs
  facts['privateChooser']=dialogs.start(env,a.dialog_tools,launch,wait)
 with (out/'keyboard.log').open('w') as log:keyboard=PhysicalKeyboard(a.keyboard_tool,env,log)
 processes.append(keyboard.process)
 if a.pdf_viewer:
  from office_fixture.pdf_viewer import configure
  facts['pdfViewer']=configure(env,a.pdf_viewer,a.xdg_tools,a.mime_data)
  from application_fixture.accessibility import start as start_accessibility
  accessibility=start_accessibility(Path(a.at_spi_prefix),env,launch,wait)
 provider=launch([app,str(source)],name);window=wait(native,'exact profile child maps a native window',provider);pid=live_pid();assert pid==window['pid']
 facts['native']=dict(conId=window['id'],pid=pid,processStart=Path('/proc/'+str(pid)+'/stat').read_text().rsplit(') ',1)[1].split()[19],executable=os.readlink('/proc/'+str(pid)+'/exe'),profileWrapperPid=provider.pid)
 command(['swaymsg','[con_id='+str(window['id'])+'] focus']);wait(lambda:any(node.get('id')==window['id'] and node.get('focused') for node in walk(tree())),'captured native focus',provider);wait(listener,'private live provider listener',provider)
 target='127.0.0.1:'+str(port) if a.app=='writer' else str(port);control=Control([cli,'mcp','--connect',target],out/'control.log',env);facts['initialize']=control.initialize();initial=control.command(a.app,'file.info' if a.app=='writer' else 'document.inspect');facts['initialLiveDocument']=initial
 if a.app=='writer':assert str(source) in json.dumps(initial),'Live controller is not bound to GUI document'
 elif a.app=='sheet':assert 'Summary' in json.dumps(initial),'Live controller returned a default headless model'
 else:assert 'Synthetic title' in json.dumps(initial),'Live controller returned a default headless model'
 checks.append('Installed profile wrapper, exact native PID/conId and private live document agree')
 if a.document_probe:
  from office_fixture.document_actions import inspect_print
  facts.update(documentProbeCli=cli,documentProbePort=port,documentPdfTools=a.pdf_tools)
  pointer=launch([a.pointer_tool,'pointer','move','0','0'],'pointer',stdin=subprocess.PIPE,text=True)
  inspect_print(a.app,control,key,command,wait,provider,source,out,facts,checks,pointer_events,tree,walk,accessibility)
 else:
  if a.app=='writer':
   marker=' Native keyboard fixture edit'
   key('-M','ctrl','-k','End','-m','ctrl');key('-d','25','--',marker)
   facts['typedLiveText']=wait(lambda: (value if marker.strip() in json.dumps(value:=control.tool('get_text')) else None),'complete physical text is acknowledged in live document',provider)
   changed=wait(lambda: (value if (value:=control.command('writer','file.info')).get('dirty') else None),'physical typing marks live document dirty',provider)
   assert formats.digest(source)==before,'Typing changed saved source before explicit Save'
   facts['dirtyLiveDocument']=changed;facts['typedText']=marker
   key('-M','ctrl','-k','s','-m','ctrl')
   wait(lambda:formats.digest(source)!=before,'physical Save changes owned source',provider)
   saved=formats.writer(source);assert marker.strip() in json.dumps(saved),'Saved DOCX lacks physical keyboard edit'
   facts['savedSha256']=formats.digest(source);facts['savedDocument']=saved
   checks.append('Physical keyboard edit stays dirty until explicit Save and appears in independently parsed DOCX')
   reopen_saved()
   if not a.new_saveas:
    facts['uiBeforeClose']=control.tool('ui_inspect')
    key('-d','25','--',' Unsaved close fixture')
    wait(lambda:'Unsaved close fixture' in json.dumps(control.tool('get_text')),'second edit acknowledged',provider)
    saved_hash=formats.digest(source)
    command(['swaymsg','[con_id='+str(window['id'])+'] kill'])
    dialog=wait(lambda:(value if (value:=control.tool('ui_inspect')).get('dialog') else None),'native close presents dirty-document decision',provider)
    facts['dirtyCloseDialog']=dialog
    assert formats.digest(source)==saved_hash,'Close prompt silently overwrote source'
    command(['grim',str(out/'dirty-close.png')])
    assert dialog['dialog'].get('dialog')=='saveChanges'
    key('-k','Escape')
    wait(lambda:not control.tool('ui_inspect').get('dialog'),'full-US Escape cancels dirty close',provider)
    assert native() and formats.digest(source)==saved_hash
    checks.append('Physical full-US Escape cancels dirty close without changing source')
    command(['swaymsg','[con_id='+str(window['id'])+'] kill'])
    wait(lambda:control.tool('ui_inspect').get('dialog'),'second captured native close prompts',provider)
    pointer=launch([a.pointer_tool,'pointer','move','0','0'],'pointer',stdin=subprocess.PIPE,text=True)
    command(['swaymsg','seat','seat0','cursor','set',str({'cancel':861,'discard':787,'save':719}[a.writer_close_outcome]),'511'])
    for event in ['m 0.5 0.5','p','r']:
     pointer.stdin.write(event+'\n');pointer.stdin.flush();time.sleep(.15)
    if a.writer_close_outcome=='cancel':
     wait(lambda:not control.tool('ui_inspect').get('dialog'),'physical Cancel button cancels dirty close',provider)
     assert native() and 'Unsaved close fixture' in json.dumps(control.tool('get_text')) and formats.digest(source)==saved_hash
     checks.append('Dirty native close presents Save/Discard/Cancel and physical Cancel preserves document and saved source')
    else:
     def released_document():
      if provider.poll() is not None or not any(node.get('id')==window['id'] for node in walk(tree())):return True
      info=control.command('writer','file.info')
      return info.get('path')!=str(source)
     wait(released_document,'captured native close releases its document')
     persisted=formats.writer(source)
     if a.writer_close_outcome=='discard':assert formats.digest(source)==saved_hash and 'Unsaved close fixture' not in json.dumps(persisted)
     else:assert formats.digest(source)!=saved_hash and 'Unsaved close fixture' in json.dumps(persisted)
     facts['closeOutcome']=dict(choice=a.writer_close_outcome,savedSha256=formats.digest(source),tree=tree())
     checks.append('Physical '+a.writer_close_outcome+' close releases document with independently validated saved-file outcome')
  if a.app=='sheet':
   marker='Native sheet edit'
   pointer=launch([a.pointer_tool,'pointer','move','0','0'],'pointer',stdin=subprocess.PIPE,text=True)
   command(['swaymsg','seat','seat0','cursor','set','156','273'])
   for event in ['m 0.5 0.5','p','r']:
    pointer.stdin.write(event+'\n');pointer.stdin.flush();time.sleep(.15)
   facts['cellBeforeEdit']=wait(lambda:(value if (value:=control.command('sheet','document.inspect'))['activeCell']=='C1' else None),'physical pointer acknowledges exact C1 cell',provider)
   key('-d','25','--',marker);key('-k','Return')
   facts['typedCell']=wait(lambda:(value if marker in json.dumps(value:=control.tool('get_cell',dict(cell='Summary!C1'))) else None),'physical spreadsheet cell entry acknowledged',provider)
   assert formats.digest(source)==before
   key('-M','ctrl','-k','s','-m','ctrl')
   wait(lambda:formats.digest(source)!=before,'physical spreadsheet Save changes source',provider)
   saved=formats.spreadsheet(source);assert marker in json.dumps(saved)
   facts['savedSha256']=formats.digest(source);facts['savedDocument']=saved
   checks.append('Physical cell entry stays unsaved until explicit Save and appears in independently parsed workbook')
   reopen_saved()
  if a.app=='slides':
   marker=' Native slide edit'
   facts['slideBeforeEdit']=control.tool('inspect_slide',dict(index=0))
   pointer=launch([a.pointer_tool,'pointer','move','0','0'],'pointer',stdin=subprocess.PIPE,text=True)
   command(['swaymsg','seat','seat0','cursor','set','520','320'])
   for event in ['m 0.5 0.5','p','r','p','r']:
    pointer.stdin.write(event+'\n');pointer.stdin.flush();time.sleep(.12)
   key('-k','End');key('--',marker)
   facts['typedSlide']=wait(lambda:(value if marker.strip() in json.dumps(value:=control.tool('inspect_slide',dict(index=0))) else None),'physical presentation text acknowledged',provider)
   assert formats.digest(source)==before
   key('-M','ctrl','-k','s','-m','ctrl')
   wait(lambda:formats.digest(source)!=before,'physical presentation Save changes source',provider)
   saved=formats.presentation(source);assert marker.strip() in json.dumps(saved)
   facts['savedSha256']=formats.digest(source);facts['savedDocument']=saved
   checks.append('Physical presentation text edit is acknowledged live and independently parsed after explicit Save')
   reopen_saved()
  if a.new_saveas:finish_new_saveas()
  if (a.close_probe or a.slides_close_outcome or a.sheet_close_outcome) and a.app in ('sheet','slides'):inspect_dirty_close()
  if a.sheet_close_outcome:
   assert a.app=='sheet'
   finish_sheet_close()
  if a.slides_close_outcome:
   assert a.app=='slides'
   finish_slides_close()
 if a.effects:
  renderer_log=(out/'sway.log').read_text()
  required=('Creating scenefx FX renderer','Using OpenGL ES','GL renderer: llvmpipe','FX RENDERER: Shaders Initialized Successfully','Trying udmabuf allocator','Created GL FBO for buffer')
  assert all(value in renderer_log for value in required),'Software effects renderer proof missing'
  facts['softwareEffectsProof']=list(required)
  checks.append('No-DRI software effects compositor initializes shaders, udmabuf and real rendered framebuffers')
 command(['grim',str(out/'opened.png')]);
 from PIL import Image
 paint_path=out/(facts.get('completedPaint','reopened.png') if facts.get('nativeCloseCompleted') else 'opened.png');facts['paintEvidence']=paint_path.name
 with Image.open(paint_path) as painted:
  colors=painted.convert('RGB').getcolors(painted.width*painted.height);nonblack=sum(count for count,color in colors if color!=(0,0,0));assert nonblack>painted.width*painted.height//4,'Native application paint is blank';facts['paintedNonBlackPixels']=nonblack
 (out/'tree-opened.json').write_text(json.dumps(tree(),indent=2));(out/'result.json').write_text(json.dumps(dict(passed=True,phase='native_partial' if len(checks)>1 else 'startup_only',app=a.app,workflowComplete=False,checks=checks,facts=facts,source=a.source_sha),indent=2))
except BaseException:
 (out/'failure-facts.json').write_text(json.dumps(facts,indent=2))
 try:(out/'failure-tree.json').write_text(json.dumps(tree(),indent=2));command(['grim',str(out/'failure.png')])
 except (OSError,ValueError,subprocess.SubprocessError):pass
 raise
finally:
 if accessibility is not None:accessibility.close()
 if keyboard is not None:keyboard.close()
 if control is not None:control.close()
 for process in reversed(processes):
  if process.poll() is None:os.killpg(process.pid,signal.SIGTERM)
 for process in reversed(processes):
  try:process.wait(timeout=5)
  except subprocess.TimeoutExpired:os.killpg(process.pid,signal.SIGKILL);process.wait(timeout=5)
 close_reaped_pipes(processes)
