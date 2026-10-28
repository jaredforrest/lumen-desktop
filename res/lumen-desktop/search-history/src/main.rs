//! Private, opt-in recent queries with generation fencing for queued requests.
use serde_json::{Value,json};
use std::{env,fs,io::{Read,Write},os::{fd::AsRawFd,unix::fs::{DirBuilderExt,MetadataExt,OpenOptionsExt}},
    path::{Path,PathBuf},process::{Command,Stdio},time::{SystemTime,UNIX_EPOCH}};

type Result<T>=std::result::Result<T,String>;
const LIMIT:usize=20;
const MAX_QUERY:usize=512;

fn state_root()->Result<PathBuf>{
    let base=env::var_os("XDG_STATE_HOME").map(PathBuf::from).unwrap_or_else(||{
        PathBuf::from(env::var_os("HOME").unwrap_or_default()).join(".local/state")
    });
    Ok(base.join("lumen-desktop/search"))
}

fn private(path:&Path,directory:bool)->Result<()> {
    let info=fs::symlink_metadata(path).map_err(|e|e.to_string())?;
    let valid=if directory{info.is_dir()}else{info.is_file()};
    if info.uid()!=unsafe{libc::getuid()} || !valid || info.mode()&0o077!=0 {
        return Err(format!("Search history requires an owned private {}",if directory{"directory"}else{"file"}));
    }
    Ok(())
}

fn settings()->Result<Value>{
    if let Some(path)=env::var_os("LUMEN_NIX_SETTINGS_ACTIVE") {
        return serde_json::from_slice(&fs::read(path).map_err(|e|e.to_string())?).map_err(|e|e.to_string());
    }
    let binary=env::var("LUMEN_NIX_SETTINGS_BIN").unwrap_or_else(|_|"lumen-nix-settings".into());
    let output=Command::new(binary).arg("read").stdin(Stdio::null()).output().map_err(|e|e.to_string())?;
    if !output.status.success(){return Err("Nix settings are unavailable".into());}
    serde_json::from_slice(&output.stdout).map_err(|e|e.to_string())
}

fn generation()->Result<String>{
    let mut bytes=[0u8;32];
    fs::File::open("/dev/urandom").and_then(|mut file|file.read_exact(&mut bytes)).map_err(|e|e.to_string())?;
    Ok(bytes.iter().map(|value|format!("{value:02x}")).collect())
}

fn write(path:&Path,value:&Value)->Result<()> {
    let unique=SystemTime::now().duration_since(UNIX_EPOCH).map_err(|e|e.to_string())?.as_nanos();
    let temporary=path.with_file_name(format!(".queries-{}-{unique}",std::process::id()));
    let result=(||{
        let mut file=fs::OpenOptions::new().write(true).create_new(true).mode(0o600).open(&temporary).map_err(|e|e.to_string())?;
        file.write_all(value.to_string().as_bytes()).map_err(|e|e.to_string())?;
        file.sync_all().map_err(|e|e.to_string())?;
        fs::rename(&temporary,path).map_err(|e|e.to_string())
    })();
    let _=fs::remove_file(&temporary);
    result
}

fn read(path:&Path)->Result<Vec<String>>{
    if !path.exists() && fs::symlink_metadata(path).is_err(){return Ok(Vec::new());}
    private(path,false)?;
    if fs::metadata(path).map_err(|e|e.to_string())?.len()>65536{return Err("Search history exceeds its size limit".into());}
    let value:Value=serde_json::from_slice(&fs::read(path).map_err(|e|e.to_string())?).map_err(|e|e.to_string())?;
    let rows=value.as_array().ok_or("Search history is malformed; clear it to reset")?;
    if rows.len()>LIMIT{return Err("Search history is malformed; clear it to reset".into());}
    let mut result=Vec::new();
    for row in rows {
        let query=row.as_str().ok_or("Search history is malformed; clear it to reset")?;
        if query.is_empty() || query.chars().count()>MAX_QUERY || query.chars().any(|c|c<' ') {
            return Err("Search history is malformed; clear it to reset".into());
        }
        result.push(query.to_owned());
    }
    Ok(result)
}

