"""Offline, London solar appearance; shared palette for desktop surfaces."""
import argparse,datetime as dt,json,os,subprocess,tempfile,fcntl,contextlib,sys
from pathlib import Path
from zoneinfo import ZoneInfo
from astral import Observer
from astral.sun import sun
HERE=Path(__file__).resolve().parent
ZONE=ZoneInfo('Europe/London')

def schedule(now):
    now=now.astimezone(ZONE)
    times=sun(Observer(51.5074,-0.1278),date=now.date(),tzinfo=ZONE)
    mode='light' if times['sunrise']<=now<times['sunset'] else 'dark'
    return mode,{k:times[k].isoformat() for k in ('sunrise','sunset')}

def write(path,text):
    path.parent.mkdir(parents=True,exist_ok=True)
    if path.exists() and path.read_text()==text:return
    with tempfile.NamedTemporaryFile(mode='w',dir=path.parent,delete=False) as f:
        f.write(text);name=f.name
    os.replace(name,path)

def run(*args):
    return subprocess.run(args,check=False,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL).returncode

DEFAULTS={'mode':'auto','accent':'blue','iconAppearance':'auto','dockMagnification':True,'reduceMotion':False,'reduceTransparency':False,'scrollbars':'auto'}
ACCENTS={'blue':('#007aff','#0a84ff'),'purple':('#a855c7','#bf5af2'),'pink':('#d94382','#ff6482'),'red':('#df3038','#ff453a'),'orange':('#b65c00','#ff9f0a'),'yellow':('#947500','#ffd60a'),'green':('#248a3d','#30d158'),'graphite':('#707078','#98989f')}

def valid(key,value):
    return (key=='scrollbars' and value in ('auto','always') or key=='mode' and value in ('auto','light','dark') or
            key=='iconAppearance' and value in ('default','auto','dark') or
            key=='accent' and isinstance(value,str) and value in ACCENTS or
            key in ('dockMagnification','reduceMotion','reduceTransparency') and type(value) is bool)

def preferences(state):
    sys.path.insert(0, str(HERE.parent/'shell'))
    from nix_settings import read_settings
    return read_settings()['appearance']

def system_info():
    info={'system':'NixOS','desktop':'Sway' if os.environ.get('SWAYSOCK') else 'Wayland'}
    try:
        values=dict(line.split('=',1) for line in Path('/etc/os-release').read_text().splitlines() if '=' in line)
        info['system']=values.get('PRETTY_NAME','NixOS').strip('"')
    except OSError: pass
    return info

