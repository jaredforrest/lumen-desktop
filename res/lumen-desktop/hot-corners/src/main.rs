//! Validated native corner transitions and exact desktop actions.
use serde_json::{Value, json};
use std::collections::HashSet;
use std::env;
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

type Result<T> = std::result::Result<T, String>;
const CORNERS: &[&str] = &["top-left","top-right","bottom-left","bottom-right"];
const MODIFIERS: &[&str] = &["shift","control","alt","super"];
const ACTIONS: &[&str] = &["disabled","overview","app-windows","show-desktop","notifications-toggle",
    "app-launcher","lock","display-sleep"];

fn validate(value:&Value) -> Result<Value> {
    let supplied=value.as_object().ok_or("Unknown corner")?;
    if supplied.keys().any(|key| !CORNERS.contains(&key.as_str())) {return Err("Unknown corner".into());}
    let mut result=json!({});
    for corner in CORNERS {
        let default=json!({});let item=supplied.get(*corner).unwrap_or(&default);
        let fields=item.as_object().ok_or("Invalid corner configuration")?;
        if fields.keys().any(|key| !["action","modifiers","allowFullscreen","allowAdjacent"].contains(&key.as_str())) {
            return Err("Invalid corner configuration".into());
        }
        let action=item.get("action").map_or(Some("disabled"),Value::as_str).ok_or("Unsupported corner action or modifiers")?;
        let modifiers=item.get("modifiers").cloned().unwrap_or_else(||json!([]));
        let modifiers=modifiers.as_array().ok_or("Unsupported corner action or modifiers")?;
        let mut seen=HashSet::new();
        if !ACTIONS.contains(&action) || modifiers.iter().any(|item| item.as_str().is_none_or(|value|
            !MODIFIERS.contains(&value) || !seen.insert(value))) {
            return Err("Unsupported corner action or modifiers".into());
        }
        let fullscreen=item.get("allowFullscreen").map_or(Some(false),Value::as_bool)
            .ok_or("Corner safeguards must be boolean")?;
        let adjacent=item.get("allowAdjacent").map_or(Some(false),Value::as_bool)
            .ok_or("Corner safeguards must be boolean")?;
        result[*corner]=json!({"action":action,"modifiers":modifiers,"allowFullscreen":fullscreen,"allowAdjacent":adjacent});
    }
    Ok(result)
}
struct CornerPolicy {config:Value,entered:Option<(String,String)>}
impl CornerPolicy {
    fn new(config:Value) -> Result<Self> {Ok(Self {config:validate(&config)?,entered:None})}
    fn transition(&mut self,event:&Value) -> Result<Option<String>> {
        if event["kind"]!="lumen-corner" || event["version"]!=1 {return Ok(None);}
        if event["corner"].is_null() {self.entered=None;return Ok(None);}
        let corner=event["corner"].as_str().ok_or("Invalid native corner transition")?;
        let output=event["output"].as_str().filter(|value| !value.is_empty()).ok_or("Invalid native corner transition")?;
        if !CORNERS.contains(&corner) {return Err("Invalid native corner transition".into());}
        let identity=(output.to_owned(),corner.to_owned());
        if self.entered.as_ref()==Some(&identity) {return Ok(None);}
        self.entered=Some(identity);
        let config=&self.config[corner];
        let empty=json!([]);let modifiers=event.get("modifiers").unwrap_or(&empty).as_array()
            .ok_or("Invalid native modifiers")?;
        if modifiers.iter().any(|value|value.as_str().is_none_or(|value|!MODIFIERS.contains(&value))) {
            return Err("Invalid native modifiers".into());
        }
        let (Some(locked),Some(fullscreen),Some(adjacent))=(event["locked"].as_bool(),
            event["fullscreen"].as_bool(),event["adjacent"].as_bool()) else {return Ok(None);};
        if locked || fullscreen && config["allowFullscreen"]!=true || adjacent && config["allowAdjacent"]!=true {
            return Ok(None);
        }
        if !config["modifiers"].as_array().is_some_and(|required| required.iter().all(|item|modifiers.contains(item))) {
            return Ok(None);
        }
        let action=config["action"].as_str().unwrap_or("disabled");
        Ok(if action=="disabled" {None}else{Some(action.to_owned())})
    }
}
fn dispatch(action:&str) -> Result<()> {
    if action=="disabled" || !ACTIONS.contains(&action) {return Err("Unsupported corner action".into());}
    let (program,args):(&str,Vec<&str>)=match action {
        "lock"=>("lumen-lock",vec![]),
        "app-launcher"=>("lumen-app-launcher",vec![]),
        "display-sleep"=>("swaymsg",vec!["output * power off"]),
        _=>("lumen-desktop-action",vec![action]),
    };
    let status=Command::new("timeout").arg("15s").arg(program).args(args).status().map_err(|e|e.to_string())?;
    if !status.success() {return Err("Corner action failed".into());}
    Ok(())
}
fn config_path() -> Result<PathBuf> {
    let args:Vec<String>=env::args().skip(1).collect();
    if args.first().map(String::as_str)==Some("--config") {
        return args.get(1).map(PathBuf::from).ok_or("Missing corner config path".into());
    }
    let state=env::var_os("XDG_STATE_HOME").map(PathBuf::from)
        .unwrap_or_else(||PathBuf::from(env::var_os("HOME").unwrap_or_default()).join(".local/state"));
    Ok(state.join("lumen-desktop/hot-corners.json"))
}
fn load(path:&Path) -> Result<Value> {
    let value=match fs::read(path) {Ok(raw)=>serde_json::from_slice(&raw).map_err(|e|e.to_string())?,
        Err(error) if error.kind()==std::io::ErrorKind::NotFound=>json!({}),Err(error)=>return Err(error.to_string())};
    validate(&value)
}
fn write_config(path:&Path,value:&Value) -> Result<()> {
    let value=validate(value)?;
    fs::create_dir_all(path.parent().ok_or("Invalid config path")?).map_err(|e|e.to_string())?;
    let pending=path.with_extension(format!("tmp-{}",std::process::id()));
    let mut file=OpenOptions::new().write(true).create_new(true).mode(0o600).open(&pending)
        .map_err(|e|e.to_string())?;
    file.write_all(value.to_string().as_bytes()).map_err(|e|e.to_string())?;
    file.sync_all().map_err(|e|e.to_string())?;
    fs::rename(pending,path).map_err(|e|e.to_string())
}
fn status(path:&Path) -> Result<Value> {
    let result=Command::new("timeout").args(["5s","swaymsg","-r","-t","get_version"])
        .output().map_err(|e|e.to_string())?;
    if !result.status.success() {return Err("Compositor version unavailable".into());}
    let version:Value=serde_json::from_slice(&result.stdout).map_err(|e|e.to_string())?;
    Ok(json!({"corners":load(path)?,"nativeAvailable":version["native_hot_corners"]==true}))
}
fn watch(path:&Path) -> Result<()> {
    let mut policy=CornerPolicy::new(load(path)?)?;
    let mut child=Command::new("swaymsg").args(["-r","-m","-t","subscribe","[\"tick\"]"])
        .stdout(Stdio::piped()).spawn().map_err(|e|e.to_string())?;
    let stdout=child.stdout.take().ok_or("No compositor event stream")?;
    for line in BufReader::new(stdout).lines() {
        let line=line.map_err(|e|e.to_string())?;
        let Ok(event)=serde_json::from_str::<Value>(&line) else {continue;};
        let Some(payload)=event["payload"].as_str().filter(|value| !value.is_empty()) else {continue;};
        let Ok(transition)=serde_json::from_str::<Value>(payload) else {continue;};
        if transition["kind"]!="lumen-corner" {continue;}
        policy.config=load(path)?;
        if let Some(action)=policy.transition(&transition)? {dispatch(&action)?;}
    }
    let _=child.kill();let _=child.wait();
    Ok(())
}
fn main() {
    let args:Vec<String>=env::args().skip(1).collect();
    let (path,action)=if args.first().map(String::as_str)==Some("--config") {
        (config_path(),args.get(2).map(String::as_str))
    } else {(config_path(),args.first().map(String::as_str))};
    let result=path.and_then(|path|match action {
        Some("status")=>status(&path).map(|value|println!("{value}")),
        Some("configure")=>{
            let mut line=String::new();
            std::io::stdin().lock().read_line(&mut line).map_err(|e|e.to_string())?;
            let value:Value=serde_json::from_str(&line).map_err(|e|e.to_string())?;
            write_config(&path,&value)?;println!("{}",json!({"ok":true}));Ok(())
        }
        Some("watch")=>watch(&path),
        _=>Err("Unknown hot corner command".into()),
    });
    if let Err(error)=result {eprintln!("{error}");std::process::exit(1);}
}

#[cfg(test)]
mod tests {
    use super::*;
    fn event(corner:&str) -> Value {json!({"version":1,"kind":"lumen-corner","corner":corner,
        "output":"A","modifiers":[],"locked":false,"fullscreen":false,"adjacent":false})}
    #[test]
    fn safeguards_and_reentry() {
        let mut policy=CornerPolicy::new(json!({"top-left":{"action":"overview"}})).unwrap();
        assert_eq!(policy.transition(&event("top-left")).unwrap().as_deref(),Some("overview"));
        assert_eq!(policy.transition(&event("top-left")).unwrap(),None);
        policy.transition(&json!({"version":1,"kind":"lumen-corner","corner":null})).unwrap();
        let mut locked=event("top-left");locked["locked"]=json!(true);
        assert_eq!(policy.transition(&locked).unwrap(),None);
        assert_eq!(policy.transition(&event("top-left")).unwrap(),None);
    }
    #[test]
    fn invalid_config_is_rejected() {
        for value in [json!({"bad":{}}),json!({"top-left":{"action":"exec rm"}}),
            json!({"top-left":{"modifiers":["super","super"]}}),json!({"top-left":{"allowFullscreen":1}})] {
            assert!(validate(&value).is_err());
        }
    }
}
