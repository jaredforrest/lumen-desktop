//! Private D-Bus, settings, compositor lock, and restart process fixture.
use lumen_notifications::{BUS, PATH, UI};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::env;
use std::fs;
use std::io::{BufRead, Read, Write};
use std::os::unix::fs::{PermissionsExt, symlink};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::process::{self, Child, Command, Stdio};
use std::sync::{Arc, Mutex, mpsc, atomic::{AtomicBool, AtomicI64, Ordering}};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use zbus::{Connection, Proxy, zvariant::{OwnedValue, Str}};

fn fake_settings() -> ! {
    let args: Vec<String> = env::args().skip(1).collect();
    let path = env::var("LUMEN_NIX_SETTINGS_ACTIVE").unwrap();
    let mut value: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    if args.first().is_some_and(|arg| arg == "set") {
        assert_eq!(args[1], "notifications");
        value["notifications"] = serde_json::from_str(&args[2]).unwrap();
        fs::write(path, value.to_string()).unwrap();
    }
    println!("{value}");
    process::exit(0)
}

fn frame(stream: &mut UnixStream, kind: u32, payload: &Value) {
    let body = payload.to_string();
    let mut header = b"i3-ipc".to_vec();
    header.extend_from_slice(&(body.len() as u32).to_le_bytes());
    header.extend_from_slice(&kind.to_le_bytes());
    stream.write_all(&header).unwrap();
    stream.write_all(body.as_bytes()).unwrap();
}
struct LockPeer {
    path: PathBuf,
    stopped: Arc<AtomicBool>,
    locked: Arc<AtomicBool>,
    revision: Arc<AtomicI64>,
    subscribers: Arc<Mutex<Vec<UnixStream>>>,
    worker: Option<thread::JoinHandle<()>>,
}
impl LockPeer {
    fn new(path: PathBuf) -> Self {
        let listener = UnixListener::bind(&path).unwrap();
        listener.set_nonblocking(true).unwrap();
        let stopped = Arc::new(AtomicBool::new(false));
        let locked = Arc::new(AtomicBool::new(false));
        let revision = Arc::new(AtomicI64::new(0));
        let subscribers = Arc::new(Mutex::new(Vec::new()));
        let worker = {
            let stopped=stopped.clone();let locked=locked.clone();let revision=revision.clone();let subscribers=subscribers.clone();
            thread::spawn(move || {
                while !stopped.load(Ordering::Relaxed) {
                    match listener.accept() {
                        Ok((mut stream,_)) => {
                            let mut header=[0u8;14];
                            if stream.read_exact(&mut header).is_err() || &header[..6]!=b"i3-ipc" {continue;}
                            let size=u32::from_le_bytes(header[6..10].try_into().unwrap()) as usize;
                            let kind=u32::from_le_bytes(header[10..14].try_into().unwrap());
                            if size>65536 {continue;}
                            let mut payload=vec![0u8;size];
                            if stream.read_exact(&mut payload).is_err() {continue;}
                            let answer=if kind==2 {json!({"success":true})}
                                else {json!({"lumen_session_lock":{"version":1,"available":true,
                                    "locked":locked.load(Ordering::Relaxed),"revision":revision.load(Ordering::Relaxed)}})};
                            frame(&mut stream,kind,&answer);
                            if kind==2 {subscribers.lock().unwrap().push(stream);}
                        }
                        Err(error) if error.kind()==std::io::ErrorKind::WouldBlock => thread::sleep(Duration::from_millis(10)),
                        Err(_) => break,
                    }
                }
            })
        };
        Self { path, stopped, locked, revision, subscribers, worker:Some(worker) }
    }
    fn set(&self, locked: bool, revision: i64) {
        self.locked.store(locked,Ordering::Relaxed);
        self.revision.store(revision,Ordering::Relaxed);
        for subscriber in self.subscribers.lock().unwrap().iter_mut() {frame(subscriber,0x80000007,&json!({}));}
    }
}
impl Drop for LockPeer {
    fn drop(&mut self) {
        self.stopped.store(true,Ordering::Relaxed);
        self.subscribers.lock().unwrap().clear();
        if let Some(worker)=self.worker.take() {worker.join().unwrap();}
        let _=fs::remove_file(&self.path);
    }
}

