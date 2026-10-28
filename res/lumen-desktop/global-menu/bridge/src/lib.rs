use serde_json::{Value,json};
use std::collections::{HashMap,HashSet};
use zbus::zvariant::{OwnedValue,Structure};

pub type Layout=(i32,HashMap<String,OwnedValue>,Vec<OwnedValue>);
pub fn focused(node:&Value)->Option<&Value> {
    if node["focused"]==true && ["con","floating_con"].contains(&node["type"].as_str().unwrap_or("")){return Some(node);}
    for child in node["nodes"].as_array().into_iter().flatten().chain(node["floating_nodes"].as_array().into_iter().flatten()) {
        if let Some(found)=focused(child){return Some(found);}
    }
    None
}
pub fn contains_focus(node:&Value)->bool {
    node["focused"]==true || node["nodes"].as_array().into_iter().flatten().chain(node["floating_nodes"].as_array().into_iter().flatten()).any(contains_focus)
}
fn string(props:&HashMap<String,OwnedValue>,key:&str)->String {
    props.get(key).and_then(|value|<&str>::try_from(value).ok()).unwrap_or("").into()
}
fn boolean(props:&HashMap<String,OwnedValue>,key:&str,default:bool)->bool {
    props.get(key).and_then(|value|bool::try_from(value).ok()).unwrap_or(default)
}
fn integer(props:&HashMap<String,OwnedValue>,key:&str)->i32 {
    props.get(key).and_then(|value|i32::try_from(value).ok()).unwrap_or(0)
}
fn child_layout(value:&OwnedValue)->Option<Layout> {
    let structure=Structure::try_from(value.try_clone().ok()?).ok()?;
    let fields=structure.fields();if fields.len()!=3{return None;}
    let id=i32::try_from(&fields[0]).ok()?;
    let props=HashMap::<String,OwnedValue>::try_from(fields[1].try_clone().ok()?).ok()?;
    let children=Vec::<OwnedValue>::try_from(fields[2].try_clone().ok()?).ok()?;
    Some((id,props,children))
}
pub fn clean(layout:&Layout,depth:usize,budget:&mut usize)->Option<Value> {
    clean_inner(layout,depth,budget,&mut HashSet::new())
}
fn clean_inner(layout:&Layout,depth:usize,budget:&mut usize,seen:&mut HashSet<i32>)->Option<Value> {
    if depth>32 || *budget==0{return None;}*budget-=1;
    let (id,props,children)=layout;
    if !seen.insert(*id) {return None;}
    for key in ["enabled","visible"] {
        if props.get(key).is_some_and(|value|bool::try_from(value).is_err()) {return None;}
    }
    let shortcuts=props.get("shortcut").and_then(|value|Vec::<Vec<String>>::try_from(value.try_clone().ok()?).ok()).unwrap_or_default();
    let shortcut=shortcuts.iter().map(|sequence|sequence.join("+")).collect::<Vec<_>>().join(" / ");
    let children=children.iter().map(|child|clean_inner(&child_layout(child)?,depth+1,budget,seen)).collect::<Option<Vec<_>>>()?;
    Some(json!({"id":id,"label":string(props,"label").chars().take(4096).collect::<String>(),"enabled":boolean(props,"enabled",true),"visible":boolean(props,"visible",true),
        "separator":string(props,"type")=="separator","toggle":string(props,"toggle-type"),"checked":integer(props,"toggle-state")==1,
        "shortcut":shortcut,"submenu":string(props,"children-display")=="submenu","children":children}))
}
pub fn context(tree:&Value,focused_id:i64)->Option<(Option<String>,i64)> {
    fn collect(node:&Value,items:&mut HashMap<i64,(Option<i64>,Option<String>)>) {
        if (node["app_id"].is_string() || !node["window"].is_null()) && node["id"].as_i64().is_some() {
            let id=node["id"].as_i64().unwrap();
            let parent=node["lumen_relationships"]["parent"].as_i64().filter(|id|*id>=0);
            let hint=node["app_id"].as_str().filter(|value|!value.is_empty())
                .or_else(||node["window_properties"]["class"].as_str().filter(|value|!value.is_empty())).map(str::to_owned);
            items.insert(id,(parent,hint));
        }
        for child in node["nodes"].as_array().into_iter().flatten().chain(node["floating_nodes"].as_array().into_iter().flatten()) {collect(child,items);}
    }
    let mut items=HashMap::new();collect(tree,&mut items);
    let mut id=focused_id;let mut seen=HashSet::new();let mut hint=None;
    while let Some((parent,current_hint))=items.get(&id) {
        if !seen.insert(id){return None;}
        if current_hint.is_some(){hint=current_hint.clone();}
        if let Some(parent)=parent.filter(|parent|items.contains_key(parent)){id=parent;}else{return Some((hint,id));}
    }
    None
}
pub fn lookup<'a>(node:&'a Value,id:i32)->Option<&'a Value> {
    if node["id"].as_i64()==Some(id as i64){return Some(node);}
    node["children"].as_array().into_iter().flatten().find_map(|child|lookup(child,id))
}
pub fn actionable<'a>(node:&'a Value,id:i32)->Option<&'a Value> {
    if node["enabled"]!=true || node["visible"]!=true || node["separator"]==true{return None;}
    if node["id"].as_i64()==Some(id as i64){return Some(node);}
    node["children"].as_array().into_iter().flatten().find_map(|child|actionable(child,id))
}
#[cfg(test)] mod tests {
    use super::*;
    use zbus::zvariant::Value as DbusValue;
    #[test] fn menu_layout_is_cleaned_and_searchable() {
        let mut props:HashMap<String,OwnedValue>=HashMap::new();
        props.insert("label".into(),OwnedValue::try_from(DbusValue::new("Save".to_owned())).unwrap());
        props.insert("shortcut".into(),OwnedValue::try_from(DbusValue::new(vec![vec!["Control".to_owned(),"S".to_owned()]])).unwrap());
        let child=(7_i32,props,Vec::<OwnedValue>::new());
        let layout=(0_i32,HashMap::new(),vec![OwnedValue::try_from(DbusValue::new(child)).unwrap()]);
        let clean=clean(&layout,0,&mut 100).unwrap();
        assert_eq!(lookup(&clean,7).unwrap()["label"],"Save");
        assert_eq!(lookup(&clean,7).unwrap()["shortcut"],"Control+S");
    }
    #[test] fn modal_parent_sets_application_context() {
        let tree=json!({"nodes":[{"id":17,"app_id":"writer","nodes":[]},{"id":18,"app_id":"utility","lumen_relationships":{"version":1,"parent_supported":true,"parent":17,"modal":true}}]});
        assert_eq!(context(&tree,18),Some((Some("writer".into()),17)));
    }
    #[test] fn hidden_parent_blocks_descendant_action() {
        let menu=json!({"id":0,"enabled":true,"visible":true,"separator":false,"children":[{"id":1,"enabled":false,"visible":true,"separator":false,"children":[{"id":2,"enabled":true,"visible":true,"separator":false,"children":[]}]}]});
        assert!(actionable(&menu,2).is_none());
        assert!(actionable(&menu,99).is_none());
        let mut hidden=menu.clone();hidden["children"][0]["enabled"]=json!(true);hidden["children"][0]["visible"]=json!(false);
        assert!(actionable(&hidden,2).is_none());
        hidden["children"][0]["visible"]=json!(true);hidden["children"][0]["separator"]=json!(true);
        assert!(actionable(&hidden,2).is_none());
    }
    #[test] fn duplicate_ids_bad_types_and_depth_are_rejected() {
        let child:Layout=(1_i32,HashMap::new(),Vec::<OwnedValue>::new());
        let repeated=OwnedValue::try_from(DbusValue::new(child)).unwrap();
        let root=(0_i32,HashMap::new(),vec![repeated.try_clone().unwrap(),repeated]);
        assert!(clean(&root,0,&mut 4096).is_none());
        let mut bad=HashMap::new();bad.insert("enabled".into(),OwnedValue::try_from(DbusValue::new("false".to_string())).unwrap());
        assert!(clean(&(0,bad,vec![]),0,&mut 4096).is_none());
        let mut deep:Layout=(40_i32,HashMap::new(),Vec::<OwnedValue>::new());
        for id in (0..40).rev(){deep=(id,HashMap::new(),vec![OwnedValue::try_from(DbusValue::new(deep)).unwrap()]);}
        assert!(clean(&deep,0,&mut 4096).is_none());
        assert!(clean(&(0,HashMap::new(),vec![]),0,&mut 0).is_none());
    }
}
