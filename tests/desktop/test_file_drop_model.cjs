const fs=require('fs'),vm=require('vm'),assert=require('assert');const m={};vm.createContext(m);vm.runInContext(fs.readFileSync('res/lumen-desktop/shell/FileDropModel.js','utf8').replace('.pragma library',''),m);
assert.deepEqual(Array.from(m.paths(['file:///tmp/one%20two','file:///tmp/%23literal','file:///tmp/one%20two'])),['/tmp/one two','/tmp/#literal']);
assert.equal(m.uris([{row:{path:'/tmp/a #b'}}]),'file:///tmp/a%20%23b\r\n');
for(const url of ['https://example.test/','file://remote/tmp/a','file:///tmp/%00x','file:///tmp/a?query','file:///tmp/%wrong','file:///tmp/%0anewline'])assert.throws(()=>m.paths([url]));
assert.throws(()=>m.paths(Array(1001).fill('file:///tmp/a')));assert.throws(()=>m.uris([{row:{path:'relative'}}]));console.log('FILE_DROP_MODEL_PASS');
