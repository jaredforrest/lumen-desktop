const assert=require('node:assert/strict');
const power=require('../../res/lumen-desktop/shell/BatteryModel.js');
const base={available:true,present:true,percentage:42,state:'discharging',low:false,secondsRemaining:5400};
assert.equal(power.description(base),'42% · On battery');
assert.equal(power.estimate(base),'About 1 hr 30 min remaining');
assert.equal(power.estimate({...base,state:'charging',secondsRemaining:1}),'About 1 min until full');
assert.match(power.description({...base,percentage:4}),/Low battery/);
assert.equal(power.normalize({...base,percentage:0}).percentage,0);
for(const bad of [null,{},false,{...base,available:false},{...base,present:'true'},{...base,percentage:true},{...base,percentage:NaN},{...base,percentage:101},{...base,state:'unsupported'},{...base,secondsRemaining:-1}]) {
    assert.equal(power.normalize(bad).percentage,null);
    assert.equal(power.normalize(bad).present,false);
}
assert.equal(power.normalize({...base,present:false}).percentage,null);
assert.equal(power.description({...base,present:false}),'No battery');
assert.equal(power.description({...base,available:false}),'Battery status unavailable');
assert.equal(power.estimate({...base,state:'charged'}),'');
console.log('PASS battery facts, percentage bounds, text states and unavailable fallback');
