//! Notification history and policy state, independent of the D-Bus transport.
use serde::Serialize;
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashSet};
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
pub mod focus;
pub mod lock;
use focus::Focus;

pub type Result<T> = std::result::Result<T, String>;
pub const BUS: &str = "org.freedesktop.Notifications";
pub const PATH: &str = "/org/freedesktop/Notifications";
pub const UI: &str = "org.lumendesktop.Notifications";
const LIMIT: usize = 100;

#[derive(Clone, Debug, Serialize)]
pub struct Action { pub id: String, pub label: String }
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Row {
    pub id: u32, pub app: String, pub desktop: String, pub summary: String, pub body: String,
    pub icon: String, pub image: String, pub actions: Vec<Action>, pub urgency: u8,
    pub resident: bool, pub transient: bool, pub timestamp: i64, pub active: bool,
    pub sender: String, pub unread: bool, pub banner: bool,
    pub delivery: String,
    pub deadline: f64, pub banner_deadline: f64,
    #[serde(skip_serializing_if="Option::is_none")]
    pub held: Option<f64>,
    #[serde(skip_serializing_if="Option::is_none")]
    pub held_by: Option<String>,
}
#[derive(Clone, Debug)]
pub struct History { pub rows: Vec<Row>, pub dnd: bool, pub next_id: u32 }
#[derive(Clone, Debug, Default)]
pub struct Effects { pub closed: Vec<(u32,u32)>, pub invoked: Vec<(u32,String)>, pub changed: bool }
#[derive(Clone, Debug)]
pub struct Notify {
    pub app: String, pub replaces_id: u32, pub icon: String, pub summary: String,
    pub body: String, pub actions: Vec<String>, pub desktop: String, pub image: String,
    pub urgency: u8, pub resident: bool, pub transient: bool, pub expire_timeout: i32,
    pub sender: String,
}
pub struct Core { pub directory: PathBuf, pub rows: Vec<Row>, pub dnd: bool, pub next_id: u32,
    pub policies: BTreeMap<String,String>, pub focus: Focus }

