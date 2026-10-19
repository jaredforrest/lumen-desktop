"""Typed sequences and nested objects survive the scoped Qt iterable migration."""
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

class JsonAdapterTests(unittest.TestCase):
    def test_typed_sequences_round_trip(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary);runtime=root/'runtime';runtime.mkdir(mode=0o700)
            data=root/'data.json';data.write_text(json.dumps(dict(numbers=[1,2,3],words=['one','two'],details={'label':'loaded'},inlineJson={'deep':{'ok':True}})))
            (root/'shell.qml').write_text('''import QtQuick
import Quickshell
import Quickshell.Io
ShellRoot {
 FileView {id:file;path:"'''+str(data)+'''"
  JsonAdapter {id:adapter
   property list<int> numbers:[]
   property list<string> words:[]
   property JsonObject details:JsonObject {property string label:""}
   property var inlineJson:({})
  }
  onSaved:{console.log("PASS typed JSON sequences serialized");Qt.quit();}
  onLoadFailed:{console.error("FAIL read fixture");Qt.quit();}
  onSaveFailed:{console.error("FAIL write fixture");Qt.quit();}
 }
 Timer {interval:30;running:true;repeat:true
  onTriggered:{if(!file.loaded)return;stop();try {
   if(JSON.stringify(Array.from(adapter.numbers))!=="[1,2,3]")throw Error("Numeric sequence lost");
   if(JSON.stringify(Array.from(adapter.words))!==JSON.stringify(["one","two"]))throw Error("String sequence lost");
   if(adapter.details.label!=="loaded" || adapter.inlineJson.deep.ok!==true)throw Error("Nested object lost");
   adapter.numbers=[8,5];adapter.words=["literal # ü","second"];adapter.details.label="saved";adapter.inlineJson={deep:{ok:false}};file.writeAdapter();
  }catch(error){console.error("FAIL "+error);Qt.quit();}}
 }
}''')
            env=dict(os.environ,HOME=str(root/'home'),XDG_RUNTIME_DIR=str(runtime),QT_QPA_PLATFORM='offscreen',QT_QPA_PLATFORMTHEME='',QT_QUICK_BACKEND='software',QT_ACCESSIBILITY='0',QT_LINUX_ACCESSIBILITY_ALWAYS_ON='0',NO_AT_BRIDGE='1',GTK_A11Y='none')
            for key in ['CONFIG','CACHE','STATE','DATA']:env['XDG_'+key+'_HOME']=str(root/key.lower())
            for key in ['DISPLAY','WAYLAND_DISPLAY','SWAYSOCK','AT_SPI_BUS_ADDRESS']:env.pop(key,None)
            command=os.environ.get('LUMEN_TEST_QUICKSHELL_BIN','quickshell')
            result=subprocess.run([command,'-p',str(root)],env=env,capture_output=True,text=True,timeout=10)
            output=result.stdout+result.stderr
            self.assertEqual(result.returncode,0,output);self.assertIn('PASS typed JSON',output);self.assertNotIn('FAIL ',output)
            self.assertEqual(json.loads(data.read_text()),dict(numbers=[8,5],words=['literal # ü','second'],details={'label':'saved'},inlineJson={'deep':{'ok':False}}))

if __name__=='__main__':unittest.main()
