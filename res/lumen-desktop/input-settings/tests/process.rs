//! Exact input identity, persistence, reversal, and reconnect against private IPC.
use serde_json::{Value, json};
use std::env;
use std::fs;
use std::io::{Read, Write};
use std::os::unix::fs::symlink;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::process::{self, Command, Stdio};
use std::sync::{Arc, Mutex, atomic::{AtomicBool, Ordering}};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

fn fake_settings() -> ! {
    let args:Vec<String>=env::args().skip(1).collect();
    let path=PathBuf::from(env::var("LUMEN_NIX_SETTINGS_ACTIVE").unwrap());
    let mut value:Value=serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    if args.first().is_some_and(|arg|arg=="set") {
        if path.with_extension("deny").exists() {
            println!("{}",json!({"error":"Fixture persistence refused"}));
            process::exit(1);
        }
        value[&args[1]]=serde_json::from_str(&args[2]).unwrap();
        let pending=path.with_extension("new");
        fs::write(&pending,value.to_string()).unwrap();fs::rename(pending,path).unwrap();
    }
    println!("{value}");
    process::exit(0)
}
fn frame(stream:&mut UnixStream,kind:u32,payload:&Value) -> std::io::Result<()> {
    let body=payload.to_string();
    let mut header=b"i3-ipc".to_vec();
    header.extend_from_slice(&(body.len() as u32).to_le_bytes());
    header.extend_from_slice(&kind.to_le_bytes());
    stream.write_all(&header)?;stream.write_all(body.as_bytes())
}
struct State {devices:Vec<Value>,actions:Vec<Vec<String>>,subscribers:Vec<UnixStream>}
struct Snapshot {devices:Vec<Value>,actions:Vec<Vec<String>>,subscribers:usize}
fn devices() -> Vec<Value> {vec![
    json!({"identifier":"1:1:Fixture_Keyboard","name":"Fixture Keyboard","type":"keyboard","repeat_rate":25,"repeat_delay":600}),
    json!({"identifier":"2:2:Fixture_Mouse","name":"Fixture Mouse","type":"pointer","scroll_factor":1.0,
        "libinput":{"accel_speed":0.0,"natural_scroll":"disabled","left_handed":"disabled"}}),
    json!({"identifier":"3:3:Fixture_Trackpad","name":"Fixture Trackpad","type":"touchpad","scroll_factor":1.0,
        "libinput":{"accel_speed":0.2,"natural_scroll":"enabled","left_handed":"disabled","tap":"disabled","dwt":"enabled"}})
]}
fn client(mut stream:UnixStream,state:Arc<Mutex<State>>) {
    loop {
        let mut header=[0u8;14];
        if stream.read_exact(&mut header).is_err(){return;}
        if &header[..6]!=b"i3-ipc" {return;}
        let size=u32::from_le_bytes(header[6..10].try_into().unwrap()) as usize;
        let kind=u32::from_le_bytes(header[10..14].try_into().unwrap());
        if size>65536{return;}
        let mut payload=vec![0u8;size];if stream.read_exact(&mut payload).is_err(){return;}
        let mut data=state.lock().unwrap();
        let result=match kind {
            100=>json!(data.devices.clone()),
            2=>{
                assert_eq!(serde_json::from_slice::<Value>(&payload).unwrap(),json!(["input"]));
                data.subscribers.push(stream.try_clone().unwrap());
                json!({"success":true})
            }
            0=>{
                let raw=String::from_utf8(payload).unwrap();
                let mut tokens:Vec<_>=raw.split_whitespace().map(str::to_owned).collect();
                assert_eq!(tokens.len(),4);assert_eq!(tokens[0],"input");
                tokens[1]=serde_json::from_str(&tokens[1]).unwrap();
                assert!(!["*","type:keyboard","type:pointer","type:touchpad"].contains(&tokens[1].as_str()));
                data.actions.push(tokens[1..].to_vec());
                for device in &mut data.devices {
                    if device["identifier"]!=tokens[1] {continue;}
                    let field=if tokens[2]=="pointer_accel" {"accel_speed"} else {&tokens[2]};
                    if ["repeat_rate","repeat_delay"].contains(&tokens[2].as_str()) {
                        device[field]=json!(tokens[3].parse::<u32>().unwrap());
                    } else if tokens[2]=="scroll_factor" {
                        device[field]=json!(tokens[3].parse::<f64>().unwrap());
                    } else if tokens[2]=="pointer_accel" {
                        device["libinput"][field]=json!(tokens[3].parse::<f64>().unwrap());
                    } else {device["libinput"][field]=json!(tokens[3]);}
                }
                json!([{"success":true}])
            }
            _=>return,
        };
        drop(data);
        if frame(&mut stream,kind,&result).is_err(){return;}
    }
}
struct Server {path:PathBuf,state:Arc<Mutex<State>>,stopped:Arc<AtomicBool>,worker:Option<thread::JoinHandle<()>>}
impl Server {
    fn new(path:PathBuf) -> Self {
        let listener=UnixListener::bind(&path).unwrap();listener.set_nonblocking(true).unwrap();
        let state=Arc::new(Mutex::new(State{devices:devices(),actions:vec![],subscribers:vec![]}));
        let stopped=Arc::new(AtomicBool::new(false));
        let worker={let state=state.clone();let stopped=stopped.clone();thread::spawn(move||{
            while !stopped.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((stream,_))=>{let state=state.clone();thread::spawn(move||client(stream,state));}
                    Err(error)if error.kind()==std::io::ErrorKind::WouldBlock=>thread::sleep(Duration::from_millis(10)),
                    Err(_)=>break,
                }
            }
        })};
        Self{path,state,stopped,worker:Some(worker)}
    }
    fn state(&self)->Snapshot{let state=self.state.lock().unwrap();Snapshot{devices:state.devices.clone(),actions:state.actions.clone(),subscribers:state.subscribers.len()}}
    fn edit<F:FnOnce(&mut State)>(&self,edit:F){edit(&mut self.state.lock().unwrap());}
    fn add(&self,device:Value){
        let mut state=self.state.lock().unwrap();
        state.devices.push(device.clone());
        state.subscribers.retain_mut(|stream|frame(stream,0x80000015,&json!({"change":"added","input":device})).is_ok());
    }
}
impl Drop for Server {fn drop(&mut self){self.stopped.store(true,Ordering::Relaxed);if let Some(worker)=self.worker.take(){worker.join().unwrap();}let _=fs::remove_file(&self.path);}}

