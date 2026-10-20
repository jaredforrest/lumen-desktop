"""Full-width identities survive listing, JavaScript, preview and mutation guards."""
import contextlib,json,os,stat,subprocess,sys,tempfile,unittest
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch
ROOT=Path(__file__).resolve().parents[2]
sys.path.insert(0,str(ROOT/'res/lumen-desktop/shell'))
import file_identity,files,file_tags
from owned_preview import PreviewSession
NODE=os.environ.get('NODE_BINARY','node')

class FileIdentity(unittest.TestCase):
 def roundtrip(self,value):
  return json.loads(subprocess.check_output([NODE,'-e','process.stdout.write(JSON.stringify(JSON.parse(process.argv[1])))',json.dumps(value)],text=True))
 def info(self,device=(1<<60)+1,inode=(1<<60)+3):
  return SimpleNamespace(st_dev=device,st_ino=inode,st_mode=stat.S_IFDIR|0o700,st_ctime_ns=1,st_mtime_ns=1,st_mtime=0,st_size=0)
 def test_canonical_unsigned64_and_safe_legacy(self):
  for value in (0,1,(1<<53)-1,(1<<60)+3,(1<<64)-1):self.assertEqual(file_identity.component(str(value)),value)
  for value in (0,1,(1<<53)-1):self.assertEqual(file_identity.component(value),value)
  for value in (True,False,-1,1.0,1<<53,1<<60,None,'','00','01','+1','-1',' 1','1 ','1e3','1.0','1\n',str(1<<64),'9'*100):
   with self.subTest(value=value),self.assertRaises(ValueError):file_identity.component(value)
  for value in ({},{'device':'1'},{'device':'1','inode':'2','extra':0},[]):
   with self.assertRaises(ValueError):file_identity.parse(value)
 def test_listing_uses_exact_strings_before_javascript_sees_numbers(self):
  info=self.info()
  with tempfile.TemporaryDirectory() as directory:
   entry=SimpleNamespace(name='selected',path=directory+'/selected',stat=lambda **kwargs:info,is_dir=lambda:True,is_symlink=lambda:False)
   with patch.object(files.os,'scandir',return_value=contextlib.nullcontext(iter([entry]))),patch.object(file_tags,'read',return_value=dict(tags=[],supported=True,error='')):
    row=files.listing(dict(path=directory))['rows'][0]
   self.assertEqual(self.roundtrip(row)['identity'],dict(device=str(info.st_dev),inode=str(info.st_ino)))
 def test_preview_roundtrip_accepts_original_rejects_replacement_and_unsafe_legacy(self):
  with tempfile.TemporaryDirectory() as directory,patch.dict(os.environ,XDG_RUNTIME_DIR=directory):
   source=Path(directory)/'selected';source.mkdir();session=PreviewSession();info=self.info();expected=self.roundtrip(file_identity.wire(info))
   try:
    with patch('pathlib.Path.stat',return_value=info),patch('owned_preview.preview',return_value=dict(type='folder')):
     value=session.request(str(source),expected=expected)
    self.assertEqual(value['sourceIdentity'],expected)
    replacement=self.info(inode=info.st_ino+1)
    with patch('pathlib.Path.stat',return_value=replacement),patch('owned_preview.preview') as renderer:
     with self.assertRaises(ValueError):session.request(str(source),expected=expected)
     renderer.assert_not_called()
    rounded=self.roundtrip(dict(device=info.st_dev,inode=info.st_ino));replacement=self.info(**rounded)
    with patch('pathlib.Path.stat',return_value=replacement),patch('owned_preview.preview') as renderer:
     with self.assertRaises(ValueError):session.request(str(source),expected=rounded)
     renderer.assert_not_called()
   finally:session.close()
 def test_regular_preview_source_facts_identity_fields_are_strings(self):
  with tempfile.TemporaryDirectory() as directory,patch.dict(os.environ,XDG_RUNTIME_DIR=directory):
   source=Path(directory)/'selected.txt';source.write_text('private synthetic');session=PreviewSession()
   try:
    value=session.request(str(source));identity=file_identity.wire(source.stat())
    self.assertEqual(value['sourceIdentity'],identity)
    self.assertEqual({key:value['sourceFacts'][key] for key in identity},identity)
   finally:session.close()
 def test_tag_guard_uses_exact_identity_and_accepts_safe_legacy(self):
  with tempfile.TemporaryDirectory() as directory:
   item=Path(directory)/'selected';item.write_text('private synthetic');info=self.info();expected=self.roundtrip(file_identity.wire(info))
   with patch.object(file_tags.os,'fstat',return_value=info),patch.object(file_tags.os,'setxattr') as write,patch.object(file_tags,'read',return_value={}):
    file_tags.write(str(item),['red'],expected);self.assertEqual(write.call_count,1)
    with self.assertRaises(ValueError):file_tags.write(str(item),['red'],dict(expected,inode=str(info.st_ino+1)))
    with self.assertRaises(ValueError):file_tags.write(str(item),['red'],dict(device=info.st_dev,inode=info.st_ino))
    self.assertEqual(write.call_count,1)
   safe=self.info(device=1,inode=2)
   with patch.object(file_tags.os,'fstat',return_value=safe),patch.object(file_tags.os,'setxattr') as write,patch.object(file_tags,'read',return_value={}):
    file_tags.write(str(item),['red'],dict(device=1,inode=2));write.assert_called_once()
 def test_shared_javascript_normalizer_history_and_desktop_models(self):
  code='''const assert=require('assert/strict'),i=require('./res/lumen-desktop/shell/FileIdentity.js');
for(const v of ['0','9007199254740991','1152921504606846979','18446744073709551615'])assert.equal(i.component(v),v);
for(const v of [0,1,Number.MAX_SAFE_INTEGER])assert.equal(i.component(v),String(v));
for(const v of [true,-1,1.5,2**53,2**60,null,'','00','01','+1','-1',' 1','1 ','1e3','1.0','1\\n','18446744073709551616'])assert.equal(i.component(v),null);
assert.equal(i.normalize({device:'1',inode:'2',extra:0}),null);'''
  subprocess.run([NODE,'-e',code],cwd=ROOT,check=True)
  for name in ('test_files_history_model.cjs','test_desktop_layout_model.cjs'):subprocess.run([NODE,str(ROOT/'tests/desktop'/name)],cwd=ROOT,check=True)

if __name__=='__main__':unittest.main()
