//! Private placement preferences for registered singleton windows.
use super::{Row, Result, rect, truthy};
use serde_json::{Value, json};
use std::env;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::os::fd::AsRawFd;
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt};
use std::path::PathBuf;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const MAX_BYTES: u64 = 16_384;

pub fn owner_key(value: &str) -> Result<&str> {
    if matches!(value,"settings.main"|"calendar.main") { Ok(value) }
    else { Err("Unsupported owned geometry registration".into()) }
}

fn dimension(value: &Value, low: i64, high: i64) -> Result<i64> {
    value.as_i64().filter(|number| *number >= low && *number <= high)
        .ok_or("Invalid owned geometry dimension".into())
}

pub fn record(value: &Value) -> Result<Value> {
    let fields=value.as_object().ok_or("Invalid owned geometry record")?;
    if fields.len()!=5 || !["output","x","y","width","height"].iter().all(|key| fields.contains_key(*key)) {
        return Err("Invalid owned geometry record".into());
    }
    let output=value["output"].as_str().filter(|output| !output.is_empty() && output.len()<=256
        && !output.chars().any(|ch| (ch as u32)<32)).ok_or("Invalid owned geometry output")?;
    Ok(json!({"output":output,"x":dimension(&value["x"],-32768,32768)?,
        "y":dimension(&value["y"],-32768,32768)?,"width":dimension(&value["width"],1,32768)?,
        "height":dimension(&value["height"],1,32768)?}))
}

fn document(value: &Value) -> Result<Value> {
    let fields=value.as_object().ok_or("Invalid owned geometry state")?;
    if fields.len()!=2 || value["version"]!=1 || !fields.contains_key("owners") {
        return Err("Invalid owned geometry state".into());
    }
    let owners=value["owners"].as_object().ok_or("Invalid owned geometry owners")?;
    let mut clean=serde_json::Map::new();
    for (key,placement) in owners {
        owner_key(key)?;
        clean.insert(key.clone(),record(placement)?);
    }
    Ok(json!({"version":1,"owners":clean}))
}

pub fn state_directory() -> PathBuf {
    env::var_os("XDG_STATE_HOME").map(PathBuf::from)
        .unwrap_or_else(||PathBuf::from(env::var_os("HOME").unwrap_or_default()).join(".local/state"))
        .join("lumen-desktop/owned-geometry")
}

pub struct Store { pub directory: PathBuf }
impl Store {
    pub fn new(directory: Option<PathBuf>) -> Self { Self { directory:directory.unwrap_or_else(state_directory) } }

    fn lock(&self) -> Result<File> {
        fs::DirBuilder::new().recursive(true).mode(0o700).create(&self.directory).map_err(|e|e.to_string())?;
        let info=fs::symlink_metadata(&self.directory).map_err(|e|e.to_string())?;
        if !info.is_dir() || info.uid()!=unsafe{libc::getuid()} { return Err("Owned geometry state directory is unsafe".into()); }
        if info.mode()&0o077!=0 { return Err("Owned geometry state directory is not private".into()); }
        let lock=OpenOptions::new().read(true).write(true).create(true).mode(0o600)
            .custom_flags(libc::O_NOFOLLOW|libc::O_CLOEXEC)
            .open(self.directory.join(".placements.lock")).map_err(|e|e.to_string())?;
        let info=lock.metadata().map_err(|e|e.to_string())?;
        if !info.is_file() || info.uid()!=unsafe{libc::getuid()} || info.nlink()!=1 || info.mode()&0o077!=0 {
            return Err("Owned geometry state lock is unsafe".into());
        }
        let deadline=Instant::now()+Duration::from_millis(500);
        loop {
            if unsafe{libc::flock(lock.as_raw_fd(),libc::LOCK_EX|libc::LOCK_NB)}==0 { return Ok(lock); }
            if Instant::now()>=deadline { return Err("Owned geometry state is busy".into()); }
            thread::sleep(Duration::from_millis(10));
        }
    }

    fn read(&self) -> Result<Value> {
        let path=self.directory.join("placements.json");
        let file=match OpenOptions::new().read(true).custom_flags(libc::O_NOFOLLOW|libc::O_NONBLOCK|libc::O_CLOEXEC).open(path) {
            Ok(file)=>file,Err(error) if error.kind()==std::io::ErrorKind::NotFound=>return Ok(json!({"version":1,"owners":{}})),
            Err(error)=>return Err(error.to_string()),
        };
        let info=file.metadata().map_err(|e|e.to_string())?;
        if !info.is_file() || info.uid()!=unsafe{libc::getuid()} || info.nlink()!=1 || info.mode()&0o077!=0 || info.len()>MAX_BYTES {
            return Err("Owned geometry state file is unsafe".into());
        }
        let mut raw=Vec::new();file.take(MAX_BYTES+1).read_to_end(&mut raw).map_err(|e|e.to_string())?;
        if raw.len() as u64>MAX_BYTES { return Err("Owned geometry state exceeds supported size".into()); }
        document(&serde_json::from_slice::<Value>(&raw).map_err(|e|e.to_string())?)
    }

