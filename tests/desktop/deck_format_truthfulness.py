"""Installed presentation format refusal in a disposable application boundary."""
import argparse, hashlib, json, os, subprocess, sys, traceback
from pathlib import Path
from application_fixture.isolation import run as isolate
from office_fixture import corpus, formats

p=argparse.ArgumentParser();p.add_argument('--slides',required=True);p.add_argument('--output',type=Path,required=True);p.add_argument('--inside',action='store_true');p.add_argument('--source-sha',default='');a=p.parse_args()
if not a.inside:
 source=Path(__file__).resolve().parents[2];tool=Path(a.slides).resolve(strict=True)
 assert str(tool).startswith('/nix/store/') and os.access(tool,os.X_OK)
 output=a.output.resolve();output.mkdir(mode=0o700,parents=True,exist_ok=False)
 command=[sys.executable,'/source/tests/desktop/deck_format_truthfulness.py','--inside','--slides',str(tool),'--output','/artifacts','--source-sha',subprocess.check_output(['git','rev-parse','HEAD'],cwd=source,text=True).strip()]
 raise SystemExit(isolate(source,output,command))
assert os.getuid()!=0 and os.getpid()==1 and Path.home()==Path('/home/fixture') and not Path('/dev/dri').exists()
out=Path('/artifacts');checks=[];commands=[];report=dict(passed=False,source=a.source_sha,checks=checks,commands=commands,scope='Format truthfulness and source preservation; no additional format engine or fidelity claim')
def call(label,args):
 result=subprocess.run([a.slides,*args],capture_output=True,text=True,timeout=60)
 for kind in ('stdout','stderr'):(out/(label+'.'+kind)).write_text(getattr(result,kind))
 commands.append(dict(label=label,argv=args,returncode=result.returncode))
 return result
def actions(items):return [value for name,data in items for value in ('--cmd',name+'='+json.dumps(data,separators=(',',':')))]
try:
 source=out/'source.pptx';r=call('create',['run',*actions(corpus.create()['deck']),'--save',str(source)]);assert r.returncode==0 and formats.package_kind(source)=='pptx'
 original=formats.digest(source)
 for label,name,data in [('inferred','file.saveAs',{}),('explicit-native','file.export',{'format':'deckcraft'}),('unknown-format','file.export',{'format':'imaginary'})]:
  target=out/(label+'.odp');target.write_bytes(b'Existing destination sentinel');before=formats.digest(target)
  r=call(label,['run','--in',str(source),*actions([(name,dict(data,path=str(target)))])])
  assert 'unknown option' not in r.stderr and 'unknown command' not in r.stderr, 'Fixture invocation did not reach provider format dispatch'
  checks.append(dict(name=label,passed=r.returncode!=0 and formats.digest(target)==before and formats.digest(source)==original,returncode=r.returncode))
 for suffix,kind in [('deckcraft','deck-native'),('slidecraft','deck-native'),('PPTX','pptx'),('potx','pptx'),('ppsx','pptx'),('pdf','pdf')]:
  target=out/('copy.'+suffix);r=call('convert-'+suffix,['convert',str(source),str(target)])
  actual=formats.package_kind(target) if target.exists() else None
  checks.append(dict(name='declared-'+suffix,passed=r.returncode==0 and actual==kind and formats.digest(source)==original,actual=actual,expected=kind))
 target=out/'unsupported.odp';r=call('convert-odp',['convert',str(source),str(target)])
 checks.append(dict(name='unsupported-convert-has-no-output',passed=r.returncode!=0 and not target.exists() and formats.digest(source)==original,returncode=r.returncode))
 report.update(passed=all(row['passed'] for row in checks),provider=dict(path=a.slides,sha256=formats.digest(a.slides)),testSourceSha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest())
except BaseException as error:report.update(error=str(error),traceback=traceback.format_exc())
finally:(out/'result.json').write_text(json.dumps(report,indent=2)+'\n')
raise SystemExit(0 if report['passed'] else 1)
