use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::io::{BufRead, Read, Write};
use std::os::unix::fs::{FileTypeExt, MetadataExt};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::time::Duration;

type Result<T> = std::result::Result<T, String>;
const MAX: usize = 1024 * 1024;

fn id(value: &Value, zero: bool) -> Result<u64> {
    value.as_u64().filter(|number| (zero || *number > 0) && *number <= i64::MAX as u64).ok_or("Invalid native group identity".into())
}
fn flag(value: &Value) -> bool { value.as_bool() == Some(true) }
fn window_mode(value: &Value) -> Result<&str> {
    value.as_str().filter(|mode| matches!(*mode,"all_together"|"one_at_a_time")).ok_or("Invalid group window presentation mode".into())
}
fn object(value: &Value) -> Result<&serde_json::Map<String, Value>> { value.as_object().ok_or("Invalid native group facts".into()) }
fn rows(value: &Value, max: usize) -> Result<&Vec<Value>> { value.as_array().filter(|rows| rows.len() <= max).ok_or("Invalid native group list".into()) }
fn group_facts(data: &Value) -> Result<Value> {
    let facts = object(data)?;
    if id(&data["version"], false)? != 1 || !["enabled", "locked", "separate_displays"].iter().all(|key| facts[*key].is_boolean()) || !flag(&data["separate_displays"]) {
        return Err("This compositor does not support native window groups".into());
    }
    id(&data["revision"], true)?;
    let mut groups = HashSet::new();
    let mut members = HashSet::new();
    let mut selected = HashSet::new();
    for group in rows(&data["groups"], 256)? {
        object(group)?;
        let gid = id(&group["id"], false)?;
        let workspace = id(&group["workspace"], false)?;
        id(&group["recent"], true)?;
        if !groups.insert(gid) || !group["selected"].is_boolean() || !group["suspended"].is_boolean() || (flag(&group["selected"]) && !selected.insert(workspace)) {
            return Err("Invalid native group state".into());
        }
        for member in rows(&group["members"], 4096)? {
            object(member)?;
            if !members.insert(id(&member["id"], false)?) || id(&member["pid"], false).is_err() || !member["group_hidden"].is_boolean() {
                return Err("Invalid group membership".into());
            }
        }
    }
    Ok(data.clone())
}

