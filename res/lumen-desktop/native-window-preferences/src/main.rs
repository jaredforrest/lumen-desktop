//! Apply declared window appearance only while the owning application is stopped.
use serde_json::{Value, json};
use std::{env, fs, io::Write, os::unix::fs::{MetadataExt,OpenOptionsExt}, path::{Path, PathBuf}, process,
    time::{SystemTime,UNIX_EPOCH}};

type Result<T> = std::result::Result<T, String>;

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    fs::create_dir_all(path.parent().ok_or("Invalid preferences path")?).map_err(|e| e.to_string())?;
    let unique=SystemTime::now().duration_since(UNIX_EPOCH).map_err(|e|e.to_string())?.as_nanos();
    let temporary = path.with_extension(format!("tmp-{}-{unique}", process::id()));
    let result = (|| {
        let mut file = fs::OpenOptions::new().write(true).create_new(true).mode(0o600).open(&temporary).map_err(|e|e.to_string())?;
        file.write_all(bytes).map_err(|e|e.to_string())?;
        file.sync_all().map_err(|e|e.to_string())?;
        fs::rename(&temporary,path).map_err(|e|e.to_string())
    })();
    let _=fs::remove_file(&temporary);
    result
}

fn update(path: &Path, key: &str, value: &Value) -> Result<()> {
    let mut data: Value = if path.exists() { serde_json::from_slice(&fs::read(path).map_err(|e|e.to_string())?)
        .map_err(|e|e.to_string())? } else { json!({}) };
    let mut target = data.as_object_mut().ok_or("Expected an application preferences object")?;
    let mut parts = key.split('.').peekable();
    while let Some(part) = parts.next() {
        if parts.peek().is_none() {
            if target.get(part)==Some(value) { return Ok(()); }
            target.insert(part.into(),value.clone());
            return write_atomic(path,data.to_string().as_bytes());
        }
        target = target.entry(part).or_insert_with(||json!({})).as_object_mut()
            .ok_or("Expected a window preferences object")?;
    }
    Err("Invalid appearance key".into())
}

fn hostname() -> Result<String> {
    fs::read_to_string("/proc/sys/kernel/hostname").map(|s|s.trim().to_owned()).map_err(|e|e.to_string())
}

fn running(root: &Path) -> bool {
    let lock=root.join("SingletonLock");
    let Ok(target)=fs::read_link(&lock) else { return lock.exists(); };
    let target=target.to_string_lossy();
    let Some((host,pid))=target.rsplit_once('-') else { return true; };
    if hostname().as_deref()!=Ok(host) { return true; }
    let Ok(pid)=pid.parse::<i32>() else { return true; };
    if pid<2 { return true; }
    let status=unsafe {libc::kill(pid,0)};
    status==0 || std::io::Error::last_os_error().raw_os_error()!=Some(libc::ESRCH)
}

fn expanded(value: &str) -> PathBuf {
    if value=="~" { return env::var_os("HOME").map(PathBuf::from).unwrap_or_else(||PathBuf::from(value)); }
    if let Some(tail)=value.strip_prefix("~/") {
        return env::var_os("HOME").map(PathBuf::from).unwrap_or_else(||PathBuf::from("~")).join(tail);
    }
    PathBuf::from(value)
}

fn brave(config:&Path,args:&[String])->Result<()> {
    let mut root=config.join("BraveSoftware/Brave-Browser");
    for (index,arg) in args.iter().enumerate() {
        if let Some(value)=arg.strip_prefix("--user-data-dir=") { root=expanded(value); }
        else if arg=="--user-data-dir" { if let Some(value)=args.get(index+1) { root=expanded(value); } }
    }
    if running(&root) { return Ok(()); }
    let mut profiles=Vec::new();
    if let Ok(entries)=fs::read_dir(&root) {
        for entry in entries.flatten() {
            let path=entry.path().join("Preferences");
            if path.is_file() { profiles.push(path); }
        }
    }
    if profiles.is_empty() { profiles.push(root.join("Default/Preferences")); }
    let appearance:Value=serde_json::from_str(include_str!("../brave-appearance.json")).map_err(|e|e.to_string())?;
    for profile in profiles {
        for (key,value) in appearance["profile"].as_object().ok_or("Invalid Brave appearance policy")? {
            update(&profile,key,value)?;
        }
    }
    for (key,value) in appearance["local_state"].as_object().ok_or("Invalid Brave state policy")? {
        update(&root.join("Local State"),key,value)?;
    }
    Ok(())
}

