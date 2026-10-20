"""Detached filesystem, process and network boundary for native saver acceptance."""
import argparse,hashlib,json,os,re,shutil,subprocess as sp,sys
from pathlib import Path
from application_fixture import isolation,offscreen
p=argparse.ArgumentParser();p.add_argument('--launcher',type=Path,required=True);p.add_argument('--sway',type=Path,required=True);p.add_argument('--pointer-tool',type=Path,required=True);p.add_argument('--lock',type=Path,required=True);p.add_argument('--fontconfig',type=Path,required=True);p.add_argument('--egl',type=Path);p.add_argument('--effects',action='store_true');p.add_argument('--output',type=Path,required=True);a=p.parse_args();repo=Path(__file__).resolve().parents[2]
def source():
 assert not sp.check_output(['git','-C',str(repo),'status','--porcelain'],text=True).strip(),'Freeze source before native acceptance'
 return sp.check_output(['git','-C',str(repo),'rev-parse','HEAD'],text=True).strip()
revision=source();tools={name:offscreen.immutable_file(getattr(a,name)) for name in ('launcher','sway','pointer_tool','lock','fontconfig')};python=offscreen.immutable_file(sys.executable)
if a.effects:tools['egl']=offscreen.immutable_file(a.egl)
for name in ('bash','dbus-run-session','dbus-daemon','grim','wtype','swaymsg','quickshell'):
 located=shutil.which(name);assert located,'Missing fixture runtime: '+name
 tools[name]=offscreen.immutable_file(located)
hashes={name:hashlib.sha256(path.read_bytes()).hexdigest() for name,path in tools.items()}
prefix=re.split(r'\n[ \t]*exec quickshell ',tools['launcher'].read_text(),maxsplit=1)[0]
env=dict(value.decode().split('=',1) for value in sp.check_output(['bash','-c',prefix+'\nexec env -0']).split(b'\0') if b'=' in value);env['FONTCONFIG_FILE']=str(tools['fontconfig']);env['PATH']=':'.join(str(path.parent) for path in tools.values())+':'+env['PATH']
args=['/source/tests/desktop/screen_saver_native.py','--output','/artifacts/result']
for name in ('launcher','sway','pointer_tool','lock'):args+=['--'+name.replace('_','-'),str(tools[name])]
if a.effects:args+=['--effects']
metadata={'__EGL_VENDOR_LIBRARY_FILENAMES':str(tools['egl'])} if a.effects else {}
code='import os,runpy,sys;os.environ.update('+repr(metadata)+');sys.path.insert(0,"/source/tests/desktop");sys.argv='+repr(args)+';runpy.run_path(sys.argv[0],run_name="__main__")'
a.output.mkdir(mode=0o700,parents=True,exist_ok=False)
result=isolation.run(repo,a.output,[str(python),'-c',code],env,software_graphics=a.effects)
unchanged=source()==revision and all(hashlib.sha256(tools[name].read_bytes()).hexdigest()==digest for name,digest in hashes.items())
(a.output/'namespace-provenance.json').write_text(json.dumps(dict(sourceRevision=revision,sourceUnchanged=unchanged,tools={name:str(path) for name,path in tools.items()},toolHashes=hashes,softwareGraphics=a.effects,exitCode=result),indent=2)+'\n')
assert unchanged,'Native acceptance source changed'
raise SystemExit(result)
