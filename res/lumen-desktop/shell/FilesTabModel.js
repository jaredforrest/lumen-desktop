function create(id, path) {
    return {id:id,path:path,history:[],historyIndex:-1,selectedPaths:[],selectedPath:"",selectionAnchor:-1,query:"",viewMode:"grid"};
}
function indexOf(tabs,id) {return tabs.findIndex(tab=>tab.id===id);}
function reorder(tabs,id,target) {
    const from=indexOf(tabs,id);
    if(from<0 || !Number.isInteger(target) || target<0 || target>=tabs.length || from===target)return tabs;
    const next=tabs.slice(),item=next.splice(from,1)[0];next.splice(target,0,item);return next;
}
function remove(tabs,id,activeId) {
    const index=indexOf(tabs,id);
    if(index<0)return {tabs:tabs,activeId:activeId,removed:false};
    const next=tabs.filter(tab=>tab.id!==id);
    return {tabs:next,activeId:activeId===id?(next[Math.min(index,next.length-1)] || {}).id || "":activeId,removed:true};
}
function save(tabs,id,state) {
    const index=indexOf(tabs,id);if(index<0)return tabs;
    const next=tabs.slice();next[index]=Object.assign({},next[index],state,{id:id,history:(state.history || []).slice(),selectedPaths:(state.selectedPaths || []).slice()});return next;
}
if(typeof module!=="undefined")module.exports={create,indexOf,reorder,remove,save};
