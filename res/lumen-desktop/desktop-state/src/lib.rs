use serde_json::{Value,json};
use std::collections::{HashMap,HashSet};

pub type Result<T>=std::result::Result<T,String>;
fn integer(value:&Value)->Option<i64>{value.as_i64().filter(|v|*v>=0)}
fn numeric(value:&Value)->Option<f64>{value.as_f64().filter(|v|v.is_finite())}
fn bool_field(value:&Value,key:&str)->Result<bool>{value[key].as_bool().ok_or_else(||format!("Invalid {key}"))}
pub fn bounds(value:&Value)->Result<Value>{
    let mut out=json!({});
    for key in ["x","y","width","height"]{
        let number=numeric(&value[key]).ok_or("Invalid compositor rectangle")?;
        if matches!(key,"width"|"height")&&number<0.0{return Err("Negative compositor dimensions".into());}
        out[key]=value[key].clone();
    }Ok(out)
}
pub fn relationships(value:&Value)->Result<Option<Value>>{
    if value.is_null(){return Ok(None);}
    if !value.is_object()||value["version"].as_i64()!=Some(1)||!value["parent_supported"].is_boolean()||(!value["parent"].is_null()&&integer(&value["parent"]).is_none())||(!value["modal"].is_null()&&!value["modal"].is_boolean()){
        return Err("Invalid native window relationships".into());
    }
    if value["parent_supported"]==false&&(!value["parent"].is_null()||!value["modal"].is_null()){return Err("Unsupported window relationship carries facts".into());}
    Ok(Some(json!({"parent_supported":value["parent_supported"],"parent":value["parent"],"modal":value["modal"]})))
}
pub fn contexts(windows:&[Value],main_by_app:Option<&Value>)->Result<Value>{
    let mut by_id=HashMap::new();
    for window in windows{
        let id=&window["id"];
        if !(id.is_string()||id.as_i64().is_some()){return Err("Invalid window identity in context graph".into());}
        let key=id.to_string();if by_id.insert(key,window).is_some(){return Err("Duplicate window in context graph".into());}
    }
    let mut result=serde_json::Map::new();
    for window in windows{
        let mut chain=Vec::new();let mut seen=HashSet::new();let mut current=Some(window);
        while let Some(item)=current{
            let key=item["id"].to_string();if !seen.insert(key){return Err("Cyclic native parent graph".into());}
            chain.push(item);
            current=if item["parent"].is_null(){None}else{by_id.get(&item["parent"].to_string()).copied()};
        }
        let app=chain.iter().rev().find_map(|item|item["app_hint"].as_str().filter(|name|!name.is_empty()).map(str::to_owned));
        let parent=if window["parent"].is_null()||!by_id.contains_key(&window["parent"].to_string()){Value::Null}else{window["parent"].clone()};
        let context=chain.last().ok_or("Invalid context graph")?["id"].clone();
        let key=window["id"].as_str().map(str::to_owned).unwrap_or_else(||window["id"].to_string());
        result.insert(key,json!({"application_hint":app,"context_window":context,"parent":parent,"main_window":null,"main_source":"unavailable"}));
    }
    if let Some(declarations)=main_by_app.and_then(Value::as_object){for (app,main) in declarations{
        if app.is_empty()||!(main.is_string()||main.as_i64().is_some())||!by_id.contains_key(&main.to_string()){return Err("Adapter main window is unavailable".into());}
        let key=main.as_str().map(str::to_owned).unwrap_or_else(||main.to_string());
        if result[&key]["application_hint"]!=app.as_str(){return Err("Adapter main window belongs to another application context".into());}
        for item in result.values_mut(){if item["application_hint"]==app.as_str(){item["main_window"]=main.clone();item["main_source"]=json!("application_adapter");}}
    }}
    Ok(Value::Object(result))
}
pub fn activation_target(windows:&[Value],target:&Value)->Result<Value>{
    let mut target=target.clone();let mut seen=HashSet::new();
    loop{
        if !windows.iter().any(|window|window["id"]==target){return Err("Activation target is unavailable".into());}
        if !seen.insert(target.to_string()){return Err("Cyclic modal parent graph".into());}
        let children=windows.iter().filter(|window|window["parent"]==target&&window["modal"]==true).collect::<Vec<_>>();
        if children.len()>1{return Err("Ambiguous modal descendants".into());}
        let Some(child)=children.first()else{return Ok(target);};target=child["id"].clone();
    }
}
pub fn tree_context(tree:&Value)->Result<Value>{
    fn walk(node:&Value,windows:&mut Vec<Value>)->Result<()>{
        if !node["app_id"].is_null()||!node["window"].is_null(){
            let relation=relationships(&node["lumen_relationships"])?;
            let app=node["app_id"].as_str().filter(|s|!s.is_empty()).or_else(||node["window_properties"]["class"].as_str());
            windows.push(json!({"id":node["id"],"app_hint":app,"parent":relation.as_ref().map(|r|r["parent"].clone()).unwrap_or(Value::Null),"modal":relation.as_ref().map(|r|r["modal"].clone()).unwrap_or(Value::Null)}));
        }
        for key in ["nodes","floating_nodes"]{if let Some(children)=node[key].as_array(){for child in children{walk(child,windows)?;}}}Ok(())
    }
    let mut windows=Vec::new();walk(tree,&mut windows)?;contexts(&windows,None)
}