struct Fixture {root:PathBuf,path:String,server:Server}
impl Fixture {
    fn new()->Self{
        let nonce=SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let root=env::temp_dir().join(format!("lumen-input-settings-{}-{nonce}",process::id()));
        fs::create_dir(&root).unwrap();
        fs::write(root.join("settings.json"),"{\"inputs\":{}}").unwrap();
        symlink(env::current_exe().unwrap(),root.join("lumen-nix-settings")).unwrap();
        let path=format!("{}:{}",root.display(),env::var("PATH").unwrap());
        let server=Server::new(root.join("ipc.sock"));
        Self{root,path,server}
    }
    fn command(&self)->Command{
        let mut command=Command::new(env!("CARGO_BIN_EXE_lumen-input-settings"));
        command.env("PATH",&self.path).env("LUMEN_NIX_SETTINGS_ACTIVE",self.root.join("settings.json"))
            .env("XDG_RUNTIME_DIR",&self.root).env("SWAYSOCK",self.root.join("ipc.sock"));
        command
    }
    fn call(&self,request:Value)->Value{
        let mut child=self.command().stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap();
        writeln!(child.stdin.take().unwrap(),"{request}").unwrap();
        let output=child.wait_with_output().unwrap();
        assert!(output.status.success(),"{}",String::from_utf8_lossy(&output.stderr));
        serde_json::from_slice(&output.stdout).unwrap()
    }
    fn state(&self)->Value{let value=self.call(json!({"action":"status"}));assert_eq!(value["available"],true);value}
    fn device(&self,kind:&str)->Value{self.state()["devices"].as_array().unwrap().iter().find(|row|row["identity"]["type"]==kind).unwrap().clone()}
    fn edit(&self,kind:&str,key:&str,value:Value)->Value{self.call(json!({"action":"set","device":self.device(kind)["key"],"key":key,"value":value}))}
}
impl Drop for Fixture{fn drop(&mut self){let _=fs::remove_dir_all(&self.root);}}

