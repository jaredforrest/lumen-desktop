"""Owned resources retain observed bytes independently of their mutable source path."""
import importlib.util,json,os,stat,sys,tempfile,unittest
from unittest.mock import patch
from pathlib import Path
sys.path.insert(0,str(Path(__file__).resolve().parents[2]/'res/lumen-desktop/shell'))
from owned_preview import PreviewSession
class OwnedPreviewTests(unittest.TestCase):
    def setUp(self):
        self.temp=tempfile.TemporaryDirectory();self.root=Path(self.temp.name);self.previous=os.environ.get('XDG_RUNTIME_DIR');os.environ['XDG_RUNTIME_DIR']=str(self.root);self.session=PreviewSession()
    def tearDown(self):
        self.session.close();self.temp.cleanup()
        if self.previous is None:os.environ.pop('XDG_RUNTIME_DIR',None)
        else:os.environ['XDG_RUNTIME_DIR']=self.previous
    def test_image_uri_retains_original_bytes_until_explicit_retirement(self):
        from PIL import Image
        p=self.root/'image.png';Image.new('RGB',(8,8),'red').save(p);p.chmod(0o640);os.utime(p,(123456789,123456789));before=p.read_bytes();value=self.session.request(str(p));resource=Path(value['resourcePath']);self.assertEqual(value['mtime'],123456789);self.assertEqual(stat.S_IMODE(value['sourceFacts']['mode']),0o640);self.assertEqual(value['title'],'image.png');self.assertEqual(resource.read_bytes(),before);self.assertEqual(stat.S_IMODE(resource.stat().st_mode),0o400)
        replacement=self.root/'new.png';Image.new('RGB',(8,8),'blue').save(replacement);replacement.replace(p);self.assertEqual(resource.read_bytes(),before);self.assertNotEqual(p.read_bytes(),before)
        with self.assertRaises(ValueError):self.session.request(str(p),token=value['sourceToken'])
        self.session.close();self.assertFalse(resource.exists())
    def test_expected_selected_identity_and_symlink_are_not_retargeted(self):
        p=self.root/'selected.txt';p.write_text('original');old=dict(device=p.stat().st_dev,inode=p.stat().st_ino);p.rename(self.root/'retired');p.write_text('replacement')
        with self.assertRaises(ValueError):self.session.request(str(p),expected=old)
        alias=self.root/'alias';alias.symlink_to(p)
        with self.assertRaises(ValueError):self.session.request(str(alias))
    def test_provider_token_pages_and_other_provider_are_scoped(self):
        p=self.root/'file.txt';p.write_text('original');value=self.session.request(str(p));again=self.session.request(str(p),page=2,token=value['sourceToken']);self.assertEqual(value['resourcePath'],again['resourcePath']);other=PreviewSession()
        try:
            with self.assertRaises(ValueError):other.request(str(p),token=value['sourceToken'])
        finally:other.close()
        with self.assertRaises(ValueError):self.session.request(str(p),page=True)
    def test_bound_failure_never_returns_original_uri(self):
        p=self.root/'large.mkv'
        with p.open('wb') as stream:stream.truncate(64*1024*1024+1)
        with self.assertRaises(ValueError):self.session.request(str(p))
        self.assertFalse(self.session.resources.preview_snapshots)
    def test_rendered_cache_resource_survives_renderer_pruning(self):
        p=self.root/'document.pdf';p.write_bytes(b'synthetic source');rendered=self.root/'page.png';rendered.write_bytes(b'observed raster')
        with patch('owned_preview.preview',return_value=dict(type='image',uri=rendered.as_uri())):
            value=self.session.request(str(p))
        retained=Path(value['uri'].removeprefix('file://'));rendered.unlink();self.assertEqual(retained.read_bytes(),b'observed raster');self.session.close();self.assertFalse(retained.exists())
    def test_two_live_sessions_do_not_retire_each_others_resources(self):
        p=self.root/'file.txt';p.write_text('one');first=self.session.request(str(p));other=PreviewSession()
        try:
            second=other.request(str(p));self.assertNotEqual(first['resourcePath'],second['resourcePath']);self.session.close();self.assertTrue(Path(second['resourcePath']).exists())
        finally:other.close()
    def test_explicit_page_lease_retains_source_but_retires_previous_raster(self):
        source=self.root/'document.pdf';source.write_bytes(b'unchanged immutable input')
        rendered=self.root/'page.png';rendered.write_bytes(b'first raster')
        self.session.requests.register(1)
        with self.session.requests.work(1) as lease,patch('owned_preview.preview',return_value=dict(type='image',uri=rendered.as_uri(),page=1,pages=2)):
            first=self.session.request(str(source),request_id=1,lease=lease)
        source_resource=Path(first['resourcePath']);first_raster=Path(first['uri'].removeprefix('file://'))
        self.session.requests.retire([1],first['sourceToken']);self.assertTrue(source_resource.exists());self.assertFalse(first_raster.exists())
        rendered.write_bytes(b'second raster');self.session.requests.register(2,first['sourceToken'])
        with self.session.requests.work(2) as lease,patch('owned_preview.preview',return_value=dict(type='image',uri=rendered.as_uri(),page=2,pages=2)):
            second=self.session.request(str(source),page=2,token=first['sourceToken'],request_id=2,lease=lease)
        self.assertEqual(second['resourcePath'],str(source_resource));self.assertNotEqual(second['uri'],first['uri'])
        self.assertEqual(Path(second['uri'].removeprefix('file://')).read_bytes(),b'second raster')
        self.session.requests.retire([2]);self.assertEqual(self.session.resources.preview_bytes,0)
    def test_failed_renderer_releases_request_and_source_after_worker_unwinds(self):
        source=self.root/'bad.pdf';source.write_bytes(b'synthetic failure');self.session.requests.register(1)
        with self.assertRaises(ValueError):
            with self.session.requests.work(1) as lease,patch('owned_preview.preview',side_effect=ValueError('fixture renderer failure')):
                self.session.request(str(source),request_id=1,lease=lease)
        self.session.requests.retire([1]);self.assertEqual(self.session.resources.preview_bytes,0);self.assertFalse(self.session.requests.sources)
if __name__=='__main__':unittest.main()
