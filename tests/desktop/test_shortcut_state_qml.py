"""Actual Qt reads effective registry and retires stale hints on file failure."""
import importlib.util,json,os,re,shutil,subprocess,tempfile,time,unittest
from pathlib import Path
ROOT=Path(__file__).resolve().parents[2]
spec=importlib.util.spec_from_file_location('shortcuts',ROOT/'res/lumen-desktop/runtime/shortcuts.py');registry=importlib.util.module_from_spec(spec);spec.loader.exec_module(registry)
class ShortcutStateQt(unittest.TestCase):
 def test_actual_file_reload_and_fail_closed(self):
  launcher=Path(os.environ.get('LUMEN_TEST_SHELL_LAUNCHER') or shutil.which('lumen-desktop-shell'))
  prefix=re.split(r'\n[ \t]*exec quickshell ',launcher.read_text(),maxsplit=1)[0]
  with tempfile.TemporaryDirectory(prefix='lumen-shortcut-qt-') as temporary:
   root=Path(temporary);root.chmod(0o700);env=dict(os.environ,HOME=temporary,XDG_RUNTIME_DIR=temporary,DBUS_SYSTEM_BUS_ADDRESS='unix:path='+temporary+'/no-system-bus')
   for key in ('CONFIG','STATE','CACHE','DATA'):env['XDG_'+key+'_HOME']=str(root/key.lower())
   for key in ('DISPLAY','WAYLAND_DISPLAY','SWAYSOCK','DBUS_SESSION_BUS_ADDRESS','AT_SPI_BUS_ADDRESS'):env.pop(key,None)
   env.update(dict(value.decode().split('=',1) for value in subprocess.check_output(['bash','-c',prefix+'\nexec env -0'],env=env).split(b'\0') if b'=' in value))
   env.update(QT_QPA_PLATFORM='offscreen',QT_QPA_PLATFORMTHEME='generic',QT_QUICK_BACKEND='software',QT_ACCESSIBILITY='0',NO_AT_BRIDGE='1',GTK_A11Y='none')
   source=Path(env['XDG_CONFIG_HOME'])/'lumen-desktop/shortcuts.json';source.parent.mkdir(parents=True)
   def publish(chord):
    value=registry.validate({'version':1,'bindings':[{'id':'test','action':'window.fullscreen.toggle','chord':chord}]});temp=source.with_suffix('.tmp');temp.write_text(json.dumps(value));temp.replace(source)
   publish('Mod4+Control+f')
   shutil.copytree(ROOT/'res/lumen-desktop/shell',root/'assets')
   qml=root/'shell.qml';qml.write_text('''import QtQuick
import Quickshell
import "assets" as D
ShellRoot {
 D.ShortcutState {id:shortcuts}
 property string last:"unreported"
 Timer {running:true;repeat:true;interval:25;onTriggered:{const value=JSON.stringify({available:shortcuts.available,hint:shortcuts.hint("window.fullscreen.toggle"),bindings:shortcuts.bindings.length});if(value!==last){last=value;console.log("SHORTCUT_STATE "+value);}}}
}''')
   log=root/'qt.log'
   with log.open('w') as stream:process=subprocess.Popen(['quickshell','-p',str(qml)],env=env,stdout=stream,stderr=subprocess.STDOUT)
   def wait(expected):
    end=time.monotonic()+8
    while time.monotonic()<end:
     states=[json.loads(line.split('SHORTCUT_STATE ',1)[1]) for line in log.read_text().splitlines() if 'SHORTCUT_STATE ' in line]
     if states and all(states[-1].get(k)==v for k,v in expected.items()):return
     if process.poll() is not None:break
     time.sleep(.03)
    self.fail(log.read_text())
   try:
    wait({'available':True,'hint':'Ctrl+Super+F','bindings':1});publish('Mod4+Control+x');wait({'available':True,'hint':'Ctrl+Super+X'})
    source.write_text('{bad');wait({'available':False,'hint':'','bindings':0});publish('Control+z');wait({'available':True,'hint':'Ctrl+Z'})
    source.unlink();wait({'available':False,'hint':'','bindings':0})
   finally:
    process.terminate();process.wait(timeout=5)
   for marker in ('ReferenceError:','TypeError:','Binding loop','Cannot assign','Failed to load configuration','ResourceWarning','Exception ignored'):
    self.assertNotIn(marker,log.read_text())
if __name__=='__main__':unittest.main()
