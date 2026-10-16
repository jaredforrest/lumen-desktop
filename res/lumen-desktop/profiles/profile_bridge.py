"""Separate native JSON preferences into Nix config, retained state and cache.

Native Craft 0.5 saves preferences on exit. Its dialogs are previews; accepting
those changes for subsequent launches requires a full NixOS rebuild after exit.
The bridge never closes documents or exports unclassified fields to Nix.
"""
import copy
import configparser
import fcntl
import json
import os
from pathlib import Path
import subprocess
import sys
import time
import signal
import tempfile


def atomic(file, value):
    file.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
    fd, name = tempfile.mkstemp(dir=file.parent)
    try:
        with os.fdopen(fd, 'w') as stream:
            json.dump(value, stream, ensure_ascii=False)
            stream.flush(); os.fsync(stream.fileno())
        os.replace(name, file)
    finally:
        Path(name).unlink(missing_ok=True)


def load(file):
    try:
        value=json.loads(file.read_text())
    except FileNotFoundError:
        return {}
    if not isinstance(value,dict):raise ValueError('Native profile must be a JSON object')
    return value


def merge(*values):
    result={}
    for value in values:
        for key,item in value.items():
            if isinstance(item,dict) and isinstance(result.get(key),dict):result[key]=merge(result[key],item)
            else:result[key]=copy.deepcopy(item)
    return result


def partition(value,policy,state_fields,prefix=''):
    unknown=set(value)-set(policy)-set(state_fields)
    if unknown:raise ValueError('Unclassified native profile fields: '+', '.join(prefix+key for key in sorted(unknown)))
    proposed={};state={}
    for key,default in policy.items():
        item=value.get(key,default)
        if isinstance(default,dict):
            if not isinstance(item,dict):raise ValueError('Invalid native object: '+prefix+key)
            proposed[key],part=partition(item,default,state_fields.get(key,{}),prefix+key+'.')
            if part:state[key]=part
        else:proposed[key]=copy.deepcopy(item)
    for key,default in state_fields.items():
        if key in policy:continue
        if key not in value:continue
        item=value[key]
        if isinstance(default,dict):
            if not isinstance(item,dict):raise ValueError('Invalid state object: '+prefix+key)
            _,state[key]=partition(item,{},default,prefix+key+'.')
        else:state[key]=copy.deepcopy(item)
    return proposed,state


class Bridge:
    def __init__(self, app, policies, fields, profile, state_root, cache_root, cache_fields=None):
        self.app=app; self.policies=policies; self.fields=fields
        self.cache_fields=cache_fields or {}
        self.profile=Path(profile)
        self.state=Path(state_root)/app
        self.cache=Path(cache_root)/app

    def read_native(self,file):
        if self.app!='prism':return load(file)
        cfg=configparser.ConfigParser(interpolation=None);cfg.optionxform=str
        if not file.exists():return {}
        cfg.read(file)
        if cfg.sections()!=['General']:raise ValueError('Unclassified Prism INI section')
        return dict(cfg['General'])

    def write_native(self,file,value):
        if self.app!='prism':return atomic(file,value)
        file.parent.mkdir(parents=True,exist_ok=True,mode=0o700)
        cfg=configparser.ConfigParser(interpolation=None);cfg.optionxform=str;cfg['General']=value
        fd,name=tempfile.mkstemp(dir=file.parent)
        try:
            with os.fdopen(fd,'w') as stream:
                cfg.write(stream);stream.flush();os.fsync(stream.fileno())
            os.replace(name,file)
        finally:Path(name).unlink(missing_ok=True)

    def split(self,name,value):
        field_defaults={**self.fields[name],**self.cache_fields.get(name,{})}
        proposed,working=partition(value,self.policies[name],field_defaults)
        state={key:item for key,item in working.items() if key in self.fields[name]}
        cached={key:item for key,item in working.items() if key in self.cache_fields.get(name,{})}
        return proposed,state,cached

    def prepare(self):
        # Validate every field before modifying anything; unknown schemas cannot
        # silently lose fields or export them as publicly readable config.
        records={};derived={}
        for name in self.policies:
            prior=self.read_native(self.profile/name)
            _,native_state,native_cache=self.split(name,prior)
            retained=load(self.state/name)
            partition(retained,{},self.fields[name])
            records[name]=merge(self.fields[name],retained,native_state)
            derived[name]={**self.cache_fields.get(name,{}),**load(self.cache/(name+'.derived')),**native_cache}
        for name,policy in self.policies.items():
            atomic(self.state/name,records[name])
            atomic(self.cache/(name+'.derived'),derived[name])
            self.write_native(self.cache/name,merge(derived[name],records[name],policy))
            self.link_projection(name)

    def link_projection(self,name):
        file=self.profile/name
        file.parent.mkdir(parents=True,exist_ok=True,mode=0o700)
        temporary=file.with_name('.'+name+'.projection')
        temporary.unlink(missing_ok=True)
        temporary.symlink_to(self.cache/name)
        temporary.replace(file)

    def collect(self):
        proposed={};records={};derived={}
        for name,policy in self.policies.items():
            file=self.profile/name
            if not file.exists():
                proposed[name]=policy
                continue
            value=self.read_native(file)
            try:proposed[name],records[name],derived[name]=self.split(name,value)
            except ValueError:
                # This is a private recovery draft, not a settings authority.
                atomic(self.state/'unclassified-profile-draft.json',{'file':name,'status':'needs-classification','native':value})
                known={key:item for key,item in value.items() if key in self.fields[name]}
                atomic(self.state/name,known)
                raise
        for name,state in records.items():atomic(self.state/name,state)
        for name,cached in derived.items():atomic(self.cache/(name+'.derived'),cached)
        return proposed

    def project(self,policies):
        for name,policy in policies.items():
            self.write_native(self.cache/name,merge(self.cache_fields.get(name,{}),load(self.cache/(name+'.derived')),
                                                   load(self.state/name),policy))
            self.link_projection(name)


