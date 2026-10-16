import json,os,sys,tempfile,unittest
from pathlib import Path
sys.path.insert(0,str(Path(__file__).resolve().parents[2]/'res/lumen-desktop/shell'))
import remote_desktop as remote
class RemoteDesktopTest(unittest.TestCase):
 def test_listing_never_exposes_credentials_or_connects(self):
  with tempfile.TemporaryDirectory() as tmp:
   p=Path(tmp)/'saved.remmina';p.write_text('[remmina]\nname=Office\nprotocol=RDP\nserver=rdp://user:sentinel-secret@host\npassword=sentinel-secret\nusername=private-user\nshareprinter=1\n')
   commands=[];before=p.read_bytes();result=remote.handle({'action':'list'},[Path(tmp)],commands.append)
   self.assertFalse(commands);self.assertNotIn('sentinel-secret',json.dumps(result));self.assertNotIn('private-user',json.dumps(result));self.assertEqual(result['connections'][0]['host'],'');self.assertTrue(result['connections'][0]['printer'])
   remote.handle({'action':'connect','id':result['connections'][0]['id']},[Path(tmp)],commands.append)
   self.assertEqual(commands[0][-2:],['--connect',str(p)]);self.assertEqual(p.read_bytes(),before)
 def test_unknown_profile_and_symlink_escape_are_rejected(self):
  with tempfile.TemporaryDirectory() as tmp:
   base=Path(tmp);outside=base/'outside.remmina';outside.write_text('[remmina]\nname=Outside\nprotocol=RDP\n');root=base/'profiles';root.mkdir();(root/'escape.remmina').symlink_to(outside)
   self.assertEqual(remote.profiles([root])[0],[])
   with self.assertRaises(ValueError):remote.handle({'action':'connect','id':'invalid'},[root],lambda x:self.fail('Unexpected launch'))
 def test_quick_connect_accepts_hosts_not_embedded_credentials(self):
  self.assertEqual(remote.quick_uri('example.test:3389','RDP'),'rdp://example.test:3389')
  self.assertEqual(remote.quick_uri('[::1]:5900','VNC'),'vnc://[::1]:5900')
  for value in ('user:password@example.test','host/path','host?key=value','host;bad','-option','host:99999','host\nother'):
   with self.assertRaises(ValueError,msg=value):remote.quick_uri(value,'RDP')
if __name__=='__main__':unittest.main()
