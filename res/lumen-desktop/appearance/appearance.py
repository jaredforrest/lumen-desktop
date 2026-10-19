"""Offline, London solar appearance; shared palette for desktop surfaces."""
import re
import argparse,datetime as dt,json,os,subprocess,tempfile,fcntl,contextlib,sys
from pathlib import Path
from zoneinfo import ZoneInfo
HERE=Path(__file__).resolve().parent
ZONE=ZoneInfo('Europe/London')

def schedule(now):
    from astral import Observer
    from astral.sun import sun
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

DEFAULTS={'textScale':1,'mode':'auto','accent':'blue','iconAppearance':'auto','switchToAppSpace':True,'dockMagnification':True,'reduceMotion':False,'reduceTransparency':False,'scrollbars':'auto','increaseContrast':False,'sidebarIconSize':'medium','dockPins':None,'dockRecentApps':True,'dockIndicators':True,'dockFolders':[],'dockPosition':'bottom','dockAutoHide':False,'dockSize':48,'dockFiles':[]}
ACCENTS={'blue':('#007aff','#0a84ff'),'purple':('#a855c7','#bf5af2'),'pink':('#d94382','#ff6482'),'red':('#df3038','#ff453a'),'orange':('#b65c00','#ff9f0a'),'yellow':('#947500','#ffd60a'),'green':('#248a3d','#30d158'),'graphite':('#707078','#98989f')}

def valid(key,value):
    if key=='textScale':return type(value) in (int,float) and value in (1,1.15,1.3,1.5)
    if key=='dockFiles':return isinstance(value,list) and all(isinstance(path,str) and path.startswith('/') and len(path.encode('utf-8'))<=4096 and not any(ord(c)<32 for c in path) for path in value) and len(value)==len(set(value))
    if key=='dockFolders':
        if not isinstance(value,list):return False
        paths=[]
        for folder in value:
            if not isinstance(folder,dict) or not set(folder).issubset({'path','sort','descending','presentation'}):return False
            path=folder.get('path')
            if not isinstance(path,str) or not path.startswith('/') or len(path.encode('utf-8'))>4096 or any(ord(c)<32 for c in path):return False
            if folder.get('sort','modified') not in ('name','modified','size','type') or type(folder.get('descending',True)) is not bool or folder.get('presentation','auto') not in ('list','grid','auto'):return False
            paths.append(path)
        return len(paths)==len(set(paths))
    if key=='dockPosition':return value in ('bottom','left','right')
    if key=='dockSize':return type(value) is int and 32<=value<=72
    if key=='dockPins':
        return value is None or (isinstance(value,list) and all(isinstance(item,str) and re.fullmatch(r'[-_.A-Za-z0-9]{1,256}',item) for item in value) and len(set(value))==len(value))
    return (key=='scrollbars' and value in ('auto','when_scrolling','always') or key=='mode' and value in ('auto','light','dark') or
            key=='iconAppearance' and value in ('default','auto','dark') or
            key=='sidebarIconSize' and value in ('small','medium','large') or
            key=='accent' and isinstance(value,str) and value in ACCENTS or
            key in ('switchToAppSpace','dockMagnification','reduceMotion','reduceTransparency','increaseContrast','dockRecentApps','dockIndicators','dockAutoHide') and type(value) is bool)

def luminance(color):
    channels=[int(color[i:i+2],16)/255 for i in (1,3,5)]
    linear=[value/12.92 if value<=.04045 else ((value+.055)/1.055)**2.4 for value in channels]
    return sum(value*weight for value,weight in zip(linear,(.2126,.7152,.0722)))

def contrast(first,second):
    low,high=sorted((luminance(first),luminance(second)))
    return (high+.05)/(low+.05)

def resolve_palette(palettes,mode,prefs):
    """Semantic foregrounds and boundaries shared by all owned surfaces."""
    colors=dict(palettes[mode])
    colors.update(error='#ffaba5' if mode=='dark' else '#b42318',
                  success='#55d788' if mode=='dark' else '#196633',
                  warning='#e0b85a' if mode=='dark' else '#775000',
                  destructiveFill='#b42318',destructiveText='#ffffff')
    colors['accent']=ACCENTS[prefs['accent']][mode=='dark']
    if prefs['accent'] in ('orange','yellow','green','graphite') and mode=='dark':colors['selectedText']='#161618'
    if prefs.get('increaseContrast',False):
        colors.update(text='#ffffff' if mode=='dark' else '#000000',
                      muted='#e0e0e0' if mode=='dark' else '#393939',
                      border='#a8a8ac' if mode=='dark' else '#626266')
    if contrast(colors['selectedText'],colors['accent'])<4.5:
        colors['selectedText']=max(('#ffffff','#000000'),key=lambda foreground:contrast(foreground,colors['accent']))
    return colors

def valid_desktop(key,value):
    return key in ('showFiles','fileDescending') and type(value) is bool or key=='fileSort' and value in ('name','modified','size','kind')