    pub fn get(&self,owner:&str) -> Result<Option<Value>> {
        owner_key(owner)?;let _guard=self.lock()?;
        Ok(self.read()?["owners"].get(owner).cloned())
    }

    pub fn put(&self,owner:&str,placement:&Value) -> Result<bool> {
        owner_key(owner)?;let placement=record(placement)?;let _guard=self.lock()?;
        let mut current=self.read()?;
        if current["owners"][owner]==placement { return Ok(false); }
        current["owners"][owner]=placement;
        let stamp=SystemTime::now().duration_since(UNIX_EPOCH).map_err(|e|e.to_string())?.as_nanos();
        let temporary=self.directory.join(format!(".owned-geometry-{}-{stamp}",std::process::id()));
        let result=(||->Result<()> {
            let mut file=OpenOptions::new().write(true).create_new(true).mode(0o600).open(&temporary).map_err(|e|e.to_string())?;
            serde_json::to_writer(&mut file,&current).map_err(|e|e.to_string())?;
            file.write_all(b"\n").and_then(|_|file.flush()).and_then(|_|file.sync_all()).map_err(|e|e.to_string())?;
            fs::rename(&temporary,self.directory.join("placements.json")).map_err(|e|e.to_string())?;
            File::open(&self.directory).and_then(|dir|dir.sync_all()).map_err(|e|e.to_string())
        })();
        let _=fs::remove_file(temporary);result?;Ok(true)
    }
}

pub fn eligible(row:&Row) -> bool {
    let node=&row.node;let state=&node["lumen_state"];let relation=&node["lumen_relationships"];let pair=&node["lumen_pair"];
    row.floating && node["type"]=="floating_con" && row.workspace.as_ref().is_some_and(|ws|ws["name"]!="__i3_scratch")
        && !truthy(&node["fullscreen_mode"]) && matches!(node["scratchpad_state"].as_str(),None|Some("none"))
        && !truthy(&pair["id"]) && !truthy(&pair["survivor"])
        && !["filled","zoomed","minimised","hidden","group_hidden","desktop_hidden"].iter().any(|key|truthy(&state[*key]))
        && relation["parent_supported"]==true && relation["parent"].is_null() && relation["modal"]!=true
}

pub fn observed(row:&Row,output:&str) -> Result<Option<Value>> {
    if !eligible(row) { return Ok(None); }
    let workspace=row.workspace.as_ref().ok_or("Window workspace unavailable")?;
    let area=rect(&workspace["rect"],"workspace rectangle")?;
    let outer=rect(&row.node["rect"],"container rectangle")?;
    let client=&row.node["window_rect"];
    let deco=row.node["deco_rect"]["height"].as_i64().unwrap_or(0);
    Ok(Some(record(&json!({"output":output,"x":outer.x-area.x,"y":outer.y-deco-area.y,
        "width":dimension(&client["width"],1,32768)?,"height":dimension(&client["height"],1,32768)?}))?))
}

pub fn restore_plan(row:&Row,saved:&Value,min_width:i64,min_height:i64,margin_width:i64,margin_height:i64) -> Result<Option<Value>> {
    if !eligible(row) { return Ok(None); }
    let saved=record(saved)?;
    let workspace=row.workspace.as_ref().ok_or("Window workspace unavailable")?;
    let area=rect(&workspace["rect"],"workspace rectangle")?;
    let outer=rect(&row.node["rect"],"container rectangle")?;
    let client=&row.node["window_rect"];
    let deco=row.node["deco_rect"]["height"].as_i64().unwrap_or(0);
    let pad_width=outer.width-dimension(&client["width"],1,32768)?;
    let pad_height=outer.height+deco-dimension(&client["height"],1,32768)?;
    if pad_width<0 || pad_height<0 { return Err("Invalid native geometry padding".into()); }
    let minimum_width=dimension(&json!(min_width),1,32768)?;
    let minimum_height=dimension(&json!(min_height),1,32768)?;
    if minimum_width+pad_width>area.width || minimum_height+pad_height>area.height { return Ok(None); }
    let width=minimum_width.max(saved["width"].as_i64().unwrap().min((area.width-margin_width.max(0)-pad_width).max(minimum_width)));
    let height=minimum_height.max(saved["height"].as_i64().unwrap().min((area.height-margin_height.max(0)-pad_height).max(minimum_height)));
    let outer_width=width+pad_width;let outer_height=height+pad_height;
    let x=area.x+(saved["x"].as_i64().unwrap().clamp(0,area.width-outer_width));
    let y=area.y+(saved["y"].as_i64().unwrap().clamp(0,area.height-outer_height));
    Ok(Some(json!({"width":width,"height":height,"outerWidth":outer_width,"outerHeight":outer_height,"x":x,"y":y})))
}

