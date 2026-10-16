"""Inject the opt-in adapter into the pinned app; fail on incompatible layout."""
import hashlib,json,pathlib,struct,sys
archive=pathlib.Path(sys.argv[1])
raw=archive.read_bytes();h=struct.unpack('<4I',raw[:16]);header=json.loads(raw[16:16+h[3]]);base=8+h[1]
def files(tree,prefix=''):
 for name,n in tree.items():
  if 'files' in n:yield from files(n['files'],prefix+name+'/')
  elif 'offset' in n:yield prefix+name,n
body=bytearray(); patched=0; native_frames=0
for path,n in files(header['files']):
 data=raw[base+int(n['offset']):base+int(n['offset'])+n['size']]
 original=data
 if path=='.vite/build/early-bootstrap.js':
  patched+=1
  data=b"if(process.env.CODEX_MENU_ADAPTER_ROOT)require(process.env.CODEX_MENU_ADAPTER_ROOT+'/menu-adapter.cjs');\n"+data
 if path.startswith('.vite/build/main-') and path.endswith('.js'):
  overlay=b'titleBarStyle:`hidden`,titleBarOverlay:j9(r)'
  zoom=b'n.setTitleBarOverlay(j9(t))'
  sync=b'if(process.platform!==`win32`&&process.platform!==`linux`||t!==`primary`'
  if overlay in data:
   assert data.count(overlay)==2 and data.count(zoom)==1 and data.count(sync)==1, 'Unsupported Codex window-overlay implementation'
   native_frames+=1
   data=data.replace(overlay,b'titleBarStyle:`default`,titleBarOverlay:!1')
   data=data.replace(zoom,b'(process.platform!==`linux`&&n.setTitleBarOverlay(j9(t)))')
   data=data.replace(sync,b'if(process.platform!==`win32`||t!==`primary`')
 if data!=original and 'integrity' in n:
  block=n['integrity'].get('blockSize',4194304)
  n['integrity']={'algorithm':'SHA256','hash':hashlib.sha256(data).hexdigest(),'blockSize':block,'blocks':[hashlib.sha256(data[i:i+block]).hexdigest() for i in range(0,len(data),block)]}
 n['offset']=str(len(body));n['size']=len(data);body.extend(data)
j=json.dumps(header,separators=(',',':')).encode();payload=struct.pack('<I',len(j))+j;payload+=b'\0'*((-len(payload))%4);hdr=struct.pack('<I',len(payload))+payload
assert patched == 1, 'Expected exactly one supported bootstrap; refusing incompatible app build'
assert native_frames == 1, 'Expected exactly one supported native window-frame implementation'
archive.write_bytes(struct.pack('<II',4,len(hdr))+hdr+body)