pub fn snapshot(tree:&Value,outputs:&[Value],epoch:&str)->Result<Value>{
    if !tree.is_object(){return Err("Invalid compositor snapshot".into());}
    struct Facts{windows:Vec<Value>,spaces:Vec<Value>,seen:HashSet<i64>,seen_spaces:HashSet<i64>,extension:bool,pairs:bool}
    fn walk(node:&Value,workspace:Option<String>,output:Option<String>,floating:bool,facts:&mut Facts)->Result<()>{
        if !node.is_object(){return Err("Invalid compositor node".into());}
        for key in ["nodes","floating_nodes"]{if !node[key].is_null()&&!node[key].is_array(){return Err("Invalid node children".into());}}
        if !node["focused"].is_null()&&!node["focused"].is_boolean(){return Err("Invalid focus state".into());}
        if !node["app_id"].is_null()&&!node["app_id"].is_string(){return Err("Invalid application hint".into());}
        if !node["window_properties"].is_null()&&!node["window_properties"].is_object(){return Err("Invalid window properties".into());}
        let properties=&node["window_properties"];
        if !properties["class"].is_null()&&!properties["class"].is_string(){return Err("Invalid application class".into());}
        let mut workspace=workspace;let mut output=output;
        if node["type"]=="output"{let name=node["name"].as_str().filter(|s|!s.is_empty()).ok_or("Invalid output identity")?;output=Some(name.into());}
        if node["type"]=="workspace"{
            let id=integer(&node["id"]).ok_or("Invalid workspace identity")?;
            if !facts.seen_spaces.insert(id)||!node["name"].is_string(){return Err("Invalid workspace identity".into());}
            workspace=Some(id.to_string());facts.spaces.push(json!({"id":format!("s-{id}"),"output":output,"internal":node["name"]=="__i3_scratch"}));
        }
        if !node["app_id"].is_null()||!node["window"].is_null(){
            let id=integer(&node["id"]).ok_or("Invalid or duplicated container identity")?;
            if !facts.seen.insert(id){return Err("Invalid or duplicated container identity".into());}
            let state=&node["lumen_state"];
            if !state.is_null(){
                if !state.is_object(){return Err("Invalid placement extension".into());}
                for key in ["minimised","filled"]{bool_field(state,key).map_err(|_|"Invalid placement extension")?;}
                for key in ["zoomed","preferred_zoom","hidden","group_hidden","desktop_hidden"]{if !state[key].is_null()&&!state[key].is_boolean(){return Err("Invalid placement extension".into());}}
                if state["zoomed"]==true&&state["filled"]!=true{return Err("Zoom state lacks a restore snapshot".into());}
                facts.extension=true;
            }
            let fullscreen=if node["fullscreen_mode"].is_null(){0}else{node["fullscreen_mode"].as_i64().filter(|v|matches!(*v,0|1|2)).ok_or("Invalid fullscreen state")?};
            let pairing=&node["lumen_pair"];let mut pair=Value::Null;
            if !pairing.is_null(){
                if !pairing.is_object()||pairing["version"].as_i64()!=Some(1)||!pairing["eligible"].is_boolean()||!pairing["survivor"].is_boolean(){return Err("Invalid paired placement extension".into());}
                facts.pairs=true;
                if ["id","partner","side","ratio"].iter().any(|key|!pairing[*key].is_null()){
                    let pair_id=pairing["id"].as_i64().filter(|v|*v>0&&*v!=id).ok_or("Invalid paired member identity or ratio")?;
                    let partner=pairing["partner"].as_i64().filter(|v|*v>0&&*v!=id).ok_or("Invalid paired member identity or ratio")?;
                    let side=pairing["side"].as_str().filter(|s|matches!(*s,"left"|"right")).ok_or("Invalid paired member identity or ratio")?;
                    let ratio=numeric(&pairing["ratio"]).filter(|v|*v>0.0&&*v<1.0).ok_or("Invalid paired member identity or ratio")?;
                    if pairing["survivor"]==true{return Err("Invalid paired member identity or ratio".into());}
                    pair=json!({"id":format!("p-{pair_id}"),"partner":format!("w-{partner}"),"side":side,"ratio":ratio});
                }
            }
            let relation=relationships(&node["lumen_relationships"])?;
            let app=node["app_id"].as_str().filter(|s|!s.is_empty()).or_else(||properties["class"].as_str());
            let placement=if !pair.is_null(){"paired"}else if fullscreen!=0||pairing["survivor"]==true{"fullscreen"}else if state["zoomed"]==true{"zoomed"}else if state["filled"]==true{"filled"}else if floating{"floating"}else{"layout_managed"};
            let border=node["border"].as_str().unwrap_or("");let frame=if border=="csd"{"client"}else if matches!(border,"normal"|"pixel"){"compositor"}else{"unknown"};
            facts.windows.push(json!({"id":format!("w-{id}"),"app_hint":app,"identity_assurance":"advertised_unverified","main_window":null,
                "parent":relation.as_ref().and_then(|r|r["parent"].as_i64()).map(|id|format!("w-{id}")),"parent_supported":relation.as_ref().is_some_and(|r|r["parent_supported"]==true),"modal":relation.as_ref().map(|r|r["modal"].clone()).unwrap_or(Value::Null),
                "key":node["focused"]==true,"minimised":if state.is_null(){Value::Null}else{state["minimised"].clone()},"group_hidden":state["group_hidden"],"app_hidden":state["hidden"],"desktop_hidden":state["desktop_hidden"],"occluded":null,
                "pair":pair,"paired_fullscreen_survivor":pairing["survivor"],"placement":placement,"space":workspace.as_ref().map(|ws|format!("s-{ws}")),"output":output,"bounds":bounds(&node["rect"])? ,"frame_owner":frame,
                "capabilities":{"document_state":false,"application_menu":false,"app_hide":false,"preferred_zoom":state["preferred_zoom"]==true}}));
        }
        if let Some(children)=node["nodes"].as_array(){for child in children{walk(child,workspace.clone(),output.clone(),floating,facts)?;}}
        if let Some(children)=node["floating_nodes"].as_array(){for child in children{walk(child,workspace.clone(),output.clone(),true,facts)?;}}
        Ok(())
    }
    let mut facts=Facts{windows:Vec::new(),spaces:Vec::new(),seen:HashSet::new(),seen_spaces:HashSet::new(),extension:false,pairs:false};
    walk(tree,None,None,false,&mut facts)?;
    let associations=contexts(&facts.windows,None)?;
    for window in &mut facts.windows{let key=window["id"].as_str().ok_or("Invalid window id")?;let context=&associations[key];for field in ["application_hint","context_window","parent","main_window","main_source"]{window[field]=context[field].clone();}}
    let mut live_outputs=Vec::new();let mut output_ids=HashSet::new();
    for output in outputs{
        if !output.is_object()||(!output["active"].is_null()&&!output["active"].is_boolean())||(!output["focused"].is_null()&&!output["focused"].is_boolean()){return Err("Invalid output state".into());}
        let name=output["name"].as_str().filter(|name|!name.is_empty()).ok_or("Invalid or repeated output identity")?;
        if !output_ids.insert(name){return Err("Invalid or repeated output identity".into());}
        let scale=if output["scale"].is_null(){1.0}else{numeric(&output["scale"]).ok_or("Invalid output scale")?};
        if scale<=0.0{return Err("Invalid output scale".into());}
        if output["active"]==true{live_outputs.push(json!({"id":name,"bounds":bounds(&output["rect"])? ,"scale":if output["scale"].is_null(){json!(1)}else{output["scale"].clone()},"focused":output["focused"]==true}));}
    }
    for space in &facts.spaces{if space["internal"]!=true&&!live_outputs.iter().any(|output|output["id"]==space["output"]){return Err("Workspace references unavailable output".into());}}
    let mut pairs:HashMap<String,Vec<&Value>>=HashMap::new();
    for window in &facts.windows{if let Some(id)=window["pair"]["id"].as_str(){pairs.entry(id.into()).or_default().push(window);}}
    for members in pairs.values(){
        if members.len()!=2{return Err("Incomplete paired membership".into());}
        let (a,b)=(members[0],members[1]);
        if a["pair"]["partner"]!=b["id"]||b["pair"]["partner"]!=a["id"]||a["pair"]["side"]==b["pair"]["side"]||a["pair"]["ratio"]!=b["pair"]["ratio"]||a["space"]!=b["space"]||a["output"]!=b["output"]{return Err("Inconsistent paired membership".into());}
    }
    let keys=facts.windows.iter().filter(|w|w["key"]==true).map(|w|w["id"].clone()).collect::<Vec<_>>();
    if keys.len()>1{return Err("Ambiguous key-window snapshot".into());}
    Ok(json!({"schema_version":1,"backend":"sway","session_epoch":epoch,"observation":"sequential IPC samples; not an atomic revisioned broker snapshot","windows":facts.windows,"spaces":facts.spaces,"outputs":live_outputs,
        "focus":{"key_window":keys.first(),"seat":"backend_default"},"capabilities":{"read_only":true,"placement_extension":facts.extension,"paired_placement_extension":facts.pairs,"application_registry":false,"atomic_snapshot":false,"per_seat_focus":false,"app_main_window":false},
        "unavailable":["Application lifetime and main-window identity require an application adapter","Hidden, occluded and unknown minimised state must not be inferred from scratchpad membership","Transport primitives do not constitute a running command broker"]}))
}