def valid_windows(key,value):
    return key in ('tileMargins','edgeTiling','optionTiling','topEdgeFill','topEdgeOverview') and type(value) is bool or key=='tileGap' and type(value) is int and 0<=value<=32 or key=='titlebarDoubleClick' and value in ('fill','minimize','none')

CONTROL_IDS=('wifi','bluetooth','sharing','media','displays','appearance','focus','brightness','sound')
def valid_control(key,value):
    return key in ('order','hidden') and isinstance(value,list) and all(isinstance(item,str) and item in CONTROL_IDS for item in value) and len(value)==len(set(value)) and (key=='hidden' or len(value)==len(CONTROL_IDS))

MENU_IDS=('battery','sound','focus','search','controls')
MENU_DEFAULTS=dict(order=list(MENU_IDS),hourCycle='locale',locale='',showDate=True,showWeekday=True,showSeconds=False)
def valid_menu(key,value):
    if key=='order':return isinstance(value,list) and len(value)==len(MENU_IDS) and all(isinstance(item,str) for item in value) and set(value)==set(MENU_IDS)
    if key=='hourCycle':return value in ('locale','12','24')
    if key=='locale':return isinstance(value,str) and (value=='' or re.fullmatch(r'[a-z]{2,3}(_[A-Z]{2})?',value) is not None)
    return key in ('showDate','showWeekday','showSeconds') and type(value) is bool

