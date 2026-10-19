"""Load the real layout surface and exercise its event authority on a private Qt instance."""
import os
from pathlib import Path
import re
import select
import shutil
import subprocess
import tempfile
import time
import unittest


class LayoutMenuTest(unittest.TestCase):
    def test_packaged_surface_and_target_validation(self):
        root = Path(__file__).resolve().parents[2]
        launcher = os.environ.get('LUMEN_TEST_SHELL_LAUNCHER') or shutil.which('lumen-desktop-shell')
        self.assertTrue(launcher)
        prefix = re.split(r'\n[ \t]*exec quickshell ', Path(launcher).read_text(), maxsplit=1)[0]
        raw = subprocess.check_output(['bash', '-c', prefix+'\nexec env -0'])
        env = dict(v.decode().split('=', 1) for v in raw.split(b'\0') if b'=' in v)
        temporary_directory = tempfile.TemporaryDirectory()
        self.addCleanup(temporary_directory.cleanup)
        temporary = temporary_directory.name
        path = Path(temporary)
        for key in ('HOME','XDG_RUNTIME_DIR','XDG_CONFIG_HOME','XDG_STATE_HOME','XDG_DATA_HOME','XDG_CACHE_HOME'):
            env[key] = str(path/key)
            Path(env[key]).mkdir(mode=0o700)
        for key in ('SWAYSOCK','WAYLAND_DISPLAY','DISPLAY','DBUS_SESSION_BUS_ADDRESS'):
            env.pop(key, None)
        env.update(QT_QPA_PLATFORM='offscreen',QT_QUICK_BACKEND='software',QT_QPA_PLATFORMTHEME='generic',QT_IM_MODULE='',NO_AT_BRIDGE='1',QT_ACCESSIBILITY='0',QT_LINUX_ACCESSIBILITY_ALWAYS_ON='0',GTK_A11Y='none')
        # The compositor wrapper otherwise starts a bus using the host's
        # /etc configuration, which is absent on clean CI runners.
        daemon = Path(shutil.which('dbus-daemon')).resolve()
        # No activation directories: this fixture tests a local menu, not
        # portals or the host's user services.
        bus_config = path/'bus.conf'
        bus_config.write_text('<busconfig><type>session</type>'
                              '<listen>unix:tmpdir='+env['XDG_RUNTIME_DIR']+'</listen>'
                              '<policy context="default"><allow own="*"/>'
                              '<allow send_destination="*"/>'
                              '<allow receive_sender="*"/></policy></busconfig>')
        bus = subprocess.Popen([str(daemon), '--nofork', '--print-address=1',
                                '--config-file='+str(bus_config)], env=env,
                               stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
        def stop_bus():
            if bus.poll() is None: bus.terminate()
            bus.wait(timeout=5)
            bus.stdout.close()
            bus.stderr.close()
        self.addCleanup(stop_bus)
        self.assertTrue(select.select([bus.stdout], [], [], 5)[0], 'private bus did not start')
        address = bus.stdout.readline().strip()
        self.assertTrue(address.startswith('unix:'), address)
        env['DBUS_SESSION_BUS_ADDRESS'] = address
        config=path/'sway.conf'
        config.write_text('output * resolution 800x600\n')
        compositor_env=dict(env,WLR_BACKENDS='headless',WLR_RENDERER='pixman',WLR_LIBINPUT_NO_DEVICES='1')
        log=(path/'compositor.log').open('w')
        self.addCleanup(log.close)
        compositor=subprocess.Popen([os.environ.get('LUMEN_TEST_COMPOSITOR','sway'),'--unsupported-gpu','-c',str(config)],env=compositor_env,stdout=log,stderr=subprocess.STDOUT)
        def stop():
            if compositor.poll() is None:compositor.terminate()
            compositor.wait(timeout=5)
        self.addCleanup(stop)
        until=time.monotonic()+15
        while time.monotonic()<until and not list(Path(env['XDG_RUNTIME_DIR']).glob('wayland-*')):
            time.sleep(.05)
        displays=[p for p in Path(env['XDG_RUNTIME_DIR']).glob('wayland-*') if not p.name.endswith('.lock')]
        self.assertTrue(displays,'headless compositor failed: '+(path/'compositor.log').read_text())
        env['WAYLAND_DISPLAY']=displays[0].name
        env['QT_QPA_PLATFORM']='wayland'
        assets = root/'res/lumen-desktop/shell'
        (path/'shell.qml').write_text('''import QtQuick
import Quickshell
import "file:'''+str(assets)+'''" as D
ShellRoot {
 QtObject {id:stub;property string popupOwner:"";property double dragged:0;function dragOverview(id){dragged=id;}signal dismissPopups(string owner);function claimPopup(owner){popupOwner=owner;dismissPopups(owner);}}
 D.WindowLayoutMenu {id:menu;shell:stub}
 Timer {id:finish;interval:200;onTriggered:{console.log("LAYOUT_MENU_PASS");menu.dismiss();Qt.quit();}}
 function check(x,m){if(!x)throw new Error(m);}
 Timer {interval:100;running:true;onTriggered:{
  menu.event('{"lumen_layout":1,"id":42,"x":20,"y":30}');check(menu.target===42,"valid target");
  menu.event('{"lumen_layout":1,"id":"43","x":20,"y":30}');check(menu.target===42,"reject string target");
  menu.event('{"lumen_layout":1,"id":-1,"x":20,"y":30}');check(menu.target===42,"reject negative target");
  menu.event('{"lumen_layout":1,"id":43,"x":"bad","y":30}');check(menu.target===42,"reject nonfinite coordinate");
  check(menu.choices.length===11,"layout choices");
  menu.updateCapabilities('{"native_edge_tiling":true}');
  menu.event('{"lumen_drag_overview":1,"id":99}');check(stub.dragged===99,"captured drag target");
  menu.event('{"lumen_drag_overview":1,"id":"100"}');check(stub.dragged===99,"reject string drag target");
  menu.updateCapabilities('{}');menu.event('{"lumen_drag_overview":1,"id":100}');check(stub.dragged===99,"ignore unavailable drag capability");
  menu.updateCapabilities('{"native_window_layouts":true,"native_show_desktop":true}');
  check(menu.supported && menu.showDesktopSupported,"native capabilities accepted");
  menu.updateCapabilities('{"native_window_layouts":"true","native_show_desktop":1}');
  check(!menu.supported && !menu.showDesktopSupported,"nonboolean capabilities rejected");
  menu.updateCapabilities('{"native_window_layouts":true,"native_show_desktop":true}');
  menu.updateCapabilities('invalid');
  check(!menu.supported && !menu.showDesktopSupported,"invalid capabilities reset both");
  menu.updateCapabilities('{"native_window_layouts":true,"native_show_desktop":true}');
  stub.dismissPopups("other");check(menu.target===0,"cancel pending on other popup");
  menu.event('{"lumen_layout":1,"id":42,"x":20,"y":30}');menu.open();
  check(menu.visible,"layout menu opens on available screen");
  finish.start();
 }}
}''')
        try:
            result = subprocess.run(['quickshell','-p',str(path/'shell.qml')],env=env,capture_output=True,text=True,timeout=20)
        except subprocess.TimeoutExpired as error:
            self.fail(str(error.stdout)+str(error.stderr))
        self.assertEqual(result.returncode,0,result.stdout+result.stderr)
        self.assertIn('LAYOUT_MENU_PASS',result.stdout+result.stderr)
        self.assertNotIn('Failed to load',result.stdout+result.stderr)


if __name__=='__main__':
    unittest.main()
