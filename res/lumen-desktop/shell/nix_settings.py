"""Compile authoritative Nix settings; runtime links are disposable build outputs."""
import fcntl
import json
import os
import shutil
from pathlib import Path
import subprocess
import sys
import tempfile

SECTIONS = {'applicationPreferences', 'appearance', 'displays', 'textTools', 'menuShortcuts', 'notifications', 'search', 'controlCentre', 'windows', 'desktop', 'menuBar', 'inputs', 'wallpaper'}


def runtime():
    directory = Path(os.environ.get('XDG_RUNTIME_DIR', '/run/user/' + str(os.getuid()))) / 'lumen-nix-settings'
    directory.mkdir(mode=0o700, parents=True, exist_ok=True)
    return directory


def read_settings():
    active=os.environ.get('LUMEN_NIX_SETTINGS_ACTIVE')
    if not active and Path('/etc/nix/desktop-settings.json').exists():active='/etc/nix/desktop-settings.json'
    if active:return json.loads(Path(active).read_text())
    file = runtime() / 'current/settings.json'
    if not file.exists():
        subprocess.run(['lumen-nix-settings', 'sync'], check=True, capture_output=True, text=True, timeout=120)
    return json.loads(file.read_text())


def set_section(section, value, *, merge=False):
    result = subprocess.run(['lumen-nix-settings', 'patch' if merge else 'set', section, json.dumps(value, ensure_ascii=False)],
                            capture_output=True, text=True, timeout=3600)
    if result.returncode:
        try: message = json.loads(result.stdout)['error']
        except (ValueError, KeyError): message = result.stderr.strip() or 'Nix settings update failed'
        raise ValueError(message)
    return json.loads(result.stdout)


def build(source):
    result = subprocess.run([(shutil.which('nix') or 'nix'), 'build', '--file', os.environ['LUMEN_NIX_SETTINGS_BUILDER'],
                             '--argstr', 'source', str(source), '--no-link', '--json'],
                            capture_output=True, text=True, timeout=90)
    if result.returncode:
        raise ValueError('Nix rejected the settings; previous configuration kept.\n' + result.stderr[-4000:])
    return Path(json.loads(result.stdout)[0]['outputs']['out'])


def activate(bundle):
    # The indirect root keeps an interactively applied bundle alive until logout.
    link = runtime() / 'current'
    pending = runtime() / 'next'
    pending.unlink(missing_ok=True)
    subprocess.run(['nix-store', '--add-root', str(runtime() / 'gc-root'), '--indirect',
                    '--realise', str(bundle)], check=True, capture_output=True, text=True, timeout=15)
    pending.symlink_to(bundle)
    pending.replace(link)


def serialize(values):
    def string(value):
        # JSON and Nix share ordinary quoted-string escapes. JSON-only control
        # escapes use a literal fromJSON expression instead of being misparsed.
        if any(ord(c)<32 and c not in '\n\r\t' for c in value):
            encoded=json.dumps(json.dumps(value,ensure_ascii=False),ensure_ascii=False).replace('${',r'\${')
            return '(builtins.fromJSON '+encoded+')'
        return json.dumps(value,ensure_ascii=False).replace('${',r'\${')
    def encode(value,depth=0):
        pad='  '*depth
        if isinstance(value,dict):
            rows=['  '*(depth+1)+string(key)+' = '+encode(item,depth+1)+';' for key,item in sorted(value.items())]
            return '{\n'+'\n'.join(rows)+'\n'+pad+'}'
        if isinstance(value,list):return '[ '+' '.join(encode(item,depth+1) for item in value)+' ]'
        if isinstance(value,str):return string(value)
        if value is True:return 'true'
        if value is False:return 'false'
        if value is None:return 'null'
        return json.dumps(value,allow_nan=False)
    return ('# Authoritative desktop settings. Edited by the Settings interface.\n'
            '# Never put secrets here: Nix build outputs are publicly readable.\n'
            + encode(values) + '\n')