fn epoch(path: &Path) -> Result<String> {
    let metadata = std::fs::symlink_metadata(path).map_err(|error| error.to_string())?;
    if !metadata.file_type().is_socket() || metadata.uid() != unsafe { libc::getuid() } { return Err("Compositor endpoint is not an owned socket".into()); }
    let raw = format!("{}:{}:{}", metadata.dev(), metadata.ino(), metadata.ctime() as i128 * 1_000_000_000 + metadata.ctime_nsec() as i128);
    Ok(format!("{:x}", Sha256::digest(raw.as_bytes()))[..24].to_owned())
}
fn socket(path: &Path, timeout: Duration) -> Result<UnixStream> {
    let stream = UnixStream::connect(path).map_err(|error| error.to_string())?;
    stream.set_read_timeout(Some(timeout)).map_err(|error| error.to_string())?;
    stream.set_write_timeout(Some(timeout)).map_err(|error| error.to_string())?;
    Ok(stream)
}
fn send(stream: &mut UnixStream, kind: u32, value: &str) -> Result<()> {
    if value.len() > MAX { return Err("Oversized compositor request".into()); }
    let mut packet = Vec::with_capacity(14 + value.len());
    packet.extend_from_slice(b"i3-ipc");
    packet.extend_from_slice(&(value.len() as u32).to_le_bytes());
    packet.extend_from_slice(&kind.to_le_bytes());
    packet.extend_from_slice(value.as_bytes());
    stream.write_all(&packet).map_err(|error| error.to_string())
}
fn receive(stream: &mut UnixStream) -> Result<(u32, Value)> {
    let mut header = [0_u8; 14];
    stream.read_exact(&mut header).map_err(|error| error.to_string())?;
    if &header[..6] != b"i3-ipc" { return Err("Invalid IPC header".into()); }
    let size = u32::from_le_bytes(header[6..10].try_into().map_err(|_| "Invalid IPC header")?) as usize;
    let kind = u32::from_le_bytes(header[10..14].try_into().map_err(|_| "Invalid IPC header")?);
    if size > MAX { return Err("Oversized IPC response".into()); }
    let mut body = vec![0_u8; size];
    stream.read_exact(&mut body).map_err(|error| error.to_string())?;
    Ok((kind, serde_json::from_slice(&body).map_err(|_| "Invalid IPC JSON")?))
}
struct Native { path: PathBuf, epoch: String, stream: UnixStream }
impl Native {
    fn open() -> Result<Self> {
        let path = PathBuf::from(std::env::var_os("SWAYSOCK").ok_or("No compatible compositor endpoint")?);
        let identity = epoch(&path)?;
        Ok(Self { stream:socket(&path, Duration::from_secs(2))?, path, epoch:identity })
    }
    fn query(&mut self, kind: u32, value: &str) -> Result<Value> {
        send(&mut self.stream, kind, value)?;
        let (reply, data) = receive(&mut self.stream)?;
        if reply != kind { return Err("Unexpected compositor reply".into()); }
        Ok(data)
    }
    fn verify(&self) -> Result<()> { if epoch(&self.path)? == self.epoch { Ok(()) } else { Err("The compositor session changed".into()) } }
    fn group_snapshot(&mut self) -> Result<Value> {
        let tree = self.query(4, "")?;
        let outputs = self.query(3, "")?;
        let workspaces = self.query(1, "")?;
        let version = self.query(7, "")?;
        let result = project(&tree, &outputs, &workspaces, &version, &self.epoch)?;
        self.verify()?;
        Ok(result)
    }
}

fn project(tree: &Value, outputs: &Value, workspaces: &Value, version: &Value, epoch: &str) -> Result<Value> {
    if !flag(&version["native_window_group_selector"]) { return Err("The compositor does not support visibly scoped group selection".into()); }
    let groups = group_facts(&tree["window_groups"])?;
    let lock = &version["lumen_session_lock"];
    if object(lock).is_err() || id(&lock["version"], false)? != 1 || !flag(&lock["available"]) || !lock["locked"].is_boolean() {
        return Err("Native session lock facts are unavailable".into());
    }
    let mut result = Vec::new();
    for output in rows(outputs, 256)? {
        object(output)?;
        if !flag(&output["active"]) { continue; }
        let name = output["name"].as_str().filter(|name| !name.is_empty() && name.len() <= 256).ok_or("Invalid output identity")?;
        let visible = rows(workspaces, 4096)?.iter().filter(|workspace| workspace["output"] == name && flag(&workspace["visible"])).collect::<Vec<_>>();
        if visible.len() != 1 { continue; }
        let workspace = id(&visible[0]["id"], false)?;
        let mut items = groups["groups"].as_array().unwrap().iter().filter(|group| group["workspace"].as_u64() == Some(workspace)).collect::<Vec<_>>();
        items.sort_by_key(|group| (std::cmp::Reverse(group["recent"].as_u64().unwrap()), group["id"].as_u64().unwrap()));
        result.push(json!({"name":name,"workspace":workspace,"suspended":items.iter().any(|group| flag(&group["suspended"])),"groups":items.iter().map(|group| json!({"id":group["id"],"selected":group["selected"],"recent":group["recent"],"count":group["members"].as_array().unwrap().len()})).collect::<Vec<_>>()}));
    }
    let focused = rows(outputs, 256)?.iter().find(|output| flag(&output["active"]) && flag(&output["focused"])).and_then(|output| output["name"].as_str()).unwrap_or("");
    let presentation = flag(&version["native_window_group_presentation"]);
    let mode = if presentation { Some(window_mode(&groups["window_mode"])?) } else { None };
    Ok(json!({"available":true,"membershipSupported":flag(&version["native_window_group_membership"]),"presentationSupported":presentation,"windowMode":mode,"enabled":groups["enabled"],"locked":flag(&groups["locked"]) || flag(&lock["locked"]),"epoch":epoch,"revision":groups["revision"],"focusedOutput":focused,"outputs":result}))
}