SEARCH_PROVIDERS=('applications','files','actions','clipboard')
def valid_search(key,value):
    if key=='queryHistory':return type(value) is bool
    return key=='disabledProviders' and isinstance(value,list) and all(isinstance(item,str) and item in SEARCH_PROVIDERS for item in value) and len(value)==len(set(value))

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
    p.add_argument('--settings',action='store_true');p.add_argument('--set',nargs=2,metavar=('KEY','JSON_VALUE'));p.add_argument('--set-search',nargs=2,metavar=('KEY','JSON_VALUE'));p.add_argument('--set-notifications',nargs=2,metavar=('KEY','JSON_VALUE'));p.add_argument('--set-control-centre',nargs=2,metavar=('KEY','JSON_VALUE'));p.add_argument('--set-windows',nargs=2,metavar=('KEY','JSON_VALUE'));p.add_argument('--set-desktop',nargs=2,metavar=('KEY','JSON_VALUE'))
    p.add_argument('--set-menu-bar',nargs=2,metavar=('KEY','JSON_VALUE'))
    a=p.parse_args()
    state=a.state_dir or Path(os.environ.get('XDG_STATE_HOME',str(Path.home()/'.local/state')))/'lumen-desktop'
    state.mkdir(parents=True,exist_ok=True)
    # The periodic solar update and interactive edits serialize read/apply/write.
    with (state/'.appearance.lock').open('a') as lock:
        fcntl.flock(lock,fcntl.LOCK_EX)
        prefs=preferences(state)
        from nix_settings import read_settings
        menu_prefs={**MENU_DEFAULTS,**read_settings().get('menuBar',{})}
        if a.set_menu_bar:
            key,raw=a.set_menu_bar
            value=json.loads(raw)
            if not valid_menu(key,value):raise ValueError('Unsupported menu bar preference or value')
            from nix_settings import set_section
            menu_prefs=set_section('menuBar',{key:value},merge=True)['menuBar']
        desktop_prefs=read_settings().get('desktop',{'showFiles':False,'fileSort':'name','fileDescending':False})
        if a.set_desktop:
            key,raw=a.set_desktop
            value=json.loads(raw)
            if not valid_desktop(key,value):raise ValueError('Unsupported desktop preference or value')
            from nix_settings import set_section
            desktop_prefs=set_section('desktop',{key:value},merge=True)['desktop']
        window_prefs=dict(tileMargins=True,tileGap=8,titlebarDoubleClick='fill',edgeTiling=True,optionTiling=True,topEdgeFill=True,topEdgeOverview=True)
        window_prefs.update(read_settings().get('windows',{}))
        if a.set_windows:
            key,raw=a.set_windows
            value=json.loads(raw)
            if not valid_windows(key,value):raise ValueError('Unsupported window preference or value')
            from nix_settings import set_section
            window_prefs=set_section('windows',{key:value},merge=True)['windows']
        search_prefs=read_settings().get('search',{'disabledProviders':[]})
        control_prefs=read_settings().get('controlCentre',{'order':list(CONTROL_IDS),'hidden':[]})
        if a.set_control_centre:
            key,raw=a.set_control_centre
            value=json.loads(raw)
            if not valid_control(key,value):raise ValueError('Unsupported control layout preference or value')
            from nix_settings import set_section
            control_prefs=set_section('controlCentre',{key:value},merge=True)['controlCentre']
        notification_prefs=read_settings().get('notifications',{'preview':'always','policies':{}})
        if a.set_notifications:
            key,raw=a.set_notifications
            value=json.loads(raw)
            if key!='preview' or value not in ('always','never'):raise ValueError('Unsupported notification preference or value')
            from nix_settings import set_section
            notification_prefs=set_section('notifications',{key:value},merge=True)['notifications']
        if a.set_search:
            key,raw=a.set_search
            value=json.loads(raw)
            if not valid_search(key,value):raise ValueError('Unsupported search preference or value')
            from nix_settings import set_section
            search_prefs=set_section('search',{key:value},merge=True)['search']
            if key=='queryHistory' and value is False:
                from search_history import handle
                handle({'action':'clear'})
        if a.set:
            key,raw=a.set
            value=json.loads(raw)
            if key not in DEFAULTS or not valid(key,value):raise ValueError('Unsupported preference or value')
            prefs[key]=value
            from nix_settings import set_section
            prefs=set_section('appearance',{key:value},merge=True)['appearance']
        now=dt.datetime.fromisoformat(a.at) if a.at else dt.datetime.now(ZONE)
        if now.tzinfo is None:now=now.replace(tzinfo=ZONE)
        solar,times=schedule(now);source=a.mode or prefs['mode'];mode=solar if source=='auto' else source
        palettes=json.loads((HERE/'palette.json').read_text());colors=resolve_palette(palettes,mode,prefs)
        data={'mode':mode,'source':source,'location':'London','timezone':'Europe/London','font':palettes['font'],'palette':colors,'preferences':prefs,'searchPreferences':search_prefs,'notificationsPreferences':notification_prefs,'controlCentrePreferences':control_prefs,'menuBarPreferences':menu_prefs,'windowsPreferences':window_prefs,'desktopPreferences':desktop_prefs,**times}
        if a.print_only:print(json.dumps(data));return
        if a.settings and not (a.set or a.set_search or a.set_notifications or a.set_control_centre or a.set_windows or a.set_desktop):
            print(json.dumps({'preferences':prefs,'searchPreferences':search_prefs,'notificationsPreferences':notification_prefs,'controlCentrePreferences':control_prefs,'menuBarPreferences':menu_prefs,'windowsPreferences':window_prefs,'desktopPreferences':desktop_prefs,'appearance':data,'system':system_info()}));return
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
            placement_preferences=False
            edge_preferences=False
            if os.environ.get('SWAYSOCK'):
                try:
                    version=subprocess.run(['swaymsg','-t','get_version','-r'],capture_output=True,text=True,timeout=3,check=True)
                    capabilities=json.loads(version.stdout)
                    native_frame=capabilities.get('native_window_controls',False)
                    compositor_effects=capabilities.get('compositor_effects',False)
                    unified_headers=capabilities.get('native_window_actions',False)
                    placement_preferences=capabilities.get('window_placement_preferences',False)
                    edge_preferences=capabilities.get('native_edge_tiling') is True
                except (OSError,ValueError,subprocess.SubprocessError):pass
            layout=':' if native_frame else 'close,minimize,maximize:'
            if run('gsettings','set','org.gnome.desktop.wm.preferences','button-layout',layout):
                warnings.append('Could not update window-control ownership')
            if native_frame:
                for state in ('focused','focused_inactive','unfocused'):
                    foreground=colors['text'] if state=='focused' else colors['muted']
                    if run('swaymsg','client.'+state,colors['border'],colors['background'],foreground,colors['accent'],colors['border']):
                        warnings.append('Could not update window-frame colours')
            if a.set_windows and not placement_preferences:warnings.append('Window placement preferences require the updated compositor at next login')
            if placement_preferences:
                for command in ('lumen_window margins '+('enable' if window_prefs['tileMargins'] else 'disable'), 'lumen_window titlebar-action '+window_prefs['titlebarDoubleClick']):
                    if run('swaymsg',command):warnings.append('Could not apply window placement preference')
            if edge_preferences:
                commands=['lumen_window tile-gap '+str(window_prefs.get('tileGap',8))]
                for key,setting in (('edgeTiling','edge-tiling'),('optionTiling','option-tiling'),('topEdgeFill','top-edge-fill'),('topEdgeOverview','top-edge-overview')):
                    commands.append('lumen_window '+setting+' '+('enable' if window_prefs.get(key,True) else 'disable'))
                for command in commands:
                    if run('swaymsg',command):warnings.append('Could not apply window drag preference')
            elif a.set_windows and a.set_windows[0] in ('tileGap','edgeTiling','optionTiling','topEdgeFill','topEdgeOverview'):
                warnings.append('Window drag preferences require the updated compositor at next login')
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
        if a.set_menu_bar or a.set or a.set_search or a.set_notifications or a.set_control_centre or a.set_windows or a.set_desktop:print(json.dumps({'preferences':prefs,'searchPreferences':search_prefs,'notificationsPreferences':notification_prefs,'controlCentrePreferences':control_prefs,'menuBarPreferences':menu_prefs,'windowsPreferences':window_prefs,'desktopPreferences':desktop_prefs,'appearance':data,'system':system_info(),'warning':'; '.join(warnings)}))
        elif previous.get('mode')!=mode:print('Appearance:',mode,'London sunset:',times['sunset'])
if __name__=='__main__':
    try:main()
    except (OSError,ValueError,TypeError) as e:
        print(json.dumps({'error':str(e)}));sys.exit(1)
