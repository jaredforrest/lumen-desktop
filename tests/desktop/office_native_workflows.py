"""Private installed office windows with real compositor focus and physical typing."""
import argparse,hashlib,json,os,re,signal,socket,subprocess,sys,time,traceback
from pathlib import Path
from application_fixture.isolation import run as isolate
from application_fixture.keyboard import PhysicalKeyboard
from fixture_resources import close_reaped_pipes
from office_fixture import corpus,formats,live
from office_fixture.control import Control
p=argparse.ArgumentParser();p.add_argument('--writer',required=True);p.add_argument('--sheet',required=True);p.add_argument('--slides',required=True);p.add_argument('--software-driver',required=True);p.add_argument('--sway',required=True);p.add_argument('--keyboard-tool',required=True);p.add_argument('--pointer-tool',required=True);p.add_argument('--effects',action='store_true');p.add_argument('--writer-close-outcome',choices=['cancel','discard','save'],default='cancel');p.add_argument('--app',choices=['writer','sheet','slides'],default='writer');p.add_argument('--output',type=Path,required=True);p.add_argument('--inside',action='store_true');p.add_argument('--profiles-json',default='');p.add_argument('--source-sha',default='');a=p.parse_args()
if not a.inside:
 source=Path(__file__).resolve().parents[2];out=a.output.resolve();out.mkdir(mode=0o700,parents=True,exist_ok=False)
 for binary in [a.writer,a.sheet,a.slides,a.sway,a.pointer_tool,a.keyboard_tool]:
  path=Path(binary).resolve();assert str(path).startswith('/nix/store/') and path.is_file() and os.access(path,os.X_OK),'Exact installed application/test binaries required'
 policies=subprocess.check_output(['nix','eval','--json','--file',str(source/'config/application-profiles.nix')],text=True)
 sha=subprocess.check_output(['git','-C',str(source),'rev-parse','HEAD'],text=True).strip()
 args=[sys.executable,'/source/tests/desktop/office_native_workflows.py','--inside','--writer',a.writer,'--sheet',a.sheet,'--slides',a.slides,'--software-driver',a.software_driver,'--sway',a.sway,'--keyboard-tool',a.keyboard_tool,'--pointer-tool',a.pointer_tool,'--writer-close-outcome',a.writer_close_outcome,'--app',a.app,'--profiles-json',policies,'--source-sha',sha,'--output','/artifacts']
 if a.effects:args.append('--effects')
 raise SystemExit(isolate(source,out,args,software_graphics=a.effects))
assert os.getpid()==1 and os.getuid()!=0 and os.uname().nodename=='fixture' and Path.home()==Path('/home/fixture')
assert not any((Path('/dev')/name).exists() for name in ('dri','snd','video0')) and Path('/dev/udmabuf').exists()==a.effects
out=Path('/artifacts');processes=[];control=None;keyboard=None;checks=[];facts={};env=dict(os.environ)
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
 with (out/'keyboard.log').open('w') as log:keyboard=PhysicalKeyboard(a.keyboard_tool,env,log)
 processes.append(keyboard.process)
 provider=launch([app,str(source)],name);window=wait(native,'exact profile child maps a native window',provider);pid=live_pid();assert pid==window['pid']
 facts['native']=dict(conId=window['id'],pid=pid,processStart=Path('/proc/'+str(pid)+'/stat').read_text().rsplit(') ',1)[1].split()[19],executable=os.readlink('/proc/'+str(pid)+'/exe'),profileWrapperPid=provider.pid)
 command(['swaymsg','[con_id='+str(window['id'])+'] focus']);wait(lambda:any(node.get('id')==window['id'] and node.get('focused') for node in walk(tree())),'captured native focus',provider);wait(listener,'private live provider listener',provider)
 target='127.0.0.1:'+str(port) if a.app=='writer' else str(port);control=Control([cli,'mcp','--connect',target],out/'control.log',env);facts['initialize']=control.initialize();initial=control.command(a.app,'file.info' if a.app=='writer' else 'document.inspect');facts['initialLiveDocument']=initial
 if a.app=='writer':assert str(source) in json.dumps(initial),'Live controller is not bound to GUI document'
 elif a.app=='sheet':assert 'Summary' in json.dumps(initial),'Live controller returned a default headless model'
 else:assert 'Synthetic title' in json.dumps(initial),'Live controller returned a default headless model'
 checks.append('Installed profile wrapper, exact native PID/conId and private live document agree')
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
 if a.effects:
  renderer_log=(out/'sway.log').read_text()
  required=('Creating scenefx FX renderer','Using OpenGL ES','GL renderer: llvmpipe','FX RENDERER: Shaders Initialized Successfully','Trying udmabuf allocator','Created GL FBO for buffer')
  assert all(value in renderer_log for value in required),'Software effects renderer proof missing'
  facts['softwareEffectsProof']=list(required)
  checks.append('No-DRI software effects compositor initializes shaders, udmabuf and real rendered framebuffers')
 command(['grim',str(out/'opened.png')]);
 from PIL import Image
 with Image.open(out/'opened.png') as painted:
  colors=painted.convert('RGB').getcolors(painted.width*painted.height);nonblack=sum(count for count,color in colors if color!=(0,0,0));assert nonblack>painted.width*painted.height//4,'Native application paint is blank';facts['paintedNonBlackPixels']=nonblack
 (out/'tree-opened.json').write_text(json.dumps(tree(),indent=2));(out/'result.json').write_text(json.dumps(dict(passed=True,phase='native_partial' if len(checks)>1 else 'startup_only',app=a.app,workflowComplete=False,checks=checks,facts=facts,source=a.source_sha),indent=2))
finally:
 if keyboard is not None:keyboard.close()
 if control is not None:control.close()
 for process in reversed(processes):
  if process.poll() is None:os.killpg(process.pid,signal.SIGTERM)
 for process in reversed(processes):
  try:process.wait(timeout=5)
  except subprocess.TimeoutExpired:os.killpg(process.pid,signal.SIGKILL);process.wait(timeout=5)
 close_reaped_pipes(processes)