struct Fixture { root:PathBuf, settings:PathBuf, binary:PathBuf }
impl Fixture {
    fn new() -> Self {
        let nonce=SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let root=env::temp_dir().join(format!("lumen-notifications-{}-{nonce}",process::id()));
        fs::create_dir(&root).unwrap();
        let settings=root.join("settings.json");
        fs::write(&settings,json!({"notifications":{"policies":{},"preview":"never"}}).to_string()).unwrap();
        let binary=root.join("lumen-nix-settings");
        symlink(env::current_exe().unwrap(),&binary).unwrap();
        Self {root,settings,binary}
    }
    fn command(&self) -> Command {
        let mut command=Command::new(env!("CARGO_BIN_EXE_lumen-notifications"));
        command.env("LUMEN_NIX_SETTINGS_BIN",&self.binary)
            .env("LUMEN_NIX_SETTINGS_ACTIVE",&self.settings)
            .env("LUMEN_NOTIFICATION_STATE",self.root.join("state"))
            .env("SWAYSOCK",self.root.join("ipc.sock"));
        command
    }
    fn set_preview(&self,preview:&str) {
        let mut data:Value=serde_json::from_slice(&fs::read(&self.settings).unwrap()).unwrap();
        data["notifications"]["preview"]=json!(preview);
        fs::write(&self.settings,data.to_string()).unwrap();
    }
}
impl Drop for Fixture {fn drop(&mut self){let _=fs::remove_dir_all(&self.root);}}
struct Service(Child);
impl Drop for Service {fn drop(&mut self){let _=self.0.kill();let _=self.0.wait();}}
async fn wait_owner(bus:&Proxy<'_>,expected:bool) {
    let end=Instant::now()+Duration::from_secs(7);
    loop {
        let owned:bool=bus.call("NameHasOwner",&(BUS,)).await.unwrap();
        if owned==expected {return;}
        assert!(Instant::now()<end,"notification service owner timeout");
        thread::sleep(Duration::from_millis(25));
    }
}
async fn snapshot(ui:&Proxy<'_>) -> Value {
    let raw:String=ui.call("Snapshot",&()).await.unwrap();
    serde_json::from_str(&raw).unwrap()
}
async fn command(ui:&Proxy<'_>,value:Value) -> zbus::Result<()> {
    ui.call("Command",&(value.to_string(),)).await
}
async fn notify(standard:&Proxy<'_>,app:&str,replaces:u32,summary:&str,body:&str,actions:&[&str],desktop:Option<&str>) -> u32 {
    let mut hints=HashMap::<String,OwnedValue>::new();
    if let Some(desktop)=desktop {hints.insert("desktop-entry".into(),OwnedValue::from(Str::from(desktop)));}
    standard.call("Notify",&(app,replaces,"",summary,body,actions.to_vec(),hints,-1_i32)).await.unwrap()
}
async fn wait_policy(ui:&Proxy<'_>,app:&str,policy:&str) {
    let end=Instant::now()+Duration::from_secs(5);
    while snapshot(ui).await["policies"][app]!=policy {
        assert!(Instant::now()<end,"notification policy was not applied");
        thread::sleep(Duration::from_millis(20));
    }
}
fn watch(fixture:&Fixture,instance:&str) {
    let mut child=fixture.command().args(["watch","7"])
        .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap();
    let output=child.stdout.take().unwrap();
    let (tx,rx)=mpsc::channel();
    let reader=thread::spawn(move || {
        for line in std::io::BufReader::new(output).lines() {
            if tx.send(line.unwrap()).is_err(){break;}
        }
    });
    let first:Value=serde_json::from_str(&rx.recv_timeout(Duration::from_secs(3)).unwrap()).unwrap();
    assert_eq!(first["transport"],"7");
    assert_eq!(first["instance"],instance);
    assert!(first["revision"].as_u64().unwrap()>0);
    let heartbeat:Value=serde_json::from_str(&rx.recv_timeout(Duration::from_secs(3)).unwrap()).unwrap();
    assert_eq!(heartbeat["transport"],"7");
    assert_eq!(heartbeat["liveness"],true);
    let _=child.kill();let _=child.wait();
    reader.join().unwrap();
}

