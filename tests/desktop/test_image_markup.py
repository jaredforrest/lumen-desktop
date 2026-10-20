"""Disposable raster copy exports never truncate originals or existing targets."""
import importlib.util,os,sys,tempfile,threading,unittest
from pathlib import Path
ROOT=Path(__file__).resolve().parents[2];sys.path.insert(0,str(ROOT/'res/lumen-desktop/shell'))
from PIL import Image
from captured_resource_actions import CapturedResourceActions,ResourceUnavailable
from image_markup import recipe
class Markup(unittest.TestCase):
 def setUp(self):
  self.temp=tempfile.TemporaryDirectory();self.root=Path(self.temp.name);self.source=self.root/'original.png';Image.new('RGB',(100,80),'white').save(self.source);self.original=self.source.read_bytes();self.snapshot=self.root/'captured.png';self.snapshot.write_bytes(self.original);self.actions=CapturedResourceActions();self.key=self.actions.capture(self.source,resource=self.snapshot)
 def tearDown(self):self.temp.cleanup()
 def request(self,name='copy.png',**extra):return dict(destination=str(self.root/name),strokes=[dict(tool='pen',color='#ff0000',width=.025,points=[[.1,.5],[.9,.5]])],**extra)
 def test_saved_copy_pixels_original_hash_and_no_staging_left(self):
  out=self.actions.annotate(self.key,self.request());self.assertTrue(out['saved']);self.assertEqual(self.source.read_bytes(),self.original)
  with Image.open(out['path']) as image:self.assertEqual(image.getpixel((50,40)),(255,0,0,255))
  self.assertFalse(list(self.root.glob('.lumen-markup-*')))
 def test_same_path_existing_and_symlink_destinations_never_overwritten(self):
  for name in ('original.png','exists.png','link.png'):
   (self.root/'exists.png').write_bytes(b'untouched')
   if not (self.root/'link.png').exists():(self.root/'link.png').symlink_to(self.source)
   with self.subTest(name=name),self.assertRaises(ResourceUnavailable) as caught:self.actions.annotate(self.key,self.request(name))
   self.assertEqual(caught.exception.reason,'target_exists');self.assertEqual(self.source.read_bytes(),self.original);self.assertEqual((self.root/'exists.png').read_bytes(),b'untouched');self.assertFalse(list(self.root.glob('.lumen-markup-*')))
 def test_replaced_source_or_retired_capability_cannot_export(self):
  self.source.unlink();self.source.write_bytes(self.original)
  with self.assertRaises(ResourceUnavailable):self.actions.annotate(self.key,self.request())
  self.assertFalse((self.root/'copy.png').exists());self.actions.retire([self.key])
  with self.assertRaises(ResourceUnavailable):self.actions.annotate(self.key,self.request())
 def test_cancel_and_stale_mid_export_cleanup(self):
  canceled=threading.Event();canceled.set()
  with self.assertRaises(ResourceUnavailable) as caught:self.actions.annotate(self.key,self.request(),canceled)
  self.assertEqual(caught.exception.reason,'canceled');self.assertFalse((self.root/'copy.png').exists())
  from image_markup import save_copy
  entry=self.actions.selected(self.key);calls=[]
  def validate():
   calls.append(True)
   if list(self.root.glob('.lumen-markup-*')):raise ResourceUnavailable('source_changed','changed')
  with self.assertRaises(ResourceUnavailable):save_copy(entry,self.request(),validate)
  self.assertFalse((self.root/'copy.png').exists());self.assertFalse(list(self.root.glob('.lumen-markup-*')))
 def test_rectangle_and_file_uri(self):
  req=self.request();req['destination']=(self.root/'uri copy.png').as_uri();req['strokes']=[dict(tool='rectangle',color='#0000ff',width=.02,points=[[.8,.8],[.2,.2]])];self.actions.annotate(self.key,req)
  with Image.open(self.root/'uri copy.png') as image:self.assertEqual(image.getpixel((20,16)),(0,0,255,255))
 def test_missing_and_denied_destination_and_nonregular_resource(self):
  req=self.request();req['destination']=str(self.root/'missing/copy.png')
  with self.assertRaises(ResourceUnavailable) as caught:self.actions.annotate(self.key,req)
  self.assertEqual(caught.exception.reason,'destination_unavailable')
  denied=self.root/'denied';denied.mkdir();denied.chmod(0o500);req['destination']=str(denied/'copy.png')
  try:
   with self.assertRaises(ResourceUnavailable) as caught:self.actions.annotate(self.key,req)
   self.assertEqual(caught.exception.reason,'permission_denied')
  finally:denied.chmod(0o700)
  fifo=self.root/'fifo';os.mkfifo(fifo);self.actions.bind_resource(self.key,fifo)
  with self.assertRaises(ResourceUnavailable) as caught:self.actions.annotate(self.key,self.request())
  self.assertEqual(caught.exception.reason,'source_changed')
 def test_raster_work_rejected_before_draw_or_publication(self):
  from unittest.mock import patch
  Image.new('RGB',(512,512),'white').save(self.source);self.snapshot.write_bytes(self.source.read_bytes());key=self.actions.capture(self.source,resource=self.snapshot);req=self.request();req['strokes']=[dict(tool='pen',color='#000000',width=.05,points=[[0,0],[1,1]]*4096)]
  with patch('PIL.ImageDraw.Draw',side_effect=AssertionError('Drawing reached before preflight')):
   with self.assertRaises(ResourceUnavailable) as caught:self.actions.annotate(key,req)
  self.assertEqual(caught.exception.reason,'policy_limit');self.assertFalse((self.root/'copy.png').exists());self.assertFalse(list(self.root.glob('.lumen-markup-*')))
 def test_requested_parent_symlink_retarget_fails_closed(self):
  from image_markup import save_copy
  first=self.root/'first';second=self.root/'second';first.mkdir();second.mkdir();alias=self.root/'alias';alias.symlink_to(first,target_is_directory=True);req=self.request();req['destination']=str(alias/'copy.png');entry=self.actions.selected(self.key)
  def validate():
   self.actions.selected(self.key)
   if list(first.glob('.lumen-markup-*')) and alias.resolve()==first:alias.unlink();alias.symlink_to(second,target_is_directory=True)
  with self.assertRaises(ResourceUnavailable) as caught:save_copy(entry,req,validate)
  self.assertEqual(caught.exception.reason,'destination_changed');self.assertEqual(list(first.iterdir()),[]);self.assertEqual(list(second.iterdir()),[])
 def test_metadata_preflight_rejects_oversized_image_header(self):
  import struct,zlib
  data=bytearray(self.original);data[16:24]=struct.pack('>II',8000,5000);data[29:33]=struct.pack('>I',zlib.crc32(data[12:29]));self.source.write_bytes(data);self.snapshot.write_bytes(data);key=self.actions.capture(self.source,resource=self.snapshot)
  with self.assertRaises(ResourceUnavailable) as caught:self.actions.annotation_info(key)
  self.assertEqual(caught.exception.reason,'policy_limit');self.assertFalse((self.root/'copy.png').exists())
 def test_oriented_jpeg_and_webp_metadata_and_saved_pixels_agree(self):
  for extension,format,orientation,expected in [('jpg','JPEG',6,(255,0,0)),('webp','WEBP',8,(255,0,0))]:
   path=self.root/('oriented.'+extension);image=Image.new('RGB',(80,40));image.paste((255,0,0),(0,0,40,20));image.paste((0,255,0),(40,0,80,20));image.paste((0,0,255),(0,20,40,40));image.paste((255,255,0),(40,20,80,40));exif=Image.Exif();exif[274]=orientation;image.save(path,format=format,exif=exif,**({'lossless':True} if format=='WEBP' else {'quality':95}));image.close();before=path.read_bytes();retained=self.root/('snapshot.'+extension);retained.write_bytes(before);key=self.actions.capture(path,resource=retained);info=self.actions.annotation_info(key);self.assertEqual((info['width'],info['height']),(80,40));req=self.request('oriented-'+extension+'.png');self.actions.annotate(key,req)
   with Image.open(req['destination']) as output:
    self.assertEqual(output.size,(80,40));self.assertEqual(output.getexif().get(274,1),1);self.assertTrue(all(abs(a-b)<20 for a,b in zip(output.getpixel((8,4))[:3],expected)))
   self.assertEqual(path.read_bytes(),before)
 def test_malformed_points_limits_and_destinations(self):
  for value in ([],[{}],[dict(tool='pen',color='#000000',width=True,points=[[0,0],[1,1]])],[dict(tool='pen',color='#000000',width=.01,points=[[float('nan'),0],[1,1]])],[dict(tool='rectangle',color='#000000',width=.01,points=[[0,0],[1,1],[.5,.5]])]):
   with self.subTest(value=value),self.assertRaises(ResourceUnavailable):recipe(value)
  for value in ('relative.png','https://example.invalid/new.png',str(self.root/'new.jpg'),'file://remote/new.png'):
   req=self.request();req['destination']=value
   with self.assertRaises(ResourceUnavailable):self.actions.annotate(self.key,req)
if __name__=='__main__':unittest.main()
