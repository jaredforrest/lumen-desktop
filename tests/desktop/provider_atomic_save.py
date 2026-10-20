"""Actual installed provider save publication; private files and no UI or host documents."""
import argparse,hashlib,json,os,stat,subprocess as sp,sys,time,zipfile
from pathlib import Path
from xml.etree import ElementTree
from application_fixture import isolation,offscreen
from office_fixture import corpus
p=argparse.ArgumentParser();p.add_argument('--wordcraft',type=Path,required=True);p.add_argument('--gridcraft',type=Path,required=True);p.add_argument('--deckcraft',type=Path,required=True);p.add_argument('--output',type=Path,required=True);p.add_argument('--profiles-json',required=True);p.add_argument('--inside',action='store_true');a=p.parse_args();repo=Path(__file__).resolve().parents[2]
tools={name:offscreen.immutable_file(getattr(a,name)) for name in ('wordcraft','gridcraft','deckcraft')}
def digest(path):return hashlib.sha256(path.read_bytes()).hexdigest()
if not a.inside:
 assert not sp.check_output(['git','-C',str(repo),'status','--porcelain'],text=True).strip(),'Freeze source before acceptance'
 revision=sp.check_output(['git','-C',str(repo),'rev-parse','HEAD'],text=True).strip();hashes={name:digest(path) for name,path in tools.items()};out=a.output.resolve();out.mkdir(mode=0o700,parents=True,exist_ok=False)
 command=[str(offscreen.immutable_file(sys.executable)),'/source/tests/desktop/provider_atomic_save.py','--inside','--output','/artifacts','--profiles-json',a.profiles_json]+sum(([ '--'+name,str(path)]for name,path in tools.items()),[])
 result=isolation.run(repo,out,command)
 unchanged=sp.check_output(['git','-C',str(repo),'rev-parse','HEAD'],text=True).strip()==revision and not sp.check_output(['git','-C',str(repo),'status','--porcelain'],text=True).strip() and all(digest(tools[name])==value for name,value in hashes.items())
 (out/'provenance.json').write_text(json.dumps(dict(sourceRevision=revision,sourceUnchanged=unchanged,tools={name:str(path) for name,path in tools.items()},toolHashes=hashes,exitCode=result),indent=2)+'\n');assert unchanged;raise SystemExit(result)
assert repo==Path('/source') and os.environ['HOME']=='/home/fixture' and os.getuid()!=0 and not Path('/.old-root').exists()
profiles=json.loads(a.profiles_json);assert isinstance(profiles,dict) and all(isinstance(profiles.get(name),dict) for name in tools)
settings=Path('/etc/nix');settings.mkdir();(settings/'desktop-settings.json').write_text(json.dumps(dict(applicationPreferences=profiles)))
out=a.output;fixture=corpus.create();records=[]
def commands(values):return [arg for name,value in values for arg in ('--cmd',name+'='+json.dumps(value))]
def invoke(tool,values,save,flag,source=None,sourceflag='--in',timeout=30,label='run'):
 argv=[str(tool),'run']+([sourceflag,str(source)]if source else [])+commands(values)+[flag,str(save)];start=time.monotonic();result=sp.run(argv,text=True,capture_output=True,timeout=timeout);(out/(label+'.stdout')).write_text(result.stdout);(out/(label+'.stderr')).write_text(result.stderr);return result,dict(command=argv,exitCode=result.returncode,elapsedSeconds=time.monotonic()-start)
def content(path,main,marker):
 with zipfile.ZipFile(path) as archive:
  assert main in archive.namelist() and len(archive.infolist())<4096 and sum(info.file_size for info in archive.infolist())<64*1024*1024
  assert archive.testzip()is None
  text=' '.join(' '.join(ElementTree.fromstring(archive.read(name)).itertext())for name in archive.namelist()if name.endswith('.xml'))
  assert marker in text,(str(path),marker)
for app,key,extension,suffix,edit,main in [('wordcraft','writer','docx','.tmp',('text.insert',{'text':'Edited copy marker'}),'word/document.xml'),('gridcraft','grid','xlsx','.sctmp',('cell.set',{'cell':'A1','input':'Edited copy marker'}),'xl/workbook.xml'),('deckcraft','deck','pptx','.saving',('slide.notes',{'text':'Edited copy marker'}),'ppt/presentation.xml')]:
 tool=tools[app];original=out/(app+'-original.'+extension);flag='--out'if app=='gridcraft'else'--save';sourceflag='--file'if app=='wordcraft'else'--in';created,_=invoke(tool,fixture[key],original,flag,label=app+'-create');assert created.returncode==0,created.stderr;before=digest(original)
 for kind in ('legacy-hardlink','legacy-symlink','destination-hardlink','destination-symlink'):
  target=out/(app+'-'+kind+'.'+extension);alias=Path(str(target)+suffix)
  if kind.startswith('legacy'):
   if kind=='legacy-hardlink':os.link(original,alias)
   else:alias.symlink_to(original)
  elif kind=='destination-hardlink':os.link(original,target)
  else:target.symlink_to(original)
  result,row=invoke(tool,[edit],target,flag,original,sourceflag,label=app+'-'+kind);row.update(app=app,case=kind,originalBefore=before,originalAfter=digest(original));assert result.returncode==0,result.stderr;assert digest(original)==before;content(target,main,'Edited copy marker')
  if kind.startswith('legacy'):
   assert digest(alias)==before and alias.is_symlink()==(kind=='legacy-symlink')
   assert stat.S_IMODE(target.stat().st_mode)==0o600
   row['newFilePrivate']=True
  assert not target.is_symlink() and target.stat().st_ino!=original.stat().st_ino;records.append(row)
 fifo=out/(app+'-fifo');os.mkfifo(fifo,mode=0o600);result,row=invoke(tool,[edit],fifo/('copy.'+extension),flag,original,sourceflag,timeout=5,label=app+'-fifo');assert result.returncode!=0 and digest(original)==before;row.update(app=app,case='non-directory-parent-refused',originalUnchanged=True);records.append(row)
 directory=out/(app+'-directory.'+extension);directory.mkdir();result,row=invoke(tool,[edit],directory,flag,original,sourceflag,label=app+'-directory');assert result.returncode!=0 and directory.is_dir() and digest(original)==before;assert not list(out.glob('.lumen-save-*'));row.update(app=app,case='failed-publication-cleaned',originalUnchanged=True);records.append(row)
 original.chmod(0o640);result,row=invoke(tool,[edit],original,flag,original,sourceflag,label=app+'-ordinary-save');assert result.returncode==0,result.stderr;content(original,main,'Edited copy marker');assert stat.S_IMODE(original.stat().st_mode)==0o640;row.update(app=app,case='ordinary-save',existingPermissionsPreserved=True);records.append(row)
(out/'result.json').write_text(json.dumps(dict(passed=True,cases=records,scope='Actual installed CLI native saves and save copies; deterministic existing aliases/failure paths, no hostile same-user or power-loss guarantee'),indent=2)+'\n')
