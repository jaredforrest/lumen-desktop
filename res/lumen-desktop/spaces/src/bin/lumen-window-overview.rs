use lumen_spaces::{Policy,Result,windows as membership_windows,membership,reconcile_membership,groups,transfer};
use serde_json::{Value,json};
use sha2::{Digest,Sha256};
use std::env;
use std::fs;
use std::io::{self,BufRead};
use std::os::unix::fs::{FileTypeExt,MetadataExt};
use std::path::PathBuf;
use std::process::Command;

fn run(args:&[&str])->Result<Value>{
    let output=Command::new("timeout").arg("4s").arg("swaymsg").arg("-r").args(args).output().map_err(|e|e.to_string())?;
    if !output.status.success(){
        if let Ok(reply)=serde_json::from_slice::<Value>(&output.stdout){if let Some(message)=reply.as_array().filter(|v|v.len()==1).and_then(|v|v[0]["error"].as_str()).filter(|s|s.starts_with("LST_REJECTED:")&&s.len()<=256&&!s.chars().any(char::is_control)){return Err(format!("{}. Refresh before trying again.",message.trim_start_matches("LST_REJECTED:").trim()));}}
        return Err("The window manager did not return a valid response".into());
    }
    serde_json::from_slice(&output.stdout).map_err(|e|e.to_string())
}
fn checked(command:&str)->Result<()> {
    let reply=run(&[command])?;
    if !reply.as_array().is_some_and(|items|!items.is_empty()&&items.iter().all(|item|item["success"]==true)){return Err("The window manager rejected this action".into());}
    Ok(())
}
fn endpoint()->Result<String>{
    let path=PathBuf::from(env::var_os("SWAYSOCK").ok_or("No compositor endpoint")?);
    let metadata=fs::symlink_metadata(path).map_err(|e|e.to_string())?;
    if !metadata.file_type().is_socket()||metadata.uid()!=unsafe{libc::getuid()}{return Err("Compositor endpoint is not an owned socket".into());}
    let time=metadata.ctime() as i128*1_000_000_000+metadata.ctime_nsec() as i128;
    Ok(format!("{:x}",Sha256::digest(format!("{}:{}:{time}",metadata.dev(),metadata.ino()).as_bytes()))[..24].to_owned())
}
fn check_epoch(data:&Value,expected:&str)->Result<()>{
    if data["sessionEpoch"]!=expected||endpoint()?!=expected{return Err("The desktop session changed; refresh and try again".into());}Ok(())
}
fn policy_path()->PathBuf{
    env::var_os("XDG_STATE_HOME").map(PathBuf::from).unwrap_or_else(||PathBuf::from(env::var_os("HOME").unwrap_or_default()).join(".local/state")).join("lumen-desktop/spaces.json")
}
fn snapshot(model:&Policy,include_overview:bool)->Result<Value>{
    let tree=run(&["-t","get_tree"])?;let epoch=endpoint()?;let native=membership_windows(&tree);
    let mut rows=Vec::new();
    fn walk(node:&Value,workspace:&str,output:&str,include:bool,native:&[Value],model:&Policy,epoch:&str,rows:&mut Vec<Value>){
        let workspace=if node["type"]=="workspace"{node["name"].as_str().unwrap_or(workspace)}else{workspace};
        let output=if node["type"]=="output"{node["name"].as_str().unwrap_or(output)}else{output};
        if node["pid"].as_u64().is_some_and(|pid|pid>0)&&matches!(node["type"].as_str(),Some("con"|"floating_con")){
            let app=node["app_id"].as_str().filter(|v|!v.is_empty()).or_else(||node["window_properties"]["class"].as_str()).unwrap_or("Application");
            let title=node["name"].as_str().filter(|v|!v.is_empty()).unwrap_or(app);
            let group=if app=="org.quickshell"&&title.starts_with("Files"){"Files"}else if app=="org.quickshell"{title}else{app};
            if include||!(app=="org.quickshell"&&node["name"]=="Window Overview"){
                let native_node=native.iter().find(|item|item["id"]==node["id"]);
                let relation=&node["lumen_relationships"];let pair=&node["lumen_pair"];let state=&node["lumen_state"];
                let restores=state["restore_workspaces"].as_array().cloned().unwrap_or_else(||vec![state["restore_workspace"].clone()]);
                let membership=native_node.map(|item|membership(model,item,epoch)).unwrap_or(json!({"status":"unassigned"}));
                rows.push(json!({"groupSource":node["app_id"].is_string()&&node["sticky"]!=true&&relation["parent_supported"]==true&&relation["parent"].is_null()&&workspace!="__i3_scratch",
                    "id":node["id"],"pid":node["pid"],"app":app,"application":Policy::application(node),"group":group,"title":title,"workspace":workspace,"output":output,
                    "focused":node["focused"].as_bool().unwrap_or(false),"fullscreen":node["fullscreen_mode"].as_i64().unwrap_or(0),"paired":!pair["id"].is_null()||pair["survivor"]==true,
                    "pairRestore":pair["restore_workspace"],"restores":restores,"allDesktopMembership":membership,"assignmentGrouped":native_node.is_some_and(|item|item["_grouped"]==true)}));
            }
        }
        if let Some(children)=node["nodes"].as_array(){for child in children{walk(child,workspace,output,include,native,model,epoch,rows);}}
        if let Some(children)=node["floating_nodes"].as_array(){for child in children{walk(child,workspace,output,include,native,model,epoch,rows);}}
    }
    walk(&tree,"","",include_overview,&native,model,&epoch,&mut rows);
    let workspaces=run(&["-t","get_workspaces"])?;
    let outputs=run(&["-t","get_outputs"])?;
    let labels=outputs.as_array().into_iter().flatten().enumerate().filter(|(_,o)|o["active"]==true).map(|(i,o)|{
        json!({"name":o["name"],"label":o["model"].as_str().filter(|model|!model.is_empty()&&*model!="Unknown").map(str::to_owned).unwrap_or_else(||format!("Display {}",i+1))})
    }).collect::<Vec<_>>();
    Ok(json!({"windowGroups":tree["window_groups"],"windows":rows,"workspaces":workspaces,"outputs":labels}))
}
fn activate_desktop(desktop:&Value)->Result<()> {
    let output=desktop["output"].as_str().ok_or("Invalid desktop output")?;let name=desktop["name"].as_str().ok_or("Invalid desktop name")?;
    checked(&format!("focus output {}; workspace {}; move workspace to output {}",json!(output),json!(name),json!(output)))
}
fn any_source(rows:&[Value],source:&str)->bool{
    rows.iter().any(|row|row["workspace"]==source||row["restores"].as_array().is_some_and(|names|names.iter().any(|name|name==source)))
}
fn remove_desktop(model:&mut Policy,data:&Value,epoch:&str)->Result<Value>{
    let revision=data["revision"].as_i64().ok_or("Invalid desktop revision")?;
    model.require_revision(revision)?;check_epoch(data,epoch)?;
    if run(&["-t","get_version"])?["desktop_rehome"]!=true{return Err("This compositor cannot safely migrate all saved window placements; restart into the updated compositor".into());}
    let source=model.desktop(data["source"].as_str().ok_or("Missing source desktop")?,true)?;
    let target=model.desktop(data["destination"].as_str().ok_or("Missing destination desktop")?,true)?;
    if source["logicalId"]==target["logicalId"]||source["outputId"]!=target["outputId"]{return Err("Choose a different surviving desktop on the same display".into());}
    let source_name=source["name"].as_str().ok_or("Invalid source desktop")?;
    let target_name=target["name"].as_str().ok_or("Invalid destination desktop")?;
    let target_output=target["output"].as_str().ok_or("Invalid destination display")?;
    let captured=snapshot(model,true)?;let all=captured["windows"].as_array().ok_or("Invalid desktop windows")?;
    let occupants=all.iter().filter(|row|row["workspace"]==source_name||row["restores"].as_array().is_some_and(|names|names.iter().any(|name|name==source_name))).cloned().collect::<Vec<_>>();
    if all.iter().any(|row|row["paired"]==true&&(row["pairRestore"].is_null()||row["pairRestore"]==source_name)){return Err("Leave paired layout before removing its saved desktop".into());}
    if occupants.iter().any(|row|row["fullscreen"].as_i64().unwrap_or(0)!=0||row["paired"]==true){return Err("Leave full screen or paired layout before removing this desktop".into());}
    for window in &occupants{
        check_epoch(data,epoch)?;let fresh=snapshot(model,true)?;
        let current=fresh["windows"].as_array().and_then(|rows|rows.iter().find(|row|row["id"]==window["id"]&&row["pid"]==window["pid"]));
        let Some(current)=current else{continue;};
        let belongs=current["workspace"]==source_name;
        let saved=current["restores"].as_array().is_some_and(|names|names.iter().any(|name|name==source_name));
        if !belongs&&!saved{continue;}
        let outputs=run(&["-t","get_outputs"])?;
        if !outputs.as_array().is_some_and(|rows|rows.iter().any(|output|Policy::display(output).ok().as_deref()==target["outputId"].as_str()&&output["active"]==true)){
            return Err("The destination display disconnected; desktop retained".into());
        }
        let id=window["id"].as_u64().ok_or("Invalid window identity")?;
        if saved{checked(&format!("[con_id={id}] lumen_window rehome {} {} {}",json!(source_name),json!(target_name),json!(target_output)))?;}
        if belongs{
            checked(&format!("[con_id={id}] move container to workspace {}",json!(target_name)))?;
            checked(&format!("[con_id={id}] move workspace to output {}",json!(target_output)))?;
        }
    }
    let fresh=snapshot(model,true)?;let fresh_rows=fresh["windows"].as_array().ok_or("Invalid desktop windows")?;
    if any_source(fresh_rows,source_name){return Err("The desktop changed during removal; desktop retained and moved windows preserved".into());}
    let focused=fresh["workspaces"].as_array().and_then(|rows|rows.iter().find(|ws|ws["focused"]==true)).cloned();
    let visible=fresh["workspaces"].as_array().is_some_and(|rows|rows.iter().any(|ws|ws["name"]==source_name&&(ws["visible"]==true||ws["focused"]==true)));
    if visible{
        activate_desktop(&target)?;
        if let Some(focused)=focused.filter(|ws|ws["name"]!=source_name){
            let remaining=run(&["-t","get_workspaces"])?;
            if remaining.as_array().is_some_and(|rows|rows.iter().any(|ws|ws["id"]==focused["id"]&&ws["name"]==focused["name"])){
                if let Some(name)=focused["name"].as_str(){checked(&format!("workspace {}",json!(name)))?;}
            }
        }
    }
    check_epoch(data,epoch)?;let final_state=snapshot(model,true)?;
    if final_state["windows"].as_array().is_some_and(|rows|any_source(rows,source_name)||rows.iter().any(|row|row["paired"]==true&&(row["pairRestore"].is_null()||row["pairRestore"]==source_name))){return Err("The desktop changed during removal; desktop retained and moved windows preserved".into());}
    if final_state["workspaces"].as_array().is_some_and(|rows|rows.iter().any(|ws|ws["name"]==source_name)){return Err("The compositor still contains this desktop; desktop retained".into());}
    let outputs=run(&["-t","get_outputs"])?;model.reconcile(final_state["workspaces"].as_array().ok_or("Invalid desktop workspaces")?,outputs.as_array().ok_or("Invalid desktop outputs")?)?;
    model.remove(source["logicalId"].as_str().ok_or("Invalid desktop source")?,target["logicalId"].as_str().ok_or("Invalid desktop target")?,model.state["revision"].as_i64().unwrap_or(-1))?;
    Ok(json!({"ok":true,"moved":occupants.iter().map(|row|row["id"].clone()).collect::<Vec<_>>(),"removed":source["logicalId"]}))
}
fn handle(data:&Value)->Result<Value>{
    if !data.is_object(){return Err("Expected an action object".into());}
    let epoch=endpoint()?;let mut model=Policy::open(policy_path())?;
    let mut state=snapshot(&model,false)?;
    let action=data["action"].as_str().unwrap_or("status");
    if action=="window-groups"{
        check_epoch(data,&epoch)?;let operation=groups::command(&state["windowGroups"],data)?;
        check_epoch(data,&epoch)?;checked(&operation)?;return Ok(json!({"ok":true}));
    }
    let native=state["workspaces"].as_array().cloned().ok_or("Invalid desktop workspaces")?;
    let outputs=run(&["-t","get_outputs"])?;
    model.reconcile(&native,outputs.as_array().ok_or("Invalid desktop outputs")?)?;
    let catalog=model.catalog();
    let labels=catalog.iter().filter_map(|item|Some((item["name"].as_str()?.to_owned(),item["label"].clone()))).collect::<std::collections::HashMap<_,_>>();
    if let Some(rows)=state["windows"].as_array_mut(){for window in rows{
        let name=window["workspace"].as_str().unwrap_or("");window["workspaceLabel"]=labels.get(name).cloned().unwrap_or_else(||json!(name));
    }}
    if let Some(rows)=state["outputs"].as_array_mut(){for output in rows{
        output["logicalId"]=model.outputs.iter().find(|(_,name)|output["name"]==**name).map(|(id,_)|json!(id)).unwrap_or(Value::Null);
    }}
    state["workspaces"]=json!(catalog);
    state["autoReorder"]=model.state["auto_reorder"].clone();state["auto_reorder"]=model.state["auto_reorder"].clone();
    state["assignments"]=model.state["assignments"].clone();state["revision"]=model.state["revision"].clone();state["sessionEpoch"]=json!(epoch);
    state["capabilities"]=json!({"desktop_lifecycle":true,"desktop_removal":run(&["-t","get_version"])?["desktop_rehome"]==true,"all_desktops":{"supported":true,"scope":"current-display","window_kind":"independent-floating"}});
    if action=="transfer"{
        if data.as_object().is_none_or(|v|v.len()!=6 || ["action","sessionEpoch","revision","snapshot","source","destination"].iter().any(|k|!v.contains_key(*k))){return Err("Invalid captured desktop transfer request".into());}
        check_epoch(data,&epoch)?;
        model.require_revision(data["revision"].as_i64().ok_or("Invalid desktop revision")?)?;
        let captured=&data["destination"];
        let desktop=model.desktop(captured["logicalId"].as_str().ok_or("Invalid destination identity")?,true)?;
        let tree=run(&["-t","get_tree"])?;
        if transfer::destination(&tree,&desktop)?!=*captured{return Err("The captured destination changed".into());}
        let current=transfer::source(&tree,&data["source"]["id"])?;
        if current!=data["source"]{return Err("The captured window changed".into());}
        // A saved assignment must not silently move the result back afterward.
        let window=state["windows"].as_array().and_then(|w|w.iter().find(|w|w["id"]==current["id"])).ok_or("Captured source retired")?;
        if model.state["assignments"][window["application"].as_str().unwrap_or("")]["kind"].as_str().is_some_and(|kind|kind!="none"){return Err("Remove the window's desktop assignment before moving it".into());}
        let command=transfer::command(&data["snapshot"],&data["source"],captured)?;
        check_epoch(data,&epoch)?;checked(&command)?;check_epoch(data,&epoch)?;
        return Ok(json!({"ok":true}));
    }
    if action=="status"{
        let tree=run(&["-t","get_tree"])?;
        state["transferSnapshot"]=transfer::snapshot(&tree).ok().unwrap_or(Value::Null);
        if !state["transferSnapshot"].is_null(){
            if let Some(rows)=state["windows"].as_array_mut(){for row in rows{
                let assigned=model.state["assignments"][row["application"].as_str().unwrap_or("")]["kind"].as_str().is_some_and(|kind|kind!="none");
                let capture=if assigned{Err("Remove the window's desktop assignment before moving it".into())}else{transfer::source(&tree,&row["id"])};
                match capture{Ok(value)=>{row["transfer"]=value;row["transferUnavailable"]=Value::Null;},Err(error)=>{row["transfer"]=Value::Null;row["transferUnavailable"]=json!(error);}}
            }}
            if let Some(desktops)=state["workspaces"].as_array_mut(){for desktop in desktops{desktop["transfer"]=transfer::destination(&tree,desktop).ok().unwrap_or(Value::Null);}}
        }
    }
    if action=="status"{return Ok(state);}
    if matches!(action,"desktop-create"|"desktop-reorder"|"desktop-remove"){
        let revision=data["revision"].as_i64().ok_or("Invalid desktop revision")?;model.require_revision(revision)?;check_epoch(data,&epoch)?;
        return match action{
            "desktop-create"=>Ok(json!({"ok":true,"created":model.create(data["outputId"].as_str().ok_or("Missing output")?,revision)?})),
            "desktop-reorder"=>{
                let order=data["order"].as_array().ok_or("Invalid desktop order")?.iter().map(|value|value.as_str().map(str::to_owned).ok_or("Invalid desktop order".into())).collect::<Result<Vec<_>>>()?;
                model.reorder(data["outputId"].as_str().ok_or("Missing output")?,&order,revision)?;Ok(json!({"ok":true}))
            },
            _=>remove_desktop(&mut model,data,&epoch),
        };
    }
    if action=="ordering"{
        model.configure(data["enabled"].as_bool().ok_or("Automatic ordering must be boolean")?)?;return Ok(json!({"ok":true}));
    }
    if action=="workspace"{
        let desktop=if let Some(key)=data["logicalId"].as_str().filter(|v|!v.is_empty()){
            check_epoch(data,&epoch)?;model.desktop(key,true)?
        }else{
            let name=data["workspace"].as_str().map(str::to_owned).unwrap_or_else(||data["workspace"].to_string());
            model.catalog().into_iter().find(|item|item["name"]==name).ok_or("This workspace no longer exists")?
        };
        activate_desktop(&desktop)?;return Ok(json!({"ok":true}));
    }
    let window=state["windows"].as_array().and_then(|rows|rows.iter().find(|row|row["id"]==data["id"]&&row["pid"]==data["pid"])).cloned().ok_or("This window is no longer available")?;
    let id=window["id"].as_u64().ok_or("Invalid window identity")?;let prefix=format!("[con_id={id}] ");
    if action=="assign"{
        check_epoch(data,&epoch)?;
        let kind=data["kind"].as_str().ok_or("Invalid application assignment")?;
        if kind=="all"&&state["windows"].as_array().is_some_and(|rows|rows.iter().any(|row|row["application"]==window["application"]&&row["assignmentGrouped"]==true)){
            return Err("Turn off window groups before assigning all desktops".into());
        }
        let application=window["application"].as_str().ok_or("An exact typed application identity is required")?;
        model.assign(application,kind,data["target"].as_str())?;
        let tree=run(&["-t","get_tree"])?;reconcile_membership(&mut model,&tree,&epoch,checked)?;
        return Ok(json!({"ok":true}));
    }
    match action{
        "focus"=>checked(&format!("{prefix}{}focus",if window["workspace"]=="__i3_scratch"{"scratchpad show, "}else{""}))?,
        "move"=>{
            if let Some(key)=data["logicalId"].as_str().filter(|key|!key.is_empty()){
                check_epoch(data,&epoch)?;let desktop=model.desktop(key,true)?;
                checked(&format!("{prefix}move container to workspace {}",json!(desktop["name"])))?;
                checked(&format!("{prefix}move workspace to output {}",json!(desktop["output"])))?;
            }else{
                let name=data["workspace"].as_str().map(str::to_owned).unwrap_or_else(||data["workspace"].to_string());
                if !native.iter().any(|ws|ws["name"]==name)&&!(1..10).any(|id|id.to_string()==name){return Err("Choose an existing desktop".into());}
                checked(&format!("{prefix}move container to workspace {}",json!(name)))?;
            }
        },
        "output"=>{
            let output=data["output"].as_str().ok_or("Display is no longer connected")?;
            if !state["outputs"].as_array().is_some_and(|rows|rows.iter().any(|item|item["name"]==output)){return Err("Display is no longer connected".into());}
            checked(&format!("{prefix}move container to output {}",json!(output)))?;
        },
        _=>return Err("Unknown window action".into()),
    }
    Ok(json!({"ok":true}))
}
fn main(){
    let stdin=io::stdin();let mut line=String::new();
    let result=(||->Result<Value>{stdin.lock().read_line(&mut line).map_err(|e|e.to_string())?;let input:Value=serde_json::from_str(&line).map_err(|e|e.to_string())?;handle(&input)})();
    println!("{}",result.unwrap_or_else(|error|json!({"error":error})));
}
