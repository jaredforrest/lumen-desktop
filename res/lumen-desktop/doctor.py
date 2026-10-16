"""Read-only installed desktop checks. No environment, window titles or user content."""
import json
import os
from pathlib import Path
import subprocess
import sys

UNITS=('lumen-desktop-shell','lumen-notifications','lumen-character-input','lumen-file-operations','lumen-display-layout')

def command(*args):
    try:
        result=subprocess.run(args,capture_output=True,text=True,timeout=8)
        return result.returncode,result.stdout.strip()
    except (OSError,subprocess.TimeoutExpired):return 1,''

def persistence_backing(user, roots=(Path('/persist'), Path('/persist-fast'))):
    """Verify identity against either configured impermanence storage root."""
    user=Path(user)
    for root in roots:
        persistent=Path(root)/str(user).lstrip('/')
        try:
            if user.samefile(persistent):return str(root)
        except OSError:pass
    return None


def report():
    current=Path('/run/current-system').resolve()
    boot=Path('/nix/var/nix/profiles/system').resolve()
    checks=[]
    def add(name,ok,detail):checks.append(dict(check=name,ok=bool(ok),detail=detail))
    add('Installed generation',current==boot,dict(running=str(current),nextBoot=str(boot)))
    for unit in UNITS:
        code,state=command('systemctl','--user','show',unit+'.service','--property=ActiveState,SubState,NRestarts','--no-pager')
        properties=dict(line.split('=',1) for line in state.splitlines() if '=' in line)
        add(unit,code==0 and properties.get('ActiveState')=='active',properties)
    for item in ('.local/state/lumen-desktop/operations','.local/state/lumen-desktop/notifications','.local/share/Trash','data'):
        backing=persistence_backing(Path.home()/item)
        add('Persistence: '+item,backing is not None,'same backing directory on '+backing if backing else 'not verified')
    for pattern,label,expected in (('emoji','Emoji font',os.environ.get('LUMEN_EMOJI_FONT_FAMILY','Noto Color Emoji')),('Interlude Variable','Interface font','Interlude')):
        code,name=command('fc-match','--format=%{family}',pattern)
        add(label,code==0 and expected in name,name)
    code,scheme=command('gsettings','get','org.gnome.desktop.interface','color-scheme')
    add('Appearance preference',code==0,scheme)
    return dict(ok=all(c['ok'] for c in checks),checks=checks,pending=['Physical reboot, sleep/wake and peripheral checks','Off-machine personal-file backup: no destination configured'])

if __name__=='__main__':
    result=report();print(json.dumps(result,indent=2));sys.exit(0 if result['ok'] else 1)
