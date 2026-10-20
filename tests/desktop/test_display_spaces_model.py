"""Typed UI authority, exact decimal generations and usable routing recovery."""
import subprocess
from pathlib import Path
source=Path(__file__).resolve().parents[2]/'res/lumen-desktop/shell/DisplaySpacesModel.js'
script=r'''const fs=require('node:fs'),vm=require('node:vm'),assert=require('node:assert/strict');const ctx={};vm.createContext(ctx);vm.runInContext(fs.readFileSync(process.argv[1],'utf8')+';this.api={valid,routing,visible,destination}',ctx);const a=ctx.api;
const identity={kind:'connector',name:'A',make:null,model:null,serial:null};
const current={protocolVersion:1,nativeSupported:true,session:'a'.repeat(32),generation:'18446744073709551614',available:true,applied:true,separateDisplays:false,independent:false,primary:'A',primaryDisplay:null,outputs:[{name:'A',label:'Display',identity}],error:''};
assert(a.valid(current,null));for(const change of [{primaryDisplay:{...identity,name:'B'},outputs:[current.outputs[0],{name:'B',label:'Other',identity:{...identity,name:'B'}}]},{independent:true},{separateDisplays:true,independent:false},{generation:1},{generation:'01'},{generation:'18446744073709551616'},{session:'unknown'},{available:1},{applied:true,available:false},{primary:'gone'},{outputs:[current.outputs[0],current.outputs[0]]},{outputs:[{name:'A',label:'X',identity:{kind:'connector',name:'A'}}]}])assert(!a.valid({...current,...change},null),JSON.stringify(change));
assert(!a.valid({...current,generation:'18446744073709551613'},current));assert(a.valid({...current,generation:'18446744073709551615'},current));assert(a.valid({...current,session:'b'.repeat(32),generation:'0'},current));
const legacy={...current,nativeSupported:false,session:null,generation:null,available:false,applied:false,separateDisplays:false,independent:true,primary:'',error:'Upgrade required'};assert(a.valid(legacy,current));
let route=a.routing(null,legacy);assert(a.visible(route,'A',[{name:'A'}]));assert(a.visible(route,'B',[{name:'B'}]));
route=a.routing(route,current);assert(!a.visible(route,'B',[{name:'A'},{name:'B'}]));assert.equal(a.destination(route,'B',[{name:'A'},{name:'B'}]),'A');
// Unavailability retains routing only; it never supplies applied authority.
route=a.routing(route,{independent:false,applied:false});assert.equal(a.destination(route,'B',[{name:'B',x:200},{name:'C',x:0}]),'C');assert(a.visible(route,'C',[{name:'B',x:200},{name:'C',x:0}]));assert(!a.visible(route,'B',[{name:'B',x:200},{name:'C',x:0}]));
console.log('PASS typed display payloads, uint64 generation/refreshed session, explicit legacy and retained connected Dock fallback');'''
result=subprocess.run(['node','-e',script,str(source)],check=True,capture_output=True,text=True,timeout=10);print(result.stdout.strip())
