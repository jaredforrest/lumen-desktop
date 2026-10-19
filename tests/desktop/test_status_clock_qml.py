"""Exercise actual control states and textual semantics without system actions."""
import os,re,shutil,subprocess,tempfile
from pathlib import Path
repo=Path(__file__).resolve().parents[2]
launcher=os.environ.get('LUMEN_TEST_SHELL_LAUNCHER') or shutil.which('lumen-desktop-shell')
prefix=re.split(r'\n[ \t]*exec quickshell ',Path(launcher).read_text(),maxsplit=1)[0]
with tempfile.TemporaryDirectory(prefix='lumen-semantic-states-') as directory:
    env=dict(os.environ,HOME=directory,XDG_RUNTIME_DIR=directory)
    for key in ('CONFIG','STATE','CACHE','DATA'):env['XDG_'+key+'_HOME']=directory+'/'+key.lower()
    for key in ('WAYLAND_DISPLAY','DISPLAY','DBUS_SESSION_BUS_ADDRESS','AT_SPI_BUS_ADDRESS'):env.pop(key,None)
    exported=subprocess.check_output(['bash','-c',prefix+'\nexec env -0'],env=env)
    env.update(dict(value.decode().split('=',1) for value in exported.split(b'\0') if b'=' in value))
    env.update(QT_QPA_PLATFORM='offscreen',QT_QPA_PLATFORMTHEME='generic',QT_QUICK_BACKEND='software',QT_ACCESSIBILITY='0',QT_LINUX_ACCESSIBILITY_ALWAYS_ON='0',NO_AT_BRIDGE='1',GTK_A11Y='none')
    path=Path(directory)/'shell.qml'
    shutil.copytree(repo/'res/lumen-desktop',Path(directory)/'assets')
    path.write_text('''import QtQuick
import Quickshell
import "assets/shell" as D
import "assets/shell/ClockFormat.js" as Clock
ShellRoot {
 D.StatusStrip {id:strip;order:["search","battery","sound","focus","controls"];available:({sound:true,search:true,controls:true});budget:76}
 D.ClockSettings {service:QtObject {property var menuBarPreferences:({hourCycle:"locale",locale:"",showDate:true,showWeekday:true,showSeconds:false,order:[]});property bool available:true;property bool busy:false;function setMenuBarPreference(k,v){}}}
 Timer {interval:100;running:true;onTriggered:{try {
  if(strip.layout.visible.join(",")!=="sound,controls" || strip.layout.overflow.join(",")!=="search")throw Error("Overflow");
  const midnight=new Date(2026,9,19,0,5,9),prefs={hourCycle:"24",showDate:false,showWeekday:false,showSeconds:true};
  if(Clock.text(midnight,Qt.locale("en_GB"),prefs)!=="00:05:09")throw Error("24h midnight "+Clock.text(midnight,Qt.locale("en_GB"),prefs));
  prefs.hourCycle="12";
  if(Clock.text(midnight,Qt.locale("en_US"),prefs)!=="12:05:09 AM")throw Error("12h midnight");
  prefs.hourCycle="locale";prefs.showSeconds=false;
  if(!Clock.text(midnight,Qt.locale("en_US"),prefs).includes("AM"))throw Error("Locale cycle");
  prefs.showDate=true;prefs.showWeekday=true;
  const french=Clock.text(midnight,Qt.locale("fr_FR"),prefs);
  if(!french.includes("lun.") || !french.includes("oct."))throw Error("Locale date "+french);
  console.log("STATUS_CLOCK_PASS");Qt.quit();
 }catch(error){console.error("STATUS_CLOCK_FAIL "+error);Qt.quit();}}}
}''')
    source=path.read_text()
    checks={"Europe/London":["00:30:00","02:30:00","01:30:00","01:30:00"],"America/New_York":["20:30:00","21:30:00","20:30:00","21:30:00"],"UTC":["00:30:00","01:30:00","00:30:00","01:30:00"]}
    for zone,expected in checks.items():
      proof='const instants=["2026-03-29T00:30:00Z","2026-03-29T01:30:00Z","2026-10-25T00:30:00Z","2026-10-25T01:30:00Z"];\n      const expected=EXPECTED;\n      const timePrefs={hourCycle:"24",showDate:false,showWeekday:false,showSeconds:true};\n      for(let i=0;i<instants.length;i++)if(Clock.text(new Date(instants[i]),Qt.locale("en_GB"),timePrefs)!==expected[i])throw Error("Timezone/DST "+instants[i]+" "+Clock.text(new Date(instants[i]),Qt.locale("en_GB"),timePrefs));\n      '.replace('EXPECTED',__import__('json').dumps(expected))
      path.write_text(source.replace('console.log("STATUS_CLOCK_PASS")',proof+'console.log("STATUS_CLOCK_PASS")'))
      env['TZ']=zone
      result=subprocess.run(['quickshell','-p',str(path)],env=env,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,timeout=15)
      assert result.returncode==0 and 'STATUS_CLOCK_PASS' in result.stdout,result.stdout
      assert not any(value in result.stdout for value in ('STATUS_CLOCK_FAIL','Binding loop','ReferenceError:','TypeError:','Failed to load configuration')),result.stdout
print('PASS actual QML status partition, settings component, midnight 12/24 and localized weekday/date')