#[tokio::main]
async fn private() {
    let fixture=Fixture::new();
    let conn=Connection::session().await.unwrap();
    let bus=Proxy::new(&conn,"org.freedesktop.DBus","/org/freedesktop/DBus","org.freedesktop.DBus").await.unwrap();
    let mut peer:Option<LockPeer>=None;
    for iteration in 0..2 {
        let child=fixture.command().arg("serve").stdout(Stdio::null()).stderr(Stdio::piped()).spawn().unwrap();
        let mut service=Service(child);
        wait_owner(&bus,true).await;
        let ui=Proxy::new(&conn,BUS,PATH,UI).await.unwrap();
        let standard=Proxy::new(&conn,BUS,PATH,BUS).await.unwrap();
        let state=snapshot(&ui).await;
        assert_eq!(state["ready"],true);
        assert_eq!(state["lock"],json!({"known":false,"locked":true}));
        assert_eq!(state["previewsVisible"],false);
        assert_eq!(state["instance"].as_str().unwrap().len(),32);
        assert_eq!(state["focus"]["active"],"");
        let probe:String=ui.call("Probe",&()).await.unwrap();
        assert_eq!(serde_json::from_str::<Value>(&probe).unwrap()["instance"],state["instance"]);
        if iteration==0 {
            watch(&fixture,state["instance"].as_str().unwrap());
            let id=notify(&standard,"Fixture",0,"Private subject","Private body",&["open","Open"],None).await;
            let state=snapshot(&ui).await;
            assert_eq!(state["rows"][0]["id"],id);
            assert_eq!(state["rows"][0]["summary"],"Notification");
            assert_eq!(state["rows"][0]["body"],"");
            assert!(fs::read_to_string(fixture.root.join("state/history.json")).unwrap().contains("Private body"));
            peer=Some(LockPeer::new(fixture.root.join("ipc.sock")));
            fixture.set_preview("when_unlocked");
            thread::sleep(Duration::from_millis(1100));
            let visible=snapshot(&ui).await;
            assert_eq!(visible["lock"],json!({"known":true,"locked":false}));
            assert_eq!(visible["rows"][0]["summary"],"Private subject");
            let replacement=notify(&standard,"Fixture",0,"First delivery","Body",&["open","Open"],None).await;
            let first=snapshot(&ui).await["rows"][0]["delivery"].clone();
            notify(&standard,"Fixture",replacement,"Replacement delivery","Body",&["open","Open again"],None).await;
            let current=snapshot(&ui).await["rows"][0]["delivery"].clone();
            assert_ne!(first,current);
            command(&ui,json!({"action":"invoke","id":replacement,"key":"open","delivery":first})).await.unwrap();
            assert_eq!(snapshot(&ui).await["rows"][0]["active"],true);
            command(&ui,json!({"action":"invoke","id":replacement,"key":"open","delivery":current})).await.unwrap();
            assert!(!snapshot(&ui).await["rows"].as_array().unwrap().iter().any(|row| row["id"]==replacement));
            assert!(!fs::read_to_string(fixture.root.join("state/history.json")).unwrap().contains("delivery"));
            let authority=peer.as_ref().unwrap();
            authority.set(true,1);
            let hidden=snapshot(&ui).await;
            assert_eq!(hidden["rows"][0]["summary"],"Notification");
            assert_eq!(hidden["previewsVisible"],false);
            authority.set(false,0);
            assert_eq!(snapshot(&ui).await["lock"],json!({"known":false,"locked":true}));
            let retired=fixture.root.join("retired.sock");
            fs::rename(fixture.root.join("ipc.sock"),&retired).unwrap();
            drop(peer.take());
            peer=Some(LockPeer::new(fixture.root.join("ipc.sock")));
            peer.as_ref().unwrap().set(true,0);
            thread::sleep(Duration::from_millis(1100));
            assert_eq!(snapshot(&ui).await["lock"],json!({"known":true,"locked":true}));
            drop(peer.take());fs::remove_file(retired).unwrap();

            command(&ui,json!({"action":"focus","operation":"profile","profile":{"id":"work","name":"Work","allowedApps":["calendar"],"allowCritical":false,"schedules":[]}})).await.unwrap();
            command(&ui,json!({"action":"focus","profile":"work"})).await.unwrap();
            assert_eq!(snapshot(&ui).await["focus"]["active"],"work");
            let blocked=notify(&standard,"chat",0,"Blocked banner","",&[],Some("chat")).await;
            assert_eq!(snapshot(&ui).await["rows"].as_array().unwrap().iter().find(|row|row["id"]==blocked).unwrap()["banner"],false);
            let allowed=notify(&standard,"calendar",0,"Allowed banner","",&[],Some("calendar")).await;
            assert_eq!(snapshot(&ui).await["rows"].as_array().unwrap().iter().find(|row|row["id"]==allowed).unwrap()["banner"],true);
            command(&ui,json!({"action":"dnd","enabled":false})).await.unwrap();
            assert_eq!(snapshot(&ui).await["focus"]["active"],"");
            command(&ui,json!({"action":"policy","app":"Fixture","policy":"silent"})).await.unwrap();
            wait_policy(&ui,"Fixture","silent").await;
            let silent=notify(&standard,"Fixture",0,"Silent","",&[],None).await;
            assert_eq!(snapshot(&ui).await["rows"].as_array().unwrap().iter().find(|row|row["id"]==silent).unwrap()["banner"],false);
            command(&ui,json!({"action":"policy","app":"Fixture","policy":"off"})).await.unwrap();
            wait_policy(&ui,"Fixture","off").await;
            let count=snapshot(&ui).await["rows"].as_array().unwrap().len();
            notify(&standard,"Fixture",0,"Off","",&[],None).await;
            assert_eq!(snapshot(&ui).await["rows"].as_array().unwrap().len(),count);
            let before=snapshot(&ui).await;
            assert!(command(&ui,json!({"action":"seen","instance":before["instance"],"expires":1})).await.is_err());
            command(&ui,json!({"action":"seen","instance":before["instance"],"expires":before["commandUntil"]})).await.unwrap();
            assert_eq!(snapshot(&ui).await["rows"][0]["unread"],false);
        } else {
            assert!(!state["rows"].as_array().unwrap().is_empty());
            assert!(state["rows"].as_array().unwrap().iter().all(|row| row["active"]==false && row["actions"]==json!([]) && row["unread"]==false));
        }
        if iteration==0 {drop(peer.take());}
        let _=service.0.kill();let _=service.0.wait();
        wait_owner(&bus,false).await;
    }
    drop(peer);
    assert_eq!(fs::metadata(fixture.root.join("state/focus.json")).unwrap().permissions().mode()&0o777,0o600);
}

