"""Run every backend test file with a private bus and pinned installed dependencies."""
import argparse
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys

parser=argparse.ArgumentParser()
parser.add_argument('--output',required=True,type=Path)
parser.add_argument('--settings-command',default=os.environ.get('LUMEN_TEST_SETTINGS_COMMAND'))
a=parser.parse_args();a.output.mkdir(parents=True,exist_ok=False)
env=dict(os.environ)
# Never pass production deployment settings to tests of configuration writes.
for key in list(env):
    if key.startswith('LUMEN_NIX_SETTINGS_'):env.pop(key)
script=Path(a.settings_command or shutil.which('lumen-nix-settings')).read_text()
match=re.search(r'^export LUMEN_NIX_SETTINGS_BUILDER=(.+)$',script,re.M)
if not match:raise RuntimeError('Installed evaluator builder is unavailable')
env['LUMEN_NIX_SETTINGS_BUILDER']=match[1].strip('"\'')
calendar=shutil.which('lumen-calendar-python')
if calendar and not env.get('DESKTOP_TEST_PINNED_RUNTIME'):
    raw=subprocess.check_output([calendar,'-c','import json,os;print(json.dumps({k:os.environ.get(k,"") for k in ["GI_TYPELIB_PATH","LD_LIBRARY_PATH"]}))'],text=True)
    env.update(json.loads(raw))
results=[]
for test in sorted(Path(__file__).parent.glob('test_*.py')):
    run=subprocess.run(['dbus-run-session','--',sys.executable,str(test)],env=env,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,timeout=180)
    (a.output/(test.stem+'.log')).write_text(run.stdout)
    results.append(dict(test=test.name,passed=run.returncode==0))
    print(('PASS ' if run.returncode==0 else 'FAIL ')+test.name,flush=True)
(a.output/'results.json').write_text(json.dumps(results,indent=2)+'\n')
raise SystemExit(0 if all(r['passed'] for r in results) else 1)
