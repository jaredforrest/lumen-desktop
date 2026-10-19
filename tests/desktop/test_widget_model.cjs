const fs=require('fs'),vm=require('vm'),assert=require('assert');
const model={};vm.createContext(model);vm.runInContext(fs.readFileSync('res/lumen-desktop/shell/WidgetModel.js','utf8'),model);
const preferences={desktopStyle:'agenda',positions:{screen:{x:.2,y:.3}}};
assert.equal(model.effective(preferences)[0].id,'legacy-calendar');
assert.equal(model.effective(preferences)[0].style,'agenda');
assert.equal(model.effective({...preferences,desktopItems:[]}).length,0);
const input=[{id:'clock-one',type:'clock',size:'large',positions:{screen:{x:-4,y:9}}},{id:'calendar-one',type:'calendar'}, {id:'clock-one',type:'calendar'},{id:'unknown',type:'network-code'}, {id:'bad/id',type:'clock'}];
const items=model.normalize(input);assert.equal(items.length,2);assert.equal(items[0].positions.screen.x,0);assert.equal(items[0].positions.screen.y,1);
const changed=model.updated(items,'clock-one',{id:'replacement',type:'calendar',size:'small',hourCycle:'24'});
assert.equal(changed[0].id,'clock-one');assert.equal(changed[0].type,'clock');assert.equal(changed[0].size,'small');assert.equal(items[0].size,'large');
assert.equal(model.updated(items,'gone',{size:'small'}),null);
assert.equal(model.normalize([{id:'constructor',type:'clock'}]).length,1);
assert.equal(Object.getPrototypeOf(model.normalize([{id:'safe',type:'clock',positions:JSON.parse('{"__proto__":{"x":0,"y":0}}')}])[0].positions),null);
for(const type of ['calendar','clock'])for(const size of ['small','medium','large'])for(const [width,height] of [[480,320],[1280,900],[10,10]]) {
 const rect=model.placed({type,size,positions:{}},'screen',7,width,height,28);
 assert(rect.x>=0 && rect.y>=0 && rect.x+rect.width<=width && rect.y+rect.height<=height,JSON.stringify(rect));
}
assert.equal(model.normalize(Array.from({length:50},(_,i)=>({id:'widget-'+i,type:'clock'}))).length,32);
console.log('PASS widget migration, stable captured IDs, allowlisted catalogue, explicit empty layout, positions and constrained geometry');
