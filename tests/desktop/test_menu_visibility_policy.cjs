const fs=require('fs'),vm=require('vm'),assert=require('assert');
const context=vm.createContext({});vm.runInContext(fs.readFileSync('res/lumen-desktop/shell/MenuVisibilityPolicy.js','utf8').replace(/^\.pragma library\n/,''),context);
const value=code=>JSON.parse(JSON.stringify(vm.runInContext(code,context)));
assert.deepEqual(value('normalize({fullscreenVisibility:"automatic",locale:"fr_FR"})'),{visibility:'desktop',locale:'fr_FR'});
assert.deepEqual(value('normalize({fullscreenVisibility:"always"})'),{visibility:'always'});
for(const mode of ['always','desktop','fullscreen','auto_hide']) {
 assert.equal(value(`automatic({visibility:${JSON.stringify(mode)}},false)`),['fullscreen','auto_hide'].includes(mode));
 assert.equal(value(`automatic({visibility:${JSON.stringify(mode)}},true)`),['desktop','auto_hide'].includes(mode));
}
assert.equal(value('automatic({},false)'),false);assert.equal(value('automatic({},true)'),true);
assert.throws(()=>value('normalize({visibility:"desktop",fullscreenVisibility:"automatic"})'),/Conflicting/);
for(const invalid of ['true','null','"hidden"'])assert.throws(()=>value(`normalize({fullscreenVisibility:${invalid}})`),/Invalid/);
assert.deepEqual(value('(()=>{const original={fullscreenVisibility:"always"};normalize(original);return original;})()'),{fullscreenVisibility:'always'});
console.log('PASS one menu visibility authority, four context combinations, pure legacy migration and explicit conflict rejection');