def main():
    p=argparse.ArgumentParser()
    p.add_argument('--mode',choices=['auto','light','dark'])
    p.add_argument('--at');p.add_argument('--print',action='store_true',dest='print_only')
    p.add_argument('--state-dir',type=Path);p.add_argument('--no-desktop',action='store_true')
    p.add_argument('--settings',action='store_true');p.add_argument('--set',nargs=2,metavar=('KEY','JSON_VALUE'))
    a=p.parse_args()
    state=a.state_dir or Path(os.environ.get('XDG_STATE_HOME',str(Path.home()/'.local/state')))/'lumen-desktop'
    state.mkdir(parents=True,exist_ok=True)
    # The periodic solar update and interactive edits serialize read/apply/write.
    with (state/'.appearance.lock').open('a') as lock:
        fcntl.flock(lock,fcntl.LOCK_EX)
        prefs=preferences(state)
        if a.set:
            key,raw=a.set
            value=json.loads(raw)
            if key not in DEFAULTS or not valid(key,value):raise ValueError('Unsupported preference or value')
            prefs[key]=value
            from nix_settings import set_section
            prefs=set_section('appearance',prefs)['appearance']
        now=dt.datetime.fromisoformat(a.at) if a.at else dt.datetime.now(ZONE)
        if now.tzinfo is None:now=now.replace(tzinfo=ZONE)
        solar,times=schedule(now);source=a.mode or prefs['mode'];mode=solar if source=='auto' else source
        palettes=json.loads((HERE/'palette.json').read_text());colors=dict(palettes[mode])
        colors['accent']=ACCENTS[prefs['accent']][mode=='dark']
        if prefs['accent'] in ('orange','yellow','green','graphite') and mode=='dark':colors['selectedText']='#161618'
        data={'mode':mode,'source':source,'location':'London','timezone':'Europe/London','font':palettes['font'],'palette':colors,'preferences':prefs,**times}
        if a.print_only:print(json.dumps(data));return
        if a.settings and not a.set:
            print(json.dumps({'preferences':prefs,'appearance':data,'system':system_info()}));return
        previous={}
        try:previous=json.loads((state/'appearance.json').read_text())
        except (OSError,ValueError):pass
        launcher=(HERE/'launcher.rasi.in').read_text()
        for k,v in colors.items():launcher=launcher.replace('@'+k+'@',v)
        launcher=launcher.replace('@font@',palettes['font']);write(state/'launcher.rasi',launcher)
        write(state/'appearance.json',json.dumps(data,indent=2)+'\n')
        warnings=[]
        if not a.no_desktop:
            native_frame=False
            compositor_effects=False
            unified_headers=False
            if os.environ.get('SWAYSOCK'):
                try:
                    version=subprocess.run(['swaymsg','-t','get_version','-r'],capture_output=True,text=True,timeout=3,check=True)
                    capabilities=json.loads(version.stdout)
                    native_frame=capabilities.get('native_window_controls',False)
                    compositor_effects=capabilities.get('compositor_effects',False)
                    unified_headers=capabilities.get('native_window_actions',False)
                except (OSError,ValueError,subprocess.SubprocessError):pass
            layout=':' if native_frame else 'close,minimize,maximize:'
            if run('gsettings','set','org.gnome.desktop.wm.preferences','button-layout',layout):
                warnings.append('Could not update window-control ownership')
            if native_frame:
                for state in ('focused','focused_inactive','unfocused'):
                    foreground=colors['text'] if state=='focused' else colors['muted']
                    if run('swaymsg','client.'+state,colors['border'],colors['background'],foreground,colors['accent'],colors['border']):
                        warnings.append('Could not update window-frame colours')
            if unified_headers:
                if run('swaymsg','[app_id="org.quickshell" title="^(Files.*|System Settings|Calendar|Text Tools|Window Overview|Quick Look|File Info|Desktop Help|Session Options|Remote Desktop)$"] border pixel 0'):
                    warnings.append('Could not apply unified app headers')
            if compositor_effects:
                commands=['corner_radius 12','smart_corner_radius disable','shadows enable','shadows_on_csd enable',
                          'shadow_blur_radius 28','shadow_color #00000060','shadow_inactive_color #00000030','shadow_offset 0 6',
                          'blur enable','blur_xray disable','blur_passes 3','blur_radius 5',
                          'animation_duration_ms '+('0' if prefs['reduceMotion'] else '160'),
                          'layer_effects "lumen-dock" "blur enable; corner_radius 22"',
                          'layer_effects "lumen-menu" "blur enable"',
                          'layer_effects "lumen-menu-popup" "blur enable; blur_ignore_transparent enable"']
                for command in commands:
                    if run('swaymsg',command):warnings.append('Could not apply compositor appearance: '+command.split()[0])
            for key,value in [('color-scheme','prefer-'+mode),('gtk-theme','WhiteSur-Dark' if mode=='dark' else 'WhiteSur-Light')]:
                if run('gsettings','set','org.gnome.desktop.interface',key,value):warnings.append('Could not update application '+key)
        if a.set:print(json.dumps({'preferences':prefs,'appearance':data,'system':system_info(),'warning':'; '.join(warnings)}))
        elif previous.get('mode')!=mode:print('Appearance:',mode,'London sunset:',times['sunset'])
if __name__=='__main__':
    try:main()
    except (OSError,ValueError,TypeError) as e:
        print(json.dumps({'error':str(e)}));sys.exit(1)