fn mode_command(snapshot: &Value, data: &Value) -> Result<String> {
    let fields = object(data)?;
    if fields.len()!=3 || !fields.keys().all(|key| ["epoch","revision","windowMode"].contains(&key.as_str())) { return Err("Invalid captured group presentation request".into()); }
    if snapshot["available"]!=true || snapshot["presentationSupported"]!=true { return Err("This compositor does not support group window presentation".into()); }
    if snapshot["locked"]!=false { return Err("Group window presentation is unavailable while locked".into()); }
    let epoch = data["epoch"].as_str().filter(|epoch|!epoch.is_empty()).ok_or("Missing captured compositor epoch")?;
    if Some(epoch) != snapshot["epoch"].as_str() || id(&data["revision"],true)?!=id(&snapshot["revision"],true)? { return Err("The group list or compositor session changed; refresh before acting".into()); }
    let mode = window_mode(&data["windowMode"])?;
    Ok(format!("lumen_window groups {} mode {mode}",snapshot["revision"]))
}

fn captured<'a>(snapshot: &'a Value, data: &Value) -> Result<&'a Value> {
    if data["epoch"] != snapshot["epoch"] || id(&data["revision"], true)? != id(&snapshot["revision"], true)? { return Err("The group list or compositor session changed; refresh before acting".into()); }
    if !flag(&snapshot["enabled"]) || flag(&snapshot["locked"]) { return Err("Window Groups is unavailable".into()); }
    let workspace = id(&data["workspace"], false)?;
    snapshot["outputs"].as_array().and_then(|outputs| outputs.iter().find(|output| output["name"] == data["output"] && output["workspace"].as_u64() == Some(workspace))).filter(|output| !flag(&output["suspended"])).ok_or("The group is no longer on the captured visible desktop".into())
}
fn select_command(snapshot: &Value, data: &Value) -> Result<String> {
    let output = captured(snapshot, data)?;
    let gid = id(&data["id"], false)?;
    if !output["groups"].as_array().is_some_and(|groups| groups.iter().any(|group| group["id"].as_u64() == Some(gid))) { return Err("The group is no longer on the captured visible desktop".into()); }
    Ok(format!("lumen_window groups {} select-scoped {gid} {} {}", snapshot["revision"], output["workspace"], output["name"]))
}
fn membership_command(snapshot: &Value, groups: &Value, data: &Value) -> Result<String> {
    let output = captured(snapshot, data).map_err(|_| "The captured group list or session changed")?;
    let workspace = id(&data["workspace"], false)?;
    let cid = id(&data["id"], false)?;
    let pid = id(&data["pid"], false)?;
    let source = id(&data["sourceGroup"], false)?;
    let members = groups["groups"].as_array().and_then(|rows| rows.iter().find(|group| group["id"].as_u64() == Some(source) && group["workspace"].as_u64() == Some(workspace))).and_then(|group| group["members"].as_array()).ok_or("The captured source window or visible desktop changed")?;
    if !members.iter().any(|item| item["id"].as_u64() == Some(cid) && item["pid"].as_u64() == Some(pid)) { return Err("The captured source window or visible desktop changed".into()); }
    let (operation, destination) = match data["kind"].as_str() {
        Some("new") => ("remove-scoped", source),
        Some("group") => {
            let target = id(&data["groupId"], false)?;
            if target == source || !groups["groups"].as_array().is_some_and(|rows| rows.iter().any(|group| group["id"].as_u64() == Some(target) && group["workspace"].as_u64() == Some(workspace) && !flag(&group["suspended"]))) {
                return Err("Choose a different live group on the same desktop".into());
            }
            ("add-scoped", target)
        }
        _ => return Err("Choose an explicit group destination".into()),
    };
    let suffix = if operation == "add-scoped" { format!(" {source}") } else { String::new() };
    Ok(format!("lumen_window groups {} {operation} {destination} {workspace} {} {cid} {pid}{suffix}", snapshot["revision"], output["name"]))
}

