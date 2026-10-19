import json,os,sys,tempfile,unittest
from pathlib import Path
from unittest.mock import patch
sys.path.insert(0,str(Path(__file__).resolve().parents[2]/'res/lumen-desktop/shell'))
import desktop_layout as layout
class DesktopLayout(unittest.TestCase):
    def setUp(self):
        self.temporary=tempfile.TemporaryDirectory();self.root=Path(self.temporary.name);self.folder=self.root/'Desktop';self.folder.mkdir();self.environment=patch.dict(os.environ,{'XDG_STATE_HOME':str(self.root/'state')});self.environment.start();self.request={'directory':str(self.folder),'items':['1:2','1:3']}
    def tearDown(self):self.environment.stop();self.temporary.cleanup()
    def test_identity_persistence_rename_and_authoritative_removal(self):
        value={'grouping':'kind','positions':{'1:2':[.2,.8],'1:3':[.4,.6]}}
        layout.handle(dict(self.request,action='save',layout=value));self.folder.rename(self.root/'Renamed');self.request['directory']=str(self.root/'Renamed')
        self.assertEqual(layout.handle(dict(self.request,action='load'))['layout'],value)
        self.request['items']=['1:2'];remaining=layout.handle(dict(self.request,action='load'))['layout'];self.assertEqual(remaining['positions'],{'1:2':[.2,.8]})
        self.request['items']=['1:2','1:3'];self.assertNotIn('1:3',layout.handle(dict(self.request,action='load'))['layout']['positions'])
    def test_distinct_directory_isolation_and_reset(self):
        layout.handle(dict(self.request,action='save',layout={'grouping':'tags','positions':{'1:2':[.5,.5]}}));other=self.root/'Other';other.mkdir()
        self.assertEqual(layout.handle(dict(self.request,action='load',directory=str(other)))['layout']['positions'],{})
        self.assertEqual(layout.handle(dict(self.request,action='reset'))['layout'],{'grouping':'none','positions':{}})
    def test_rejects_nonfinite_out_of_bounds_identity_unknown_fields_and_unsafe_storage(self):
        for point in [[float('nan'),0],[0,float('inf')],[-.1,0],[0,1.1],[True,0]]:
            with self.assertRaises(ValueError):layout.handle(dict(self.request,action='save',layout={'grouping':'none','positions':{'1:2':point}}))
        with self.assertRaises(ValueError):layout.handle(dict(self.request,action='load',items=['../../outside']))
        with self.assertRaises(ValueError):layout.handle(dict(self.request,action='load',unexpected=True))
        layout.handle(dict(self.request,action='load'));root=self.root/'state/lumen-desktop/desktop-layouts';root.chmod(0o755)
        with self.assertRaises(ValueError):layout.handle(dict(self.request,action='load'))
    def test_rejects_symlink_file_without_writing_target(self):
        layout.handle(dict(self.request,action='load'));path=next((self.root/'state/lumen-desktop/desktop-layouts').glob('*.json'));path.unlink();target=self.root/'target';target.write_text('keep');path.symlink_to(target)
        with self.assertRaises(OSError):layout.handle(dict(self.request,action='load'))
        self.assertEqual(target.read_text(),'keep')
if __name__=='__main__':unittest.main()
