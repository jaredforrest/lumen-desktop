const fs=require('fs'),vm=require('vm'),assert=require('assert');
const context=vm.createContext({});vm.runInContext(fs.readFileSync('res/lumen-desktop/shell/StatusStripModel.js','utf8'),context);
const value=code=>JSON.parse(JSON.stringify(vm.runInContext(code,context)));
assert.deepEqual(value('normalize(["search","search","bogus"])'),['search','battery','sound','focus','controls']);
assert.deepEqual(value('move(ids,"search",0)'),['search','battery','sound','focus','controls']);
assert.deepEqual(value('layout(move(ids,"search",0),{sound:true,controls:true},76)'),{visible:['sound','controls'],overflow:[]});
assert.deepEqual(value('layout(move(ids,"search",0),{search:true,sound:true,controls:true},38)'),{visible:['controls'],overflow:['search','sound']});
assert.deepEqual(value('move(move(ids,"search",0),"search",3)'),['battery','sound','focus','search','controls']);
console.log('PASS stable complete identity order, unavailable preservation, exact bounds and overflow');