fn handle(data:&Value)->Result<Value>{
    let request=data.as_object().ok_or("A search-history request is required")?;
    let action=request.get("action").and_then(Value::as_str).ok_or("Unsupported search-history action")?;
    if !["list","record","clear"].contains(&action){return Err("Unsupported search-history action".into());}
    let root=state_root()?;
    fs::DirBuilder::new().recursive(true).mode(0o700).create(&root).map_err(|e|e.to_string())?;
    private(&root,true)?;
    let lock_path=root.join("history.lock");
    let lock=fs::OpenOptions::new().read(true).write(true).create(true).mode(0o600)
        .custom_flags(libc::O_NOFOLLOW).open(&lock_path).map_err(|e|e.to_string())?;
    private(&lock_path,false)?;
    if unsafe{libc::flock(lock.as_raw_fd(),libc::LOCK_EX)}!=0{return Err("Search history lock failed".into());}
    let allowed=settings()?["search"]["queryHistory"]==true;
    let path=root.join("queries.json");
    let epoch=root.join("generation.json");
    let mut current=if epoch.exists() || fs::symlink_metadata(&epoch).is_ok(){
        private(&epoch,false)?;
        if fs::metadata(&epoch).map_err(|e|e.to_string())?.len()>128{return Err("Invalid history generation".into());}
        let value:Value=serde_json::from_slice(&fs::read(&epoch).map_err(|e|e.to_string())?).map_err(|e|e.to_string())?;
        let token=value.as_str().ok_or("Invalid history generation")?;
        if token.len()!=64 || !token.bytes().all(|c|c.is_ascii_hexdigit()&&!c.is_ascii_uppercase()){
            return Err("Invalid history generation".into());
        }
        token.to_owned()
    }else{let token=generation()?;write(&epoch,&json!(token))?;token};
    if action=="clear" || !allowed {
        current=generation()?;
        write(&epoch,&json!(current))?;
        if path.exists() || fs::symlink_metadata(&path).is_ok(){fs::remove_file(&path).map_err(|e|e.to_string())?;}
        return Ok(json!({"enabled":allowed,"queries":[],"generation":current}));
    }
    let mut values=read(&path)?;
    if action=="record" {
        if request.get("generation").and_then(Value::as_str)!=Some(&current) {
            return Ok(json!({"enabled":allowed,"queries":values,"generation":current,"retired":true}));
        }
        let query=request.get("query").and_then(Value::as_str).ok_or("A query string is required")?.trim();
        if query.chars().count()>MAX_QUERY || query.chars().any(|c|c<' '){return Err("Query cannot be saved to history".into());}
        if !query.is_empty(){
            values.retain(|old|old!=query);
            values.insert(0,query.to_owned());
            values.truncate(LIMIT);
            write(&path,&json!(values))?;
        }
    }
    Ok(json!({"enabled":allowed,"queries":values,"generation":current}))
}

fn main(){
    let mut input=String::new();
    let result=std::io::stdin().take(65537).read_to_string(&mut input).map_err(|e|e.to_string())
        .and_then(|_|if input.len()>65536{Err("Search-history request is too large".into())}else{Ok(())})
        .and_then(|_|serde_json::from_str::<Value>(&input).map_err(|e|e.to_string()))
        .and_then(|value|handle(&value));
    match result{Ok(value)=>println!("{value}"),Err(error)=>{println!("{}",json!({"error":error}));std::process::exit(1);}}
}

#[cfg(test)] mod tests {
    use super::*;
    #[test] fn generation_is_lowercase_hex(){
        let value=generation().unwrap();
        assert_eq!(value.len(),64);
        assert!(value.bytes().all(|c|c.is_ascii_hexdigit()&&!c.is_ascii_uppercase()));
    }
}
