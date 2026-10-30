import QtQuick
import Quickshell
import "BatteryModel.js" as Battery
import "DesktopDragModel.js" as Drag
import "DesktopOrderModel.js" as Order
import "FileDropModel.js" as Drop
import "ScrollGeometry.js" as Scroll
import "StatusStripModel.js" as Status
import "MenuVisibilityPolicy.js" as Menu
import "FilesTabModel.js" as Tabs
import "FileLayoutModel.js" as FileLayout
import "ShortcutHints.js" as Hints
import "WidgetModel.js" as Widget
import "DesktopLayoutModel.js" as DesktopLayout
import "FilesHistoryModel.js" as History
import "FilesSelectionModel.js" as Selection
import "DockInteraction.js" as Interaction
import "AppSwitcherModel.js" as AppSwitcher

ShellRoot {
    function equal(actual, expected, message) {
        if (actual !== expected) throw Error(message || (JSON.stringify(actual) + " !== " + JSON.stringify(expected)));
    }
    function deep(actual, expected, message) {
        function canonical(value) {
            if (Array.isArray(value)) return value.map(canonical);
            if (value !== null && typeof value === 'object') {
                const result = {};
                for (const key of Object.keys(value).sort()) result[key] = canonical(value[key]);
                return result;
            }
            return value;
        }
        if (JSON.stringify(canonical(actual)) !== JSON.stringify(canonical(expected))) throw Error(message || (JSON.stringify(actual) + " !== " + JSON.stringify(expected)));
    }
    function throws(callback) {
        let rejected = false;
        try { callback(); } catch (error) { rejected = true; }
        if (!rejected) throw Error("Expected an exception");
    }
    function different(actual, expected, message) {
        if (actual === expected) throw Error(message || 'Expected different values');
    }
    function battery() {
        const base = {available:true,present:true,percentage:42,state:'discharging',low:false,secondsRemaining:5400};
        equal(Battery.description(base),'42% · On battery');
        equal(Battery.estimate(base),'About 1 hr 30 min remaining');
        equal(Battery.estimate(Object.assign({},base,{state:'charging',secondsRemaining:1})),'About 1 min until full');
        if (!Battery.description(Object.assign({},base,{percentage:4})).includes('Low battery')) throw Error('Low battery label');
        equal(Battery.normalize(Object.assign({},base,{percentage:0})).percentage,0);
        for (const bad of [null,{},false,Object.assign({},base,{available:false}),Object.assign({},base,{present:'true'}),Object.assign({},base,{percentage:true}),Object.assign({},base,{percentage:NaN}),Object.assign({},base,{percentage:101}),Object.assign({},base,{state:'unsupported'}),Object.assign({},base,{secondsRemaining:-1})]) {
            equal(Battery.normalize(bad).percentage,null);
            equal(Battery.normalize(bad).present,false);
        }
        equal(Battery.normalize(Object.assign({},base,{present:false})).percentage,null);
        equal(Battery.description(Object.assign({},base,{present:false})),'No battery');
        equal(Battery.description(Object.assign({},base,{available:false})),'Battery status unavailable');
        equal(Battery.estimate(Object.assign({},base,{state:'charged'})),'');
    }
    function desktopDrag() {
        const item=(key,path,x,y)=>({key,row:{path},x,y,width:108,height:100,stack:false,overflow:false});
        const items=[item('1:1','/one',100,100),item('1:2','/two',300,200),item('1:3','/other',500,300)];
        const snapshot=Drag.begin(items,'1:1',['/one','/two'],'none');equal(snapshot.length,2);
        const moved=Drag.translate(snapshot,-500,500,800,600);equal(moved[0].x,0);equal(moved[1].x,200);equal(moved[1].y,500);equal(moved[1].y-moved[0].y,100);equal(items[0].x,100);
        equal(Drag.begin(items,'1:3',['/one','/two'],'none').length,1);equal(Drag.begin(items,'1:1',['/one'],'kind').length,0);
        const stack={key:'stack:kind:Documents',x:200,y:100,width:108,height:100,stack:true};const child=Object.assign({},items[0],{parentStack:stack.key});equal(Drag.begin([stack,child,items[1]],stack.key,[],'kind').length,2);
        const positions=Drag.normalized(moved,800,600);equal(positions['1:1'][0],0);equal(positions['1:2'][1],1);
        equal(Drag.translate(snapshot,NaN,0,800,600).length,0);
    }
    function desktopOrder() {
        const desktops=[{logicalId:'one',outputId:'A',available:true},{logicalId:'two',outputId:'A',available:true},{logicalId:'other',outputId:'B',available:true},{logicalId:'three',outputId:'A',available:true}];
        const gesture=Order.begin(desktops,'one',4,'session');deep(gesture.order,['one','two','three']);
        const preview=Order.move(gesture,2);deep(preview.order,['two','three','one']);deep(gesture.order,['one','two','three']);equal(desktops[0].logicalId,'one');
        equal(Order.request(gesture,4,'session'),null);equal(Order.request(preview,5,'session'),null);equal(Order.request(preview,4,'other-session'),null);
        const request=Order.request(preview,4,'session');equal(request.action,'desktop-reorder');deep(request.order,['two','three','one']);equal(request.outputId,'A');
        equal(Order.begin(desktops,'missing',4,'session'),null);equal(Order.begin([{logicalId:'one',outputId:'A',available:false}],'one',4,'session'),null);
        equal(Order.move(gesture,-1),gesture);equal(Order.move(gesture,99),gesture);equal(Order.request(null,4,'session'),null);
    }
    function fileDrop() {
        deep(Drop.paths(['file:///tmp/one%20two','file:///tmp/%23literal','file:///tmp/one%20two']),['/tmp/one two','/tmp/#literal']);
        equal(Drop.uris([{row:{path:'/tmp/a #b'}}]),'file:///tmp/a%20%23b\r\n');
        for(const url of ['https://example.test/','file://remote/tmp/a','file:///tmp/%00x','file:///tmp/a?query','file:///tmp/%wrong','file:///tmp/%0anewline']) throws(()=>Drop.paths([url]));
        throws(()=>Drop.paths(Array(1001).fill('file:///tmp/a')));throws(()=>Drop.uris([{row:{path:'relative'}}]));
    }
    function scrollGeometry() {
        equal(Scroll.reveal(0,200,600,250,40),98);
        equal(Scroll.reveal(100,200,600,20,40),12);
        equal(Scroll.reveal(100,200,600,140,30),100);
        equal(Scroll.reveal(0,200,600,250,300),242);
        equal(Scroll.reveal(0,200,100,50,30),0);
        equal(Scroll.reveal(500,200,600,590,30),400);
    }
    function statusStrip() {
        deep(Status.normalize(['search','search','bogus']),['search','battery','sound','focus','controls']);
        deep(Status.move(Status.ids,'search',0),['search','battery','sound','focus','controls']);
        deep(Status.layout(Status.move(Status.ids,'search',0),{sound:true,controls:true},76),{visible:['sound','controls'],overflow:[]});
        deep(Status.layout(Status.move(Status.ids,'search',0),{search:true,sound:true,controls:true},38),{visible:['controls'],overflow:['search','sound']});
        deep(Status.move(Status.move(Status.ids,'search',0),'search',3),['battery','sound','focus','search','controls']);
    }
    function menuVisibility() {
        deep(Menu.normalize({fullscreenVisibility:'automatic',locale:'fr_FR'}),{visibility:'desktop',locale:'fr_FR'});
        deep(Menu.normalize({fullscreenVisibility:'always'}),{visibility:'always'});
        for(const mode of ['always','desktop','fullscreen','auto_hide']) {
            equal(Menu.automatic({visibility:mode},false),['fullscreen','auto_hide'].includes(mode));
            equal(Menu.automatic({visibility:mode},true),['desktop','auto_hide'].includes(mode));
        }
        equal(Menu.automatic({},false),false);equal(Menu.automatic({},true),true);
        throws(()=>Menu.normalize({visibility:'desktop',fullscreenVisibility:'automatic'}));
        for(const invalid of [true,null,'hidden']) throws(()=>Menu.normalize({fullscreenVisibility:invalid}));
        const original={fullscreenVisibility:'always'};Menu.normalize(original);deep(original,{fullscreenVisibility:'always'});
    }
    function filesTab() {
        const first=Tabs.create('first','/first'),second=Tabs.create('second','/second'),third=Tabs.create('third','/third');
        const history=['/one','/first'],selected=['/first/document.txt'];
        const saved=Tabs.save([first,second,third],'first',{id:'cannot-replace',path:'/first',history,historyIndex:1,selectedPaths:selected,selectedPath:selected[0],query:'document',viewMode:'list'});
        history.push('/mutated');selected.length=0;
        equal(saved[0].id,'first');deep(saved[0].history,['/one','/first']);deep(saved[0].selectedPaths,['/first/document.txt']);
        const moved=Tabs.reorder(saved,'first',2);
        deep(moved.map(t=>t.id),['second','third','first']);deep(moved[2],saved[0]);deep(saved.map(t=>t.id),['first','second','third']);
        for(const target of [-1,3,1.5,'2']) equal(Tabs.reorder(saved,'first',target),saved);
        equal(Tabs.reorder(saved,'missing',1),saved);
        const closeInactive=Tabs.remove(moved,'second','first');equal(closeInactive.activeId,'first');deep(closeInactive.tabs.map(t=>t.id),['third','first']);
        const closeActive=Tabs.remove(saved,'second','second');equal(closeActive.activeId,'third');
        equal(Tabs.remove(saved,'third','third').activeId,'second');equal(Tabs.remove([first],'first','first').activeId,'');
        equal(Tabs.remove(saved,'missing','first').removed,false);
        equal(FileLayout.sidebarWidth(340,480),180);equal(FileLayout.sidebarWidth(340,900),340);equal(FileLayout.sidebarWidth(500,900),360);equal(FileLayout.sidebarWidth(-2,900),120);equal(FileLayout.sidebarWidth(undefined,900),190);
        for(const width of [480,600,720,900,1280]) for(const savedWidth of [120,190,280,360]) if(width-FileLayout.sidebarWidth(savedWidth,width)<300) throw Error('Sidebar content bound');
    }
    function shortcutHints() {
        const physical={key:'x',modifiers:['Control','Mod4'],trigger:'press',device:'*',locked:false,inhibited:false};
        const binding={id:'a',action:'window.fullscreen.toggle',chord:'Mod4+Control+x',scope:'global',appId:'',mode:'default',enabled:true,owner:'consumer',physical};
        const document={version:1,valid:true,bindings:[binding],hints:[{action:binding.action,scope:'global',appId:'',bindings:[{id:'a',chord:binding.chord,physical}]}]};
        equal(Hints.hint(Hints.read(document).hints,binding.action,''),'Ctrl+Super+X');
        equal(Hints.menuAction('fullscreen'),binding.action);equal(Hints.menuAction('unknown'),'');
        for(const mutate of [d=>d.valid=false,d=>d.version=true,d=>d.bindings[0].enabled=false,d=>d.bindings[0].owner='reserved',d=>d.hints[0].bindings[0].id='missing',d=>d.hints[0].bindings[0].physical.trigger='release',d=>d.hints[0].bindings[0].chord='Ctrl+y']) {
            const copy=JSON.parse(JSON.stringify(document));mutate(copy);throws(()=>Hints.read(copy));
        }
        equal(Hints.hint([],binding.action,''),'');
        const aliases=JSON.parse(JSON.stringify(document));aliases.hints[0].bindings.push(Object.assign({},aliases.hints[0].bindings[0]));equal(Hints.hint(aliases.hints,binding.action,''),'Ctrl+Super+X');
    }
    function widgetModel() {
        const preferences={desktopStyle:'agenda',positions:{screen:{x:.2,y:.3}}};
        equal(Widget.effective(preferences)[0].id,'legacy-calendar');
        equal(Widget.effective(preferences)[0].style,'agenda');
        equal(Widget.effective(Object.assign({},preferences,{desktopItems:[]})).length,0);
        const input=[{id:'clock-one',type:'clock',size:'large',positions:{screen:{x:-4,y:9}}},{id:'calendar-one',type:'calendar'}, {id:'clock-one',type:'calendar'},{id:'unknown',type:'network-code'}, {id:'bad/id',type:'clock'}];
        const items=Widget.normalize(input);equal(items.length,2);equal(items[0].positions.screen.x,0);equal(items[0].positions.screen.y,1);
        const changed=Widget.updated(items,'clock-one',{id:'replacement',type:'calendar',size:'small',hourCycle:'24'});
        equal(changed[0].id,'clock-one');equal(changed[0].type,'clock');equal(changed[0].size,'small');equal(items[0].size,'large');
        equal(Widget.updated(items,'gone',{size:'small'}),null);
        equal(Widget.normalize([{id:'constructor',type:'clock'}]).length,1);
        equal(Object.getPrototypeOf(Widget.normalize([{id:'safe',type:'clock',positions:JSON.parse('{"__proto__":{"x":0,"y":0}}')}])[0].positions),null);
        for(const type of ['calendar','clock'])for(const size of ['small','medium','large'])for(const bounds of [[480,320],[1280,900],[10,10]]) {
            const width=bounds[0],height=bounds[1];
            const rect=Widget.placed({type,size,positions:{}},'screen',7,width,height,28);
            if(!(rect.x>=0 && rect.y>=0 && rect.x+rect.width<=width && rect.y+rect.height<=height))throw Error(JSON.stringify(rect));
        }
        equal(Widget.normalize(Array.from({length:50},(_,i)=>({id:'widget-'+i,type:'clock'}))).length,32);
    }
    function desktopLayout() {
        const row=(name,inode,kind)=>({name,path:'/fixture/'+name,identity:{device:1,inode},directory:false,kind:kind || 'text/plain',tags:[],mtime:1});
        const rows=[row('first',2),row('second',3),row('image',4,'image/png')];
        const value=DesktopLayout.build(rows,{grouping:'none',positions:{'1:2':[.3,.4]}},[],1000,700,Date.now());
        equal(value[0].x,.3*892);equal(value[0].y,.4*600);if(value[1].x+':'+value[1].y===value[2].x+':'+value[2].y)throw Error('Desktop slots overlap');
        const moved=DesktopLayout.build([Object.assign({},rows[0],{name:'renamed'})],{grouping:'none',positions:{'1:2':[.3,.4]}},[],500,400,Date.now());equal(moved[0].x,.3*392);
        const grouped=DesktopLayout.build(rows,{grouping:'kind',positions:{}},[],1000,700,Date.now());equal(grouped.length,2);equal(grouped[0].children.length,2);
        const expanded=DesktopLayout.build(rows,{grouping:'kind',positions:{}},[grouped[0].key],1000,700,Date.now());equal(expanded.length,4);equal(expanded[1].row.name,'first');equal(expanded[2].row.name,'second');
        const links=DesktopLayout.identities([row('one',9),row('two',9)]);if(links[0]===links[1] || !/^1:9-[a-f0-9]{16}$/.test(links[0]))throw Error('Hardlink desktop keys');
        equal(DesktopLayout.category(Object.assign({},rows[0],{tags:[{id:'red'},{id:'blue'}]}),'tags',Date.now()),'blue, red');
        equal(DesktopLayout.category(Object.assign({},rows[0],{mtime:Date.now()/1000}),'modified',Date.now()),'Today');
        deep(DesktopLayout.point(-5,999,500,400,108,100),[0,1]);
        const anchored=DesktopLayout.build(rows,{grouping:'kind',positions:{},stacks:{'stack:kind:Documents':[.5,.4]}},['stack:kind:Documents'],1000,700,Date.now());
        const anchor=anchored.find(item=>item.stack&&item.label==='Documents'),anchoredChild=anchored.find(item=>item.parentStack===anchor.key);equal(anchor.x,446);equal(anchor.y,240);equal(anchoredChild.x,anchor.x);equal(anchoredChild.y,anchor.y+100);
        const resized=DesktopLayout.build(rows,{grouping:'kind',positions:{},stacks:{'stack:kind:Documents':[.5,.4]}},[],500,400,Date.now());equal(resized.find(item=>item.stack&&item.label==='Documents').x,196);
        const highRows=[row('high-a','1152921504606846977'),row('high-b','1152921504606846978')];
        deep(DesktopLayout.identities(highRows),['1:1152921504606846977','1:1152921504606846978']);
        deep(DesktopLayout.identities([Object.assign({},highRows[0],{name:'renamed'})]),['1:1152921504606846977']);
        const highLinks=DesktopLayout.identities([highRows[0],Object.assign({},highRows[0],{name:'link'})]);if(highLinks[0]===highLinks[1])throw Error('High inode hardlink keys');
    }
    function filesHistory() {
        const h=History;
        const row=(path,inode)=>({path,identity:{device:1,inode},renameFollow:true});
        let first=h.commit([],-1,'/first',-1,'one');
        const state={view:'list',query:'needle',selection:[row('/first/a',10)],current:row('/first/a',10),anchor:row('/first/a',10),scroll:{list:{offset:240,anchor:row('/first/a',10),delta:4}}};
        let history=h.save(first.history,0,state);state.selection.length=0;state.scroll.list.offset=999;
        equal(history[0].presentation.scroll.list.offset,240);equal(history[0].presentation.selection.length,1);
        const second=h.commit(history,0,'/second',-1,'two');equal(second.index,1);equal(h.commit(second.history,1,'/first',0,'ignored').index,0);
        const branch=h.commit(second.history,0,'/branch',-1,'three');deep(branch.history.map(e=>e.id),['one','three']);deep(second.history.map(e=>e.id),['one','two']);
        deep(h.reconcile(history[0].presentation,[row('/first/a',11)]).paths,[],'same path replaced inode rejected');
        deep(h.reconcile(history[0].presentation,[row('/first/renamed',10)]).paths,['/first/renamed']);
        deep(h.reconcile(history[0].presentation,[row('/first/b',10),row('/first/c',10)]).paths,[],'ambiguous hardlinks not guessed');
        deep(h.reconcile(history[0].presentation,[{path:'/first/a'}]).paths,[],'known identity cannot degrade to path');
        const bounded=h.presentation({view:'unknown',query:'x'.repeat(5000),selection:Array(600).fill(row('/a',1)),scroll:{list:{offset:Infinity,delta:-2},grid:{offset:1e20},columns:{x:NaN,columns:Array(8).fill({path:'/a',offset:-1})}}});
        equal(bounded.view,'grid');equal(bounded.query.length,4096);equal(bounded.selection.length,0);equal(bounded.selectionOverflow,true);equal(bounded.current,null);equal(bounded.scroll.list.offset,0);equal(bounded.scroll.grid.offset,1e7);equal(bounded.scroll.columns.columns.length,3);
        let many=[];for(let i=0;i<150;i++)many=h.commit(many,many.length-1,'/'+i,-1,String(i)).history;equal(many.length,128);
        const tabs=Tabs;const saved=tabs.save([tabs.create('a','/first'),tabs.create('b','/second')],'a',{history,historyIndex:0});history[0].presentation.query='changed';equal(saved[0].history[0].presentation.query,'needle');deep(saved[1].history,[]);
        const high={device:'1152921504606846977',inode:'1152921504606846979'};
        const large=JSON.parse(JSON.stringify({path:'/large/selected',identity:high,renameFollow:true}));
        deep(h.ref(large).identity,high);
        const largeState=h.presentation({selection:[large],current:large,anchor:large,scroll:{list:{anchor:large}}});
        deep(h.reconcile(largeState,[{path:'/large/renamed',identity:high,renameFollow:true}]).paths,['/large/renamed']);
        deep(h.reconcile(largeState,[{path:large.path,identity:Object.assign({},high,{inode:'1152921504606846980'})}]).paths,[]);
        deep(h.reconcile(largeState,[{path:'/large/a',identity:high,renameFollow:true},{path:'/large/b',identity:high,renameFollow:true}]).paths,[]);
        deep(h.reconcile(largeState,[large,{path:'/large/link',identity:high}]).paths,[large.path]);
        deep(h.ref(row('/safe',10)).identity,{device:'1',inode:'10'});
        const unsafe={path:'/unsafe',identity:{device:1,inode:Math.pow(2,60)}};
        deep(h.reconcile(h.presentation({selection:[unsafe],current:unsafe}),[{path:'/unsafe'}]).paths,[],'unsafe identity must not degrade to path-only');
        for(const malformed of [0,false,"",NaN,[],{}, {device:1,inode:2,extra:3}]){
         const invalid={path:'/malformed',identity:malformed};
         deep(h.reconcile(h.presentation({selection:[invalid],current:invalid}),[{path:invalid.path}]).paths,[],'present malformed identity cannot become path-only');
         equal(h.locate([{path:invalid.path}],invalid),-1);
         equal(h.locate([invalid],{path:invalid.path}),-1);
        }
        for(const absent of [undefined,null])deep(h.reconcile(h.presentation({selection:[{path:'/legacy',identity:absent}]}),[{path:'/legacy'}]).paths,['/legacy']);
        const independent=h.presentation({selection:[row('/selected',101)],current:row('/focused',102),anchor:row('/focused',102)});
        deep(h.reconcile(independent,[row('/selected',101),row('/focused',999)]),{paths:['/selected'],selected:-1,anchor:-1},'Replacing focused unselected row cannot focus another selected object');
        deep(h.reconcile(h.presentation({selection:[row('/selected',101)]}),[row('/selected',101)]),{paths:['/selected'],selected:-1,anchor:-1},'No focused object remains distinct from a nonempty selected set');

        const linked=Object.assign({},row("/linked-one",123),{renameFollow:false});
        deep(h.reconcile(h.presentation({selection:[linked],current:linked}),[Object.assign({},row("/linked-two",123),{renameFollow:true})]).paths,[],"Unlinked hardlink must not retarget remaining name");
        deep(h.reconcile(h.presentation({selection:[{path:"/unknown",identity:{device:1,inode:123}}]}),[row("/renamed",123)]).paths,[],"Missing provenance cannot follow another name");
        deep(h.reconcile(h.presentation({selection:[linked]}),[linked]).paths,["/linked-one"],"Same-path hardlink remains a valid selected entry");
    }
    function filesSelection() {
        const s=Selection;
        const row=(path,inode)=>({path,identity:{device:'1152921504606846977',inode:String(inode)},renameFollow:true});
        const rows=[row('/one','1152921504606846978'),row('/two','1152921504606846979'),row('/three','1152921504606846980')];
        const initial=s.capture(rows,['/one'],1,2);
        deep(s.reconcile(rows,initial),{paths:['/one'],current:1,anchor:2},'Unselected focused object stays focused');
        deep(s.reconcile([rows[2],rows[0],rows[1]],initial),{paths:['/one'],current:2,anchor:0},'Resort preserves exact independent roles');
        deep(s.reconcile([rows[0],row('/two',99),rows[2]],initial),{paths:['/one'],current:-1,anchor:2},'Replacement cannot inherit focused object');
        const transaction=s.begin(rows,['/one'],1,2,'folder:grid:output1','replace');
        deep(s.finish(transaction,rows,'folder:grid:output1',[rows[2]]),{paths:['/three'],current:1,anchor:2});
        deep(initial.selection,[s.ref(rows[0])],'Band preview must not mutate the original set');
        deep(s.combine(initial,[rows[0],rows[1]],'toggle').selection,[s.ref(rows[1])]);
        deep(s.combine(initial,[rows[0],rows[1]],'add').selection,[s.ref(rows[0]),s.ref(rows[1])]);
        equal(s.finish(transaction,rows,'other:grid:output1',[rows[2]]),null,'Different folder/output/view cannot receive release');
        equal(s.finish(transaction,[rows[1],rows[0],rows[2]],transaction.context,[rows[2]]),null,'Resort cancels a captured geometry transaction');
        equal(s.finish(transaction,[rows[0],row('/two',99),rows[2]],transaction.context,[rows[2]]),null,'Replacement cancels band');
        equal(s.begin([{path:'/unknown'}],[],0,0,'folder','replace'),null,'Missing identity fails closed');
        equal(s.ref({path:'/unsafe',identity:{device:1,inode:Math.pow(2,60)}}),null,'Already rounded identity fails closed');
        const rect=s.rectangle({x:40,y:50},{x:10,y:5});deep(rect,{x:10,y:5,width:30,height:45});
        const boxes=[{row:rows[0],x:0,y:0,width:15,height:15},{row:rows[1],x:20,y:20,width:20,height:20},{row:rows[2],x:40,y:0,width:20,height:30}];
        deep(s.hits(rect,boxes),[s.ref(rows[0]),s.ref(rows[1])],'Only positive intersection selects rows');
        deep(s.hits(s.rectangle({x:10,y:10},{x:10,y:10}),boxes),[],'Click is not a selection rectangle');
        equal(s.rectangle({x:Infinity,y:0},{x:1,y:1}),null);
        equal(s.rectangle({x:0,y:0},{x:NaN,y:1}),null);
        const renamed=[row('/renamed',rows[0].identity.inode),rows[1],rows[2]];deep(s.reconcile(renamed,initial).paths,['/renamed']);
        deep(s.reconcile([row('/a',rows[0].identity.inode),row('/b',rows[0].identity.inode)],initial).paths,[],'Ambiguous hard links are never guessed');

        const linked=Object.assign({},rows[0],{renameFollow:false});
        deep(s.reconcile([row("/remaining",linked.identity.inode)],{selection:[linked]}).paths,[]);
        deep(s.reconcile([rows[0]],{selection:[{path:"/missing",identity:rows[0].identity}]}).paths,[]);
        deep(s.reconcile([linked],{selection:[linked]}).paths,[linked.path]);
        deep(s.combine(initial,[rows[0],rows[0]],"toggle").selection,[],"Duplicate hit boxes toggle once only");

        equal(s.contains(boxes[0],{x:14.9,y:14.9}),true);
        equal(s.contains(boxes[0],{x:15,y:5}),false,"Actual delegate edge leaves explicit blank gutter");
        equal(s.contains(boxes[0],{x:Infinity,y:5}),false);
    }
    function dockInteraction() {
        const a={key:'a'},b={key:'b'},replacement={key:'c'};
        equal(Interaction.releaseTarget([a,b],'a',0),a);
        equal(Interaction.releaseTarget([a,b],'a',1),null);
        equal(Interaction.releaseTarget([replacement,b],'a',0),null);
        equal(Interaction.releaseTarget([b,a],'a',0),null);
        equal(Interaction.releaseTarget([a,b],'a',-1),null);
        equal(Interaction.releaseTarget([a,b],'',0),null);
        const a1={},a2={},b1={},destroyed={};
        equal(Interaction.activationWindow([a1,a2],a1,[a2,a1]),a1);
        equal(Interaction.activationWindow([a1,a2],b1,[destroyed,a2,a1]),a2);
        equal(Interaction.activationWindow([],b1,[a1]),null);
        equal(Interaction.activationWindow([a1],b1,[]),a1);
    }
    function appSwitcher() {
        deep(AppSwitcher.recoverApplicationOrder(['b','a','c'],['b','c','a'],'b',{a:['old']},{a:['new']},false,['a']),['b','a','c']);
        // An app that exited before observer hydration cannot lend its rank to
        // a later process advertising the same identifier.
        deep(AppSwitcher.recoverApplicationOrder(['short','kept'],['kept','short'],'kept',{}, {short:['replacement'],kept:['live']},false,[]),['kept','short']);
        deep(AppSwitcher.recoverApplicationOrder(['short','kept'],['kept','short'],'kept',{}, {short:['replacement'],kept:['live']},false,['short']),['kept','short']);
        deep(AppSwitcher.recoverApplicationOrder(['short','kept'],['short','kept'],'new', {short:['old']},{short:['replacement']},false,[]),['new','short','kept']);
        deep(AppSwitcher.recoverApplicationOrder(['short'],[],'',{}, {short:['replacement']},false,[]),[]);
        deep(AppSwitcher.recoverApplicationOrder(['short'],[],'',{}, {},false,['short']),['short']);
        deep(AppSwitcher.recoverApplicationOrder(['old'],['recovered'],'new',{}, {},true,['old']),['new','recovered']);
        const odd={app:'__proto__'};
        equal(AppSwitcher.snapshot([odd],[],window=>window.app).length,1);
        const a1={app:'a'},a2={app:'a'},b1={app:'b'},c1={app:'c'},gone={app:'gone'};
        const identity=window=>window.app;
        const items=AppSwitcher.snapshot([a1,a2,b1,c1],[a2,b1,gone,a1],identity);
        deep(items.map(item=>item.key),['a','b','c']);
        equal(items[0].window,a2);
        equal(AppSwitcher.step(0,3,1),1);equal(AppSwitcher.step(0,3,-1),2);equal(AppSwitcher.step(2,3,1),0);
        equal(AppSwitcher.step(0,0,1),-1);
        equal(AppSwitcher.validSelection(items,1,[a1,c1],identity),null);
        const b2={app:'b'};
        equal(AppSwitcher.validSelection(items,1,[a1,b2],identity),items[1]);
        equal(AppSwitcher.validSelection(items,9,[a1,b1],identity),null);
        deep(items.map(item=>item.key),['a','b','c']);
        const background={key:'zero',window:null},visible={key:'visible',window:a1};
        const appOrder=AppSwitcher.applicationOrder([visible,background],['gone','zero','visible']);
        equal(appOrder[0],background);equal(appOrder[1],visible);
        const creation=AppSwitcher.creationOrder([a1,b1],[b1,a1,a2]);
        equal(creation[0],a1);equal(creation[1],b1);equal(creation[2],a2);
        equal(AppSwitcher.cycleWindow(creation,[a1,a2,b1],a1,identity,1),a2);
        equal(AppSwitcher.cycleWindow(creation,[a1,a2,b1],a1,identity,-1),a2);
        equal(AppSwitcher.cycleWindow(creation,[a1,a2,b1],b1,identity,1),b1);
    }
    Timer {
        interval: 0
        running: true
        onTriggered: {
            try {
                battery();
                desktopDrag();
                desktopOrder();
                fileDrop();
                scrollGeometry();
                statusStrip();
                menuVisibility();
                filesTab();
                shortcutHints();
                widgetModel();
                desktopLayout();
                filesSelection();
                dockInteraction();
                appSwitcher();
                filesHistory();
                console.log('PASS QML JavaScript model tests');
            } catch (error) {
                console.error('FAIL QML JavaScript model tests: ' + error);
            }
            Qt.quit();
        }
    }
}
