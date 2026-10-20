"""Temporary exact-executable userns policy on disposable hosted Ubuntu only."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import stat
import subprocess

PARAMETERS = ('kernel/apparmor_restrict_unprivileged_userns', 'kernel/apparmor_restrict_unprivileged_unconfined', 'kernel/unprivileged_userns_clone', 'user/max_user_namespaces')

def hosted(environment, release):
    expected = {'GITHUB_ACTIONS':'true', 'RUNNER_ENVIRONMENT':'github-hosted', 'RUNNER_OS':'Linux'}
    if any(environment.get(k) != v for k,v in expected.items()):
        raise ValueError('Only disposable GitHub-hosted Linux jobs may prepare this policy')
    if release.get('ID') != 'ubuntu' or release.get('VERSION_ID') != '24.04':
        raise ValueError('Revalidate policy for this runner distribution')

def profile_for(executable):
    path = str(executable)
    if not re.fullmatch(r'/nix/store/[a-z0-9]{32}-util-linux-[A-Za-z0-9_.+-]+/bin/unshare', path):
        raise ValueError('Exact immutable Nix unshare required')
    name = 'lumen-native-tests-' + hashlib.sha256(path.encode()).hexdigest()[:16]
    return name, 'abi <abi/4.0>,\nprofile '+name+' "'+path+'" flags=(unconfined) {\n  userns,\n}\n'

def parameters(root=Path('/proc/sys')):
    return {name:(root/name).read_text().strip() if (root/name).exists() else None for name in PARAMETERS}

def execute(parser, operation, text):
    # The privileged parser receives reconstructed bytes, never a mutable path.
    subprocess.run(['sudo','--non-interactive',str(parser),operation], input=text, text=True, check=True, timeout=20)

def profile_loaded(name):
    result=subprocess.run(['sudo','--non-interactive','/usr/bin/cat','/sys/kernel/security/apparmor/profiles'],text=True,capture_output=True,check=True,timeout=5)
    matches=[line for line in result.stdout.splitlines() if line.startswith(name+' (')]
    if len(matches)>1:raise ValueError('Ambiguous test profile identity')
    return bool(matches)

def cleanup_policy(parser,name,text,attempted,present=profile_loaded,apply=execute):
    if not attempted:return
    if present(name):apply(parser,'--remove',text)
    if present(name):raise ValueError('Test profile remains loaded after cleanup')

def main():
    p=argparse.ArgumentParser();p.add_argument('operation',choices=('prepare','cleanup'));p.add_argument('--output',type=Path,required=True);a=p.parse_args()
    release=dict(line.split('=',1) for line in Path('/etc/os-release').read_text().splitlines() if '=' in line)
    hosted(os.environ,{k:v.strip('"') for k,v in release.items()})
    parser=Path('/usr/sbin/apparmor_parser');info=parser.stat()
    if info.st_uid!=0 or stat.S_IMODE(info.st_mode)&0o022 or not os.access(parser,os.X_OK):raise ValueError('Root-owned runner AppArmor parser required')
    out=a.output.absolute()
    if out.is_symlink():raise ValueError('Policy artifacts cannot be symlinks')
    if a.operation=='prepare':
        executable=Path(shutil.which('unshare') or '').resolve()
        if not executable.is_file() or not os.access(executable,os.X_OK):raise ValueError('Nix unshare missing')
        name,text=profile_for(executable);out.mkdir(mode=0o700,parents=True,exist_ok=False);profile=out/'profile';profile.write_text(text)
        record=dict(profile=name,executable=str(executable),executableSha256=hashlib.sha256(executable.read_bytes()).hexdigest(),profileSha256=hashlib.sha256(text.encode()).hexdigest(),parametersBefore=parameters(),loaded=False,attempted=False)
        (out/'result.json').write_text(json.dumps(record,indent=2)+'\n')
        if profile_loaded(name):raise ValueError('Refusing to replace a preexisting test profile')
        record['attempted']=True;(out/'result.json').write_text(json.dumps(record,indent=2)+'\n')
        execute(parser,'--add',text);record['loaded']=profile_loaded(name);(out/'result.json').write_text(json.dumps(record,indent=2)+'\n')
        if not record['loaded']:raise ValueError('Namespace profile was not loaded')
        record['parametersAfter']=parameters();(out/'result.json').write_text(json.dumps(record,indent=2)+'\n')
        if record['parametersAfter']!=record['parametersBefore']:raise ValueError('Global namespace controls changed unexpectedly')
        print('Prepared exact immutable test executable policy; global controls unchanged')
    elif out.exists():
        if out.stat().st_uid!=os.getuid() or stat.S_IMODE(out.stat().st_mode)&0o077:raise ValueError('Policy artifact ownership changed')
        record=json.loads((out/'result.json').read_text());profile=out/'profile';name,text=profile_for(record['executable'])
        if name!=record['profile'] or profile.is_symlink() or profile.read_text()!=text:raise ValueError('Refusing changed policy cleanup')
        cleanup_policy(parser,name,text,record['attempted']);record['loaded']=False
        record['cleaned']=True;record['parametersAfterCleanup']=parameters();(out/'result.json').write_text(json.dumps(record,indent=2)+'\n')
        if record['parametersAfterCleanup']!=record['parametersBefore']:raise ValueError('Global namespace controls changed during job')
        print('Removed only the test executable policy; global controls unchanged')

if __name__=='__main__':main()
