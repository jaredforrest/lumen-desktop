use lumen_notifications::{BUS, PATH, UI, Core, Effects, Notify, Result, monotonic};
use lumen_notifications::lock::{LockAuthority, previews_allowed};
use futures_util::StreamExt;
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashMap};
use std::env;
use std::fs::OpenOptions;
use std::io::Read;
use std::os::unix::fs::OpenOptionsExt;
use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::sync::Mutex;
use zbus::{Connection, interface, message::Header, zvariant::{OwnedValue, Structure, Value as DbusValue}};

struct ServiceState { core:Core, lock:LockAuthority, signaled_lock:Value, instance:String, revision:u64,
    preview:String, host_locked:bool, preferences_checked:f64, focus_published:Value }
impl ServiceState {
    fn new(core:Core, settings:&Value) -> Result<Self> {
        let instance=std::fs::read_to_string("/proc/sys/kernel/random/uuid").map_err(|e|e.to_string())?
            .trim().replace('-', "");
        let preview=settings["notifications"]["preview"].as_str().unwrap_or("when_unlocked").to_owned();
        let focus_published=core.focus.state();
        let lock=LockAuthority::new();
        let signaled_lock=lock.facts().clone();
        Ok(Self {core,lock,signaled_lock,instance,revision:0,preview,
            host_locked:env::var("LUMEN_LOCK_PREVIEWS").as_deref()==Ok("1"),preferences_checked:0.0,
            focus_published})
    }
    fn refresh_preferences(&mut self) -> bool {
        let mut changed=false;
        match read_settings() {
            Ok(settings) => {
                let preview=settings["notifications"]["preview"].as_str().unwrap_or("when_unlocked");
                let preview=if matches!(preview,"always" | "when_unlocked" | "never") {preview}else{"never"};
                if self.preview!=preview {self.preview=preview.to_owned();changed=true;}
                if let Ok(next)=policies(&settings) {
                    if next!=self.core.policies {
                        self.core.policies=next;
                        for row in &mut self.core.rows {
                            let app=if row.desktop.is_empty(){&row.app}else{&row.desktop};
                            if self.core.policies.get(app).map(String::as_str).unwrap_or("banners")!="banners" {row.banner=false;}
                        }
                        changed=true;
                    }
                }
            }
            Err(_) => {if self.preview!="never" {self.preview="never".into();changed=true;}}
        }
        changed
    }
    fn snapshot(&mut self) -> Value {
        self.refresh_preferences();
        let lock=self.lock.snapshot();
        let allowed=previews_allowed(&self.preview,&lock,self.host_locked);
        let mut state=self.core.state();
        if !allowed {
            if let Some(rows)=state["rows"].as_array_mut() {
                for row in rows { row["summary"]=json!("Notification");row["body"]=json!("");
                    row["image"]=json!("");row["icon"]=json!("");row["actions"]=json!([]); }
            }
        }
        self.revision+=1;
        state["instance"]=json!(self.instance);
        state["revision"]=json!(self.revision);
        state["commandUntil"]=json!(boot_time_ms()+1000);
        state["lock"]=lock;
        state["previewsVisible"]=json!(allowed);
        state["preview"]=json!(self.preview);
        state
    }
    fn tick(&mut self) -> Result<Effects> {
        self.lock.refresh();
        let current_lock=self.lock.facts();
        let lock_changed=current_lock!=self.signaled_lock;
        if lock_changed {self.signaled_lock=current_lock;}
        let mut effects=self.core.tick()?;
        let mut changed=lock_changed;
        let focus=self.core.focus.state();
        if focus!=self.focus_published {
            self.focus_published=focus;
            self.core.dnd=self.core.focus.active();
            for row in &mut self.core.rows {
                if !self.core.focus.allows(if row.desktop.is_empty(){&row.app}else{&row.desktop},row.urgency) {row.banner=false;}
            }
            changed=true;
        }
        if monotonic()-self.preferences_checked>=1.0 {
            self.preferences_checked=monotonic();
            changed|=self.refresh_preferences();
        }
        if changed {effects.changed=true;}
        Ok(effects)
    }
}
fn boot_time_ms() -> u64 {
    let mut stamp=libc::timespec{tv_sec:0,tv_nsec:0};
    unsafe {libc::clock_gettime(libc::CLOCK_BOOTTIME,&mut stamp);}
    stamp.tv_sec as u64*1000+stamp.tv_nsec as u64/1_000_000
}
fn authority(data:&Value,instance:&str) -> Result<()> {
    if data.get("instance").is_some() || data.get("expires").is_some() {
        let until=data["expires"].as_u64().ok_or("Expired notification command authority")?;
        let now=boot_time_ms();
        if data["instance"]!=instance || until==0 || until>9_007_199_254_740_991 || until<=now || until>now+1000 {
            return Err("Expired notification command authority".into());
        }
    }
    let action=data["action"].as_str().ok_or("Unknown notification command")?;
    if !["hold","dnd","focus","policy","seen","dismiss","clear","hide","invoke"].contains(&action) {
        return Err("Unknown notification command".into());
    }
    if data.get("enabled").is_some_and(|value| !value.is_boolean()) {
        return Err("Notification enabled state must be boolean".into());
    }
    if data.get("id").is_some_and(|value| value.as_u64().is_none_or(|id| id>u32::MAX as u64)) {
        return Err("Invalid notification identity".into());
    }
    Ok(())
}
#[derive(Clone)] struct Notifications { state:Arc<Mutex<ServiceState>> }
#[derive(Clone)] struct Ui { state:Arc<Mutex<ServiceState>> }
fn error(message:impl ToString) -> zbus::fdo::Error { zbus::fdo::Error::Failed(message.to_string()) }
fn sender(header:&Header<'_>) -> String { header.sender().map(|name|name.to_string()).unwrap_or_default() }
fn hint_string(hints:&HashMap<String,OwnedValue>, key:&str) -> String {
    hints.get(key).and_then(|value|<&str>::try_from(value).ok()).unwrap_or("").to_owned()
}
fn hint_bool(hints:&HashMap<String,OwnedValue>, key:&str) -> bool {
    hints.get(key).and_then(|value|bool::try_from(value).ok()).unwrap_or(false)
}
fn hint_urgency(hints:&HashMap<String,OwnedValue>) -> u8 {
    hints.get("urgency").and_then(|value|u8::try_from(value).ok()).unwrap_or(1).min(2)
}
fn inline_image(hints:&HashMap<String,OwnedValue>, directory:&std::path::Path, id:u32) -> Option<String> {
    let raw=hints.get("image-data").or_else(||hints.get("image_data")).or_else(||hints.get("icon_data"))?;
    let structure=Structure::try_from(raw.try_clone().ok()?).ok()?;
    let fields=structure.fields();if fields.len()!=7 {return None;}
    let width=i32::try_from(&fields[0]).ok()?;
    let height=i32::try_from(&fields[1]).ok()?;
    let stride=i32::try_from(&fields[2]).ok()?;
    let alpha=bool::try_from(&fields[3]).ok()?;
    let bits=i32::try_from(&fields[4]).ok()?;
    let channels=i32::try_from(&fields[5]).ok()?;
    if !(1..=2048).contains(&width) || !(1..=2048).contains(&height) || bits!=8
        || channels!=if alpha{4}else{3} || stride<width*channels || stride>16384 {return None;}
    let DbusValue::Array(array)=&fields[6] else{return None;};
    let bytes=array.inner().iter().map(u8::try_from).collect::<std::result::Result<Vec<_>,_>>().ok()?;
    let required=(stride as usize).checked_mul(height as usize-1)?.checked_add(width as usize*channels as usize)?;
    if bytes.len()<required || bytes.len()>16*1024*1024 {return None;}
    let mut packed=Vec::with_capacity(width as usize*height as usize*channels as usize);
    for line in 0..height as usize {
        let begin=line*stride as usize;let end=begin+width as usize*channels as usize;
        packed.extend_from_slice(bytes.get(begin..end)?);
    }
    let stamp=std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).ok()?.as_nanos();
    let path=directory.join(format!("image-{id}-{stamp}.png"));
    let file=OpenOptions::new().write(true).create_new(true).mode(0o600).open(&path).ok()?;
    let mut encoder=png::Encoder::new(file,width as u32,height as u32);
    encoder.set_color(if alpha{png::ColorType::Rgba}else{png::ColorType::Rgb});
    encoder.set_depth(png::BitDepth::Eight);
    let result=encoder.write_header().and_then(|mut writer|writer.write_image_data(&packed));
    if result.is_err(){let _=std::fs::remove_file(path);return None;}
    lumen_notifications::local_image(path.to_str()?) .into()
}
async fn emit(conn:&Connection, effects:Effects, state:Option<String>) -> zbus::Result<()> {
    for (id,reason) in effects.closed {
        conn.emit_signal(None::<&str>,PATH,BUS,"NotificationClosed",&(id,reason)).await?;
    }
    for (id,action) in effects.invoked {
        conn.emit_signal(None::<&str>,PATH,BUS,"ActionInvoked",&(id,action)).await?;
    }
    if effects.changed {
        conn.emit_signal(None::<&str>,PATH,UI,"Changed",&(state.unwrap_or_default(),)).await?;
    }
    Ok(())
}
fn read_settings() -> Result<Value> {
    let binary=env::var("LUMEN_NIX_SETTINGS_BIN").unwrap_or_else(|_|"lumen-nix-settings".into());
    let output=Command::new(binary).arg("read").output().map_err(|e|e.to_string())?;
    if !output.status.success() { return Err(String::from_utf8_lossy(&output.stderr).trim().to_owned()); }
    serde_json::from_slice(&output.stdout).map_err(|e|e.to_string())
}
fn set_settings(value:Value) -> Result<Value> {
    let binary=env::var("LUMEN_NIX_SETTINGS_BIN").unwrap_or_else(|_|"lumen-nix-settings".into());
    let output=Command::new(binary).args(["set","notifications",&value.to_string()]).output().map_err(|e|e.to_string())?;
    if !output.status.success() { return Err(String::from_utf8_lossy(&output.stderr).trim().to_owned()); }
    serde_json::from_slice(&output.stdout).map_err(|e|e.to_string())
}
fn policies(settings:&Value) -> Result<BTreeMap<String,String>> {
    settings["notifications"]["policies"].as_object().ok_or("Missing notification policies")?
        .iter().map(|(app,policy)|Ok((app.clone(),policy.as_str().ok_or("Invalid policy")?.to_owned()))).collect()
}
#[interface(name="org.freedesktop.Notifications")]
impl Notifications {
    fn get_capabilities(&self) -> Vec<&'static str> { vec!["body","actions","icon-static","persistence"] }
    fn get_server_information(&self) -> (&'static str,&'static str,&'static str,&'static str) {
        ("Lumen Desktop Notifications","lumen-desktop","1.0","1.2")
    }
    async fn notify(&self, app_name:String, replaces_id:u32, app_icon:String, summary:String, body:String,
        actions:Vec<String>, hints:HashMap<String,OwnedValue>, expire_timeout:i32,
        #[zbus(header)] header:Header<'_>, #[zbus(connection)] conn:&Connection) -> zbus::fdo::Result<u32> {
        let image={let state=self.state.lock().await;
            inline_image(&hints,&state.core.directory,replaces_id.max(state.core.next_id))};
        let image=image.unwrap_or_else(||{
            let value=hint_string(&hints,"image-path");
            if value.is_empty(){hint_string(&hints,"image_path")}else{value}
        });
        let request=Notify {app:app_name,replaces_id,icon:app_icon,summary,body,actions,
            desktop:hint_string(&hints,"desktop-entry"),image,urgency:hint_urgency(&hints),
            resident:hint_bool(&hints,"resident"),transient:hint_bool(&hints,"transient"),
            expire_timeout,sender:sender(&header)};
        let (id,effects,snapshot)={let mut state=self.state.lock().await;
            let (id,effects)=state.core.notify(request).map_err(error)?;
            let snapshot=if effects.changed{Some(state.snapshot().to_string())}else{None};(id,effects,snapshot)};
        emit(conn,effects,snapshot).await.map_err(error)?;
        Ok(id)
    }
    async fn close_notification(&self, id:u32, #[zbus(header)] header:Header<'_>,
        #[zbus(connection)] conn:&Connection) -> zbus::fdo::Result<()> {
        let (effects,snapshot)={let mut state=self.state.lock().await;
            let effects=state.core.close_notification(id,&sender(&header)).map_err(error)?;
            let snapshot=if effects.changed{Some(state.snapshot().to_string())}else{None};(effects,snapshot)};
        emit(conn,effects,snapshot).await.map_err(error)
    }
}
#[interface(name="org.lumendesktop.Notifications")]
impl Ui {
    async fn probe(&self) -> String {
        let state=self.state.lock().await;
        json!({"instance":state.instance,"commandUntil":boot_time_ms()+1000}).to_string()
    }
    async fn snapshot(&self) -> String { self.state.lock().await.snapshot().to_string() }
    async fn command(&self, encoded:String, #[zbus(header)] header:Header<'_>,
        #[zbus(connection)] conn:&Connection) -> zbus::fdo::Result<()> {
        let data:Value=serde_json::from_str(&encoded).map_err(error)?;
        {
            let state=self.state.lock().await;
            authority(&data,&state.instance).map_err(error)?;
        }
        if data["action"]=="policy" {
            let app=data["app"].as_str().ok_or_else(||error("Invalid notification policy"))?.to_owned();
            let policy=data["policy"].as_str().ok_or_else(||error("Invalid notification policy"))?.to_owned();
            if app.is_empty() || app.chars().count()>255 || !["banners","silent","off"].contains(&policy.as_str()) {
                return Err(error("Invalid notification policy"));
            }
            let service=self.state.clone();let conn=conn.clone();
            let applied_app=app.clone();let applied_policy=policy.clone();
            tokio::spawn(async move {
                let result=tokio::task::spawn_blocking(move || -> Result<Value> {
                    let mut settings=read_settings()?;
                    settings["notifications"]["policies"][&app]=json!(policy);
                    set_settings(settings["notifications"].clone())
                }).await;
                match result {
                    Ok(Ok(settings)) => {
                        if let Ok(next)=policies(&settings) {
                            let (effects,snapshot)={let mut state=service.lock().await;
                                state.core.policies=next;
                                let effects=match state.core.set_policy(&applied_app,&applied_policy) {
                                    Ok(effects)=>effects,
                                    Err(error)=>{eprintln!("Notification policy was not applied: {error}");return;}
                                };
                                let snapshot=Some(state.snapshot().to_string());(effects,snapshot)};
                            let _=emit(&conn,effects,snapshot).await;
                        }
                    }
                    other=>eprintln!("Notification policy was not applied: {other:?}"),
                }
            });
            return Ok(());
        }
        let alive=if data["action"]=="invoke" {
            let owner={let mut state=self.state.lock().await;
                let facts=state.lock.snapshot();
                if !previews_allowed(&state.preview,&facts,state.host_locked) {return Ok(());}
                state.core.rows.iter().find(|row|Some(row.id as u64)==data["id"].as_u64()).map(|row|row.sender.clone())};
            if let Some(owner)=owner {
                let bus=zbus::Proxy::new(conn,"org.freedesktop.DBus","/org/freedesktop/DBus","org.freedesktop.DBus")
                    .await.map_err(error)?;
                bus.call::<_,_,bool>("NameHasOwner",&(owner.as_str(),)).await.unwrap_or(false)
            } else {false}
        } else {false};
        let (effects,snapshot)={let mut state=self.state.lock().await;
            let effects=state.core.command(&data,&sender(&header),|_|alive).map_err(error)?;
            let snapshot=if effects.changed{Some(state.snapshot().to_string())}else{None};(effects,snapshot)};
        emit(conn,effects,snapshot).await.map_err(error)
    }
}
async fn serve() -> Result<()> {
    unsafe { libc::umask(0o077); }
    let state=env::var_os("LUMEN_NOTIFICATION_STATE").map(PathBuf::from).unwrap_or_else(||
        env::var_os("XDG_STATE_HOME").map(PathBuf::from).unwrap_or_else(||PathBuf::from(env::var_os("HOME").unwrap_or_default()).join(".local/state"))
            .join("lumen-desktop/notifications"));
    let settings=read_settings()?;
    let core=Core::new(state,policies(&settings)?,env::var("LUMEN_INITIAL_DND").as_deref()==Ok("1"))?;
    let state=Arc::new(Mutex::new(ServiceState::new(core,&settings)?));
    let conn=zbus::connection::Builder::session().map_err(|e|e.to_string())?
        .name(BUS).map_err(|e|e.to_string())?
        .serve_at(PATH,Notifications{state:state.clone()}).map_err(|e|e.to_string())?
        .serve_at(PATH,Ui{state:state.clone()}).map_err(|e|e.to_string())?
        .build().await.map_err(|e|e.to_string())?;
    let bus=zbus::Proxy::new(&conn,"org.freedesktop.DBus","/org/freedesktop/DBus","org.freedesktop.DBus")
        .await.map_err(|e|e.to_string())?;
    let mut owners=bus.receive_signal("NameOwnerChanged").await.map_err(|e|e.to_string())?;
    let owner_state=state.clone();let owner_conn=conn.clone();
    tokio::spawn(async move {while let Some(message)=owners.next().await {
        if let Ok((name,old,new))=message.body().deserialize::<(String,String,String)>() {
            if name.starts_with(':') && !old.is_empty() && new.is_empty() {
                let (effects,snapshot)={let mut state=owner_state.lock().await;
                    let effects=state.core.owner_changed(&name);
                    let snapshot=if effects.changed{Some(state.snapshot().to_string())}else{None};(effects,snapshot)};
                let _=emit(&owner_conn,effects,snapshot).await;
            }
        }
    }});
    let mut timer=tokio::time::interval(std::time::Duration::from_millis(250));
    loop {
        timer.tick().await;
        let (effects,snapshot)={let mut state=state.lock().await;
            let effects=state.tick()?;
            let snapshot=if effects.changed{Some(state.snapshot().to_string())}else{None};(effects,snapshot)};
        emit(&conn,effects,snapshot).await.map_err(|e|e.to_string())?;
    }
}
fn transport_snapshot(raw:&str,instance:&mut Option<String>,revision:&mut u64,generation:&str) -> Option<String> {
    let mut value:Value=serde_json::from_str(raw).ok()?;
    let next=value["instance"].as_str()?;
    if next.len()!=32 || !next.bytes().all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase()) {
        return None;
    }
    let number=value["revision"].as_u64().filter(|number| *number>0 && *number<=9_007_199_254_740_991)?;
    if instance.as_ref().is_some_and(|previous| previous!=next) || number<=*revision { return None; }
    *instance=Some(next.to_owned());*revision=number;
    value["transport"]=json!(generation);
    Some(value.to_string())
}
async fn client(mode:&str,args:&[String]) -> Result<()> {
    let conn=Connection::session().await.map_err(|e|e.to_string())?;
    let bus=zbus::Proxy::new(&conn,"org.freedesktop.DBus","/org/freedesktop/DBus","org.freedesktop.DBus")
        .await.map_err(|e|e.to_string())?;
    let owner:String=bus.call("GetNameOwner",&(BUS,)).await.map_err(|e|e.to_string())?;
    let proxy=zbus::Proxy::new(&conn,owner.as_str(),PATH,UI).await.map_err(|e|e.to_string())?;
    match mode {
        "dnd" => proxy.call::<_,_,()>("Command",&(json!({"action":"dnd"}).to_string(),)).await.map_err(|e|e.to_string()),
        "status" | "mode" => {
            let state:String=proxy.call("Snapshot",&()).await.map_err(|e|e.to_string())?;
            if mode=="status" {println!("{state}");}else{
                let value:Value=serde_json::from_str(&state).map_err(|e|e.to_string())?;
                println!("{}",if value["dnd"]==true{"do-not-disturb"}else{"default"});
            }
            Ok(())
        }
        "focus" => {
            if args.is_empty() {
                let state:String=proxy.call("Snapshot",&()).await.map_err(|e|e.to_string())?;
                let value:Value=serde_json::from_str(&state).map_err(|e|e.to_string())?;
                println!("{}",value["focus"]);
                return Ok(());
            }
            let command=if args[0]=="configure" {
                if args.len()!=1 {return Err("Provide one profile JSON object on standard input".into());}
                let mut input=String::new();
                std::io::stdin().take(65537).read_to_string(&mut input).map_err(|e|e.to_string())?;
                let profile:Value=serde_json::from_str(&input).map_err(|e|e.to_string())?;
                json!({"action":"focus","operation":"profile","profile":profile})
            } else {
                if args.len()>2 {return Err("Usage: lumen-notifications focus PROFILE|off|auto [SECONDS]".into());}
                let profile=match args[0].as_str() {"auto"=>Value::Null,"off"=>json!(""),name=>json!(name)};
                let seconds=if args.len()>1 {args[1].parse::<u64>().map_err(|e|e.to_string())?} else {0};
                json!({"action":"focus","profile":profile,"seconds":seconds})
            };
            proxy.call::<_,_,()>("Command",&(command.to_string(),)).await.map_err(|e|e.to_string())
        }
        "watch" => {
            let generation=args.first().map(String::as_str).unwrap_or("");
            if generation.len()>32 || !generation.bytes().all(|byte| byte.is_ascii_digit()) {
                return Err("Invalid notification watch generation".into());
            }
            let mut changes=proxy.receive_signal("Changed").await.map_err(|e|e.to_string())?;
            let mut owners=bus.receive_signal("NameOwnerChanged").await.map_err(|e|e.to_string())?;
            let initial:String=proxy.call("Snapshot",&()).await.map_err(|e|e.to_string())?;
            let mut instance=None;let mut revision=0;
            if let Some(state)=transport_snapshot(&initial,&mut instance,&mut revision,generation) {println!("{state}");}
            let mut lines=BufReader::new(tokio::io::stdin()).lines();
            let mut heartbeat=tokio::time::interval(std::time::Duration::from_millis(400));
            loop {tokio::select! {
                _=heartbeat.tick()=>{
                    let probe=proxy.call::<_,_,String>("Probe",&()).await;
                    let valid=probe.ok().and_then(|raw|serde_json::from_str::<Value>(&raw).ok())
                        .filter(|value| value["instance"].as_str()==instance.as_deref()
                            && value["commandUntil"].as_u64().is_some_and(|until| until>0 && until<=9_007_199_254_740_991));
                    if let Some(mut value)=valid {
                        value["liveness"]=json!(true);value["transport"]=json!(generation);
                        println!("{value}");
                    } else {println!("{}",json!({"error":"Notification service is unavailable","transport":generation}));break;}
                },
                line=lines.next_line()=>match line.map_err(|e|e.to_string())? {
                    Some(line)=>if proxy.call::<_,_,()>("Command",&(line,)).await.is_err(){println!("{}",json!({"error":"Notification request failed","transport":generation}));},
                    None=>break,
                },
                message=changes.next()=>match message {
                    Some(message)=>if let Ok((raw,))=message.body().deserialize::<(String,)>() {
                        if let Some(state)=transport_snapshot(&raw,&mut instance,&mut revision,generation){println!("{state}");}
                    },
                    None=>break,
                },
                message=owners.next()=>if let Some(message)=message {
                    if let Ok((name,old,new))=message.body().deserialize::<(String,String,String)>() {
                        if name==BUS && old==owner && new!=owner {break;}
                    }
                }
            }}
            Ok(())
        }
        _=>Err("Unknown command".into()),
    }
}
#[tokio::main]
async fn main() {
    let args:Vec<String>=env::args().skip(1).collect();
    let mode=args.first().map(String::as_str).unwrap_or("serve");
    let result=if mode=="serve" {serve().await}else{client(mode,args.get(1..).unwrap_or(&[])).await};
    if let Err(error)=result {eprintln!("{error}");std::process::exit(1);}
}
#[cfg(test)] mod tests {
    use super::*;
    #[test] fn transport_rejects_replay_and_owner_change() {
        let mut owner=None;let mut revision=0;
        let frame=|instance:&str,revision:u64|json!({"instance":instance,"revision":revision}).to_string();
        assert!(transport_snapshot(&frame(&"a".repeat(32),7),&mut owner,&mut revision,"2").is_some());
        assert!(transport_snapshot(&frame(&"a".repeat(32),6),&mut owner,&mut revision,"2").is_none());
        assert!(transport_snapshot(&frame(&"b".repeat(32),8),&mut owner,&mut revision,"2").is_none());
        assert!(transport_snapshot(&frame(&"a".repeat(32),8),&mut owner,&mut revision,"2").is_some());
        assert!(transport_snapshot(&frame("invalid",9),&mut owner,&mut revision,"2").is_none());
    }
    #[test] fn command_authority_rejects_expired_and_wrong_instance() {
        assert!(authority(&json!({"action":"seen","instance":"wrong","expires":boot_time_ms()+500}),"correct").is_err());
        assert!(authority(&json!({"action":"seen","instance":"correct","expires":1}),"correct").is_err());
        assert!(authority(&json!({"action":"dismiss","id":true}),"correct").is_err());
    }
    #[test] fn inline_image_hint_writes_private_png() {
        let root=env::temp_dir().join(format!("notification-image-{}",std::process::id()));
        std::fs::create_dir(&root).unwrap();
        let pixels=vec![255_u8,0,0,255];
        let structure:Structure<'_>=(1_i32,1_i32,4_i32,true,8_i32,4_i32,pixels).into();
        let value=OwnedValue::try_from(DbusValue::new(structure)).unwrap();
        let hints=HashMap::from([("image-data".into(),value)]);
        let image=inline_image(&hints,&root,7).unwrap();
        assert!(image.starts_with("file://"));
        let file=std::fs::read_dir(&root).unwrap().next().unwrap().unwrap().path();
        assert_eq!(&std::fs::read(&file).unwrap()[..8],b"\x89PNG\r\n\x1a\n");
        std::fs::remove_dir_all(root).unwrap();
    }
}
