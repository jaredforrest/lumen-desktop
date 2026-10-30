use lumen_spaces::{Policy,Result,windows,display_runtime};
use serde_json::{Value,json};
use sha2::{Digest,Sha256};
use std::collections::HashSet;
use std::env;
use std::fs;
use std::io::{Read,Write};
use std::os::fd::AsRawFd;
use std::os::unix::fs::{FileTypeExt,MetadataExt};
use std::os::unix::net::UnixStream;
use std::path::{Path,PathBuf};
use std::process::Command;
use std::time::Duration;

fn sway(args:&[&str])->Result<Value>{
    let output=Command::new("timeout").arg("5s").arg("swaymsg").arg("-r").args(args).output().map_err(|e|e.to_string())?;
    if !output.status.success(){return Err("Compositor command failed".into());}
    serde_json::from_slice(&output.stdout).map_err(|e|e.to_string())
}
fn membership_command(operation:&str)->Result<lumen_spaces::projection::CommandOutcome> {
    if !operation.starts_with("lumen_projection ") {command(operation)?;return Ok(lumen_spaces::projection::CommandOutcome::Applied);}
    let output=Command::new("timeout").arg("5s").arg("swaymsg").arg("-r").arg(operation).output().map_err(|e|e.to_string())?;
    lumen_spaces::projection::checked_reply(output.status.code(),&output.stdout)
}
fn command(command:&str)->Result<()> {
    let reply=sway(&[command])?;
    if !reply.as_array().is_some_and(|rows|!rows.is_empty()&&rows.iter().all(|row|row["success"]==true)){return Err("Compositor rejected desktop command".into());}
    Ok(())
}
fn refresh(policy:&mut Policy)->Result<()>{
    let workspaces=sway(&["-t","get_workspaces"])?;let outputs=sway(&["-t","get_outputs"])?;
    policy.reconcile(workspaces.as_array().ok_or("Invalid compositor workspaces")?,outputs.as_array().ok_or("Invalid compositor outputs")?)?;
    policy.display_status=Some(display_runtime::synchronize(policy,&mut |args|sway(args),None)
        .unwrap_or_else(|error|json!({"available":false,"applied":false,"error":error})));
    Ok(())
}
fn epoch(path:&Path)->Result<String>{
    let metadata=fs::symlink_metadata(path).map_err(|e|e.to_string())?;
    if !metadata.file_type().is_socket()||metadata.uid()!=unsafe{libc::getuid()}{return Err("Compositor endpoint is not an owned socket".into());}
    let time=metadata.ctime() as i128*1_000_000_000+metadata.ctime_nsec() as i128;
    let raw=format!("{}:{}:{time}",metadata.dev(),metadata.ino());
    Ok(format!("{:x}",Sha256::digest(raw.as_bytes()))[..24].to_owned())
}
fn socket()->Result<PathBuf>{Ok(PathBuf::from(env::var_os("SWAYSOCK").ok_or("No supported compositor session")?))}
fn receive(stream:&mut UnixStream)->Result<(u32,Value)>{
    let mut header=[0_u8;14];stream.read_exact(&mut header).map_err(|e|e.to_string())?;
    if &header[..6]!=b"i3-ipc"{return Err("Invalid compositor protocol".into());}
    let size=u32::from_le_bytes(header[6..10].try_into().map_err(|_|"Invalid compositor frame")?) as usize;
    let kind=u32::from_le_bytes(header[10..14].try_into().map_err(|_|"Invalid compositor frame")?);
    if size>16*1024*1024{return Err("Compositor response exceeds limit".into());}
    let mut body=vec![0_u8;size];stream.read_exact(&mut body).map_err(|e|e.to_string())?;
    let value=serde_json::from_slice(&body).map_err(|e|e.to_string())?;Ok((kind,value))
}
fn send(stream:&mut UnixStream,kind:u32,value:&str)->Result<()>{
    let data=value.as_bytes();let len=u32::try_from(data.len()).map_err(|_|"Compositor request exceeds limit")?;
    stream.write_all(b"i3-ipc").and_then(|_|stream.write_all(&len.to_le_bytes())).and_then(|_|stream.write_all(&kind.to_le_bytes())).and_then(|_|stream.write_all(data)).map_err(|e|e.to_string())
}
fn place(policy:&Policy,node:&Value)->Result<Value>{
    let result=policy.destination(node);
    if result["status"]=="available"{
        let id=node["id"].as_u64().filter(|id|*id>0).ok_or("Invalid compositor window identity")?;
        let workspace=result["workspace"].as_str().ok_or("Invalid assignment workspace")?;
        let mut operation=format!("[con_id={id}] move container to workspace {}",json!(workspace));
        if let Some(output)=result["recreate_on_output"].as_str(){operation.push_str(&format!("; [con_id={id}] move workspace to output {}",json!(output)));}
        command(&operation)?;
    }
    Ok(result)
}
fn run_watch(mut policy:Policy)->Result<()>{
    let path=socket()?;let expected=epoch(&path)?;
    let mut events=UnixStream::connect(&path).map_err(|e|e.to_string())?;
    events.set_read_timeout(Some(Duration::from_secs(5))).map_err(|e|e.to_string())?;
    send(&mut events,2,"[\"window\",\"workspace\",\"output\",\"tick\"]")?;
    let (kind,ack)=receive(&mut events)?;
    if kind!=2||ack["success"]!=true{return Err("Compositor subscription failed".into());}
    refresh(&mut policy)?;
    let tree=sway(&["-t","get_tree"])?;lumen_spaces::reconcile_membership_observed(&mut policy,&tree,&expected,&sway(&["-t","get_version"])?,membership_command)?;
    println!("{}",json!({"ready":true}));std::io::stdout().flush().map_err(|e|e.to_string())?;
    events.set_read_timeout(None).map_err(|e|e.to_string())?;policy.release();
    let mut pending=HashSet::<u64>::new();
    loop{
        let mut descriptor=libc::pollfd{fd:events.as_raw_fd(),events:libc::POLLIN,revents:0};
        let status=unsafe{libc::poll(&mut descriptor,1,1000)};
        if status<0{if std::io::Error::last_os_error().kind()==std::io::ErrorKind::Interrupted{continue;}return Err("Compositor observation failed".into());}
        if epoch(&path)?!=expected{return Err("Compositor session changed".into());}
        let ready=status>0;let event=if ready{receive(&mut events)?.1}else{Value::Null};
        policy.begin()?;
        // Refresh also observes settings changes and renews captured display registrations.
        refresh(&mut policy)?;
        if event["change"]=="focus"&&event["current"]["type"]=="workspace"{if let Some(id)=event["current"]["id"].as_i64(){policy.focus(id)?;}}
        let node=&event["container"];
        if event["change"]=="new"{if let Some(id)=node["id"].as_u64(){pending.insert(id);}}
        if node["id"].as_u64().is_some_and(|id|pending.contains(&id))&&Policy::application(node).is_some(){
            let result=place(&policy,node)?;println!("{result}");std::io::stdout().flush().map_err(|e|e.to_string())?;
            if let Some(id)=node["id"].as_u64(){pending.remove(&id);}
        }
        if event["change"]=="close"{if let Some(id)=node["id"].as_u64(){pending.remove(&id);}}
        let tree=sway(&["-t","get_tree"])?;lumen_spaces::reconcile_membership_observed(&mut policy,&tree,&expected,&sway(&["-t","get_version"])?,membership_command)?;policy.release();
    }
}
fn run()->Result<()> {
    let mut args=env::args().skip(1).collect::<Vec<_>>();
    let state=if args.first().is_some_and(|arg|arg=="--state"){
        if args.len()<2{return Err("Missing desktop state path".into());}
        let path=PathBuf::from(args.remove(1));args.remove(0);path
    }else{
        PathBuf::from(env::var_os("XDG_STATE_HOME").unwrap_or_else(||PathBuf::from(env::var_os("HOME").unwrap_or_default()).join(".local/state").into_os_string())).join("lumen-desktop/spaces.json")
    };
    let action=args.first().map(String::as_str).ok_or("Missing desktop command")?;
    let mut policy=Policy::open(state)?;
    if action=="watch"{return run_watch(policy);}
    refresh(&mut policy)?;
    match action{
        "status"=>println!("{}",policy.status()),
        "ordering"=>{let auto=match args.get(1).map(String::as_str){Some("true")=>true,Some("false")=>false,_=>return Err("Invalid desktop ordering".into())};policy.configure(auto)?;},
        "assign"=>{
            let app=args.get(1).ok_or("Missing application")?;let kind=args.get(2).ok_or("Missing assignment kind")?;
            let tree=sway(&["-t","get_tree"])?;
            if kind=="all"&&windows(&tree).iter().any(|node|lumen_spaces::projection::grouped_assignment_blocked(node)&&Policy::application(node).as_deref()==Some(app)){return Err("Turn off window groups before assigning all desktops".into());}
            policy.assign(app,kind,args.get(3).map(String::as_str))?;
            let identity=epoch(&socket()?)?;lumen_spaces::reconcile_membership_observed(&mut policy,&tree,&identity,&sway(&["-t","get_version"])?,membership_command)?;
        },
        "switch"=>{
            let direction=args.get(1).map(String::as_str).ok_or("Missing switch direction")?;
            if !matches!(direction,"previous"|"next"){return Err("Invalid switch direction".into());}
            if let Some(ws)=policy.workspaces.iter().find(|ws|ws["focused"]==true){
                let output=ws["output"].as_str().ok_or("Invalid desktop output")?;let order=policy.order(Some(output));
                let key=policy.current.get(&ws["id"].as_i64().ok_or("Invalid desktop id")?).ok_or("Unknown desktop")?;
                let index=order.iter().position(|item|item==key).ok_or("Unknown desktop")?;
                let target=if direction=="previous"{index.checked_sub(1)}else{index.checked_add(1).filter(|next|*next<order.len())};
                if let Some(target)=target{
                    let desktop=policy.desktop(&order[target],true)?;
                    let operation=display_runtime::activation(&policy,&desktop,&mut |args|sway(args))?
                        .unwrap_or_else(||format!("workspace {}",json!(desktop["name"])));
                    command(&operation)?;
                }
            }
        },
        _=>return Err("Unsupported desktop command".into()),
    }
    Ok(())
}
fn main(){if let Err(error)=run(){eprintln!("{error}");std::process::exit(1);}}