#[cfg(test)]
mod tests{
    use super::*;
    fn fixture()->(Value,Vec<Value>){
        let rect=json!({"x":0,"y":0,"width":800,"height":600});
        let tree=json!({"type":"root","nodes":[{"type":"output","name":"TEST-1","nodes":[{"type":"workspace","id":3,"name":"__i3_scratch","floating_nodes":[{"id":7,"type":"con","app_id":"org.example.Editor","name":"PRIVATE DOCUMENT","focused":true,"rect":rect,"border":"csd","nodes":[],"floating_nodes":[]}]}]}]});
        let outputs=vec![json!({"name":"TEST-1","active":true,"focused":true,"rect":rect,"scale":1.5})];(tree,outputs)
    }
    #[test]
    fn snapshot_redacts_titles_and_keeps_unknown_state(){
        let (mut tree,outputs)=fixture();let report=snapshot(&tree,&outputs,"fixture").unwrap();
        let window=&report["windows"][0];assert_eq!(window["app_hint"],"org.example.Editor");assert_eq!(window["minimised"],Value::Null);
        assert_eq!(window["app_hidden"],Value::Null);assert_eq!(window["frame_owner"],"client");assert!(!report.to_string().contains("PRIVATE DOCUMENT"));
        tree["nodes"][0]["nodes"][0]["floating_nodes"][0]["lumen_state"]=json!({"minimised":false,"filled":true,"zoomed":true,"preferred_zoom":true,"desktop_hidden":true});
        let report=snapshot(&tree,&outputs,"fixture").unwrap();assert_eq!(report["windows"][0]["placement"],"zoomed");assert_eq!(report["windows"][0]["desktop_hidden"],true);
        tree["nodes"][0]["nodes"][0]["floating_nodes"][0]["lumen_state"]["filled"]=json!(false);assert!(snapshot(&tree,&outputs,"fixture").is_err());
        tree["nodes"][0]["nodes"][0]["floating_nodes"][0]["lumen_state"]["filled"]=json!(true);
        tree["nodes"][0]["nodes"][0]["floating_nodes"][0]["fullscreen_mode"]=json!(true);assert!(snapshot(&tree,&outputs,"fixture").is_err());
    }
    #[test]
    fn duplicate_focus_workspace_and_invalid_pair_are_rejected(){
        let (mut tree,outputs)=fixture();
        let other=tree["nodes"][0]["nodes"][0]["floating_nodes"][0].clone();
        tree["nodes"][0]["nodes"][0]["floating_nodes"].as_array_mut().unwrap().push(other);assert!(snapshot(&tree,&outputs,"fixture").is_err());
        tree["nodes"][0]["nodes"][0]["floating_nodes"][1]["id"]=json!(8);assert!(snapshot(&tree,&outputs,"fixture").is_err());
        tree["nodes"][0]["nodes"][0]["floating_nodes"][1]["focused"]=json!(false);
        tree["nodes"][0]["nodes"][0]["floating_nodes"][0]["lumen_pair"]=json!({"version":1,"eligible":true,"survivor":false,"id":9,"partner":8,"side":"left","ratio":0.5});
        tree["nodes"][0]["nodes"][0]["floating_nodes"][1]["lumen_pair"]=json!({"version":1,"eligible":true,"survivor":false,"id":9,"partner":7,"side":"right","ratio":0.5});
        assert_eq!(snapshot(&tree,&outputs,"fixture").unwrap()["windows"][0]["placement"],"paired");
        tree["nodes"][0]["nodes"][0]["floating_nodes"][1]["lumen_pair"]["ratio"]=json!(0.7);assert!(snapshot(&tree,&outputs,"fixture").is_err());
    }
    #[test]
    fn contexts_use_parent_graph_and_adapter_authority(){
        let windows=vec![json!({"id":1,"app_hint":"org.example.Editor","parent":null,"modal":false}),json!({"id":2,"app_hint":null,"parent":1,"modal":true})];
        let result=contexts(&windows,None).unwrap();assert_eq!(result["2"]["application_hint"],"org.example.Editor");assert_eq!(result["2"]["context_window"],1);
        assert_eq!(activation_target(&windows,&json!(1)).unwrap(),2);
        let adapted=contexts(&windows,Some(&json!({"org.example.Editor":1}))).unwrap();assert_eq!(adapted["2"]["main_window"],1);
        let mut ambiguous=windows;ambiguous.push(json!({"id":3,"parent":1,"modal":true}));assert!(activation_target(&ambiguous,&json!(1)).is_err());
    }
}
