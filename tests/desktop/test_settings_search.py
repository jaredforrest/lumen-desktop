"""Typed setting navigation contracts, without launching providers."""
import json,re,subprocess
from pathlib import Path
root=Path(__file__).resolve().parents[2]
source=root/'res/lumen-desktop/shell/SettingsSearch.js'
script='''const assert=require('node:assert/strict'),vm=require('node:vm'),fs=require('node:fs');
const context={};vm.createContext(context);vm.runInContext(fs.readFileSync(process.argv[1],'utf8')+';this.api={controls,results,destination,control}',context);
const a=context.api,pages=[...new Set(a.controls.map(row=>row.page))].map(name=>({name,terms:''}));
assert.equal(new Set(a.controls.map(row=>row.id)).size,a.controls.length);
for(const row of a.controls){assert(row.target && row.label && row.page);assert.equal(a.results(row.label,pages)[0].control,row.id);assert.equal(a.destination(row.page,row.id,pages).control,row.id);}
assert.equal(a.destination('Search','title-bar-double-click-action',pages),null);
assert.equal(a.destination('Unknown','',pages),null);
assert.equal(a.destination('Search','bogus',pages),null);
assert.equal(a.results('   ',pages).length,0);
assert.equal(a.results('double-click title',pages)[0].control,'title-bar-double-click-action');
assert.equal(a.results('No setting with this visible label',pages).length,0);
console.log('PASS '+a.controls.length+' exact setting labels, typed destinations, mismatched/stale identifier refusal and word-order search');'''
result=subprocess.run(['node','-e',script,str(source)],text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,check=True)
print(result.stdout.strip())
# Every authored target must exist, not silently become a dead search result.
records=json.loads(re.search(r'const controls = (\[.*?\]);',source.read_text(),re.S)[1])
qml='\n'.join(path.read_text() for path in (root/'res/lumen-desktop/shell').glob('*.qml'))
for row in records:
 if row['target'].startswith('settings-appearance-'):
  assert row['target'].removeprefix('settings-appearance-') in ('light','dark','auto') and 'objectName:"settings-appearance-"+modelData' in qml,row
 else:assert 'objectName:"'+row['target']+'"' in qml,row
