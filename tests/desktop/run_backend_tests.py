"""Run every backend test file with a private bus and pinned installed dependencies."""
import argparse
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile

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
    with tempfile.TemporaryDirectory(prefix='lumen-backend-') as private:
        test_env=dict(env,HOME=private,XDG_RUNTIME_DIR=private,QT_ACCESSIBILITY='0',QT_LINUX_ACCESSIBILITY_ALWAYS_ON='0',NO_AT_BRIDGE='1',GTK_A11Y='none')
        for key in ('CONFIG','STATE','CACHE','DATA'):test_env['XDG_'+key+'_HOME']=str(Path(private)/key.lower())
        for key in ('AT_SPI_BUS_ADDRESS','SWAYSOCK','WAYLAND_DISPLAY','DISPLAY'):test_env.pop(key,None)
        run=subprocess.run(['dbus-run-session','--',sys.executable,str(test)],env=test_env,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,timeout=180)
    (a.output/(test.stem+'.log')).write_text(run.stdout)
    results.append(dict(test=test.name,passed=run.returncode==0))
    print(('PASS ' if run.returncode==0 else 'FAIL ')+test.name,flush=True)
(a.output/'results.json').write_text(json.dumps(results,indent=2)+'\n')
raise SystemExit(0 if all(r['passed'] for r in results) else 1)
