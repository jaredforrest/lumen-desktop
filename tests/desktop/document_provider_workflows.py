"""Installed document print payload and session-version observations, without host services."""
import argparse,hashlib,json,os,subprocess,sys,traceback
from pathlib import Path
from application_fixture.isolation import run as isolate
from office_fixture import formats
p=argparse.ArgumentParser();p.add_argument('--writer',required=True);p.add_argument('--sheet',required=True);p.add_argument('--slides',required=True);p.add_argument('--pdf-tools',required=True);p.add_argument('--output',type=Path,required=True);p.add_argument('--inside',action='store_true');p.add_argument('--source-sha',default='');a=p.parse_args()
if not a.inside:
 source=Path(__file__).resolve().parents[2];out=a.output.resolve();out.mkdir(mode=0o700,parents=True,exist_ok=False)
 for value in (a.writer,a.sheet,a.slides,str(Path(a.pdf_tools)/'bin/pdfinfo'),str(Path(a.pdf_tools)/'bin/pdftotext')):
  path=Path(value).resolve()
  if not str(path).startswith('/nix/store/') or not path.is_file() or not os.access(path,os.X_OK):raise ValueError('Pinned installed executables required')
 command=[sys.executable,'/source/tests/desktop/document_provider_workflows.py','--inside','--writer',a.writer,'--sheet',a.sheet,'--slides',a.slides,'--pdf-tools',a.pdf_tools,'--output','/artifacts','--source-sha',subprocess.check_output(['git','rev-parse','HEAD'],cwd=source,text=True).strip()]
 raise SystemExit(isolate(source,out,command))
assert os.getpid()==1 and os.getuid()!=0 and Path.home()==Path('/home/fixture') and not Path('/dev/dri').exists()
out=Path('/artifacts');checks=[];commands=[];report=dict(passed=False,source=a.source_sha,checks=checks,commands=commands,scope='CLI payload and session-version observations only; no native print dialog or physical printer acceptance')
def call(label,args,success=True):
 result=subprocess.run(args,capture_output=True,text=True,timeout=30)
 (out/(label+'.stdout')).write_text(result.stdout);(out/(label+'.stderr')).write_text(result.stderr)
 commands.append(dict(label=label,argv=args,returncode=result.returncode,stdoutSha256=hashlib.sha256(result.stdout.encode()).hexdigest(),stderrSha256=hashlib.sha256(result.stderr.encode()).hexdigest()))
 if success and result.returncode:raise AssertionError(label+' failed')
 return result

def actions(items):return [arg for name,data in items for arg in ('--cmd',name+'='+json.dumps(data,separators=(',',':')))]
try:
 for name,tool in [('writer',a.writer),('sheet',a.sheet),('slides',a.slides)]:
  call(name+'-version',[tool,'version' if name=='sheet' else '--version']);call(name+'-commands',[tool,'commands',*(['--json'] if name!='slides' else [])])
 source=out/'version-source.docx';copy=out/'restored-copy.docx'
 call('writer-create',[a.writer,'run',*actions([('text.insert',dict(text='Original version marker.'))]),'--save',str(source)])
 before=formats.digest(source)
 result=call('writer-version-restore',[a.writer,'run','--file',str(source),*actions([('file.versions',dict(save='Captured version')),('caret.docEnd',{}),('text.insert',dict(text=' Later unsaved marker.')),('file.versions',dict(restore=0)),('file.versions',{})]),'--save',str(copy),'--print'])
 facts=formats.writer(copy);text='\n'.join(p['text'] for p in facts['paragraphs']);assert 'Original version marker.' in text and 'Later unsaved marker.' not in text and formats.digest(source)==before
 assert 'Captured version' in result.stdout;checks.append('Word processor session snapshot can restore a captured document without mutating the opened source')
 reopened=call('writer-version-reopen',[a.writer,'run','--file',str(copy),*actions([('file.versions',{})]),'--print']);assert 'file.versions: []' in reopened.stdout;checks.append('Session snapshots are not represented as persistent version history after reopening the saved document')
 bad=call('writer-invalid-version',[a.writer,'run','--file',str(source),*actions([('file.versions',dict(restore=99))]),'--save',str(out/'invalid.docx')],False);assert bad.returncode and not (out/'invalid.docx').exists() and formats.digest(source)==before;checks.append('Unavailable session version is refused without publishing a misleading restored document')
 sheet=out/'print-source.xlsx';pdf=out/'printed.pdf'
 call('sheet-create',[a.sheet,'run',*actions([('cell.set',dict(cell='A1',input='Document print fixture')),('cell.set',dict(cell='B2',input='42'))]),'--out',str(sheet),'--quiet']);before=formats.digest(sheet)
 call('sheet-print-file',[a.sheet,'run','--in',str(sheet),*actions([('file.print',dict(path=str(pdf)))]),'--quiet']);assert formats.package_kind(pdf)=='pdf' and formats.digest(sheet)==before
 meta=call('sheet-print-info',[str(Path(a.pdf_tools)/'bin/pdfinfo'),str(pdf)]).stdout;text=call('sheet-print-text',[str(Path(a.pdf_tools)/'bin/pdftotext'),str(pdf),'-']).stdout;assert 'Pages:' in meta and 'Document print fixture' in text and '42' in text;checks.append('Spreadsheet print-to-file produces independently readable PDF content and preserves the source')
 invalid=out/'missing-parent'/'failed.pdf';bad=call('sheet-print-unavailable-file',[a.sheet,'run','--in',str(sheet),*actions([('file.print',dict(path=str(invalid)))]),'--quiet'],False);assert bad.returncode and not invalid.exists() and formats.digest(sheet)==before;checks.append('Unavailable print-file destination is explicitly refused without output or source mutation')
 report.update(passed=True,installedTools={tool:formats.digest(tool) for tool in (a.writer,a.sheet,a.slides)},testSourceSha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest())
except BaseException as error:report.update(error=str(error),traceback=traceback.format_exc())
finally:(out/'result.json').write_text(json.dumps(report,indent=2)+'\n')
raise SystemExit(0 if report['passed'] else 1)