fn accept(result: &Value) -> Result<()> {
    if !result.as_array().is_some_and(|rows| !rows.is_empty() && rows.iter().all(|row| flag(&row["success"]))) { return Err("The compositor rejected the captured group selection".into()); }
    Ok(())
}
fn activate(membership: bool, data: &Value) -> Result<Value> {
    let mut native = Native::open()?;
    let version = native.query(7, "")?;
    if membership && !flag(&version["native_window_group_membership"]) { return Err("The compositor cannot safely commit scoped membership intents".into()); }
    let snapshot = native.group_snapshot()?;
    let command = if membership {
        let groups = group_facts(&native.query(4, "")?["window_groups"])?;
        if groups["revision"] != snapshot["revision"] { return Err("The captured group list or session changed".into()); }
        membership_command(&snapshot, &groups, data)?
    } else { select_command(&snapshot, data)? };
    native.verify()?;
    accept(&native.query(0, &command)?)?;
    Ok(json!({"ok":true}))
}

fn apply_mode(data: &Value) -> Result<Value> {
    let mut native = Native::open()?;
    let snapshot = native.group_snapshot()?;
    let command = mode_command(&snapshot,data)?;
    native.verify()?;
    accept(&native.query(0,&command)?)?;
    native.verify()?;
    Ok(json!({"ok":true,"epoch":snapshot["epoch"],"windowMode":data["windowMode"]}))
}

fn watch_groups() -> Result<()> {
    let empty = json!({"available":false,"enabled":false,"locked":true,"outputs":[]});
    println!("{empty}");
    let result = (|| -> Result<()> {
        let mut native = Native::open()?;
        let mut events = socket(&native.path, Duration::from_millis(600))?;
        send(&mut events, 2, "[\"window\",\"workspace\",\"output\",\"tick\",\"shutdown\"]")?;
        if !flag(&receive(&mut events)?.1["success"]) { return Err("Compositor event subscription failed".into()); }
        loop {
            let value = native.group_snapshot()?;
            println!("{value}");
            match receive(&mut events) {
                Ok((0x8000_0006, _)) => return Ok(()),
                Ok(_) => (),
                Err(error) if error.contains("timed out") || error.contains("temporarily unavailable") || error.contains("Resource temporarily unavailable") => (),
                Err(error) => return Err(error),
            }
        }
    })();
    println!("{empty}");
    result
}

fn drag_state(seats: &Value, seat: &str, serial: &mut i64, active: &mut bool) -> bool {
    let Some(found) = seats.as_array().and_then(|rows| rows.iter().find(|row| row["name"] == seat)) else { return false; };
    let value = &found["lumen_drag"];
    if value["version"].as_u64() != Some(1) || !value["active"].is_boolean() { return false; }
    let Some(next) = value["serial"].as_i64().filter(|next| *next >= 0) else { return false; };
    if next > *serial { *serial = next; *active = flag(&value["active"]); }
    true
}
fn watch_drag() -> Result<()> {
    let seat = std::env::var("LUMEN_DESKTOP_SEAT").unwrap_or_else(|_| "seat0".into());
    println!("{}", json!({"active":false,"available":false}));
    let result = (|| -> Result<()> {
        let mut native = Native::open()?;
        let mut events = socket(&native.path, Duration::from_secs(5))?;
        send(&mut events, 2, "[\"tick\"]")?;
        if !flag(&receive(&mut events)?.1["success"]) { return Err("Compositor event subscription failed".into()); }
        if !flag(&native.query(7, "")?["native_file_drag_state"]) { return Err("Native drag facts unavailable".into()); }
        let mut serial = -1;
        let mut active = false;
        if !drag_state(&native.query(101, "")?, &seat, &mut serial, &mut active) { return Err("Native drag facts unavailable".into()); }
        println!("{}", json!({"active":active,"available":true}));
        events.set_read_timeout(None).map_err(|error| error.to_string())?;
        loop {
            let (kind, event) = receive(&mut events)?;
            if kind != 0x8000_0007 { continue; }
            let Some(payload) = event["payload"].as_str().and_then(|raw| serde_json::from_str::<Value>(raw).ok()) else { continue; };
            if payload["kind"] != "lumen-drag" { continue; }
            if !drag_state(&native.query(101, "")?, &seat, &mut serial, &mut active) { return Err("Native drag facts unavailable".into()); }
            println!("{}", json!({"active":active,"available":true}));
        }
    })();
    println!("{}", json!({"active":false,"available":false}));
    result
}

