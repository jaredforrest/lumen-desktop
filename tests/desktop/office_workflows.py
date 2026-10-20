"""Isolated installed office round trips; capability reports are not a parity verdict."""
import argparse,hashlib,json,os,subprocess,sys,traceback
from pathlib import Path
from office_fixture import corpus,formats
from application_fixture.isolation import run as isolate

p=argparse.ArgumentParser();p.add_argument('--writer',required=True);p.add_argument('--sheet',required=True);p.add_argument('--slides',required=True);p.add_argument('--pdf-tools',required=True);p.add_argument('--output',type=Path,required=True);p.add_argument('--inside',action='store_true');p.add_argument('--source-sha',default='');a=p.parse_args()
if not a.inside:
 output=a.output.resolve();output.mkdir(mode=0o700,parents=True,exist_ok=False);source=Path(__file__).resolve().parents[2]
 sha=subprocess.check_output(['git','-C',str(source),'rev-parse','HEAD'],text=True).strip()
 for binary in [a.writer,a.sheet,a.slides,*[str(Path(a.pdf_tools)/'bin'/name) for name in ('pdfinfo','pdftotext','pdftoppm')]]:
  path=Path(binary).resolve();assert str(path).startswith('/nix/store/') and path.is_file() and os.access(path,os.X_OK),'Use exact pinned installed tools'
 command=[sys.executable,'/source/tests/desktop/office_workflows.py','--inside','--writer',a.writer,'--sheet',a.sheet,'--slides',a.slides,'--pdf-tools',a.pdf_tools,'--output','/artifacts','--source-sha',sha]
 raise SystemExit(isolate(source,output,command))
assert os.getuid()!=0 and os.getpid()==1 and os.uname().nodename=='fixture' and Path.home()==Path('/home/fixture'),'Private application boundary required'
OUT=Path('/artifacts');commands=[];capabilities=[];fixture=corpus.create();(OUT/'synthetic.png').write_bytes(fixture['image'])
apps={'writer':a.writer,'sheet':a.sheet,'slides':a.slides};inspect={'writer':formats.writer,'sheet':formats.spreadsheet,'slides':formats.presentation};extension={'writer':'docx','sheet':'xlsx','slides':'pptx'}
def call(label,command,required=True):
 result=subprocess.run(command,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True,timeout=60)
 stem=OUT/label;stem.with_suffix('.stdout').write_text(result.stdout);stem.with_suffix('.stderr').write_text(result.stderr)
 commands.append(dict(label=label,command=command,returncode=result.returncode,stdoutSha256=hashlib.sha256(result.stdout.encode()).hexdigest(),stderrSha256=hashlib.sha256(result.stderr.encode()).hexdigest()))
 if required and result.returncode:raise AssertionError(label+' failed; inspect retained stderr')
 return result

def action_list(items):return [value for name,data in items for value in ('--cmd',name+'='+json.dumps(data,separators=(',',':')))]
def capability(id,action,status,**evidence):capabilities.append(dict(id=id,action=action,status=status,**evidence))
def assert_source(path,expected):assert formats.digest(path)==expected,'Source mutated: '+str(path)
def pdf(app,source,label,expected_text):
 before=formats.digest(source);target=OUT/(label+'.pdf')
 if app=='sheet':call(label,[apps[app],'run','--in',str(source),*action_list([('file.exportPdf',dict(path=str(target),sheets='all'))]),'--quiet'])
 else:call(label,[apps[app],'convert',str(source),str(target)])
 assert_source(source,before);assert formats.package_kind(target)=='pdf'
 metadata=call(label+'-info',[str(Path(a.pdf_tools)/'bin/pdfinfo'),str(target)]).stdout
 pages=int(next(line.split(':',1)[1] for line in metadata.splitlines() if line.startswith('Pages:')));assert pages>0
 text=call(label+'-text',[str(Path(a.pdf_tools)/'bin/pdftotext'),'-layout',str(target),'-']).stdout
 for expected in expected_text:assert expected in text,(label,expected,text)
 call(label+'-raster',[str(Path(a.pdf_tools)/'bin/pdftoppm'),'-scale-to','1000','-png',str(target),str(OUT/(label+'-page'))])
 return dict(path=str(target),sha256=formats.digest(target),pages=pages,text=text,sourceSha256=before,images=[str(path) for path in sorted(OUT.glob(label+'-page-*.png'))])

