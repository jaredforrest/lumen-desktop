"""Installed provider document-version ownership in a disposable boundary."""
import argparse,hashlib,json,os,subprocess,sys,traceback
from pathlib import Path
from application_fixture.isolation import run as isolate
from office_fixture import formats
p=argparse.ArgumentParser();p.add_argument('--writer',required=True);p.add_argument('--output',type=Path,required=True);p.add_argument('--inside',action='store_true');p.add_argument('--source-sha',default='');a=p.parse_args()
if not a.inside:
 source=Path(__file__).resolve().parents[2];tool=Path(a.writer).resolve(strict=True)
 if not str(tool).startswith('/nix/store/') or not tool.is_file() or not os.access(tool,os.X_OK):raise ValueError('Immutable installed command-line provider required')
 output=a.output.resolve();output.mkdir(mode=0o700,parents=True,exist_ok=False)
 command=[sys.executable,'/source/tests/desktop/word_version_ownership.py','--inside','--writer',str(tool),'--output','/artifacts','--source-sha',subprocess.check_output(['git','rev-parse','HEAD'],cwd=source,text=True).strip()]
 raise SystemExit(isolate(source,output,command))
assert os.getuid()!=0 and os.getpid()==1 and Path.home()==Path('/home/fixture') and not Path('/dev/dri').exists()
out=Path('/artifacts');commands=[];checks=[];report=dict(passed=False,source=a.source_sha,checks=checks,commands=commands,scope='Session snapshots only; no persistent-history or native version-browser claim')
def actions(items):return [value for name,data in items for value in ('--cmd',name+'='+json.dumps(data,separators=(',',':')))]
def call(label,items,*,source=None,target=None):
 argv=[a.writer,'run']+(['--file',str(source)] if source else [])+actions(items)+(['--save',str(target)] if target else [])+['--print']
 result=subprocess.run(argv,capture_output=True,text=True,timeout=30)
 (out/(label+'.stdout')).write_text(result.stdout);(out/(label+'.stderr')).write_text(result.stderr)
 commands.append(dict(label=label,argv=argv,returncode=result.returncode,stdoutSha256=hashlib.sha256(result.stdout.encode()).hexdigest(),stderrSha256=hashlib.sha256(result.stderr.encode()).hexdigest()))
 return result
def lists(result,name='file.versions'):
 return [json.loads(line.split(': ',1)[1]) for line in result.stdout.splitlines() if line.startswith(name+': ')]
def text(path):return '\n'.join(row['text'] for row in formats.writer(path)['paragraphs'])
def check(name,passed,**facts):checks.append(dict(name=name,passed=bool(passed),facts=facts))
try:
 first=out/'first.docx';second=out/'second.docx'
 for label,path,marker in [('first',first,'First document marker'),('second',second,'Second document marker')]:
  result=call('create-'+label,[('text.insert',dict(text=marker))],target=path);assert result.returncode==0 and marker in text(path)
 hashes={str(path):formats.digest(path) for path in (first,second)}
 prefix=[('file.versions',dict(save='First only'))]
 for name,transition,marker in [('new',[('file.new',{}),('text.insert',dict(text='New document marker'))],'New document marker'),('open',[('file.open',dict(path=str(second)))],'Second document marker')]:
  target=out/(name+'-copy.docx');result=call(name+'-list',prefix+transition+[('file.versions',{}),('file.recover',{})],source=first,target=target)
  version_lists=lists(result);recovery_lists=lists(result,'file.recover');actual=text(target) if target.exists() else None
  check(name+' document has no versions owned by its predecessor',result.returncode==0 and version_lists[-1:]==[[]] and recovery_lists==[[]] and actual is not None and marker in actual and 'First document marker' not in actual,versions=version_lists,recovery=recovery_lists,document=actual)
  refused=out/(name+'-misleading-restore.docx');result=call(name+'-restore',prefix+transition+[('file.versions',dict(restore=0))],source=first,target=refused)
  check(name+' refuses a previous-document restore without misleading output',result.returncode!=0 and not refused.exists(),returncode=result.returncode,outputExists=refused.exists())
 target=out/'same-document-restored.docx';items=[('file.versions',dict(save='First')),('caret.docEnd',{}),('text.insert',dict(text=' Edited marker')),('file.versions',dict(restore=0)),('file.versions',dict(save='Second')),('caret.docEnd',{}),('text.insert',dict(text=' Edited marker')),('file.versions',dict(restore=0)),('file.versions',{}),('file.versions',dict(restore=0)),('file.versions',{}),('file.recover',{})]
 result=call('same-document',items,source=first,target=target);version_lists=lists(result);expected=['First','Second'];actual=text(target) if target.exists() else None
 check('Repeated same-document restores retain both owned snapshots and complete content',result.returncode==0 and actual is not None and 'First document marker' in actual and 'Edited marker' not in actual and all([row['label'] for row in value]==expected for value in version_lists[-4:]) and [row['label'] for row in lists(result,'file.recover')[-1]]==expected,versions=version_lists,document=actual)
 target=out/'failed-open-output.docx';result=call('failed-open',prefix+[('file.open',dict(path=str(out/'missing.docx')))],source=first,target=target)
 check('Failed open refuses output and leaves both saved sources unchanged',result.returncode!=0 and not target.exists() and all(formats.digest(path)==value for path,value in hashes.items()),returncode=result.returncode,sourceHashes=hashes)
 report.update(passed=all(row['passed'] for row in checks),provider=dict(executable=a.writer,sha256=formats.digest(a.writer)),testSourceSha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest())
except BaseException as error:report.update(error=str(error),traceback=traceback.format_exc())
finally:(out/'result.json').write_text(json.dumps(report,indent=2)+'\n')
raise SystemExit(0 if report['passed'] else 1)