fn main() {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let result = match args.as_slice() {
        [mode] if mode == "watch" => watch_groups(),
        [mode] if mode == "drag" => watch_drag(),
        [mode] if mode == "select" || mode == "membership" || mode == "mode" => {
            let mut bytes = Vec::new();
            let result = std::io::stdin().lock().take((MAX + 1) as u64).read_until(b'\n', &mut bytes).map_err(|error| error.to_string()).and_then(|_| {
                if bytes.len() > MAX { return Err("Group request exceeds its declared limit".into()); }
                let data: Value = serde_json::from_slice(&bytes).map_err(|_| "Invalid group request")?;
                let response = if mode=="mode" { apply_mode(&data)? } else { activate(mode == "membership", &data)? };
                println!("{response}");
                Ok(())
            });
            result
        }
        _ => Err("Group command required".into()),
    };
    if let Err(error) = result {
        if !matches!(args.as_slice(), [mode] if mode == "watch" || mode == "drag") { println!("{}", json!({"error":error})); }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn presentation_requires_supported_fresh_unlocked_typed_capture() {
        let snapshot=serde_json::json!({"available":true,"presentationSupported":true,"locked":false,"enabled":false,"epoch":"owned-session","revision":7});
        let request=serde_json::json!({"epoch":"owned-session","revision":7,"windowMode":"one_at_a_time"});
        assert_eq!(super::mode_command(&snapshot,&request).unwrap(),"lumen_window groups 7 mode one_at_a_time");
        for (key,value) in [("epoch",serde_json::json!("replaced-session")),("epoch",serde_json::Value::Null),("revision",serde_json::json!(6)),("revision",serde_json::json!(true)),("windowMode",serde_json::json!("one_at_a_time; kill")),("windowMode",serde_json::json!(true)),("extra",serde_json::json!(true))] {
            let mut invalid=request.clone();invalid[key]=value;assert!(super::mode_command(&snapshot,&invalid).is_err(),"Accepted {invalid}");
        }
        for (key,value) in [("presentationSupported",serde_json::json!(false)),("available",serde_json::json!(false)),("locked",serde_json::json!(true)),("locked",serde_json::Value::Null)] {
            let mut invalid=snapshot.clone();invalid[key]=value;assert!(super::mode_command(&invalid,&request).is_err());
        }
        let mut all=request.clone();all["windowMode"]=serde_json::json!("all_together");
        assert_eq!(super::mode_command(&snapshot,&all).unwrap(),"lumen_window groups 7 mode all_together");
    }
    use super::*;
    #[test]
    fn selected_group_requires_exact_capture() {
        let tree = json!({"window_groups":{"version":1,"enabled":true,"locked":false,"separate_displays":true,"revision":4,"groups":[{"id":7,"workspace":3,"selected":true,"suspended":false,"recent":9,"members":[{"id":11,"pid":22,"group_hidden":false}]}]}});
        let outputs = json!([{"name":"HDMI-A-1","active":true,"focused":true}]);
        let workspaces = json!([{"id":3,"output":"HDMI-A-1","visible":true}]);
        let version = json!({"native_window_group_selector":true,"native_window_group_membership":true,"lumen_session_lock":{"version":1,"available":true,"locked":false}});
        let state = project(&tree,&outputs,&workspaces,&version,"epoch").unwrap();
        let request = json!({"epoch":"epoch","revision":4,"workspace":3,"output":"HDMI-A-1","id":7});
        assert_eq!(select_command(&state,&request).unwrap(),"lumen_window groups 4 select-scoped 7 3 \"HDMI-A-1\"");
        assert!(select_command(&state,&json!({"epoch":"stale","revision":4,"workspace":3,"output":"HDMI-A-1","id":7})).is_err());
        let mut member = request.clone(); member["id"] = json!(11);member["pid"] = json!(22);member["sourceGroup"] = json!(7);member["kind"] = json!("new");
        assert_eq!(membership_command(&state,&tree["window_groups"],&member).unwrap(),"lumen_window groups 4 remove-scoped 7 3 \"HDMI-A-1\" 11 22");
        for bad in [json!(true), json!(0), json!(-1), json!("7"), json!({})] {
            let mut changed = request.clone();changed["id"] = bad;
            assert!(select_command(&state,&changed).is_err());
        }
        let mut changed = member.clone();changed["kind"] = json!("group");changed["groupId"] = json!(7);
        assert!(membership_command(&state,&tree["window_groups"],&changed).is_err());
        assert!(!drag_state(&json!([{"name":"seat0","lumen_drag":{"version":true,"serial":1,"active":true}}]),"seat0",&mut -1,&mut false));
    }

    #[test]
    fn visible_output_recent_order_and_lock_facts() {
        let group = |group_id, workspace, recent, selected| json!({"id":group_id,"workspace":workspace,"recent":recent,"selected":selected,"suspended":false,"members":[]});
        let tree = json!({"window_groups":{"version":1,"revision":8,"enabled":true,"locked":false,"separate_displays":true,"groups":[group(1,10,1,true),group(2,10,9,false),group(3,20,30,true)]}});
        let outputs = json!([{"name":"A","active":true,"focused":true},{"name":"B","active":true}]);
        let workspaces = json!([{"id":10,"output":"A","visible":true},{"id":11,"output":"A","visible":false},{"id":20,"output":"B","visible":true}]);
        let mut version = json!({"native_window_group_selector":true,"native_window_group_membership":true,"lumen_session_lock":{"version":1,"available":true,"locked":false}});
        let projected = project(&tree,&outputs,&workspaces,&version,"epoch").unwrap();
        assert_eq!(projected["outputs"][0]["groups"][0]["id"],2);
        assert_eq!(projected["outputs"][1]["groups"][0]["id"],3);
        assert_eq!(projected["focusedOutput"],"A");
        version["lumen_session_lock"]["locked"] = json!(true);
        assert_eq!(project(&tree,&outputs,&workspaces,&version,"epoch").unwrap()["locked"],true);
        version["lumen_session_lock"]["version"] = json!(true);
        assert!(project(&tree,&outputs,&workspaces,&version,"epoch").is_err());
    }

    #[test]
    fn drag_snapshot_never_accepts_stale_or_foreign_seat() {
        let mut serial = -1;
        let mut active = false;
        assert!(!drag_state(&json!([{"name":"seat1","lumen_drag":{"version":1,"serial":3,"active":true}}]),"seat0",&mut serial,&mut active));
        assert!(drag_state(&json!([{"name":"seat0","lumen_drag":{"version":1,"serial":3,"active":false}}]),"seat0",&mut serial,&mut active));
        assert!(drag_state(&json!([{"name":"seat0","lumen_drag":{"version":1,"serial":2,"active":true}}]),"seat0",&mut serial,&mut active));
        assert_eq!((serial,active),(3,false));
        assert!(!drag_state(&json!([{"name":"seat0","lumen_drag":{"version":true,"serial":4,"active":true}}]),"seat0",&mut serial,&mut active));
        assert!(drag_state(&json!([{"name":"seat0","lumen_drag":{"version":1,"serial":4,"active":true}}]),"seat0",&mut serial,&mut active));
        assert_eq!((serial,active),(4,true));
    }
}
