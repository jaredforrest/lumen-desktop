use lumen_desktop_state::{Result,snapshot,contexts,tree_context,activation_target};
use serde_json::{Value,json};
use sha2::{Digest,Sha256};
use std::env;
use std::fs;
use std::io::{self,Read};
use std::os::unix::fs::{FileTypeExt,MetadataExt};
use std::path::Path;
use std::process::Command;

fn epoch(path:&Path)->Result<String>{
    let metadata=fs::symlink_metadata(path).map_err(|e|e.to_string())?;
    if !metadata.file_type().is_socket()||metadata.uid()!=unsafe{libc::getuid()}{return Err("Compositor endpoint is not an owned socket".into());}
    let time=metadata.ctime() as i128*1_000_000_000+metadata.ctime_nsec() as i128;
    Ok(format!("{:x}",Sha256::digest(format!("{}:{}:{time}",metadata.dev(),metadata.ino()).as_bytes()))[..24].to_owned())
}
fn query(kind:&str)->Result<Value>{
    let output=Command::new("timeout").args(["4s","swaymsg","-r","-t",kind]).output().map_err(|e|e.to_string())?;
    if !output.status.success(){return Err("Compositor query failed".into());}
    serde_json::from_slice(&output.stdout).map_err(|e|e.to_string())
}
fn stdin_json()->Result<Value>{
    let mut bytes=Vec::new();io::stdin().take(16*1024*1024).read_to_end(&mut bytes).map_err(|e|e.to_string())?;
    serde_json::from_slice(&bytes).map_err(|e|e.to_string())
}
fn run()->Result<Value>{
    let args=env::args().skip(1).collect::<Vec<_>>();
    if args.first().is_some_and(|arg|arg=="--tree-context-json"){return tree_context(&stdin_json()?);}
    if args.first().is_some_and(|arg|arg=="--contexts-json"){
        let input=stdin_json()?;return contexts(input["windows"].as_array().ok_or("Invalid context windows")?,input.get("mainByApp"));
    }
    if args.first().is_some_and(|arg|arg=="--activate-json"){
        let input=stdin_json()?;return activation_target(input["windows"].as_array().ok_or("Invalid context windows")?,&input["target"]);
    }
    if args.first().is_some_and(|arg|arg=="--snapshot-json"){
        let input=stdin_json()?;return snapshot(&input["tree"],input["outputs"].as_array().ok_or("Invalid compositor outputs")?,input["epoch"].as_str().unwrap_or("fixture"));
    }
    if args.first().is_some_and(|arg|arg=="--fixture"){
        let path=args.get(1).ok_or("Missing fixture path")?;
        let input:Value=serde_json::from_slice(&fs::read(path).map_err(|e|e.to_string())?).map_err(|e|e.to_string())?;
        return snapshot(&input["tree"],input["outputs"].as_array().ok_or("Invalid compositor outputs")?,"fixture");
    }
    if !args.is_empty(){return Err("Unsupported desktop state command".into());}
    let socket=env::var_os("SWAYSOCK").ok_or("No compatible compositor endpoint")?;
    let first=epoch(Path::new(&socket))?;let tree=query("get_tree")?;let outputs=query("get_outputs")?;
    if epoch(Path::new(&socket))?!=first{return Err("Compositor changed during snapshot; retry".into());}
    snapshot(&tree,outputs.as_array().ok_or("Invalid compositor outputs")?,&first)
}
fn main(){match run(){Ok(value)=>println!("{value}"),Err(_)=>{eprintln!("{}",json!({"ok":false,"error":"StateError","message":"Desktop state could not be validated"}));std::process::exit(1);}}}
