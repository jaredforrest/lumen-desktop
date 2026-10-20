"""Actual private Qt IPC and byte preflight for the native offer fixture source."""
import argparse,ast,hashlib,json,os,shutil,signal,subprocess as sp,sys,time
from pathlib import Path
from application_fixture import offscreen
from private_native_bus import start_private_bus
from fixture_resources import close_reaped_pipes
p=argparse.ArgumentParser();p.add_argument('--launcher',type=Path,required=True);p.add_argument('--font-config',type=Path,required=True);p.add_argument('--output',type=Path,required=True);p.add_argument('--inside',action='store_true');a=p.parse_args();repo=Path(__file__).resolve().parents[2]
if not a.inside:
 command=[sys.executable,'/source/tests/desktop/dock_offer_source_qml.py','--inside','--launcher',str(a.launcher.resolve()),'--font-config',str(a.font_config.resolve()),'--output','/artifacts/result']
 raise SystemExit(offscreen.run(repo,a.output,a.launcher,a.font_config,command))
env=offscreen.environment();out=a.output;out.mkdir(parents=True,exist_ok=False);children=[];source=repo/'tests/desktop/dock_offer_native.py';matches=[n.value for n in ast.walk(ast.parse(source.read_text())) if isinstance(n,ast.Constant) and isinstance(n.value,str) and 'target:"offerPeer"' in n.value];assert len(matches)==1
qml=out/'peer.qml';qml.write_text(matches[0]);expected=['file:///synthetic/Alpha%20%23%20%C3%BC.txt','file:///synthetic/Beta.txt']
def call(*args):return sp.check_output(['quickshell','-p',str(qml),'ipc',*args],env=env,text=True,timeout=3).strip()
def status():return json.loads(call('call','offerPeer','status'))
try:
 start_private_bus(env,out,children)
 with (out/'qt.log').open('w') as log:process=sp.Popen(['quickshell','-p',str(qml)],env=env,stdout=log,stderr=sp.STDOUT,start_new_session=True)
 children.append(process);deadline=time.monotonic()+5
 while time.monotonic()<deadline:
  try:before=status();break
  except (OSError,ValueError,sp.SubprocessError):time.sleep(.02)
 else:raise AssertionError('Private offer source did not start')
 methods=call('show');(out/'methods.txt').write_text(methods);ack=call('call','offerPeer','configureUris',*expected);after=status();result=dict(ack=ack,before=before,after=after,expected=expected,fixtureSha256=hashlib.sha256(qml.read_bytes()).hexdigest(),sourceFixtureSha256=hashlib.sha256(source.read_bytes()).hexdigest());(out/'observed.json').write_text(json.dumps(result,indent=2));assert ack=='true' and after['uris']==expected and after['mimeBytes']==sum(len(uri)+2 for uri in expected),result
 # Exercise the exact observer against the actual installed GIO drop helper,
 # independently of native input, including permitted URI/local-path argv forms.
 observers=[n.value for n in ast.walk(ast.parse(source.read_text())) if isinstance(n,ast.Constant) and isinstance(n.value,str) and 'for uri in sys.argv[2:]' in n.value];assert len(observers)==1
 observer=out/'observer.py';observer.write_text(observers[0]);opened=out/'opened.jsonl';folder=out/'files';folder.mkdir();first=folder/'Alpha # % ü.txt';second=folder/'Beta.txt';first.write_text('first observer bytes');second.write_text('second observer bytes');uris=[first.as_uri(),second.as_uri()]
 apps=Path(env['XDG_DATA_HOME'])/'applications';apps.mkdir(parents=True,exist_ok=True);env['XDG_DATA_DIRS']=env['XDG_DATA_HOME'];(apps/'fixture-accept.desktop').write_text('[Desktop Entry]'+chr(10)+'Type=Application'+chr(10)+'Name=Private Observer'+chr(10)+'Exec='+sys.executable+' '+str(observer)+' '+str(opened)+' %U'+chr(10)+'MimeType=text/plain;'+chr(10)+'Terminal=false'+chr(10))
 helper=shutil.which('lumen-dock-drop',path=env['PATH']);assert helper and str(Path(helper).resolve()).startswith('/nix/store/')
 run=sp.run([helper,'fixture-accept',json.dumps(uris)],env=env,capture_output=True,text=True,timeout=10);(out/'helper.json').write_text(json.dumps(dict(returncode=run.returncode,stdout=run.stdout,stderr=run.stderr,helper=helper),indent=2));assert run.returncode==0 and json.loads(run.stdout).get('count')==2,(run.stdout,run.stderr)
 deadline=time.monotonic()+5
 while not opened.exists() and time.monotonic()<deadline:time.sleep(.02)
 assert opened.exists(),'Actual private registered observer did not complete'
 launches=[json.loads(line) for line in opened.read_text().splitlines()];assert len(launches)==1 and [row['uri'] for row in launches[0]]==uris,launches
 assert [row['sha256'] for row in launches[0]]==[hashlib.sha256(path.read_bytes()).hexdigest() for path in (first,second)]
 missing=folder/'Unavailable.txt';assert not missing.exists();refused=sp.run([helper,'fixture-accept',json.dumps([first.as_uri(),missing.as_uri()])],env=env,capture_output=True,text=True,timeout=10);assert refused.returncode==1 and 'unavailable or unreadable' in json.loads(refused.stdout)['error'],(refused.stdout,refused.stderr);assert len(opened.read_text().splitlines())==1
 (out/'result.json').write_text(json.dumps(dict(passed=True,sourcePreflight=result,helper=str(helper),launches=launches,unavailableRefusal=json.loads(refused.stdout),checks=8),indent=2));print(json.dumps(dict(passed=True,checks=8)))
finally:
 for process in reversed(children):
  if process.poll() is None:
   os.killpg(process.pid,signal.SIGTERM)
   try:process.wait(timeout=5)
   except sp.TimeoutExpired:os.killpg(process.pid,signal.SIGKILL);process.wait(timeout=5)
 close_reaped_pipes(children)