def main():
    app,binary,*args=sys.argv[1:]
    if app not in ('wordcraft','gridcraft','deckcraft','prism'):raise ValueError('Unsupported native profile')
    # Command-line inspection/conversion must never migrate a user profile.
    if any(a in ('--version','--help','-h') for a in args) or os.environ.get(app.upper()+'_NO_PREFS') is not None:
        os.execv(binary,[binary,*args])
    compiled=load(Path(os.environ.get('LUMEN_PROFILE_ACTIVE','/etc/nix/desktop-settings.json')))
    policies=compiled['applicationPreferences'][app]
    fields=load(Path(os.environ['LUMEN_PROFILE_STATE_FIELDS']))[app]
    home=Path.home()
    state=Path(os.environ.get('XDG_STATE_HOME',home/'.local/state'))/'lumen-desktop/apps'
    cache=Path(os.environ.get('XDG_CACHE_HOME',home/'.cache'))/'lumen-desktop/profiles'
    profile=(Path(os.environ.get('XDG_DATA_HOME',home/'.local/share'))/'PrismLauncher') if app=='prism' else Path(os.environ.get('XDG_CONFIG_HOME',home/'.config'))/app
    cache_fields=load(Path(os.environ['LUMEN_PROFILE_CACHE_FIELDS'])).get(app,{})
    runtime=Path(os.environ.get('XDG_RUNTIME_DIR','/run/user/'+str(os.getuid())))
    lockfile=runtime/('lumen-profile-'+app+'.lock')
    registry=runtime/('lumen-profile-'+app+'.json')
    def alive(pid):
        try:os.kill(int(pid),0);return True
        except (ProcessLookupError,ValueError):return False
    bridge=Bridge(app,policies,fields,profile,state,cache,cache_fields)
    with lockfile.open('a') as lock:
        fcntl.flock(lock,fcntl.LOCK_EX)
        running=[pid for pid in load(registry).get('pids',[]) if alive(pid)]
        if not running:bridge.prepare()
        child=subprocess.Popen([binary,*args])
        atomic(registry,{'pids':running+[child.pid]})
    def forward(signum,_frame):
        if child.poll() is None:child.send_signal(signum)
    signal.signal(signal.SIGTERM,forward)
    signal.signal(signal.SIGINT,forward)
    previous=None
    while child.poll() is None:
        stamp=[]
        for name in policies:
            file=bridge.profile/name
            try:info=file.stat();stamp.append((name,info.st_mtime_ns,info.st_size))
            except FileNotFoundError:stamp.append((name,None,None))
        if stamp!=previous:
            try:bridge.collect();previous=stamp
            except (json.JSONDecodeError,configparser.Error):pass  # Native write still in progress.
            except ValueError:previous=stamp  # A private unclassified draft has been retained.
        time.sleep(.25)
    status=child.wait()
    with lockfile.open('a') as lock:
        fcntl.flock(lock,fcntl.LOCK_EX)
        running=[pid for pid in load(registry).get('pids',[]) if pid!=child.pid and alive(pid)]
        atomic(registry,{'pids':running})
        proposed=bridge.collect()
        if running:return status
        if app=='prism':
            protected=('Env','JvmArgs','PreLaunchCommand','PostExitCommand','WrapperCommand')
            changes=proposed['prismlauncher.cfg'];original=policies['prismlauncher.cfg']
            if any(changes[key]!=original[key] for key in protected):
                atomic(bridge.state/'private-preference-proposal.json',{'status':'needs-private-review','preferences':proposed})
                bridge.project(policies)
                subprocess.run(['zenity','--error','--title=Private configuration needs review',
                                '--text=Command and environment preferences may contain secrets. They were not copied into the public Nix store. State and credentials were preserved. A private draft was saved for review; these fields need secret handling before they can be applied.'])
                return status
        current=load(Path(os.environ.get('LUMEN_PROFILE_ACTIVE','/etc/nix/desktop-settings.json')))['applicationPreferences'][app]
        if proposed!=policies and current!=policies:
            atomic(bridge.state/'preference-proposal.json',{'status':'configuration-conflict','preferences':proposed})
            bridge.project(current)
            subprocess.run(['zenity','--error','--title=Preferences changed elsewhere',
                            '--text=Nix preferences changed while the app was open. Your working state and a private preference draft were saved; the newer Nix configuration was preserved.'])
            return status
        if proposed!=policies:
            question=('Save the preference changes from '+app+' to NixOS?\n\n'
                      'This rebuilds the system and requests authentication. Recent documents and window state are already saved. '
                      'The preferences will take effect on the next launch.')
            accepted=subprocess.run(['zenity','--question','--title=Apply NixOS preferences','--text='+question]).returncode==0
            if accepted:
                result=subprocess.run(['lumen-nix-settings','set-application',app,json.dumps(proposed)],
                                      capture_output=True,text=True)
                if result.returncode:
                    subprocess.run(['zenity','--error','--title=NixOS preferences not applied',
                                    '--text=The rebuild or authentication failed. Your working state was saved; previous configured preferences remain active.'])
        current=load(Path(os.environ.get('LUMEN_PROFILE_ACTIVE','/etc/nix/desktop-settings.json')))['applicationPreferences'][app]
        bridge.project(current)
    return status



if __name__=='__main__':
    try:sys.exit(main())
    except Exception as error:
        print('Native profile separation: '+str(error),file=sys.stderr)
        sys.exit(1)
