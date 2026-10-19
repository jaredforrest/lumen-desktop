const ids=["battery","sound","focus","search","controls"];
function normalize(order) { return (order || []).filter((id,i,a)=>ids.includes(id) && a.indexOf(id)===i).concat(ids.filter(id=>!(order || []).includes(id))); }
function move(order,id,target) { const result=normalize(order),from=result.indexOf(id);if(from<0)return result;result.splice(from,1);result.splice(Math.max(0,Math.min(result.length,target)),0,id);return result; }
function itemWidth(id) { return id==="battery"?70:38; }
function layout(order,available,budget) { const candidates=normalize(order).filter(id=>available[id]);let used=0,first=candidates.length;while(first>0 && used+itemWidth(candidates[first-1])<=budget){used+=itemWidth(candidates[--first]);}return {visible:candidates.slice(first),overflow:candidates.slice(0,first)}; }
