"""Verify scope invocation and direct fallback without launching real apps."""
import json
import os
import subprocess
import sys
import tempfile
from pathlib import Path

helper = Path(__file__).resolve().parents[2] / 'res/lumen-desktop/launch-app.py'
with tempfile.TemporaryDirectory(prefix='lumen-launch-args-') as temporary:
    folder = Path(temporary)
    stub = folder / 'systemd-run'
    stub.write_text('#!' + sys.executable + '\nimport os,sys,json\nprint(json.dumps({"argv":sys.argv[1:],"cwd":os.getcwd(),"sentinel":os.environ.get("LUMEN_LAUNCH_TEST_SENTINEL")}))\n')
    stub.chmod(0o700)
    env = dict(os.environ, PATH=str(folder) + ':' + os.environ['PATH'], LUMEN_LAUNCH_SCOPES='1', LUMEN_LAUNCH_TEST_SENTINEL='preserved value')
    literal = ['literal command', 'space value', '$HOME', '$'+'{PATH}', '$(touch should-not-exist)', ';', '--flag']
    result = subprocess.run([sys.executable, str(helper), '--', *literal], env=env, cwd=folder, text=True, capture_output=True, check=True)
    value = json.loads(result.stdout)
    assert value['argv'][:5] == ['--user','--scope','--collect','--quiet','--expand-environment=no']
    assert value['argv'][5].startswith('--unit=app-lumen-desktop-')
    assert value['argv'][6:] == ['--', *literal]
    assert value['cwd'] == str(folder) and value['sentinel'] == 'preserved value'
    env['LUMEN_LAUNCH_SCOPES'] = '0'
    result = subprocess.run([sys.executable, str(helper), sys.executable, '-c', 'import json,os,sys; print(json.dumps([os.getcwd(),os.environ["LUMEN_LAUNCH_TEST_SENTINEL"],sys.argv[1:]]))', *literal], env=env, cwd=folder, text=True, capture_output=True, check=True)
    assert json.loads(result.stdout) == [str(folder),'preserved value',literal]
    assert subprocess.run([sys.executable,str(helper)],env=env,capture_output=True).returncode == 2
    env['LUMEN_LAUNCH_SCOPES']='1'
    stub.write_text('#!' + sys.executable + '\nimport sys\nsys.exit(19)\n')
    result = subprocess.run([sys.executable,str(helper),sys.executable,'-c','raise Exception("must not run fallback")'],env=env,capture_output=True)
    assert result.returncode == 19 and not result.stderr
print('PASS scope flags, literal argv, inherited environment/cwd, direct fixture fallback, missing command and no production fallback')