fn main(){
    if Path::new(&env::args().next().unwrap()).file_name().is_some_and(|name|name=="lumen-nix-settings"){fake_settings();}
    let fixture=Fixture::new();
    assert_eq!(fixture.state()["devices"].as_array().unwrap().len(),3);
    assert!(!fixture.device("pointer")["controls"].as_array().unwrap().iter().any(|control|control["key"]=="tap"));
    let before=fixture.server.state().devices;
    let first_edit=fixture.edit("pointer","natural_scroll",json!(true));
    assert!(first_edit.get("message").is_some(),"{first_edit}");
    let after=fixture.server.state();
    assert_eq!(after.devices[0],before[0]);assert_eq!(after.devices[2],before[2]);
    assert_eq!(after.devices[1]["libinput"]["natural_scroll"],"enabled");
    assert_eq!(after.actions.last().unwrap(),&["2:2:Fixture_Mouse","natural_scroll","enabled"]);
    let key=fixture.device("pointer")["key"].as_str().unwrap().to_owned();
    let saved:Value=serde_json::from_slice(&fs::read(fixture.root.join("settings.json")).unwrap()).unwrap();
    assert_eq!(saved["inputs"][&key]["baseline"]["natural_scroll"],false);
    assert!(fixture.call(json!({"action":"restore","device":key,"key":"natural_scroll"})).get("message").is_some());
    assert_eq!(fixture.device("pointer")["saved"],false);
    assert_eq!(fixture.server.state().devices[1]["libinput"]["natural_scroll"],"disabled");
    for request in [json!({"action":"set","device":key,"key":"natural_scroll","value":1}),
        json!({"action":"set","device":key,"key":"events","value":"disabled"}),
        json!({"action":"set","device":key,"key":"pointer_accel","value":5})] {
        assert!(fixture.call(request).get("error").is_some());
    }
    fs::write(fixture.root.join("settings.deny"),"").unwrap();
    assert!(fixture.call(json!({"action":"set","device":key,"key":"natural_scroll","value":true})).get("error").is_some());
    assert_eq!(fixture.server.state().devices[1]["libinput"]["natural_scroll"],"disabled");
    fs::remove_file(fixture.root.join("settings.deny")).unwrap();
    fixture.server.edit(|state|{let duplicate=state.devices[1].clone();state.devices.push(duplicate);});
    assert_eq!(fixture.device("pointer")["editable"],false);
    assert!(fixture.edit("pointer","natural_scroll",json!(true)).get("error").is_some());
    fixture.server.edit(|state|{state.devices.pop();});
    assert!(fixture.edit("pointer","natural_scroll",json!(true)).get("message").is_some());
    fixture.server.edit(|state|state.devices[1]["libinput"]["natural_scroll"]=json!("disabled"));
    let mut watcher=fixture.command().arg("watch").stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap();
    let deadline=Instant::now()+Duration::from_secs(4);
    while fixture.server.state().subscribers==0 && Instant::now()<deadline{thread::sleep(Duration::from_millis(20));}
    assert!(fixture.server.state().subscribers>0);
    let mut mouse=None;
    fixture.server.edit(|state|mouse=Some(state.devices.remove(1)));
    assert_eq!(fixture.state()["devices"].as_array().unwrap().iter().find(|row|row["key"]==key).unwrap()["connected"],false);
    let mut mouse=mouse.unwrap();mouse["libinput"]["natural_scroll"]=json!("disabled");
    fixture.server.add(mouse);
    let deadline=Instant::now()+Duration::from_secs(4);
    while fixture.server.state().devices.last().unwrap()["libinput"]["natural_scroll"]!="enabled" && Instant::now()<deadline{thread::sleep(Duration::from_millis(20));}
    assert_eq!(fixture.server.state().devices.last().unwrap()["libinput"]["natural_scroll"],"enabled");
    let _=watcher.kill();let output=watcher.wait_with_output().unwrap();
    assert!(output.stderr.is_empty(),"{}",String::from_utf8_lossy(&output.stderr));
    assert!(fixture.call(json!({"action":"forget","device":key,"confirmed":true})).get("message").is_some());
    let saved:Value=serde_json::from_slice(&fs::read(fixture.root.join("settings.json")).unwrap()).unwrap();
    assert_eq!(saved["inputs"],json!({}));
}