fn timestamp_ns() -> Result<u128> {
    Ok(SystemTime::now().duration_since(UNIX_EPOCH).map_err(|e|e.to_string())?.as_nanos())
}
fn timestamp_ms() -> Result<i64> { Ok((timestamp_ns()? / 1_000_000) as i64) }
pub fn monotonic() -> f64 {
    let mut time=libc::timespec { tv_sec:0,tv_nsec:0 };
    unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC,&mut time); }
    time.tv_sec as f64+time.tv_nsec as f64/1e9
}
fn encode_uri(path: &Path) -> String {
    use std::os::unix::ffi::OsStrExt;
    let mut uri="file://".to_owned();
    for byte in path.as_os_str().as_bytes() {
        if byte.is_ascii_alphanumeric() || b"/-._~".contains(byte) { uri.push(*byte as char); }
        else { uri.push_str(&format!("%{byte:02X}")); }
    }
    uri
}
fn decode_percent(value: &str) -> Option<String> {
    let bytes=value.as_bytes();let mut result=Vec::new();let mut i=0;
    while i<bytes.len() {
        if bytes[i]==b'%' {
            let code=std::str::from_utf8(bytes.get(i+1..i+3)?).ok()?;
            result.push(u8::from_str_radix(code,16).ok()?);i+=3;
        } else {result.push(bytes[i]);i+=1;}
    }
    String::from_utf8(result).ok()
}
pub fn local_image(value: &str) -> String {
    let decoded=if let Some(rest)=value.strip_prefix("file://") {
        let slash=rest.find('/').unwrap_or(rest.len());
        if !["","localhost"].contains(&&rest[..slash]) { return String::new(); }
        match decode_percent(&rest[slash..]) { Some(path)=>path,None=>return String::new() }
    } else { value.to_owned() };
    let path=Path::new(&decoded);
    if path.is_absolute() && path.is_file() { encode_uri(path) } else { String::new() }
}
pub fn themed_icon(value: &str) -> String {
    let local=local_image(value);
    if !local.is_empty() { return local; }
    if !value.is_empty() && value.chars().all(|c|c.is_alphanumeric() || "_.+-".contains(c)) { value.to_owned() }
    else { String::new() }
}
fn bounded_text(value: &Value, key: &str, limit: usize) -> Result<String> {
    let text=value[key].as_str().ok_or("Invalid notification text")?;
    if text.chars().count()>limit || text.contains('\u{FDD0}') { return Err("Invalid notification text".into()); }
    Ok(text.to_owned())
}
fn sanitize_lone_surrogates(raw:&[u8]) -> (Vec<u8>,bool) {
    let mut output=Vec::with_capacity(raw.len());let mut i=0;let mut quoted=false;let mut changed=false;
    while i<raw.len() {
        if raw[i]==b'"' {quoted=!quoted;output.push(raw[i]);i+=1;continue;}
        if quoted && raw[i]==b'\\' && i+1<raw.len() {
            if raw[i+1]==b'\\' {output.extend_from_slice(&raw[i..i+2]);i+=2;continue;}
            if raw[i+1]==b'u' && i+6<=raw.len() {
                let number=std::str::from_utf8(&raw[i+2..i+6]).ok().and_then(|value|u16::from_str_radix(value,16).ok());
                if let Some(number)=number {
                    if (0xD800..=0xDBFF).contains(&number) && i+12<=raw.len() && &raw[i+6..i+8]==b"\\u" {
                        let next=std::str::from_utf8(&raw[i+8..i+12]).ok().and_then(|value|u16::from_str_radix(value,16).ok());
                        if next.is_some_and(|next|(0xDC00..=0xDFFF).contains(&next)) {
                            output.extend_from_slice(&raw[i..i+12]);i+=12;continue;
                        }
                    }
                    if (0xD800..=0xDFFF).contains(&number) {
                        output.extend_from_slice(b"\\uFDD0");i+=6;changed=true;continue;
                    }
                }
            }
            output.extend_from_slice(&raw[i..i+2]);i+=2;continue;
        }
        output.push(raw[i]);i+=1;
    }
    (output,changed)
}
pub fn restored_row(value: &Value) -> Result<Row> {
    let app=bounded_text(value,"app",256)?;let desktop=bounded_text(value,"desktop",256)?;
    let summary=bounded_text(value,"summary",1024)?;let body=bounded_text(value,"body",8192)?;
    let icon=bounded_text(value,"icon",16384)?;let image=bounded_text(value,"image",16384)?;
    let id=value["id"].as_u64().filter(|v|*v>0 && *v<=u32::MAX as u64).ok_or("Invalid notification number")? as u32;
    let timestamp=value["timestamp"].as_i64().filter(|v|*v>=0 && *v<=8_640_000_000_000_000).ok_or("Invalid notification number")?;
    let urgency=value["urgency"].as_u64().filter(|v|*v<=2).ok_or("Invalid notification number")? as u8;
    let resident=value["resident"].as_bool().ok_or("Invalid notification flag")?;
    let transient=value["transient"].as_bool().ok_or("Invalid notification flag")?;
    let unread=value["unread"].as_bool().ok_or("Invalid notification flag")?;
    if transient { return Err("Transient notifications are not history".into()); }
    Ok(Row { id,app,desktop,summary,body,icon:themed_icon(&icon),image:local_image(&image),
        actions:Vec::new(),urgency,resident,transient,timestamp,active:false,sender:String::new(),
        unread,banner:false,delivery:String::new(),deadline:0.,banner_deadline:0.,held:None,held_by:None })
}
pub fn restore_history(file: &Path, initial_dnd: bool) -> Result<History> {
    let mut history=History {rows:Vec::new(),dnd:initial_dnd,next_id:1};
    let raw=match fs::read(file) { Ok(raw)=>raw,Err(e) if e.kind()==std::io::ErrorKind::NotFound=>return Ok(history),Err(e)=>return Err(e.to_string()) };
    let (safe,mut invalid)=sanitize_lone_surrogates(&raw);
    let state=match serde_json::from_slice::<Value>(&safe) {Ok(state) if state.is_object()=>state,_=>{invalid=true;json!({})}};
    if let Some(dnd)=state.get("dnd") { if let Some(value)=dnd.as_bool() { history.dnd=value; } else { invalid=true; } }
    if let Some(next)=state.get("nextId") {
        if let Some(value)=next.as_u64().filter(|v|*v>0 && *v<=u32::MAX as u64) { history.next_id=value as u32; }
        else { invalid=true; }
    }
    let rows=match state.get("rows") {Some(Value::Array(rows))=>rows.clone(),None=>Vec::new(),_=>{invalid=true;Vec::new()}};
    let mut seen=HashSet::new();
    for value in rows {
        if history.rows.len()>=LIMIT { break; }
        match restored_row(&value) {
            Ok(row) if seen.insert(row.id)=>history.rows.push(row),
            _=>invalid=true,
        }
    }
    if invalid {
        let backup=file.with_file_name(format!("history-invalid-{}.json",timestamp_ns()?));
        let images=fs::read_dir(file.parent().ok_or("Invalid history path")?).map_err(|e|e.to_string())?
            .filter_map(|item|item.ok().map(|item|item.path()))
            .filter(|path|path.file_name().is_some_and(|name|name.to_string_lossy().starts_with("image-"))
                && path.extension().is_some_and(|ext|ext=="png") && fs::symlink_metadata(path).is_ok_and(|m|m.is_file() && !m.file_type().is_symlink()))
            .collect::<Vec<_>>();
        if !images.is_empty() {
            let dir=backup.with_extension("images");fs::create_dir(&dir).map_err(|e|e.to_string())?;
            fs::set_permissions(&dir,fs::Permissions::from_mode(0o700)).map_err(|e|e.to_string())?;
            for image in images { let copied=dir.join(image.file_name().unwrap());
                fs::copy(&image,&copied).map_err(|e|e.to_string())?;
                fs::set_permissions(copied,fs::Permissions::from_mode(0o600)).map_err(|e|e.to_string())?;
            }
        }
        fs::rename(file,&backup).map_err(|e|e.to_string())?;
        fs::set_permissions(&backup,fs::Permissions::from_mode(0o600)).map_err(|e|e.to_string())?;
        eprintln!("Recovered notification history; original saved to {}",backup.display());
    }
    Ok(history)
}
impl Core {
    pub fn new(directory: PathBuf, policies: BTreeMap<String,String>, initial_dnd: bool) -> Result<Self> {
        fs::create_dir_all(&directory).map_err(|e|e.to_string())?;
        fs::set_permissions(&directory,fs::Permissions::from_mode(0o700)).map_err(|e|e.to_string())?;
        let history=restore_history(&directory.join("history.json"),initial_dnd)?;
        let focus_path=directory.join("focus.json");
        let fresh=!focus_path.exists();
        let mut focus=Focus::new(focus_path)?;
        if fresh && history.dnd { focus.command(&json!({"profile":"quiet"}))?; }
        let dnd=focus.active();
        let core=Self {directory,rows:history.rows,dnd,next_id:history.next_id,policies,focus};
        core.save()?;
        Ok(core)
    }
    pub fn state(&self) -> Value {
        let rows=self.rows.iter().map(|row|{
            let mut data=serde_json::to_value(row).unwrap_or(Value::Null);
            if let Some(value)=data.as_object_mut(){value.remove("sender");}
            data
        }).collect::<Vec<_>>();
        json!({"ready":true,"dnd":self.dnd,"focus":self.focus.state(),"policies":self.policies,"rows":rows})
    }
    pub fn save(&self) -> Result<()> {
        let file=self.directory.join("history.json");
        let rows=self.rows.iter().filter(|row|!row.transient).map(|row|json!({
            "id":row.id,"app":row.app,"desktop":row.desktop,"summary":row.summary,"body":row.body,
            "icon":row.icon,"image":row.image,"urgency":row.urgency,"resident":row.resident,
            "transient":row.transient,"timestamp":row.timestamp,"unread":row.unread
        })).collect::<Vec<_>>();
        let contents=json!({"version":1,"nextId":self.next_id,"dnd":self.dnd,"rows":rows}).to_string();
        let temp=self.directory.join(format!("history-{}.tmp",timestamp_ns()?));
        let mut output=OpenOptions::new().write(true).create_new(true).mode(0o600).open(&temp).map_err(|e|e.to_string())?;
        output.write_all(contents.as_bytes()).map_err(|e|e.to_string())?;
        output.sync_all().map_err(|e|e.to_string())?;
        fs::rename(&temp,&file).map_err(|e|e.to_string())?;
        let directory=File::open(&self.directory).map_err(|e|e.to_string())?;
        directory.sync_all().map_err(|e|e.to_string())?;
        let used=self.rows.iter().map(|row|row.image.as_str()).collect::<HashSet<_>>();
        for entry in fs::read_dir(&self.directory).map_err(|e|e.to_string())? {
            let entry=entry.map_err(|e|e.to_string())?;let path=entry.path();
            if path.file_name().is_some_and(|name|name.to_string_lossy().starts_with("image-")) && path.extension().is_some_and(|ext|ext=="png")
                && !used.contains(encode_uri(&path).as_str()) { let _=fs::remove_file(path); }
        }
        Ok(())
    }
    fn close_index(&mut self, index: usize, reason: u32, keep: bool, effects: &mut Effects) {
        let row=&mut self.rows[index];
        let active=row.active;let id=row.id;let transient=row.transient;
        row.active=false;row.actions.clear();row.banner=false;row.deadline=0.;row.banner_deadline=0.;row.held=None;row.held_by=None;
        if !keep || transient { self.rows.remove(index); }
        if active { effects.closed.push((id,reason)); }
        effects.changed=true;
    }
    pub fn notify(&mut self, request: Notify) -> Result<(u32,Effects)> {
        let mut effects=Effects::default();
        let existing=if request.replaces_id>0 {self.rows.iter().position(|row|row.id==request.replaces_id && row.active && row.sender==request.sender)}else{None};
        let id=if let Some(index)=existing {self.rows.remove(index).id} else {
            let mut candidate=self.next_id;
            while self.rows.iter().any(|row|row.id==candidate) {candidate=if candidate==u32::MAX {1}else{candidate+1};}
            self.next_id=if candidate==u32::MAX {1}else{candidate+1};candidate
        };
        let now=monotonic();
        let duration=request.expire_timeout;
        let banner_duration=if duration==0 || request.urgency==2 {0.}else if duration>0 {duration as f64/1000.}else{7.};
        let deadline=if duration>0 {now+duration as f64/1000.}else if request.transient && duration<0 {now+7.}else{0.};
        let app=request.app.chars().take(256).collect::<String>();
        let desktop=request.desktop.chars().take(256).collect::<String>();
        let app=if app.is_empty(){if desktop.is_empty(){"Notification".into()}else{desktop.clone()}}else{app};
        let actions=request.actions.chunks_exact(2).take(16).map(|pair|Action {
            id:pair[0].chars().take(256).collect(),label:pair[1].chars().take(256).collect()
        }).collect();
        let allowed=self.focus.allows(if desktop.is_empty(){&app}else{&desktop},request.urgency);
        let row=Row{id,app,desktop,summary:request.summary.chars().take(1024).collect(),
            body:request.body.chars().take(8192).collect(),icon:themed_icon(&request.icon.chars().take(2048).collect::<String>()),
            image:local_image(&request.image),actions,urgency:request.urgency.min(2),resident:request.resident,
            transient:request.transient,timestamp:timestamp_ms()?,active:true,sender:request.sender,
            unread:true,banner:allowed,delivery:timestamp_ns()?.to_string(),deadline,banner_deadline:if banner_duration>0. {now+banner_duration}else{0.},
            held:None,held_by:None};
        let group=if row.desktop.is_empty(){&row.app}else{&row.desktop};
        let policy=self.policies.get(group).map(String::as_str).unwrap_or("banners");
        if policy=="off" { effects.closed.push((id,2));effects.changed=true;self.save()?;return Ok((id,effects)); }
        let mut row=row;
        if policy!="banners" {row.banner=false;}
        self.rows.insert(0,row);
        while self.rows.len()>LIMIT {self.close_index(LIMIT,2,false,&mut effects);}
        let mut banners=0;
        for row in &mut self.rows {if row.banner {banners+=1;if banners>3{row.banner=false;}}}
        effects.changed=true;
        self.save()?;
        Ok((id,effects))
    }
    pub fn close_notification(&mut self, id:u32, sender:&str) -> Result<Effects> {
        let mut effects=Effects::default();
        if let Some(index)=self.rows.iter().position(|row|row.id==id && row.active && row.sender==sender) {
            self.close_index(index,3,false,&mut effects);self.save()?;
        }
        Ok(effects)
    }
    pub fn tick(&mut self) -> Result<Effects> {
        let now=monotonic();let mut effects=Effects::default();
        let previous=self.dnd;
        let expired=self.focus.expire()?;
        self.dnd=self.focus.active();
        if expired || previous!=self.dnd {
            for row in &mut self.rows {
                if !self.focus.allows(if row.desktop.is_empty(){&row.app}else{&row.desktop},row.urgency) { row.banner=false; }
            }
            effects.changed=true;
        }
        let mut index=0;
        while index<self.rows.len() {
            if self.rows[index].deadline>0. && now>=self.rows[index].deadline {
                let retained=!self.rows[index].transient;
                self.close_index(index,1,retained,&mut effects);
                if !retained {continue;}
            } else if self.rows[index].banner_deadline>0. && now>=self.rows[index].banner_deadline {
                self.rows[index].banner=false;self.rows[index].banner_deadline=0.;effects.changed=true;
            }
            index+=1;
        }
        if effects.changed {self.save()?;}
        Ok(effects)
    }
    pub fn set_policy(&mut self, app:&str, policy:&str) -> Result<Effects> {
        if app.is_empty() || app.chars().count()>255 || !["banners","silent","off"].contains(&policy) {
            return Err("Invalid notification policy".into());
        }
        self.policies.insert(app.to_owned(),policy.to_owned());
        if policy!="banners" {for row in &mut self.rows {
            if (if row.desktop.is_empty(){&row.app}else{&row.desktop})==app {row.banner=false;}
        }}
        self.save()?;
        Ok(Effects{changed:true,..Effects::default()})
    }
    pub fn owner_changed(&mut self, name:&str) -> Effects {
        let mut effects=Effects::default();let now=monotonic();
        for row in &mut self.rows {
            if row.held_by.as_deref()==Some(name) {
                if let Some(held)=row.held.take() {row.banner_deadline=now+held;row.held_by=None;effects.changed=true;}
            }
            if row.sender==name && !row.actions.is_empty() {row.actions.clear();effects.changed=true;}
        }
        effects
    }
    pub fn command(&mut self, data:&Value, sender:&str, owner_alive:impl Fn(&str)->bool) -> Result<Effects> {
        let action=data["action"].as_str().ok_or("Missing notification action")?;
        let id=data["id"].as_u64().filter(|id|*id<=u32::MAX as u64).unwrap_or(0) as u32;
        let mut effects=Effects::default();
        if action=="hold" {
            if let Some(row)=self.rows.iter_mut().find(|row|row.id==id) {
                if data["enabled"]==true && row.banner_deadline>0. {
                    row.held_by=Some(sender.into());row.held=Some((row.banner_deadline-monotonic()).max(1.));row.banner_deadline=0.;
                } else if data["enabled"]==false {
                    if let Some(held)=row.held.take(){row.banner_deadline=monotonic()+held;row.held_by=None;}
                }
            }
            return Ok(effects);
        }
        match action {
            "dnd" | "focus" => {
                if action=="dnd" {
                    let enabled=data["enabled"].as_bool().unwrap_or(!self.dnd);
                    self.focus.command(&json!({"profile":if enabled {"quiet"} else {""}}))?;
                } else { self.focus.command(data)?; }
                self.dnd=self.focus.active();
                for row in &mut self.rows {
                    if !self.focus.allows(if row.desktop.is_empty(){&row.app}else{&row.desktop},row.urgency) { row.banner=false; }
                }
            }
            "seen" => {for row in &mut self.rows {row.unread=false;row.banner=false;}}
            "dismiss" => {
                if let Some(index)=self.rows.iter().position(|row|row.id==id) {self.close_index(index,2,false,&mut effects);}
            }
            "clear" => {
                let group=data["group"].as_str();let mut index=0;
                while index<self.rows.len() {
                    let row=&self.rows[index];let key=if row.desktop.is_empty(){&row.app}else{&row.desktop};
                    if group.is_none_or(|group|key==group) {self.close_index(index,2,false,&mut effects);}else{index+=1;}
                }
            }
            "hide" => {if let Some(row)=self.rows.iter_mut().find(|row|row.id==id){row.banner=false;}}
            "invoke" => {
                if let Some(index)=self.rows.iter().position(|row|row.id==id && row.active) {
                    if data.get("delivery").is_some_and(|value| value != &self.rows[index].delivery) { return Ok(effects); }
                    let key=data["key"].as_str().unwrap_or("default");
                    let row=&self.rows[index];
                    if row.actions.iter().any(|action|action.id==key) && owner_alive(&row.sender) {
                        effects.invoked.push((id,key.to_owned()));
                        if !row.resident {self.close_index(index,2,false,&mut effects);}
                    }
                }
            }
            "policy" => return Err("Policy update requires settings transaction".into()),
            _ => (),
        }
        effects.changed=true;
        self.save()?;
        Ok(effects)
    }
}
#[cfg(test)] mod tests {
    use super::*;
    fn fixture(label:&str) -> PathBuf {
        let path=std::env::temp_dir().join(format!("notification-{label}-{}",timestamp_ns().unwrap()));
        fs::create_dir(&path).unwrap();path
    }
    fn row(id:u32) -> Value { json!({"id":id,"app":"Fixture","desktop":"fixture.desktop",
        "summary":"Saved notification","body":"Retained history","icon":"dialog-information","image":"",
        "urgency":1,"resident":false,"transient":false,"timestamp":1791000000000_i64,"unread":true}) }
    fn request() -> Notify { Notify {app:"Fixture".into(),replaces_id:0,icon:String::new(),summary:"Example".into(),
        body:"Body".into(),actions:Vec::new(),desktop:String::new(),image:String::new(),urgency:1,
        resident:false,transient:false,expire_timeout:-1,sender:":1.22".into()} }
    #[test] fn restore_keeps_valid_rows_and_strips_process_state() {
        let root=fixture("valid");let file=root.join("history.json");
        let mut saved=row(7);saved["active"]=json!(true);saved["sender"]=json!(":1.22");
        saved["actions"]=json!({"bad":["shape"]});saved["heldBy"]=json!(["bad"]);
        fs::write(&file,json!({"rows":[saved],"dnd":true,"nextId":8}).to_string()).unwrap();
        let restored=restore_history(&file,false).unwrap();
        assert_eq!(restored.rows.len(),1);assert!(restored.dnd);assert_eq!(restored.next_id,8);
        assert!(!restored.rows[0].active);assert!(restored.rows[0].actions.is_empty());
        assert!(restored.rows[0].sender.is_empty());assert!(restored.rows[0].held_by.is_none());
        fs::remove_dir_all(root).unwrap();
    }
    #[test] fn invalid_rows_preserve_neighbours_and_image_backup() {
        let root=fixture("bad-row");let file=root.join("history.json");
        let image=root.join("image-8.png");fs::write(&image,b"attachment fixture").unwrap();
        let raw=json!({"rows":[Value::Null,row(7),json!({"id":1}),row(8),row(7)],"dnd":true,"nextId":8}).to_string();
        fs::write(&file,&raw).unwrap();
        let core=Core::new(root.clone(),BTreeMap::new(),false).unwrap();
        assert_eq!(core.rows.iter().map(|r|r.id).collect::<Vec<_>>(),vec![7,8]);
        let backup=fs::read_dir(&root).unwrap().filter_map(|e|e.ok().map(|e|e.path()))
            .find(|p|p.extension().is_some_and(|ext|ext=="json") && p!=&file).unwrap();
        assert_eq!(fs::read(&backup).unwrap(),raw.as_bytes());
        assert_eq!(fs::metadata(&backup).unwrap().permissions().mode()&0o777,0o600);
        assert_eq!(fs::read(backup.with_extension("images").join("image-8.png")).unwrap(),b"attachment fixture");
        assert!(!image.exists());
        assert_eq!(serde_json::from_slice::<Value>(&fs::read(&file).unwrap()).unwrap()["rows"].as_array().unwrap().len(),2);
        fs::remove_dir_all(root).unwrap();
    }
    #[test] fn malformed_json_is_quarantined() {
        let root=fixture("nested");let file=root.join("history.json");
        let mut raw=b"{\"rows\":".to_vec();raw.extend(vec![b'[';20000]);raw.push(b'0');raw.extend(vec![b']';20000]);raw.push(b'}');
        fs::write(&file,&raw).unwrap();
        let restored=restore_history(&file,false).unwrap();
        assert!(restored.rows.is_empty());assert!(!restored.dnd);assert_eq!(restored.next_id,1);
        assert!(!file.exists());
        fs::remove_dir_all(root).unwrap();
    }
    #[test] fn policies_control_banners_without_moving_dnd_into_settings() {
        let root=fixture("policy");let mut core=Core::new(root.clone(),BTreeMap::new(),false).unwrap();
        core.notify(request()).unwrap();assert!(core.rows[0].banner);
        core.set_policy("Fixture","silent").unwrap();
        core.notify(request()).unwrap();assert_eq!(core.rows.len(),2);assert!(!core.rows[0].banner);
        core.set_policy("Fixture","off").unwrap();
        let (_,effects)=core.notify(request()).unwrap();assert_eq!(core.rows.len(),2);assert_eq!(effects.closed.len(),1);
        core.command(&json!({"action":"dnd","enabled":true}),":1.99",|_|true).unwrap();
        assert!(core.dnd);assert!(core.state()["policies"]["Fixture"]=="off");
        let stored:Value=serde_json::from_slice(&fs::read(root.join("history.json")).unwrap()).unwrap();
        assert_eq!(stored["dnd"],true);
        fs::remove_dir_all(root).unwrap();
    }
    #[test] fn lone_surrogate_row_does_not_discard_valid_neighbours() {
        let root=fixture("surrogate");let file=root.join("history.json");
        let bad=row(8).to_string().replace("Retained history","\\uD800");
        let raw=format!("{{\"rows\":[{},{},{}]}}",row(7),bad,row(9));
        fs::write(&file,&raw).unwrap();
        let restored=restore_history(&file,false).unwrap();
        assert_eq!(restored.rows.iter().map(|row|row.id).collect::<Vec<_>>(),vec![7,9]);
        assert!(fs::read_dir(&root).unwrap().any(|entry|entry.unwrap().file_name().to_string_lossy().starts_with("history-invalid-")));
        fs::remove_dir_all(root).unwrap();
    }
    #[test] fn remote_images_are_never_restored() {
        let mut saved=row(7);
        saved["image"]=json!("https://invalid.test/x.png");
        saved["icon"]=json!("https://invalid.test/icon");
        let restored=restored_row(&saved).unwrap();
        assert!(restored.image.is_empty() && restored.icon.is_empty());
    }
    #[test] fn focus_exceptions_keep_other_banners_suppressed() {
        let root=fixture("focus");let mut core=Core::new(root.clone(),BTreeMap::new(),false).unwrap();
        core.command(&json!({"action":"focus","operation":"profile","profile":{"id":"work","name":"Work",
            "allowedApps":["calendar"],"allowCritical":false,"schedules":[]}}),":1.1",|_|true).unwrap();
        core.command(&json!({"action":"focus","profile":"work"}),":1.1",|_|true).unwrap();
        let mut blocked=request();blocked.app="chat".into();core.notify(blocked).unwrap();
        assert!(!core.rows[0].banner);
        let mut allowed=request();allowed.app="calendar".into();core.notify(allowed).unwrap();
        assert!(core.rows[0].banner);
        core.command(&json!({"action":"focus","profile":""}),":1.1",|_|true).unwrap();
        assert!(!core.rows[1].banner);
        fs::remove_dir_all(root).unwrap();
    }
}
