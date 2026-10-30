use serde_json::{Value, json};
use std::io::{Read, Write};
use std::os::fd::AsRawFd;
use std::os::unix::fs::MetadataExt;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};
mod owned_geometry;

type Result<T> = std::result::Result<T, String>;
#[derive(Clone)]
struct Row { node: Value, workspace: Option<Value>, presentation_workspace: Option<Value>, floating: bool }
#[derive(Clone, Copy)]
struct Box4 { x:i64, y:i64, width:i64, height:i64 }

fn integer(value: &Value, name: &str, minimum:i64) -> Result<i64> { value.as_i64().filter(|number| *number >= minimum).ok_or_else(|| format!("Invalid {name}")) }
fn rect(value:&Value, name:&str) -> Result<Box4> {
    if !value.is_object() { return Err(format!("Missing {name}")); }
    Ok(Box4 {x:integer(&value["x"],&format!("{name} x"),i32::MIN as i64)?,y:integer(&value["y"],&format!("{name} y"),i32::MIN as i64)?,width:integer(&value["width"],&format!("{name} width"),1)?,height:integer(&value["height"],&format!("{name} height"),1)?})
}
fn truthy(value:&Value) -> bool { match value { Value::Null => false, Value::Bool(value)=>*value, Value::Number(number)=>number.as_i64()!=Some(0), Value::String(text)=>!text.is_empty(), Value::Array(rows)=>!rows.is_empty(), Value::Object(rows)=>!rows.is_empty() } }
fn criterion(value:&str) -> Result<String> {
    if value.is_empty() { return Err("Invalid window identity".into()); }
    Ok(format!("\"\\A{}\\z\"", value.chars().map(|ch| format!("\\x{{{:x}}}",ch as u32)).collect::<String>()))
}
fn walk(node:&Value, workspace:Option<&Value>, floating:bool, app:&str, title:&str, pid:u32, container:u64, matches:&mut Vec<Row>) {
    let workspace = if node["type"] == "workspace" {Some(node)} else {workspace};
    let floating = floating || node["type"] == "floating_con";
    if node["app_id"] == app && node["name"] == title && node["pid"].as_u64() == Some(pid as u64) && (container == 0 || node["id"].as_u64() == Some(container)) {
        matches.push(Row {node:node.clone(),workspace:workspace.cloned(),presentation_workspace:None,floating});
    }
    for kind in ["nodes","floating_nodes"] {
        if let Some(children) = node[kind].as_array() { for child in children { walk(child,workspace,floating,app,title,pid,container,matches); } }
    }
}
fn matching(tree:&Value, app:&str, title:&str, pid:u32, container:u64) -> Vec<Row> {
    let mut rows=Vec::new();walk(tree,None,false,app,title,pid,container,&mut rows);rows
}
fn exact(tree:&Value, app:&str, title:&str, pid:u32, container:u64) -> Result<Row> {
    let mut rows=matching(tree,app,title,pid,container);
    if rows.len()!=1 { return Err("Owned window is unavailable or ambiguous".into()); }
    let mut row=rows.remove(0);
    if let Some(p)=lumen_window_policy::projection::parse(&row.node)? {if p.enabled {
        if !p.supported{return Err("Unsupported projection fitting".into());}
        let placement=p.effective_workspace.ok_or("Missing effective workspace")?;
        fn workspace(node:&Value,id:u64,name:&str)->Option<Value>{
            if node["type"]=="workspace"&&node["id"].as_u64()==Some(id)&&node["name"]==name{return Some(node.clone());}
            for key in ["nodes","floating_nodes"] {for child in node[key].as_array().into_iter().flatten(){if let Some(found)=workspace(child,id,name){return Some(found);}}}None
        }
        row.presentation_workspace=Some(workspace(tree,placement.id,&placement.name).ok_or("Effective workspace is unavailable")?);
    }}
    Ok(row)
}
fn run(args:&[&str]) -> Result<Value> {
    let output=Command::new("timeout").arg("4").arg("swaymsg").arg("-r").args(args).output().map_err(|error|error.to_string())?;
    if !output.status.success() { return Err("Native geometry request unavailable".into()); }
    serde_json::from_slice(&output.stdout).map_err(|error|error.to_string())
}
fn tree() -> Result<Value> {run(&["-t","get_tree"])}
fn checked(command:&str) -> Result<()> {
    let reply=run(&[command])?;
    if !reply.as_array().is_some_and(|rows| !rows.is_empty() && rows.iter().all(|row| row["success"]==true)) {return Err("Native fit request rejected".into());}
    Ok(())
}
fn owner_start(pid:u32) -> Result<u64> {
    let raw=std::fs::read_to_string(format!("/proc/{pid}/stat")).map_err(|error|error.to_string())?;
    raw.rsplit_once(") ").and_then(|(_,rest)|rest.split_whitespace().nth(19)).and_then(|start|start.parse::<u64>().ok()).filter(|start|*start>0).ok_or("Invalid process start".into())
}
fn socket_identity(path:&Path) -> Result<(u64,u64,i128)> {
    let info=std::fs::metadata(path).map_err(|error|error.to_string())?;
    Ok((info.dev(),info.ino(),info.mtime() as i128 * 1_000_000_000 + info.mtime_nsec() as i128))
}
fn identity(node:&Value) -> Result<String> {
    let cid=integer(&node["id"],"container id",1)?;
    let pid=integer(&node["pid"],"owner PID",1)?;
    let app=criterion(node["app_id"].as_str().ok_or("Invalid window identity")?)?;
    let title=criterion(node["name"].as_str().ok_or("Invalid window identity")?)?;
    Ok(format!("con_id={cid} pid={pid} app_id={app} title={title}"))
}
fn fit(row:&Row, mut budget_width:i64, mut budget_height:i64, min_width:i64, min_height:i64, margins:Option<(i64,i64)>) -> Result<(Value,Option<String>)> {
    for (number,name) in [(budget_width,"outer width"),(budget_height,"outer height"),(min_width,"minimum width"),(min_height,"minimum height")] {if number<1{return Err(format!("Invalid {name}"));}}
    let node=&row.node;
    let state=&node["lumen_state"];let pair=&node["lumen_pair"];let relation=&node["lumen_relationships"];
    if !row.floating || node["type"]!="floating_con" || truthy(&node["fullscreen_mode"]) || !matches!(node["scratchpad_state"].as_str(),None|Some("none")) || truthy(&pair["id"]) || truthy(&pair["survivor"]) || ["filled","minimised","hidden","desktop_hidden"].iter().any(|key|truthy(&state[*key])) || !relation["parent"].is_null() || relation["modal"]==true {
        return Ok((json!({"changed":false,"supported":false,"reason":"Only visible independent floating windows are fitted"}),None));
    }
    let workspace=row.presentation_workspace.as_ref().or(row.workspace.as_ref()).filter(|workspace|workspace["name"]!="__i3_scratch").ok_or("Window workspace unavailable")?;
    // Native `move position center` still targets structural ownership. Do not
    // resize/recenter against a different presentation area until that native
    // command has an effective-placement contract.
    if row.workspace.as_ref().is_some_and(|origin|origin["rect"]!=workspace["rect"]) {
        return Ok((json!({"changed":false,"supported":false,"reason":"Projected work-area fitting is unavailable"}),None));
    }
    let box4=rect(&node["rect"],"container rectangle")?;
    let client=rect(&node["window_rect"],"client rectangle")?;
    let area=rect(&workspace["rect"],"workspace rectangle")?;
    let deco_height=if node["deco_rect"].is_object(){integer(&node["deco_rect"]["height"],"decoration height",0)?}else{0};
    let pad_width=box4.width-client.width;
    let pad_height=box4.height+deco_height-client.height;
    if pad_width<0 || pad_height<0 {return Err("Inconsistent native window padding".into());}
    if let Some((width,height))=margins {if width<0||height<0{return Err("Invalid workspace margin".into());}budget_width=(area.width-width).max(1);budget_height=(area.height-height).max(1);}
    let available_width=budget_width.min(area.width);let available_height=budget_height.min(area.height);
    if min_width+pad_width>area.width || min_height+pad_height>area.height {return Ok((json!({"changed":false,"supported":false,"reason":"Minimum client size exceeds usable workspace"}),None));}
    let width=client.width.min(min_width.max(available_width-pad_width));
    let height=client.height.min(min_height.max(available_height-pad_height));
    let outer_width=width+pad_width;let outer_height=height+pad_height;
    let x=box4.x;let y=box4.y-deco_height;
    let outside=x<area.x || y<area.y || x+outer_width>area.x+area.width || y+outer_height>area.y+area.height;
    let resized=width!=client.width || height!=client.height;
    let result=json!({"changed":resized||outside,"supported":true,"clientWidth":width,"clientHeight":height,"paddingWidth":pad_width,"paddingHeight":pad_height,"outerWidth":outer_width,"outerHeight":outer_height,"budgetWidth":available_width,"budgetHeight":available_height,"minimumLimitsMargin":outer_width>available_width||outer_height>available_height,"recentred":outside});
    if !resized && !outside{return Ok((result,None));}
    let mut commands=Vec::new();if resized{commands.push(format!("resize set width {outer_width} px height {outer_height} px"));}if outside{commands.push("move position center".into());}
    Ok((result,Some(format!("[{}] {}",identity(node)?,commands.join(", ")))))
}
fn preferred(row:&Row,width:i64,height:i64) -> Result<String> {
    if width<1||height<1||width>32768||height>32768{return Err("Preferred size exceeds supported bounds".into());}
    Ok(format!("[{}] lumen_window preferred-size {width} {height}",identity(&row.node)?))
}
fn send(stream:&mut UnixStream,kind:u32,value:&str) -> Result<()> {
    let mut packet=Vec::with_capacity(14+value.len());packet.extend_from_slice(b"i3-ipc");packet.extend_from_slice(&(value.len() as u32).to_le_bytes());packet.extend_from_slice(&kind.to_le_bytes());packet.extend_from_slice(value.as_bytes());
    stream.write_all(&packet).map_err(|error|error.to_string())
}
fn receive(stream:&mut UnixStream) -> Result<(u32,Value)> {
    let mut header=[0_u8;14];stream.read_exact(&mut header).map_err(|error|error.to_string())?;
    if &header[..6]!=b"i3-ipc"{return Err("Invalid IPC header".into());}
    let length=u32::from_le_bytes(header[6..10].try_into().map_err(|_|"Invalid IPC header")?) as usize;
    if length>1024*1024{return Err("Oversized IPC response".into());}
    let kind=u32::from_le_bytes(header[10..14].try_into().map_err(|_|"Invalid IPC header")?);
    let mut body=vec![0_u8;length];stream.read_exact(&mut body).map_err(|error|error.to_string())?;
    Ok((kind,serde_json::from_slice(&body).map_err(|_|"Invalid IPC JSON")?))
}
fn mapped(app:&str,title:&str,pid:u32,container:u64,path:&Path) -> Result<Row> {
    let deadline=Instant::now()+Duration::from_secs(4);
    let rows=matching(&tree()?,app,title,pid,container);
    if rows.len()==1{return Ok(rows[0].clone());}
    if rows.len()>1||container!=0{return Err("Owned window is unavailable or ambiguous".into());}
    let mut events=UnixStream::connect(path).map_err(|error|error.to_string())?;
    events.set_read_timeout(Some(Duration::from_secs(4))).map_err(|error|error.to_string())?;
    send(&mut events,2,"[\"window\"]")?;
    let (kind,ack)=receive(&mut events)?;
    if kind!=2||ack["success"]!=true{return Err("Native map subscription rejected".into());}
    loop {
        let rows=matching(&tree()?,app,title,pid,0);
        if rows.len()==1{return Ok(rows[0].clone());}
        if rows.len()>1{return Err("Owned window is ambiguous".into());}
        let remaining=deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero(){return Err("Owned window did not map within the request budget".into());}
        events.set_read_timeout(Some(remaining)).map_err(|error|error.to_string())?;
        let (kind,event)=receive(&mut events)?;
        if kind!=0x8000_0003||!event.is_object(){return Err("Invalid native window event".into());}
    }
}
#[derive(Clone)]
struct Args { app:String,title:String,container:u64,watch:bool,outer_width:i64,outer_height:i64,min_width:i64,min_height:i64,margins:Option<(i64,i64)>,preferred:Option<(i64,i64)>,owner_key:Option<String>,restore_owned:bool,observer_token:i64 }
fn arguments() -> Result<Args> {
    let mut values=std::collections::HashMap::new();let mut watch=false;let mut restore_owned=false;
    let mut input=std::env::args().skip(1);
    while let Some(key)=input.next(){if key=="--watch"{watch=true;continue;}if key=="--restore-owned"{restore_owned=true;continue;}if !key.starts_with("--"){return Err("Invalid geometry argument".into());}values.insert(key,input.next().ok_or("Missing geometry value")?);}
    let text=|key:&str|values.get(key).cloned().ok_or_else(||format!("Missing {key}"));
    let number=|key:&str,default:i64|values.get(key).map(|value|value.parse::<i64>().map_err(|_|format!("Invalid {key}"))).unwrap_or(Ok(default));
    let margin_width=values.get("--margin-width");let margin_height=values.get("--margin-height");
    if margin_width.is_some()!=margin_height.is_some(){return Err("Both native workspace margins are required".into());}
    let preferred_width=values.get("--preferred-width");let preferred_height=values.get("--preferred-height");
    if preferred_width.is_some()!=preferred_height.is_some(){return Err("Both preferred dimensions are required".into());}
    let container=number("--container",0)?;
    if container<0{return Err("Invalid captured container".into());}
    let owner_key=values.get("--owner-key").cloned();
    if let Some(key)=&owner_key{owned_geometry::owner_key(key)?;if !watch{return Err("Owned registration requires the geometry observer".into());}}
    if restore_owned && owner_key.is_none(){return Err("Owned restoration requires an explicit registered owner".into());}
    let observer_token=number("--observer-token",0)?;
    if owner_key.is_some() && observer_token<1{return Err("Invalid observer token".into());}
    Ok(Args{app:text("--app")?,title:text("--title")?,container:container as u64,watch,outer_width:number("--outer-width",1)?,outer_height:number("--outer-height",1)?,min_width:number("--minimum-width",1)?,min_height:number("--minimum-height",1)?,margins:if margin_width.is_some(){Some((number("--margin-width",0)?,number("--margin-height",0)?))}else{None},preferred:if preferred_width.is_some(){Some((number("--preferred-width",0)?,number("--preferred-height",0)?))}else{None},owner_key,restore_owned,observer_token})
}
fn execute(args:&Args) -> Result<Value> {
    let owner=unsafe{libc::getppid()} as u32;
    let start=owner_start(owner)?;
    let path=PathBuf::from(std::env::var_os("SWAYSOCK").ok_or("No compositor endpoint")?);
    let session=socket_identity(&path)?;
    let preferred_supported=if args.preferred.is_some(){run(&["-t","get_version"])?["native_preferred_zoom"]==true}else{false};
    let first=mapped(&args.app,&args.title,owner,args.container,&path)?;
    let captured=(&first.node["id"],&first.node["pid"],&first.node["app_id"],&first.node["name"]);
    let second=exact(&tree()?,&args.app,&args.title,owner,first.node["id"].as_u64().ok_or("Invalid container identity")?)?;
    let actual=(&second.node["id"],&second.node["pid"],&second.node["app_id"],&second.node["name"]);
    if captured!=actual||owner_start(owner)?!=start||socket_identity(&path)?!=session{return Err("Owned window identity changed before fitting".into());}
    let (mut result,mut command)=fit(&second,args.outer_width,args.outer_height,args.min_width,args.min_height,args.margins)?;
    result["container"]=first.node["id"].clone();result["pid"]=json!(owner);result["processStart"]=json!(start);result["app"]=json!(args.app);result["title"]=json!(args.title);
    if let Some((width,height))=args.preferred {
        let registration=preferred(&second,width,height)?;
        result["preferredSizeSupported"]=json!(preferred_supported);
        if preferred_supported {command=Some(match command{Some(fit)=>format!("{registration}; {fit}"),None=>registration});result["preferredSize"]=json!({"width":width,"height":height});}
    }
    if let Some(command)=command{checked(&command)?;}
    Ok(result)
}
fn watch(args:&Args) -> Result<()> {
    if args.owner_key.is_some(){return owned_watch(args);}
    let owner=unsafe{libc::getppid()} as u32;
    let start=owner_start(owner)?;
    let path=PathBuf::from(std::env::var_os("SWAYSOCK").ok_or("No compositor endpoint")?);
    let session=socket_identity(&path)?;
    let mut captured=if args.container==0{None}else{Some(args.container)};
    let mut previous:Option<(Value,i64,i64,i64,i64)>=None;
    let mut events=UnixStream::connect(&path).map_err(|error|error.to_string())?;
    send(&mut events,2,"[\"window\",\"workspace\",\"output\"]")?;
    let (kind,ack)=receive(&mut events)?;
    if kind!=2||ack["success"]!=true{return Err("Native geometry subscription rejected".into());}
    println!("{}",json!({"ready":true}));
    loop {
        if owner_start(owner)?!=start||socket_identity(&path)?!=session{return Err("Owned geometry session changed".into());}
        let live=tree()?;
        let rows=matching(&live,&args.app,&args.title,owner,captured.unwrap_or(0));
        if rows.len()==1 {
            let row=exact(&live,&args.app,&args.title,owner,captured.unwrap_or(0))?;if captured.is_none(){captured=row.node["id"].as_u64();}
            if let Some(workspace)=row.presentation_workspace.as_ref().or(row.workspace.as_ref()) {
                let area=rect(&workspace["rect"],"workspace rectangle")?;
                let fingerprint=(workspace["id"].clone(),area.x,area.y,area.width,area.height);
                if previous.as_ref()!=Some(&fingerprint){previous=Some(fingerprint);println!("{}",json!({"changed":true,"container":captured,"pid":owner,"app":args.app,"title":args.title,"workspace":workspace["id"],"area":{"x":area.x,"y":area.y,"width":area.width,"height":area.height}}));}
            }
        }
        let (kind,event)=receive(&mut events)?;
        if !matches!(kind,0x8000_0003|0x8000_0001|0x8000_0000)||!event.is_object(){return Err("Invalid native geometry event".into());}
    }
}
fn owned_emit(args:&Args,mut value:Value) {
    value["observer"]=json!(args.observer_token);
    println!("{value}");let _=std::io::stdout().flush();
}
fn owned_current(args:&Args,owner:u32,start:u64,path:&Path,session:(u64,u64,i128),captured:u64)->Result<(Value,Row)> {
    if owner_start(owner)?!=start||socket_identity(path)?!=session{return Err("Owned geometry session changed".into());}
    let tree=tree()?;let row=exact(&tree,&args.app,&args.title,owner,captured)?;
    Ok((tree,row))
}
fn fingerprint(tree:&Value,row:&Row)->Option<Value>{
    let workspace=row.workspace.as_ref()?;let id=workspace["id"].as_i64()?;
    Some(json!({"area":{"workspace":id,"output":owned_geometry::output_for(tree,id),"rect":workspace["rect"]},
        "geometry":{"rect":row.node["rect"],"window_rect":row.node["window_rect"],"deco_rect":row.node["deco_rect"]}}))
}
fn owned_dispatch(args:&Args,owner:u32,start:u64,path:&Path,session:(u64,u64,i128),captured:u64,operation:&str)->Result<()> {
    let (_,row)=owned_current(args,owner,start,path,session,captured)?;
    if !owned_geometry::enabled() || !owned_geometry::eligible(&row){return Err("Owned restoration is no longer permitted".into());}
    checked(operation).map_err(|_|"Owned geometry request rejected".into())
}
fn owned_watch(args:&Args)->Result<()> {
    let key=args.owner_key.as_deref().ok_or("Missing owned registration")?;
    owned_geometry::owner_key(key)?;
    let owner=unsafe{libc::getppid()} as u32;let start=owner_start(owner)?;
    let path=PathBuf::from(std::env::var_os("SWAYSOCK").ok_or("No compositor endpoint")?);
    let session=socket_identity(&path)?;
    let first=mapped(&args.app,&args.title,owner,args.container,&path)?;
    let captured=first.node["id"].as_u64().ok_or("Invalid container identity")?;
    let store=owned_geometry::Store::new(None);
    let version=run(&["-t","get_version"])?;
    let supported=version["owned_geometry_events"]==true;
    let mut events=UnixStream::connect(&path).map_err(|error|error.to_string())?;
    events.set_read_timeout(Some(Duration::from_secs(4))).map_err(|error|error.to_string())?;
    send(&mut events,2,"[\"window\",\"output\"]")?;
    let (kind,ack)=receive(&mut events)?;
    if kind!=2||ack["success"]!=true{return Err("Native geometry subscription rejected".into());}
    let mut restored=false;let mut restore_error=String::new();
    if owned_geometry::enabled() && !supported {
        restore_error="Remembered placement requires the updated compositor at next login".into();
    }
    if args.restore_owned && supported && owned_geometry::enabled(){
        let attempt=(||->Result<bool>{
            let Some(saved)=store.get(key)? else{return Ok(false)};
            let (mut current_tree,mut row)=owned_current(args,owner,start,&path,session,captured)?;
            if !owned_geometry::eligible(&row){return Ok(false);}
            let target=saved["output"].as_str().ok_or("Invalid saved output")?;
            let valid=target.len()<=128 && !target.is_empty() && target.bytes().all(|byte|byte.is_ascii_alphanumeric()||b"-_.:".contains(&byte));
            if valid && run(&["-t","get_outputs"])?.as_array().is_some_and(|outputs|outputs.iter().any(|output|output["name"]==target&&output["active"]==true)) {
                owned_dispatch(args,owner,start,&path,session,captured,&format!("[con_id={captured}] move container to output \"{target}\""))?;
                (current_tree,row)=owned_current(args,owner,start,&path,session,captured)?;
            }
            let _=current_tree;
            let (margin_width,margin_height)=args.margins.unwrap_or((0,0));
            let Some(plan)=owned_geometry::restore_plan(&row,&saved,args.min_width,args.min_height,margin_width,margin_height)? else{return Ok(false)};
            owned_dispatch(args,owner,start,&path,session,captured,&format!("[con_id={captured}] resize set width {} px height {} px, move absolute position {} px {} px",
                plan["outerWidth"],plan["outerHeight"],plan["x"],plan["y"]))?;
            Ok(true)
        })();
        match attempt{Ok(value)=>restored=value,Err(error)=>restore_error=error}
    }
    let (initial_tree,initial_row)=owned_current(args,owner,start,&path,session,captured)?;
    let mut previous=fingerprint(&initial_tree,&initial_row);
    owned_emit(args,json!({"ready":true,"container":captured,"restored":restored,
        "persistenceSupported":supported,"restorationError":restore_error}));
    let mut pending=Vec::<u8>::new();let mut armed=false;
    let mut capture:Option<(Value,Value,Instant)>=None;
    loop {
        if owner_start(owner)?!=start||socket_identity(&path)?!=session{return Err("Owned geometry session changed".into());}
        let timeout=capture.as_ref().map(|(_,_,until)|until.saturating_duration_since(Instant::now()).as_millis().min(i32::MAX as u128) as i32).unwrap_or(-1);
        let mut fds=[libc::pollfd{fd:events.as_raw_fd(),events:libc::POLLIN,revents:0},
            libc::pollfd{fd:0,events:libc::POLLIN,revents:0}];
        let ready=unsafe{libc::poll(fds.as_mut_ptr(),2,timeout)};
        if ready<0 {if std::io::Error::last_os_error().kind()==std::io::ErrorKind::Interrupted{continue;}return Err("Owned geometry observation failed".into());}
        if capture.as_ref().is_some_and(|(_,_,until)|Instant::now()>=*until) {
            let (tree,row)=owned_current(args,owner,start,&path,session,captured)?;
            if let Some((placement,stamp,_))=capture.take() {
                if owned_geometry::enabled() && owned_geometry::eligible(&row) && fingerprint(&tree,&row)==Some(stamp) {
                    match store.put(key,&placement) {Ok(_)=>owned_emit(args,json!({"remembered":true,"container":captured})),
                        Err(error)=>owned_emit(args,json!({"storageError":error}))}
                }
            }
        }
        if fds[1].revents&libc::POLLIN!=0 {
            let mut chunk=[0_u8;65536];let count=std::io::stdin().read(&mut chunk).map_err(|e|e.to_string())?;
            if count==0{return Ok(());}
            pending.extend_from_slice(&chunk[..count]);
            if pending.len()>65536{return Err("Owned geometry request exceeds supported size".into());}
            while let Some(index)=pending.iter().position(|byte|*byte==b'\n') {
                let line=pending.drain(..=index).collect::<Vec<_>>();
                let outcome=(||->Result<Value>{
                    let request:Value=serde_json::from_slice(&line).map_err(|e|e.to_string())?;
                    if request["action"]!="fit" || !request.is_object(){return Err("Invalid owned geometry operation".into());}
                    let mut fit_args=args.clone();fit_args.container=captured;
                    fit_args.outer_width=integer(&request["width"],"width",1)?;
                    fit_args.outer_height=integer(&request["height"],"height",1)?;
                    fit_args.min_width=integer(&request["minimumWidth"],"minimumWidth",1)?;
                    fit_args.min_height=integer(&request["minimumHeight"],"minimumHeight",1)?;
                    fit_args.preferred=Some((integer(&request["preferredWidth"],"preferredWidth",1)?,
                        integer(&request["preferredHeight"],"preferredHeight",1)?));
                    let result=execute(&fit_args)?;
                    let (tree,row)=owned_current(args,owner,start,&path,session,captured)?;
                    let now=fingerprint(&tree,&row);
                    if result["changed"]==true{previous=now;capture=None;}
                    armed=true;Ok(result)
                })();
                match outcome {Ok(result)=>owned_emit(args,json!({"fitted":true,"result":result})),
                    Err(error)=>owned_emit(args,json!({"fitted":true,"error":error}))}
            }
        }
        if fds[0].revents&libc::POLLIN!=0 {
            let (kind,event)=receive(&mut events)?;
            if !matches!(kind,0x8000_0003|0x8000_0001)||!event.is_object(){return Err("Invalid native geometry event".into());}
            let (tree,row)=owned_current(args,owner,start,&path,session,captured)?;
            let now=fingerprint(&tree,&row);
            let previous_area=previous.as_ref().map(|value|&value["area"]);
            let current_area=now.as_ref().map(|value|&value["area"]);
            if current_area!=previous_area {
                previous=now.clone();capture=None;
                owned_emit(args,json!({"changed":true,"container":captured,"pid":owner,"app":args.app,"title":args.title}));
            } else if now!=previous {previous=now.clone();}
            if armed && supported && owned_geometry::enabled() && now.is_some()
                && event["change"]=="lumen_geometry" && event["container"]["id"]==captured {
                let output=now.as_ref().unwrap()["area"]["output"].as_str().unwrap_or("");
                if let Some(placement)=owned_geometry::observed(&row,output)? {
                    capture=Some((placement,now.unwrap(),Instant::now()+Duration::from_millis(200)));
                }
            }
        }
    }
}
fn main(){
    let result=arguments().and_then(|args|{if args.watch{watch(&args).map(|()|None)}else{execute(&args).map(Some)}});
    match result{Ok(Some(value))=>println!("{value}"),Ok(None)=>(),Err(error)=>{println!("{}",json!({"error":error}));std::process::exit(1);}}
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn projected_fit_uses_effective_area_without_rewriting_origin() {
        let projection=json!({"version":1,"enabled":true,"supported":true,"reason":null,"revision":"1","projected":true,"origin_workspace":{"id":"1","name":"origin"},"origin_output":{"id":"4","name":"display"},"effective_workspace":{"id":"2","name":"current"},"effective_output":{"id":"4","name":"display"}});
        let tree=json!({"nodes":[{"type":"workspace","id":1,"name":"origin","rect":{"x":0,"y":0,"width":1000,"height":800},"floating_nodes":[{"type":"floating_con","id":3,"pid":42,"app_id":"A","name":"B","lumen_all_desktops":projection}]},{"type":"workspace","id":2,"name":"current","rect":{"x":0,"y":0,"width":700,"height":600}}]});
        let row=exact(&tree,"A","B",42,3).unwrap();
        assert_eq!(row.workspace.as_ref().unwrap()["id"],1);
        assert_eq!(row.presentation_workspace.as_ref().unwrap()["id"],2);
        assert_eq!(fingerprint(&tree,&row).unwrap()["area"]["workspace"],1);
        let refused=fit(&row,600,400,100,100,None).unwrap();
        assert_eq!(refused.0["supported"],false);
        assert_eq!(refused.0["reason"],"Projected work-area fitting is unavailable");
        assert!(refused.1.is_none());
        let mut stale=tree.clone();stale["nodes"][1]["id"]=json!(9);
        assert!(exact(&stale,"A","B",42,3).is_err());
    }
    #[test]
    fn fit_uses_exact_qualified_identity() {
        let workspace=json!({"name":"1","rect":{"x":0,"y":0,"width":1000,"height":700}});
        let node=json!({"type":"floating_con","id":7,"pid":11,"app_id":"org.example.\"Quoted\"","name":"A\\B","scratchpad_state":"none","rect":{"x":900,"y":650,"width":500,"height":300},"window_rect":{"x":0,"y":0,"width":480,"height":270},"deco_rect":{"height":10}});
        let row=Row{node,workspace:Some(workspace),presentation_workspace:None,floating:true};
        let (result,command)=fit(&row,600,400,100,100,None).unwrap();
        assert_eq!(result["changed"],true);
        let command=command.unwrap();
        assert!(command.contains("con_id=7 pid=11"));
        assert!(command.contains("\\x{22}"));
        assert!(command.contains("move position center"));
        assert!(preferred(&row,640,440).unwrap().contains("preferred-size 640 440"));
    }
    #[test]
    fn ambiguous_or_tiled_windows_fail_closed() {
        let tree=json!({"type":"root","nodes":[{"type":"con","pid":9,"app_id":"A","name":"B","id":1},{"type":"con","pid":9,"app_id":"A","name":"B","id":2}]});
        assert!(exact(&tree,"A","B",9,0).is_err());
        let row=Row{node:json!({"type":"con"}),workspace:None,presentation_workspace:None,floating:false};
        assert_eq!(fit(&row,100,100,1,1,None).unwrap().0["supported"],false);
    }
    #[test]
    fn native_padding_margins_and_minimum_are_explicit() {
        let workspace=json!({"name":"1","rect":{"x":0,"y":28,"width":960,"height":692}});
        let node=json!({"type":"floating_con","id":5,"pid":42,"app_id":"org.quickshell","name":"Settings","scratchpad_state":"none","rect":{"x":50,"y":100,"width":792,"height":521},"window_rect":{"x":1,"y":0,"width":790,"height":520},"deco_rect":{"height":32}});
        let mut row=Row{node,workspace:Some(workspace),presentation_workspace:None,floating:true};
        let (result,command)=fit(&row,928,570,480,320,None).unwrap();
        assert_eq!(result["changed"],false);assert!(command.is_none());
        assert_eq!(result["paddingWidth"],2);assert_eq!(result["paddingHeight"],33);
        row.node["rect"]["width"]=json!(1202);row.node["rect"]["height"]=json!(901);
        row.node["window_rect"]["width"]=json!(1200);row.node["window_rect"]["height"]=json!(900);
        let (result,command)=fit(&row,928,570,480,320,None).unwrap();
        assert_eq!((result["clientWidth"].as_i64(),result["clientHeight"].as_i64()),(Some(926),Some(537)));
        assert!(command.unwrap().contains("resize set width 928 px height 570 px"));
        row.workspace.as_mut().unwrap()["rect"]["height"]=json!(300);
        assert_eq!(fit(&row,928,570,480,320,None).unwrap().0["supported"],false);
        row.workspace.as_mut().unwrap()["rect"]["height"]=json!(560);
        row.workspace.as_mut().unwrap()["rect"]["width"]=json!(800);
        let (result,_)=fit(&row,1888,1340,480,320,Some((32,100))).unwrap();
        assert_eq!(result["outerWidth"],768);assert_eq!(result["outerHeight"],460);
    }
}
