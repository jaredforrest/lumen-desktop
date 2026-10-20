"""Optional status presentation must remain usable without a provider record."""
import subprocess
from pathlib import Path

source=Path(__file__).resolve().parents[2]/'res/lumen-desktop/shell/NetworkState.js'
script=r'''const assert=require('node:assert/strict'),vm=require('node:vm'),fs=require('node:fs');
const c={};vm.createContext(c);vm.runInContext(fs.readFileSync(process.argv[1],'utf8').replace(/^\.pragma library\s*/,''),c);
for(const value of [undefined,null,false,1,'unavailable',[],{}]){
 const row=c.internet(value);assert.equal(row.status,'unknown');assert.equal(typeof row.message,'string');assert.equal(row.checks.length,0);assert.equal(row.routes['4'].available,false);assert.equal(row.vpn.selected_route,false);
}
const row=c.internet({status:'online',message:'Working',routes:{'4':{available:true,interface:'private0'},'6':null},dns:{status:'ok'},vpn:{state:'connected',selected_route:true,locked_down:false},checks:[{ok:true,endpoint:'not forwarded'},null,{ok:false},{ok:1}],secret:'not forwarded'});
assert.equal(row.status,'online');assert.equal(row.message,'Working');assert.equal(row.routes['4'].interface,'private0');assert.equal(row.checks.length,2);assert.equal(row.checks[0].ok,true);assert(!('endpoint' in row.checks[0]));assert(!('secret' in row));
const bad=c.internet({status:'nonsense',message:{},checks:'not an array',routes:{'4':{available:'yes',interface:5}},vpn:{selected_route:'yes',locked_down:1}});
assert.equal(bad.status,'unknown');assert.equal(typeof bad.message,'string');assert.equal(bad.checks.length,0);assert.equal(bad.routes['4'].available,false);assert.equal(bad.vpn.locked_down,false);
assert.equal(c.internet({checks:Array.from({length:50},()=>({ok:true}))}).checks.length,32);
assert.equal(c.internet({message:'x'.repeat(513)}).message,'Internet monitoring is unavailable.');
for(const status of ['offline','degraded','portal'])assert.equal(c.internet({status}).status,status);
console.log('PASS absent, partial, malformed and populated optional presentation records; no coercion or unrelated payload forwarding');'''
subprocess.run(['node','-e',script,str(source)],check=True)
