"""Disposable image references, exact output scope and fallback acceptance."""
import copy,json,os,sys,tempfile,unittest
from pathlib import Path
from unittest.mock import patch
from PIL import Image
ROOT=Path(__file__).resolve().parents[2];sys.path.insert(0,str(ROOT/'res/lumen-desktop/shell'));sys.path.insert(0,str(Path(__file__).parent/'fixtures'))
import wallpaper_settings as wall
from input_ipc import settings_adapter

class WallpaperSettings(unittest.TestCase):
    def test_images_scope_restart_disconnect_and_failure(self):
        original=dict(os.environ)
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory);os.environ.update(settings_adapter(root));os.environ['XDG_RUNTIME_DIR']=directory
            red=root/'red ; image.png';blue=root/'blue.jpg';Image.new('RGB',(100,80),'red').save(red);Image.new('RGB',(80,120),'blue').save(blue)
            outputs=[dict(name='DP-1',make='Fixture',model='Panel',serial='123',active=True,scale=1),dict(name='DP-2',make='Fixture',model='Panel',serial='456',active=True,scale=2)]
            actions=[]
            def provider(*args):
                if args==('-t','get_outputs'):return copy.deepcopy(outputs)
                actions.append(args[0]);return [dict(success=True)]
            try:
                with patch.object(wall,'sway',side_effect=provider):
                    wall.change(dict(scope='default',image=str(red)))
                    wall.change(dict(scope='display',identity=wall.identity(outputs[1]),image=str(blue)))
                    snapshot=wall.status();self.assertEqual([row['image'] for row in snapshot['outputs']],[str(red),str(blue)])
                    before=red.read_bytes();self.assertEqual(snapshot['outputs'][0]['metadata']['format'],'PNG');self.assertEqual(red.read_bytes(),before)
                    with patch.object(wall.Image,'open',side_effect=AssertionError('Unchanged image decoded again')):self.assertEqual(wall.image_info(str(red))['format'],'PNG')
                    listed=wall.browse(str(red));self.assertEqual(listed['selected'],str(red));self.assertTrue(any(row['path']==str(blue) for row in listed['entries']))
                    with self.assertRaises(ValueError):wall.browse('relative')
                    outputs.pop();self.assertEqual(len(wall.status()['disconnected']),1);wall.apply();self.assertIn('DP-1',actions[-1]);self.assertNotIn('DP-2',actions[-1])
                    with self.assertRaisesRegex(ValueError,'disconnected'):wall.change(dict(scope='display',identity='connector:missing',image=str(red)))
                    outputs.append(dict(name='renamed',make='Fixture',model='Panel',serial='456',active=True,scale=1.5));self.assertEqual(wall.status()['outputs'][1]['image'],str(blue))
                    # Fresh read, with selected image missing, retains its reference and uses fallback.
                    blue.unlink();snapshot=wall.apply();self.assertTrue(snapshot['outputs'][1]['fallback']);self.assertEqual(str((wall.runtime()/'wallpaper-images'/(snapshot['outputs'][1]['key']+'.png')).readlink()),wall.fallback());self.assertEqual(json.loads((root/'settings.json').read_text())['wallpaper']['displays'][wall.key_for(wall.identity(outputs[1]))]['image'],str(blue))
                    wall.change(dict(scope='display',identity=wall.identity(outputs[1]),image=None));self.assertEqual(wall.status()['outputs'][1]['image'],str(red))
                    (root/'settings.deny').touch();previous=(root/'settings.json').read_text()
                    with self.assertRaises(ValueError):wall.change(dict(scope='default',image=None))
                    self.assertEqual((root/'settings.json').read_text(),previous)
                    for path in ('https://example.invalid/x.png','relative.png','/bad\nname'):
                        with self.assertRaises(ValueError):wall.image_info(path)
                    bad=root/'bad.png';bad.write_text('not an image')
                    with self.assertRaises(OSError):wall.image_info(str(bad))
                    animated=root/'animated.gif';Image.new('RGB',(10,10),'red').save(animated)
                    with self.assertRaises(ValueError):wall.image_info(str(animated))
                    red.write_bytes(b'changed invalid contents');self.assertTrue(wall.status()['outputs'][0]['fallback'])
            finally:os.environ.clear();os.environ.update(original)

if __name__=='__main__':unittest.main()