def merge_section(current, patch):
    """Merge under the transaction lock; stale readers cannot erase sibling keys."""
    if not isinstance(current, dict) or not isinstance(patch, dict):
        raise ValueError('Preference patches must be objects')
    if any(key not in current for key in patch):
        raise ValueError('Unsupported preference key')
    return {**current, **patch}


def rebuild_system(candidate):
    """Build the sole authoritative repository at its current clean commit."""
    if subprocess.run(['pgrep','-x','resize2fs'],capture_output=True).returncode==0:
        raise ValueError('Finish the active filesystem resize before rebuilding configuration.')
    command=os.environ.get('LUMEN_NIX_SETTINGS_DEPLOY')
    if not command:raise ValueError('Use the installed Git-backed desktop deployment workflow')
    result=subprocess.run([command,'build'],capture_output=True,text=True,timeout=3300)
    if result.returncode:raise ValueError('NixOS rebuild failed; committed source retained.\n'+result.stdout[-4000:])
    return Path(json.loads(result.stdout)['system'])


def git_transaction(section, value, application, merge=False):
    # Immutable applied settings can be read during activation without the edit lock.
    if section is None:return read_settings()
    source=Path(os.environ['LUMEN_NIX_SETTINGS_SOURCE'])
    repo=Path(os.environ['LUMEN_NIX_SETTINGS_BUILD_SOURCE']).resolve()
    if source.resolve()!=repo/'settings/desktop.nix':raise ValueError('Settings source is outside the authoritative repository')
    def git(*args):
        result=subprocess.run(['git','-C',str(repo),*args],capture_output=True,text=True)
        if result.returncode:raise ValueError('Git operation failed: '+result.stderr[-2000:])
        return result.stdout.strip()
    with (runtime()/'transaction.lock').open('a') as lock:
        fcntl.flock(lock,fcntl.LOCK_EX)
        if section not in SECTIONS:raise ValueError('Unsupported Nix settings section')
        if git('status','--porcelain'):raise ValueError('Commit or set aside pending configuration edits before changing Settings')
        previous=git('rev-parse','HEAD');original=source.read_bytes()
        values=json.loads((build(source)/'settings.json').read_text())
        if application is not None:
            if section!='applicationPreferences' or application not in values[section]:raise ValueError('Unsupported application preferences')
            values[section][application]=value
        else:values[section]=merge_section(values[section],value) if merge else value
        fd,name=tempfile.mkstemp(prefix='.desktop-settings-',suffix='.nix',dir=source.parent)
        candidate=Path(name);committed=None;system=None;replaced=False;candidate_bytes=None
        try:
            with os.fdopen(fd,'w') as stream:stream.write(serialize(values));stream.flush();os.fsync(stream.fileno())
            bundle=build(candidate)
            if git('rev-parse','HEAD')!=previous or source.read_bytes()!=original:raise ValueError('Configuration changed during evaluation; retry')
            if candidate.read_bytes()==original:return json.loads((bundle/'settings.json').read_text())
            candidate_bytes=candidate.read_bytes()
            candidate.replace(source);replaced=True
            git('add','--','settings/desktop.nix')
            git('commit','--only','-m','Update desktop setting: '+section,'--','settings/desktop.nix')
            committed=git('rev-parse','HEAD')
            system=rebuild_system(source)
            subprocess.run([os.environ.get('LUMEN_NIX_SETTINGS_PKEXEC','/run/wrappers/bin/pkexec'),
                '/run/current-system/sw/bin/lumen-activate-settings',str(system),str(Path('/run/current-system').resolve())],check=True,timeout=300)
        except Exception:
            # Never roll the source back behind a system that was already activated.
            if system is not None and Path('/run/current-system').resolve()==system:raise
            if committed is not None:
                if git('rev-parse','HEAD')!=committed or source.read_bytes()!=candidate_bytes:raise ValueError('Configuration changed concurrently; automatic rollback stopped')
                source.write_bytes(original);git('add','--','settings/desktop.nix')
                git('commit','--only','-m','Restore settings after unsuccessful activation','--','settings/desktop.nix')
            elif replaced:
                if source.read_bytes()!=candidate_bytes:raise ValueError("Source changed concurrently; automatic rollback stopped")
                source.write_bytes(original);git('reset','--','settings/desktop.nix')
            raise
        finally:candidate.unlink(missing_ok=True)
        try:
            result=subprocess.run([os.environ['LUMEN_NIX_SETTINGS_DEPLOY'],'sync'],capture_output=True,text=True,timeout=65)
            if result.returncode:raise ValueError('GitHub sync pending')
            (runtime()/'git-sync-pending').unlink(missing_ok=True)
        except Exception:
            (runtime()/'git-sync-pending').write_text(committed+'\n')
            subprocess.run(['notify-send','Configuration saved locally','GitHub sync is pending; it will retry automatically.'],check=False)
        return json.loads((bundle/'settings.json').read_text())


