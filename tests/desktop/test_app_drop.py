"""A rejected or mixed application drop must never partially launch or move files."""
import importlib.util
from pathlib import Path
import tempfile
import subprocess
import sys
import os
import json
import time
import types
import unittest
from unittest.mock import Mock

SOURCE=Path(__file__).resolve().parents[2]/'res/lumen-desktop/runtime/app_drop.py'
spec=importlib.util.spec_from_file_location('app_drop',SOURCE)
drop=importlib.util.module_from_spec(spec);spec.loader.exec_module(drop)

class AppDropTests(unittest.TestCase):
    def setUp(self):
        self.temp=tempfile.TemporaryDirectory();self.addCleanup(self.temp.cleanup)
        self.root=Path(self.temp.name)
        self.a=self.root/'literal ü # ? $(untouched).txt';self.a.write_text('first')
        self.b=self.root/'second.png';self.b.write_text('second')
        self.app=Mock();self.app.supports_files.return_value=True
        self.app.get_supported_types.return_value=['text/plain']
        self.app.launch_uris.return_value=True
        self.desktop=Mock();self.desktop.new.return_value=self.app
        def file_for(path):
            mime='image/png' if path.endswith('.png') else 'text/plain'
            return types.SimpleNamespace(query_info=lambda *args:types.SimpleNamespace(get_content_type=lambda:mime))
        self.gio=types.SimpleNamespace(File=types.SimpleNamespace(new_for_path=file_for),FileQueryInfoFlags=types.SimpleNamespace(NONE=0),content_type_is_a=lambda mime,kind:mime==kind,AppLaunchContext=lambda:None)
    def test_compatible_drop_preserves_literal_urls_and_original(self):
        result=drop.open_drop('fixture',[self.a.as_uri()],self.gio,self.desktop)
        self.assertEqual(result,{'ok':True,'count':1})
        self.desktop.new.assert_called_once_with('fixture.desktop')
        self.app.launch_uris.assert_called_once_with([self.a.as_uri()],None)
        self.assertEqual(self.a.read_text(),'first')
    def test_mixed_mime_drop_never_partially_launches(self):
        with self.assertRaises(ValueError):drop.open_drop('fixture',[self.a.as_uri(),self.b.as_uri()],self.gio,self.desktop)
        self.app.launch_uris.assert_not_called()
        self.assertTrue(self.a.exists());self.assertTrue(self.b.exists())
    def test_nonlocal_missing_and_malformed_urls_fail_before_launch(self):
        for uri in ['https://example.com/a','file://server/path','file:///tmp/%ZZ','file:///tmp/%00',self.a.as_uri()+'#bad',(self.root/'missing').as_uri(),self.root.as_uri()]:
            with self.subTest(uri=uri),self.assertRaises(ValueError):drop.open_drop('fixture',[self.a.as_uri(),uri],self.gio,self.desktop)
        self.app.launch_uris.assert_not_called()
    def test_unregistered_or_non_file_app_rejects_drop(self):
        self.desktop.new.return_value=None
        with self.assertRaises(ValueError):drop.open_drop('missing',[self.a.as_uri()],self.gio,self.desktop)
    def test_real_gio_file_launch_uses_selected_desktop_entry(self):
        try:
            import gi
        except ImportError:
            self.skipTest('GIO runtime required')
        apps=self.root/'data/applications';apps.mkdir(parents=True)
        log=self.root/'launch.json';receiver=self.root/'receiver.py'
        receiver.write_text('import json,sys\nfrom pathlib import Path\nPath(sys.argv[1]).write_text(json.dumps(sys.argv[2:]))\n')
        (apps/'lumen-drop-fixture.desktop').write_text('[Desktop Entry]\nType=Application\nName=Drop Fixture\nExec='+sys.executable+' '+str(receiver)+' '+str(log)+' %U\nMimeType=text/plain;\nTerminal=false\n')
        env=dict(os.environ,XDG_DATA_HOME=str(self.root/'data'),XDG_DATA_DIRS=str(self.root/'data'),XDG_CONFIG_HOME=str(self.root/'config'))
        result=subprocess.run([sys.executable,str(SOURCE),'lumen-drop-fixture',json.dumps([self.a.as_uri()])],env=env,capture_output=True,text=True,timeout=10)
        self.assertEqual(result.returncode,0,result.stdout+result.stderr)
        for _ in range(50):
            if log.exists():break
            time.sleep(.02)
        self.assertEqual(json.loads(log.read_text()),[str(self.a)])
        self.assertEqual(self.a.read_text(),'first')
    def test_empty_or_oversized_drop_rejected(self):
        for uris in [[],[self.a.as_uri()]*101,'file:///a']:
            with self.subTest(uris=type(uris)),self.assertRaises(ValueError):drop.open_drop('fixture',uris,self.gio,self.desktop)
        self.app.launch_uris.assert_not_called()

if __name__=='__main__':unittest.main()