def compare_rasters(before,after):
 from PIL import Image,ImageChops
 changes=[]
 for index in range(max(len(before),len(after))):
  if index>=len(before) or index>=len(after):changes.append(dict(page=index+1,kind='missing_page'));continue
  with Image.open(before[index]) as x,Image.open(after[index]) as y:
   if x.size!=y.size:changes.append(dict(page=index+1,kind='page_dimensions',before=x.size,after=y.size));continue
   diff=ImageChops.difference(x.convert('RGB'),y.convert('RGB'));box=diff.getbbox()
   if box:
    pixels=diff.tobytes();changes.append(dict(page=index+1,kind='painted_pixels',bounds=box,changedPixels=sum(any(pixels[i:i+3]) for i in range(0,len(pixels),3))))
 return changes

report=dict(isolation=dict(uid=os.getuid(),gid=os.getgid(),hostname=os.uname().nodename,home=str(Path.home()),capabilities=[line.strip() for line in Path('/proc/self/status').read_text().splitlines() if line.startswith(('CapEff:','CapBnd:','NoNewPrivs:'))]),installedTools={path:formats.digest(path) for path in [a.writer,a.sheet,a.slides,*[str(Path(a.pdf_tools)/'bin'/name) for name in ('pdfinfo','pdftotext','pdftoppm')]]},source=a.source_sha,commands=commands,capabilities=capabilities,workflowComplete=False,harnessPassed=False)
try:
 # Version and command metadata come from the exact installed providers.
 for app,command in apps.items():
  call(app+'-version',[command,'version' if app=='sheet' else '--version'])
  call(app+'-commands',[command,'commands',*(['--json'] if app!='slides' else [])])
 sources={};baseline={};copies={};exports={}
 for app,key in [('writer','writer'),('sheet','grid'),('slides','deck')]:
  path=OUT/(app+'-source.'+extension[app]);args=['run',*action_list(fixture[key]),'--save' if app!='sheet' else '--out',str(path)]
  if app=='writer':args+=['--author','Fixture']
  call(app+'-create',[apps[app],*args]);assert formats.package_kind(path)==extension[app]
  sources[app]=path;baseline[app]=inspect[app](path);(OUT/(app+'-facts.json')).write_text(json.dumps(baseline[app],indent=2))
  capability('APP-OFFICE-0'+str(('writer','sheet','slides').index(app)+1),'create '+extension[app],'fixture_verified',facts=str(OUT/(app+'-facts.json')))
 writer=baseline['writer'];assert len(writer['sections'])==2 and len(writer['tables'])==1 and writer['media'] and writer['stories'] and writer['comments'] and writer['changes'] and writer['links'];assert any(run['format'].get('b') is not None for paragraph in writer['paragraphs'] for run in paragraph['runs'])
 sheet=baseline['sheet'];assert len(sheet['sheets'])==2 and sheet['charts'] and sheet['names'];data=sheet['sheets'][0];assert data['filter'];assert data['cells']['F2']['numberFormat']=='yyyy-mm-dd';assert data['cells']['E2']['formula']=='SUM(B2:B4)'
 assert sheet['sheets'][1]['cells']['B1']['type']=='e' and sheet['sheets'][1]['cells']['B1']['value']=='#NAME?'
 slides=baseline['slides'];assert len(slides['slides'])==2 and slides['media'] and any('Synthetic speaker notes' in note for note in slides['notes']) and slides['slides'][0]['transitions'] and slides['fonts']
 capability('APP-OFFICE-02','unknown formula feedback','fixture_verified',cell='Summary!B1',value='#NAME?')
 # Save an intermediate edited copy, undo that edit, then save another copy.
 for app in apps:
  source=sources[app];source_hash=formats.digest(source);copy=OUT/(app+'-copy.'+extension[app]);edited=OUT/(app+'-edited.'+extension[app])
  edit={'writer':[('caret.docEnd',{}),('text.insert',dict(text=' Temporary reversible edit.'))], 'sheet':[('cell.set',dict(sheet='Summary',cell='C1',input='Temporary reversible edit'))], 'slides':[('slide.notes',dict(index=0,text='Temporary reversible edit'))]}[app]
  args=['run','--file' if app=='writer' else '--in',str(source),*action_list([*edit,('file.saveAs',dict(path=str(edited))),('edit.undo',{})]),'--save' if app!='sheet' else '--out',str(copy)]
  call(app+'-edit-undo',[apps[app],*args]);assert_source(source,source_hash);assert formats.package_kind(copy)==extension[app];assert inspect[app](edited)!=baseline[app],app+' edit was not observable'
  facts=inspect[app](copy);diff=formats.differences(baseline[app],facts);(OUT/(app+'-roundtrip-differences.json')).write_text(json.dumps(diff,indent=2));copies[app]=copy
  capability('WF-OFFICE-01',extension[app]+' edit/save-copy/reopen/undo','fixture_verified' if not diff else 'degraded',differences=diff,sourceSha256=source_hash)
 for app,phrases in [('writer',['Workflow heading','Second section.']),('sheet',['Item','Amount']),('slides',['Synthetic title','Second slide'])]:
  before=pdf(app,sources[app],app+'-export',phrases);after=pdf(app,copies[app],app+'-copy-export',phrases);differences=compare_rasters(before['images'],after['images']);exports[app]=dict(original=before,copy=after,paintedDifferences=differences)
  capability('APP-OFFICE-05',app+' PDF export/source preservation','fixture_verified',sourceSha256=before['sourceSha256'],pages=before['pages'],copyPages=after['pages'],paintedDifferences=differences)
 # Standard container identity is checked independently of extension and success code.
 original=sources['writer'];source_hash=formats.digest(original);odt=OUT/'writer.odt';back=OUT/'writer-odt-back.docx';call('writer-odt',[a.writer,'convert',str(original),str(odt)]);assert formats.package_kind(odt)=='odt';call('writer-odt-back',[a.writer,'convert',str(odt),str(back)]);assert_source(original,source_hash);diff=formats.differences(baseline['writer'],formats.writer(back));(OUT/'writer-odt-differences.json').write_text(json.dumps(diff,indent=2));capability('APP-OFFICE-01','DOCX to ODT and back','fixture_verified' if not diff else 'degraded',differences=diff)
 odt_before=formats.digest(odt);odt_copy=OUT/'writer-copy.odt';odt_copy_docx=OUT/'writer-odt-copy.docx';call('writer-odt-edit-undo',[a.writer,'run','--file',str(odt),*action_list([('caret.docEnd',{}),('text.insert',dict(text=' Temporary reversible edit.')),('edit.undo',{})]),'--save',str(odt_copy)]);assert_source(odt,odt_before);assert formats.package_kind(odt_copy)=='odt';call('writer-odt-copy-check',[a.writer,'convert',str(odt_copy),str(odt_copy_docx)]);odt_diff=formats.differences(formats.writer(back),formats.writer(odt_copy_docx));capability('APP-OFFICE-01','ODT same-format edit/undo/save-copy/reopen','fixture_verified' if not odt_diff else 'degraded',differences=odt_diff,scope='Starts with the separately measured degraded DOCX-to-ODT fixture; does not erase those conversion losses')
 source_hash=formats.digest(sources['sheet']);result=call('sheet-ods',[a.sheet,'convert',str(sources['sheet']),str(OUT/'sheet.ods')],required=False);assert_source(sources['sheet'],source_hash);capability('APP-OFFICE-02','ODS export','unsupported' if result.returncode else 'format_probe_required',providerReturncode=result.returncode,reason=result.stderr.strip())
 odp=OUT/'slides.odp';source_hash=formats.digest(sources['slides']);result=call('slides-odp',[a.slides,'convert',str(sources['slides']),str(odp)],required=False);assert_source(sources['slides'],source_hash);actual=formats.package_kind(odp) if odp.exists() else 'absent';capability('APP-OFFICE-03','ODP export','provider_defect' if result.returncode==0 and actual!='odp' else 'unsupported' if result.returncode else 'fixture_verified',providerReturncode=result.returncode,expectedFormat='odp',actualFormat=actual,outputSha256=formats.digest(odp) if odp.exists() else None)
 for id,action in [('APP-OFFICE-04','GUI template/recent/recovery and save conflict lifecycle'),('APP-OFFICE-05','GUI format warning cancel and print-dialog acceptance'),('APP-OFFICE-06','collaboration and versions; separate from file synchronization'),('APP-OFFICE-07','interactive PDF selection/search/rotation/annotation/forms/signatures'),('WF-OFFICE-01','GUI open/edit/save/unsaved close and killed-process recovery'),('APP-OFFICE-03','interactive presenter view')]:capability(id,action,'not_yet_verified')
 report.update(harnessPassed=True,exports=exports,formatFacts=baseline,sourceHashes={app:formats.digest(path) for app,path in sources.items()},fixtureModules={str(path.relative_to(Path('/source'))):formats.digest(path) for path in [Path(__file__),*sorted(Path('/source/tests/desktop/office_fixture').glob('*.py')),Path('/source/tests/desktop/application_fixture/isolation.py')]})
except Exception as error:
 report['failure']=str(error);report['traceback']=traceback.format_exc()
finally:(OUT/'result.json').write_text(json.dumps(report,indent=2))
print(json.dumps(dict(harnessPassed=report['harnessPassed'],workflowComplete=False,capabilityRecords=len(capabilities),result='/artifacts/result.json')))
raise SystemExit(0 if report['harnessPassed'] else 1)