pub fn enabled() -> bool {
    let path=env::var_os("LUMEN_APPEARANCE_STATE").map(PathBuf::from)
        .unwrap_or_else(||state_directory().parent().unwrap().join("appearance.json"));
    fs::metadata(&path).is_ok_and(|info|info.len()<=1024*1024)
        && fs::read(&path).ok().and_then(|bytes|serde_json::from_slice::<Value>(&bytes).ok())
            .is_some_and(|value|value["windowsPreferences"]["rememberOwnedGeometry"]==true)
}

pub fn output_for(tree:&Value,workspace_id:i64) -> Option<String> {
    fn walk(node:&Value,workspace_id:i64,output:Option<&str>) -> Option<String> {
        let output=if node["type"]=="output" {node["name"].as_str()} else {output};
        if node["type"]=="workspace" && node["id"]==workspace_id { return output.map(ToOwned::to_owned); }
        for child in ["nodes","floating_nodes"].into_iter().flat_map(|kind|node[kind].as_array().into_iter().flatten()) {
            if let Some(found)=walk(child,workspace_id,output) { return Some(found); }
        }
        None
    }
    walk(tree,workspace_id,None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn store_rejects_unsafe_state_and_keeps_other_owner() {
        let root=env::temp_dir().join(format!("lumen-owned-geometry-test-{}",std::process::id()));
        let _=fs::remove_dir_all(&root);
        let store=Store::new(Some(root.clone()));
        let row=json!({"output":"A","x":10,"y":20,"width":400,"height":300});
        assert_eq!(store.put("settings.main",&row).unwrap(),true);
        assert_eq!(store.put("settings.main",&row).unwrap(),false);
        assert_eq!(store.get("settings.main").unwrap(),Some(row.clone()));
        assert_eq!(store.get("calendar.main").unwrap(),None);
        assert!(store.put("other",&row).is_err());
        fs::set_permissions(root.join("placements.json"),std::os::unix::fs::PermissionsExt::from_mode(0o644)).unwrap();
        assert!(store.get("settings.main").is_err());
        let _=fs::remove_dir_all(root);
    }

    #[test]
    fn records_and_published_preference_are_strict() {
        let valid=json!({"output":"HEADLESS-1","x":20,"y":30,"width":800,"height":600});
        assert_eq!(record(&valid).unwrap(),valid);
        for changed in [json!({"output":"HEADLESS-1","x":true,"y":30,"width":800,"height":600}),
            json!({"output":"HEADLESS-1","x":20.5,"y":30,"width":800,"height":600}),
            json!({"output":"HEADLESS-1","x":20,"y":30,"width":32769,"height":600}),
            json!({"output":"\n","x":20,"y":30,"width":800,"height":600}),
            json!({"output":"HEADLESS-1","x":20,"y":30,"width":800,"height":600,"pid":42})] {
            assert!(record(&changed).is_err());
        }
        assert!(owner_key("files.1").is_err());
        assert!(document(&json!({"version":true,"owners":{}})).is_err());
        assert!(document(&json!({"version":1,"owners":{"foreign":valid}})).is_err());
    }

    #[test]
    fn restore_is_bounded_by_live_area_and_rejects_altered_roles() {
        let node=json!({"type":"floating_con","rect":{"x":50,"y":60,"width":802,"height":601},
            "window_rect":{"width":800,"height":600},"deco_rect":{"height":32},
            "lumen_relationships":{"parent_supported":true,"parent":null,"modal":false}});
        let workspace=json!({"name":"one","rect":{"x":960,"y":28,"width":800,"height":500}});
        let row=Row{node,workspace:Some(workspace),floating:true};
        let saved=json!({"output":"HEADLESS-1","x":600,"y":500,"width":1200,"height":900});
        let plan=restore_plan(&row,&saved,480,320,32,32).unwrap().unwrap();
        assert_eq!((plan["width"].as_i64(),plan["height"].as_i64()),(Some(766),Some(435)));
        assert!(plan["x"].as_i64().unwrap()>=960 && plan["y"].as_i64().unwrap()>=28);
        assert!(plan["x"].as_i64().unwrap()+plan["outerWidth"].as_i64().unwrap()<=1760);
        assert!(plan["y"].as_i64().unwrap()+plan["outerHeight"].as_i64().unwrap()<=528);
        for (field,value) in [("lumen_state",json!({"zoomed":true})),("fullscreen_mode",json!(1)),
            ("lumen_pair",json!({"survivor":true})),("lumen_relationships",json!({"parent_supported":true,"parent":42})),
            ("lumen_relationships",json!({"parent_supported":true,"modal":true}))] {
            let mut altered=row.clone();altered.node[field]=value;
            assert!(restore_plan(&altered,&saved,480,320,0,0).unwrap().is_none());
            assert!(observed(&altered,"HEADLESS-1").unwrap().is_none());
        }
    }

    #[test]
    fn reopening_preserves_both_owners_and_refuses_malformed_state_without_replacing_it() {
        let root=env::temp_dir().join(format!("lumen-owned-two-{}-{}",std::process::id(),
            SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()));
        let store=Store::new(Some(root.clone()));
        let settings=json!({"output":"HEADLESS-1","x":20,"y":30,"width":800,"height":600});
        let calendar=json!({"output":"HEADLESS-2","x":40,"y":50,"width":640,"height":480});
        assert!(store.put("settings.main",&settings).unwrap());
        assert!(Store::new(Some(root.clone())).put("calendar.main",&calendar).unwrap());
        assert_eq!(store.get("settings.main").unwrap(),Some(settings.clone()));
        assert_eq!(store.get("calendar.main").unwrap(),Some(calendar));
        assert_eq!(fs::metadata(&root).unwrap().mode()&0o777,0o700);
        let path=root.join("placements.json");
        assert_eq!(fs::metadata(&path).unwrap().mode()&0o777,0o600);
        for bad in [b"{".to_vec(),json!({"version":true,"owners":{}}).to_string().into_bytes(),
            json!({"version":1,"owners":{"foreign":settings}}).to_string().into_bytes()] {
            fs::write(&path,&bad).unwrap();
            assert!(store.put("calendar.main",&settings).is_err());
            assert_eq!(fs::read(&path).unwrap(),bad);
        }
        let _=fs::remove_dir_all(root);
    }

    #[test]
    fn symlink_fifo_and_oversized_state_are_rejected_without_following_or_blocking() {
        let root=env::temp_dir().join(format!("lumen-owned-unsafe-{}-{}",std::process::id(),
            SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()));
        let store=Store::new(Some(root.join("state")));
        assert_eq!(store.get("settings.main").unwrap(),None);
        let target=root.join("target");fs::write(&target,b"untouched").unwrap();
        let path=store.directory.join("placements.json");
        std::os::unix::fs::symlink(&target,&path).unwrap();
        assert!(store.get("settings.main").is_err());
        assert_eq!(fs::read(&target).unwrap(),b"untouched");
        fs::remove_file(&path).unwrap();
        let name=std::ffi::CString::new(path.as_os_str().as_encoded_bytes()).unwrap();
        assert_eq!(unsafe{libc::mkfifo(name.as_ptr(),0o600)},0);
        let start=Instant::now();assert!(store.get("settings.main").is_err());
        assert!(start.elapsed()<Duration::from_millis(500));
        fs::remove_file(&path).unwrap();
        fs::write(&path,vec![b' ';MAX_BYTES as usize+1]).unwrap();
        fs::set_permissions(&path,std::os::unix::fs::PermissionsExt::from_mode(0o600)).unwrap();
        assert!(store.get("settings.main").is_err());
        let _=fs::remove_dir_all(root);
    }

    #[test]
    fn locked_state_returns_bounded_busy_error() {
        let root=env::temp_dir().join(format!("lumen-owned-lock-{}-{}",std::process::id(),
            SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()));
        let store=Store::new(Some(root.clone()));
        assert_eq!(store.get("settings.main").unwrap(),None);
        let lock=OpenOptions::new().read(true).write(true).open(root.join(".placements.lock")).unwrap();
        assert_eq!(unsafe{libc::flock(lock.as_raw_fd(),libc::LOCK_EX|libc::LOCK_NB)},0);
        let start=Instant::now();
        assert!(store.get("settings.main").unwrap_err().contains("busy"));
        assert!(start.elapsed()<Duration::from_secs(2));
        assert_eq!(unsafe{libc::flock(lock.as_raw_fd(),libc::LOCK_UN)},0);
        let _=fs::remove_dir_all(root);
    }
}
