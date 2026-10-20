"""Private non-native Qt presentation fixtures; no host display, fonts or providers."""
import hashlib,json,os,re,socket,subprocess,sys
from pathlib import Path
from . import isolation


def immutable_file(value):
    value=Path(value).resolve()
    if not str(value).startswith('/nix/store/') or not value.is_file():raise ValueError('Existing immutable fixture dependency required')
    return value


def run(source,output,launcher,font_config,command):
    source,output=Path(source).resolve(),Path(output).resolve()
    launcher,font_config=immutable_file(launcher),immutable_file(font_config)
    if not command or Path(command[0]).resolve()!=Path(sys.executable).resolve():raise ValueError('Use the current pinned Python runtime')
    if output.exists():raise FileExistsError('Fresh artifact directory required')
    revision=subprocess.check_output(['git','-C',str(source),'rev-parse','HEAD'],text=True).strip()
    if subprocess.check_output(['git','-C',str(source),'status','--porcelain'],text=True).strip():raise ValueError('Freeze source before private Qt acceptance')
    script=Path(command[1]) if len(command)>1 else Path('.')
    if not script.is_relative_to('/source'):raise ValueError('A source-mounted Python fixture is required')
    script_source=source/script.relative_to('/source')
    if not script_source.is_file():raise ValueError('Fixture source script is unavailable')
    hashes={key:hashlib.sha256(path.read_bytes()).hexdigest() for key,path in (('sourceSha256',script_source),('launcherSha256',launcher),('fontConfigSha256',font_config))}
    output.mkdir(mode=0o700,parents=True,exist_ok=False)
    prefix=re.split(r'\n[ \t]*exec quickshell ',launcher.read_text(),maxsplit=1)[0]
    environment=dict(value.decode().split('=',1) for value in subprocess.check_output(['bash','-c',prefix+'\nexec env -0']).split(b'\0') if b'=' in value)
    environment['FONTCONFIG_FILE']=str(font_config)
    result=isolation.run(source,output,command,environment)
    unchanged=(subprocess.check_output(['git','-C',str(source),'rev-parse','HEAD'],text=True).strip()==revision and not subprocess.check_output(['git','-C',str(source),'status','--porcelain'],text=True).strip() and hashlib.sha256(script_source.read_bytes()).hexdigest()==hashes['sourceSha256'])
    (output/'offscreen-boundary.json').write_text(json.dumps(dict(sourceRevision=revision,sourceScript=str(script),**hashes,launcher=str(launcher),fontConfig=str(font_config),command=command,exitCode=result,sourceUnchanged=unchanged),indent=2))
    if not unchanged:raise ValueError('Fixture source changed during private Qt acceptance')
    return result


def environment():
    if os.getuid()==0 or os.environ.get('HOME')!='/home/fixture' or set(Path('/home').iterdir())!={Path('/home/fixture')} or Path('/.old-root').exists():raise ValueError('Full private fixture boundary required')
    if [name for _,name in socket.if_nameindex()]!=['lo']:raise ValueError('Private loopback network required')
    if any((Path('/dev')/name).exists() for name in ('dri','snd','video0','tty','udmabuf')):raise ValueError('No hardware or presentation devices may be exposed')
    env=dict(os.environ,QT_QPA_PLATFORM='offscreen',QT_QUICK_BACKEND='software',QT_QPA_PLATFORMTHEME='generic',QT_ACCESSIBILITY='0',QT_LINUX_ACCESSIBILITY_ALWAYS_ON='0',NO_AT_BRIDGE='1',GTK_A11Y='none',GLOBAL_MENU_PYTHON=sys.executable,LUMEN_NATIVE_WINDOW_CONTROLS='0',LUMEN_UNIFIED_OWNED_HEADERS='1',LUMEN_NEARBY_SHARING_LABEL='Nearby Sharing',LUMEN_NEARBY_SHARING_PROVIDER='',LUMEN_FILE_SERVICE='0',LUMEN_LAUNCH_SCOPES='0')
    for key in ('WAYLAND_DISPLAY','DISPLAY','SWAYSOCK','AT_SPI_BUS_ADDRESS'):env.pop(key,None)
    return env
