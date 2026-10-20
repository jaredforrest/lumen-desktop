const fs=require('fs'),vm=require('vm'),assert=require('assert');
const model={};vm.createContext(model);vm.runInContext(fs.readFileSync('res/lumen-desktop/shell/ShortcutHints.js','utf8'),model);
const physical={key:'x',modifiers:['Control','Mod4'],trigger:'press',device:'*',locked:false,inhibited:false};
const binding={id:'a',action:'window.fullscreen.toggle',chord:'Mod4+Control+x',scope:'global',appId:'',mode:'default',enabled:true,owner:'consumer',physical};
const document={version:1,valid:true,bindings:[binding],hints:[{action:binding.action,scope:'global',appId:'',bindings:[{id:'a',chord:binding.chord,physical}]}]};
assert.equal(model.hint(model.read(document).hints,binding.action,''),'Ctrl+Super+X');
assert.equal(model.menuAction('fullscreen'),binding.action);assert.equal(model.menuAction('unknown'),'');
for(const mutate of [d=>d.valid=false,d=>d.version=true,d=>d.bindings[0].enabled=false,d=>d.bindings[0].owner='reserved',d=>d.hints[0].bindings[0].id='missing',d=>d.hints[0].bindings[0].physical.trigger='release',d=>d.hints[0].bindings[0].chord='Ctrl+y']){
 const copy=JSON.parse(JSON.stringify(document));mutate(copy);assert.throws(()=>model.read(copy));
}
assert.equal(model.hint([],binding.action,''),'');
console.log('shortcut hint model PASS');
