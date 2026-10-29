"""Built recovery script status guard and terminal contract, with inert endpoints."""
import argparse,hashlib,json,os,re,subprocess as sp,sys
from pathlib import Path
from application_fixture.isolation import run
from application_fixture.provenance import FrozenSource
p=argparse.ArgumentParser();p.add_argument('--command',type=Path,required=True);p.add_argument('--output',type=Path,required=True);p.add_argument('--inside',action='store_true');a=p.parse_args();repo=Path(__file__).resolve().parents[2];a.command=a.command.resolve();assert str(a.command).startswith('/nix/store/')and a.command.is_file()
if not a.inside:
    a.output=a.output.resolve();a.output.mkdir(mode=0o700,parents=True,exist_ok=False);runtime=Path(sys.executable).resolve();frozen=FrozenSource(repo,__file__,[a.command,runtime]);code=1
    try:code=run(repo,a.output,[str(runtime),'/source/tests/desktop/session_recovery_fallback.py','--inside','--command',str(a.command),'--output','/artifacts'],dict(os.environ,PATH=str(runtime.parent)))
    finally:frozen.finish(a.output,code)
    raise SystemExit(code)
assert Path.home()==Path('/home/fixture')and not Path('/.old-root').exists()
out=a.output;stubs=out/'stubs';stubs.mkdir();original=a.command.read_text();python=str(Path(sys.executable).resolve());bash=original.splitlines()[0][2:]
status=stubs/'systemctl';status.write_text('#!'+python+'\nimport json,os,sys\nfrom pathlib import Path\nexpected=["--user","show","--property=Result","--value","lumen-desktop-shell.service"]\nassert sys.argv[1:]==expected,sys.argv\nwith Path(os.environ["RECOVERY_REQUESTS"]).open("a")as stream:stream.write(json.dumps(sys.argv[1:])+"\\n")\nprint(os.environ["RECOVERY_RESULT"])\n');status.chmod(0o700)
terminal=stubs/'foot';terminal.write_text('#!'+python+'\nimport json,os,sys\nfrom pathlib import Path\nassert sys.argv[1:3]==["--app-id=lumen-session-recovery","--title=Desktop Recovery"],sys.argv\nPath(os.environ["RECOVERY_TERMINAL"]).write_text(json.dumps(sys.argv[1:]))\nos.execv(sys.argv[3],sys.argv[3:])\n');terminal.chmod(0o700)
# Only command destinations change. All status branching and terminal text/body
# execute from the actual built package, including the final ordinary shell.
modified,count=re.subn(r'^export PATH="[^"]+"$',lambda match:match.group(0)+'\nexport PATH='+str(stubs)+':"$PATH"',original,flags=re.M);assert count==1
modified,count=re.subn(r'/nix/store/[^/]+-foot-[^/]+/bin/foot',str(terminal),modified);assert count==1
script=out/'recovery';script.write_text(modified);script.chmod(0o700)
receipts=[]
for result in ['success','exit-code','start-limit-hit']:
    requests=out/(result+'-requests.jsonl');opened=out/(result+'-terminal.json');env=dict(os.environ,RECOVERY_RESULT=result,RECOVERY_REQUESTS=str(requests),RECOVERY_TERMINAL=str(opened))
    process=sp.run([bash,str(script)],env=env,stdin=sp.DEVNULL,stdout=sp.PIPE,stderr=sp.PIPE,text=True,timeout=5);assert process.returncode == 0 and not process.stderr,process
    assert [json.loads(line)for line in requests.read_text().splitlines()]==[['--user','show','--property=Result','--value','lumen-desktop-shell.service']]
    assert opened.exists()==(result=='start-limit-hit')
    if opened.exists():
        lines=process.stdout.splitlines();assert len(lines)==6,lines;assert lines[0]=='The desktop shell stopped after repeated failures.';assert lines[-1]=='  systemctl --user start lumen-desktop-shell.service';assert all('\\n'not in line for line in lines);assert '\n' in process.stdout
    else:assert process.stdout==''
    receipts.append(dict(result=result,terminalOpened=opened.exists(),readonlyStatusRequests=1,stdout=process.stdout))
(out/'result.json').write_text(json.dumps(dict(passed=True,cases=receipts,originalCommand=str(a.command),originalSha256=hashlib.sha256(original.encode()).hexdigest(),fixtureSha256=hashlib.sha256(modified.encode()).hexdigest(),scope='Built shell-script branching and terminal argv/body only. Read-only status and terminal executable destinations are inert private fixtures. No actual systemd user manager, real terminal or host recovery executed.'),indent=2)+'\n')
