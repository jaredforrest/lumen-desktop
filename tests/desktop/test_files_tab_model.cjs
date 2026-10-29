const assert=require('node:assert/strict');
const tabs=require('../../res/lumen-desktop/shell/FilesTabModel.js');
const first=tabs.create('first','/first'),second=tabs.create('second','/second'),third=tabs.create('third','/third');
const history=['/one','/first'],selected=['/first/document.txt'];
const saved=tabs.save([first,second,third],'first',{id:'cannot-replace',path:'/first',history,historyIndex:1,selectedPaths:selected,selectedPath:selected[0],query:'document',viewMode:'list'});
history.push('/mutated');selected.length=0;
assert.equal(saved[0].id,'first');assert.deepEqual(saved[0].history,['/one','/first']);assert.deepEqual(saved[0].selectedPaths,['/first/document.txt']);
const moved=tabs.reorder(saved,'first',2);
assert.deepEqual(moved.map(t=>t.id),['second','third','first']);assert.deepEqual(moved[2],saved[0]);assert.deepEqual(saved.map(t=>t.id),['first','second','third']);
for(const target of [-1,3,1.5,'2'])assert.equal(tabs.reorder(saved,'first',target),saved);
assert.equal(tabs.reorder(saved,'missing',1),saved);
const closeInactive=tabs.remove(moved,'second','first');assert.equal(closeInactive.activeId,'first');assert.deepEqual(closeInactive.tabs.map(t=>t.id),['third','first']);
const closeActive=tabs.remove(saved,'second','second');assert.equal(closeActive.activeId,'third');
assert.equal(tabs.remove(saved,'third','third').activeId,'second');assert.equal(tabs.remove([first],'first','first').activeId,'');
assert.equal(tabs.remove(saved,'missing','first').removed,false);
console.log('PASS stable tab identity, immutable history/selection snapshots, reorder and exact close');

const layout=require('../../res/lumen-desktop/shell/FileLayoutModel.js');
assert.equal(layout.sidebarWidth(340,480),180);assert.equal(layout.sidebarWidth(340,900),340);assert.equal(layout.sidebarWidth(500,900),360);assert.equal(layout.sidebarWidth(-2,900),120);assert.equal(layout.sidebarWidth(undefined,900),190);
for(const width of [480,600,720,900,1280])for(const saved of [120,190,280,360])assert.ok(width-layout.sidebarWidth(saved,width)>=300);
console.log('PASS sidebar clamp preserves minimum content bounds without altering saved width');

for(const scale of [1,1.15,1.3,1.5])for(const available of [0,180,300,420,600,1000])for(const requested of [-2,160,240,600,900,undefined]){
 const limits=layout.previewLimits(available,scale),actual=layout.previewWidth(requested,available,scale);
 if(limits.enabled){assert.ok(actual>=limits.minimum);assert.ok(actual+limits.leadingMinimum+8<=available);assert.ok(actual<=600);}
 else assert.equal(actual,0);
 const column=layout.columnWidth(requested,available,scale);assert.ok(column>=Math.ceil(200*scale));assert.ok(column<=Math.max(Math.ceil(200*scale),Math.min(600,available)));
}
const preferred=500;assert.equal(layout.previewWidth(preferred,1000,1),preferred);assert.ok(layout.previewWidth(preferred,420,1)<preferred);assert.equal(layout.previewWidth(preferred,1000,1),preferred);
assert.equal(layout.previewWidth(240,300,1.5),0);assert.equal(layout.columnWidth(500,1000,1.5),500);
console.log('PASS preview and column minima across text/output capacity, non-destructive clamping and wider-workarea restoration');