fn main() {
    let program=env::args().next().unwrap();
    if Path::new(&program).file_name().is_some_and(|name| name=="lumen-nix-settings") {fake_settings();}
    if env::args().any(|arg| arg=="--private") {private();return;}
    let nonce=SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let bus_root=env::temp_dir().join(format!("lumen-notification-bus-{}-{nonce}",process::id()));
    fs::create_dir(&bus_root).unwrap();
    let config=bus_root.join("bus.conf");
    fs::write(&config,format!("<busconfig><type>session</type><listen>unix:tmpdir={}</listen><auth>EXTERNAL</auth><policy context=\"default\"><allow send_destination=\"*\"/><allow receive_sender=\"*\"/><allow own=\"*\"/></policy></busconfig>",bus_root.display())).unwrap();
    let result=Command::new("timeout").args(["40","dbus-run-session"])
        .arg(format!("--config-file={}",config.display())).arg("--")
        .arg(env::current_exe().unwrap()).arg("--private")
        .env("HOME",&bus_root).env("XDG_RUNTIME_DIR",&bus_root)
        .env_remove("DBUS_SESSION_BUS_ADDRESS").output().unwrap();
    fs::remove_dir_all(&bus_root).unwrap();
    assert!(result.status.success(),"{} {}",String::from_utf8_lossy(&result.stdout),String::from_utf8_lossy(&result.stderr));
}
