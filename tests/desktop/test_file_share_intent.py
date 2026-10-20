"""Captured presentation selection must never become a later sharing selection."""
import subprocess
from pathlib import Path

source = Path(__file__).resolve().parents[2]/'res/lumen-desktop/shell/FileShareIntent.js'
script = r'''const assert=require('node:assert/strict'),vm=require('node:vm'),fs=require('node:fs');
const c={};vm.createContext(c);vm.runInContext(fs.readFileSync(require('node:path').join(require('node:path').dirname(process.argv[1]),'FileIdentity.js'),'utf8')+';this.FileIdentity={normalize};',c);vm.runInContext(fs.readFileSync(process.argv[1],'utf8').replace(/^\.(?:pragma|import).*\n/gm,''),c);
const context={path:'/fixture',tab:'first'},paths=['/fixture/a $(literal).txt'],rows=[{path:paths[0],directory:false,identity:{device:4,inode:8}}];
const intent=c.capture(context,paths,rows);assert(intent);assert(c.current(intent,context,paths,rows));
assert.deepEqual(Array.from(c.argumentsFor(intent)),['lumen-nearby-sharing','send',paths[0]]);
assert(!c.current(intent,context,['/fixture/b'],[{path:'/fixture/b',directory:false,identity:{device:4,inode:9}}]));
assert(!c.current(intent,{...context,tab:'second'},paths,rows));
assert(!c.current(intent,{...context,path:'/other'},paths,rows));
assert(!c.current(intent,context,paths,[{...rows[0],identity:{device:4,inode:9}}]));
assert(!c.current(intent,context,paths,[{...rows[0],directory:true}]));
assert(!c.capture(context,paths,[rows[0],rows[0]]));assert(!c.capture(context,[paths[0],paths[0]],rows));
assert(!c.capture(context,[],rows));assert(!c.capture(context,paths,[{...rows[0],identity:null}]));
assert(!c.capture({...context,path:'trash:'},paths,rows));
assert(!c.capture(context,Array.from({length:129},(_,i)=>'/fixture/'+i),[]));
rows[0].identity.inode=99;assert.equal(intent.selection[0].inode,'8');assert(!c.current(intent,context,paths,rows));
const wide=[{path:paths[0],directory:false,identity:{device:'18446744073709551615',inode:'9007199254740993'}}];const full=c.capture(context,paths,wide);assert(full && c.current(full,context,paths,wide));assert.equal(full.selection[0].inode,'9007199254740993');assert(!c.capture(context,paths,[{...wide[0],identity:{device:'1',inode:'18446744073709551616'}}]));
console.log('PASS exact literal arguments and changed selection/tab/folder/inode/type/duplicate/limit refusal; snapshot independent');'''
subprocess.run(['node','-e',script,str(source)],check=True)