fn gimp_running()->bool {
    let Ok(entries)=fs::read_dir("/proc") else { return false; };
    for entry in entries.flatten() {
        if !entry.file_name().to_string_lossy().bytes().all(|c|c.is_ascii_digit()) { continue; }
        let path=entry.path();
        if fs::metadata(&path).is_ok_and(|m|m.uid()==unsafe {libc::geteuid()}) {
            if fs::read_link(path.join("exe")).ok().and_then(|p|p.file_name().map(|s|s.to_string_lossy().to_string()))
                .is_some_and(|name|name.trim_start_matches('.').starts_with("gimp")) { return true; }
        }
    }
    false
}

fn gimp_line(line:&str)->bool {
    let trimmed=line.trim();
    let Some(rest)=trimmed.strip_prefix("(theme-color-scheme") else { return false; };
    let Some(rest)=rest.strip_suffix(')') else { return false; };
    rest.chars().next().is_some_and(char::is_whitespace)
        && ["light","dark","gray","system"].contains(&rest.trim())
}

fn gimp(config:&Path,args:&[String])->Result<()> {
    if args.iter().any(|arg|["--help","--version","-v","--dump-gimprc"].contains(&arg.as_str())) || gimp_running() {return Ok(());}
    let mut path=env::var_os("GIMP3_DIRECTORY").map(PathBuf::from).unwrap_or_else(||config.join("GIMP/3.2")).join("gimprc");
    for (index,arg) in args.iter().enumerate() {
        if let Some(value)=arg.strip_prefix("--gimprc=") {path=PathBuf::from(value);}
        else if ["--gimprc","-g"].contains(&arg.as_str()) {if let Some(value)=args.get(index+1){path=PathBuf::from(value);}}
    }
    let original=if path.exists(){fs::read_to_string(&path).map_err(|e|e.to_string())?}else{String::new()};
    let mut lines=Vec::new();
    for line in original.split_inclusive('\n') {
        if gimp_line(line) {lines.push(format!("(theme-color-scheme system){}",if line.ends_with('\n'){"\n"}else{""}));}
        else {lines.push(line.to_owned());}
    }
    let mut text=lines.concat();
    if text==original && !text.contains("(theme-color-scheme system)") {
        text=format!("{}\n(theme-color-scheme system)\n",original.trim_end());
    }
    if text!=original {write_atomic(&path,text.as_bytes())?;}
    Ok(())
}

fn run(app:&str,args:&[String])->Result<()> {
    let home=env::var_os("HOME").ok_or("HOME is missing")?;
    let config=env::var_os("XDG_CONFIG_HOME").map(PathBuf::from).unwrap_or_else(||PathBuf::from(home).join(".config"));
    match app {
        "brave"=>brave(&config,args),
        "gimp"=>gimp(&config,args),
        "obsidian"=>{let root=config.join("obsidian");if !running(&root){update(&root.join("obsidian.json"),"frame",&json!("native"))?;}Ok(())},
        _=>Err("Unsupported window preferences adapter".into()),
    }
}

fn main(){
    let args:Vec<String>=env::args().skip(1).collect();
    let result=args.first().ok_or("Missing application".into()).and_then(|app|run(app,&args[1..]));
    if let Err(error)=result {eprintln!("{error}");process::exit(1);}
}

#[cfg(test)] mod tests {
    use super::*;
    #[test] fn gimp_rule_recognizes_only_declared_values(){
        assert!(gimp_line("  (theme-color-scheme dark)  \n"));
        assert!(!gimp_line("(theme-color-scheme rainbow)"));
        assert!(!gimp_line("# (theme-color-scheme dark)"));
    }
    #[test] fn policy_is_valid_json(){
        let value:Value=serde_json::from_str(include_str!("../brave-appearance.json")).unwrap();
        assert!(value["profile"].is_object());
        assert!(value["local_state"].is_object());
    }
}
