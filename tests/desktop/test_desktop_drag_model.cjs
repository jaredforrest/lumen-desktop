const fs=require('fs'),vm=require('vm'),assert=require('assert');const m={};vm.createContext(m);vm.runInContext(fs.readFileSync('res/lumen-desktop/shell/DesktopDragModel.js','utf8').replace('.pragma library',''),m);
const item=(key,path,x,y)=>({key,row:{path},x,y,width:108,height:100,stack:false,overflow:false});
const items=[item('1:1','/one',100,100),item('1:2','/two',300,200),item('1:3','/other',500,300)];
const snapshot=m.begin(items,'1:1',['/one','/two'],'none');assert.equal(snapshot.length,2);
const moved=m.translate(snapshot,-500,500,800,600);assert.equal(moved[0].x,0);assert.equal(moved[1].x,200);assert.equal(moved[1].y,500);assert.equal(moved[1].y-moved[0].y,100);assert.equal(items[0].x,100);
assert.equal(m.begin(items,'1:3',['/one','/two'],'none').length,1);assert.equal(m.begin(items,'1:1',['/one'],'kind').length,0);
const stack={key:'stack:kind:Documents',x:200,y:100,width:108,height:100,stack:true};const child={...items[0],parentStack:stack.key};assert.equal(m.begin([stack,child,items[1]],stack.key,[],'kind').length,2);
const positions=m.normalized(moved,800,600);assert.equal(positions['1:1'][0],0);assert.equal(positions['1:2'][1],1);
assert.equal(m.translate(snapshot,NaN,0,800,600).length,0);console.log('DESKTOP_DRAG_MODEL_PASS');