def transaction(section=None, value=None, application=None, *, merge=False):
    if os.environ.get("LUMEN_NIX_SETTINGS_DEPLOY"):
        return git_transaction(section,value,application,merge)
    source = Path(os.environ['LUMEN_NIX_SETTINGS_SOURCE'])
    with (runtime() / 'transaction.lock').open('a') as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        if section is None:
            active=os.environ.get('LUMEN_NIX_SETTINGS_ACTIVE')
            if active:
                # Applied settings come from the NixOS generation, never from
                # an unactivated source edit or a remembered preference cache.
                return json.loads(Path(active).read_text())
            bundle = build(source)
            activate(bundle)
            return json.loads((bundle / 'settings.json').read_text())
        if section not in SECTIONS:
            raise ValueError('Unsupported Nix settings section')
        original=source.read_bytes()
        current=build(source)
        if source.read_bytes()!=original:
            raise ValueError('The Nix source changed during evaluation; retry the edit')
        values=json.loads((current/'settings.json').read_text())
        if application is not None:
            if section!='applicationPreferences' or application not in values[section]:
                raise ValueError('Unsupported application preferences')
            values[section][application]=value
        else:values[section]=merge_section(values[section],value) if merge else value
        fd,name=tempfile.mkstemp(prefix='.desktop-settings-',suffix='.nix',dir=source.parent)
        candidate=Path(name)
        try:
            with os.fdopen(fd,'w') as stream:
                stream.write(serialize(values));stream.flush();os.fsync(stream.fileno())
            bundle=build(candidate)
            system=rebuild_system(candidate) if os.environ.get('LUMEN_NIX_SETTINGS_BUILD_SOURCE') else None
            if source.read_bytes()!=original:
                raise ValueError('The Nix source changed during validation; retry the edit')
            candidate.replace(source)
            try:
                if system:
                    previous=Path('/run/current-system').resolve()
                    subprocess.run(['pkexec','/run/current-system/sw/bin/lumen-activate-settings',
                                    str(system),str(previous)],check=True,timeout=300)
                else:
                    # Isolated evaluator checks only; production always builds
                    # and activates a complete NixOS system generation.
                    activate(bundle)
            except Exception:
                source.write_bytes(original)
                raise
            return json.loads((bundle/'settings.json').read_text())
        finally:
            candidate.unlink(missing_ok=True)


if __name__ == '__main__':
    try:
        command=sys.argv[1] if len(sys.argv)>1 else 'read'
        if command=='set':result=transaction(sys.argv[2],json.loads(sys.argv[3]))
        elif command=='patch':result=transaction(sys.argv[2],json.loads(sys.argv[3]),merge=True)
        elif command=='set-application':result=transaction('applicationPreferences',json.loads(sys.argv[3]),application=sys.argv[2])
        elif command=='sync':result=transaction()
        elif command=='apply':
            raise ValueError('Manual configuration edits require a NixOS rebuild and authenticated activation.')
        elif command=='read':result=read_settings()
        else:raise ValueError('Unknown Nix settings command')
        print(json.dumps(result))
    except Exception as exc:
        print(json.dumps({'error':str(exc)}));sys.exit(1)
