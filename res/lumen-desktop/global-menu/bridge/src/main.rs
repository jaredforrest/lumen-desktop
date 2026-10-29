//! Focus-bound AppMenu registrar and Quickshell JSON bridge.
use lumen_menu_bridge::{Layout,actionable,clean,contains_focus,context,focused};
use futures_util::StreamExt;
use serde_json::{Value,json};
use std::collections::HashMap;
use std::env;
use std::fs;
use std::io::Write;
use std::process::Stdio;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool,Ordering};
use std::time::{Duration,Instant};
use tokio::io::{AsyncBufReadExt,BufReader};
use tokio::process::Command;
use tokio::sync::{Mutex,mpsc};
use zbus::{Connection,MatchRule,MessageStream,interface,message::{Header,Type},zvariant::{OwnedObjectPath,OwnedValue}};

const REG:&str="com.canonical.AppMenu.Registrar";
const PATH:&str="/com/canonical/AppMenu/Registrar";
const MENU:&str="com.canonical.dbusmenu";
type Result<T>=std::result::Result<T,String>;
#[derive(Clone)] struct Registration{owner:String,path:String,epoch:u64}
struct State {
    registrations:HashMap<u32,Registration>,epoch:u64,snapshot:Value,held_token:String,
    output_name:String,last_emit:String,last_refresh:Instant,pending_action:u64,
}
impl State {
    fn new()->Self {Self{registrations:HashMap::new(),epoch:0,
        snapshot:json!({"token":"","title":"","supported":false,"menu":null,"revision":null,"windowId":0,"outputName":"","contextAppName":null,"contextWindowId":null,"mainWindowId":null,"appName":"Desktop"}),
        held_token:String::new(),output_name:String::new(),last_emit:String::new(),last_refresh:Instant::now()-Duration::from_secs(5),pending_action:0}}
}
#[derive(Clone)] struct Runtime{state:Arc<Mutex<State>>,refresh:Arc<AtomicBool>,events:mpsc::UnboundedSender<Event>}
enum Event {Refresh,Input(Value),Stop}
#[derive(Clone)] struct Registrar(Runtime);
#[derive(Clone)] struct Ui(Runtime);
fn emit(value:&Value) {
    let stdout=std::io::stdout();let mut lock=stdout.lock();
    let _=writeln!(lock,"{value}");let _=lock.flush();
}
impl Runtime {
    fn queue_refresh(&self) {
        if !self.refresh.swap(true,Ordering::SeqCst){let _=self.events.send(Event::Refresh);}
    }
    async fn publish(&self) {
        let mut state=self.state.lock().await;let data=state.snapshot.to_string();
        if data!=state.last_emit{emit(&state.snapshot);state.last_emit=data;}
    }
    async fn name_has_owner(conn:&Connection,name:&str)->bool {
        let Ok(proxy)=zbus::Proxy::new(conn,"org.freedesktop.DBus","/org/freedesktop/DBus","org.freedesktop.DBus").await else{return false;};
        proxy.call::<_,_,bool>("NameHasOwner",&(name,)).await.unwrap_or(false)
    }
    async fn owner_pid(conn:&Connection,name:&str)->Result<u32> {
        let proxy=zbus::Proxy::new(conn,"org.freedesktop.DBus","/org/freedesktop/DBus","org.freedesktop.DBus").await.map_err(|e|e.to_string())?;
        proxy.call::<_,_,u32>("GetConnectionUnixProcessID",&(name,)).await.map_err(|e|e.to_string())
    }
    fn helper_for_window(owner_pid:u32,window_pid:u32)->bool {
        let proc=PathBuf::from("/proc").join(owner_pid.to_string());
        let Ok(cmdline)=fs::read(proc.join("cmdline")) else{return false;};
        let helper=cmdline.split(|byte|*byte==0).any(|arg|arg.rsplit(|byte|*byte==b'/').next()==Some(b"export-helper".as_slice()));
        if !helper{return false;}
        let Ok(status)=fs::read_to_string(proc.join("status")) else{return false;};
        let parent=status.lines().find_map(|line|line.strip_prefix("PPid:").and_then(|value|value.trim().parse::<u32>().ok()));
        parent==Some(window_pid)
    }
    async fn current(&self,conn:&Connection)->Result<(Option<Value>,Option<Registration>,String,String,Option<(Option<String>,i64)>)> {
        let output=tokio::time::timeout(Duration::from_secs(2),Command::new("swaymsg").args(["-r","-t","get_tree"]).output())
            .await.map_err(|_|"Sway tree timed out")?.map_err(|e|e.to_string())?;
        if !output.status.success(){return Err("Sway tree unavailable".into());}
        let tree:Value=serde_json::from_slice(&output.stdout).map_err(|e|e.to_string())?;
        let window=focused(&tree).cloned();
        let window_context=window.as_ref().and_then(|win|win["id"].as_i64()).and_then(|id|context(&tree,id));
        let output=tree["nodes"].as_array().into_iter().flatten().find(|node|node["type"]=="output" && contains_focus(node))
            .and_then(|node|node["name"].as_str()).map(str::to_owned);
        let (mut item,old_output)={let state=self.state.lock().await;
            (window.as_ref().and_then(|window|window["window"].as_u64()).and_then(|id|u32::try_from(id).ok()).and_then(|id|state.registrations.get(&id).cloned()),state.output_name.clone())};
        if let (Some(reg),Some(win))=(&item,&window) {
            let owner_pid=Self::owner_pid(conn,&reg.owner).await;
            let window_pid=win["pid"].as_u64().and_then(|pid|u32::try_from(pid).ok());
            if !owner_pid.is_ok_and(|owner_pid|window_pid.is_some_and(|window_pid|owner_pid==window_pid || Self::helper_for_window(owner_pid,window_pid))) {
                item=None;
            }
        }
        let token=if let (Some(win),Some(reg))=(&window,&item){
            format!("{}:{}:{}",win["id"].as_i64().unwrap_or(0),win["window"].as_i64().unwrap_or(0),reg.epoch)
        }else{String::new()};
        Ok((window,item,token,output.unwrap_or(old_output),window_context))
    }
    async fn menu_layout(conn:&Connection,reg:&Registration)->Result<(u32,Value)> {
        let proxy=zbus::Proxy::new(conn,reg.owner.as_str(),reg.path.as_str(),MENU).await.map_err(|e|e.to_string())?;
        let response=tokio::time::timeout(Duration::from_secs(2),proxy.call::<_,_,(u32,Layout)>("GetLayout",&(0_i32,-1_i32,Vec::<String>::new())))
            .await.map_err(|_|"Menu layout timed out")?.map_err(|e|e.to_string())?;
        clean(&response.1,0,&mut 4096).map(|menu|(response.0,menu)).ok_or("Invalid menu layout".into())
    }
    async fn refresh(&self,conn:&Connection)->Result<()> {
        let (window,item,token,output_name,window_context)=self.current(conn).await?;
        let (held,snapshot,reg)= {let state=self.state.lock().await;
            let reg=state.held_token.split(':').collect::<Vec<_>>();
            let reg=if reg.len()==3{reg[1].parse::<u32>().ok().and_then(|id|state.registrations.get(&id).cloned()).filter(|item|Some(item.epoch)==reg[2].parse().ok())}else{None};
            (state.held_token.clone(),state.snapshot.clone(),reg)
        };
        if window.is_none() && !held.is_empty() && snapshot["token"]==held {
            if let Some(reg)=reg.filter(|reg|!reg.owner.is_empty()) {
                if Self::name_has_owner(conn,&reg.owner).await {
                    if let Ok((revision,menu))=Self::menu_layout(conn,&reg).await {
                        let mut state=self.state.lock().await;state.snapshot["menu"]=menu;state.snapshot["revision"]=json!(revision);state.last_refresh=Instant::now();
                        drop(state);self.publish().await;return Ok(());
                    }
                }
            }
            self.state.lock().await.held_token.clear();
        }
        let mut menu=None;let mut revision=None;let mut error=String::new();
        if let Some(reg)=&item {match Self::menu_layout(conn,reg).await {Ok((number,value))=>{revision=Some(number);menu=Some(value)},Err(reason)=>error=reason}}
        let mut state=self.state.lock().await;
        if window.is_some() && token!=state.held_token{state.held_token.clear();}
        state.output_name=output_name.clone();
        let title=window.as_ref().and_then(|win|win["name"].as_str()).unwrap_or("");
        let app=window.as_ref().map(|win|win["app_id"].as_str().or_else(||win["window_properties"]["class"].as_str()).unwrap_or("Application")).unwrap_or("Desktop");
        let context_app=window_context.as_ref().and_then(|(hint,_)|hint.clone());
        let context_window=window_context.as_ref().map(|(_,id)|*id);
        state.snapshot=json!({"token":token,"title":title,"supported":menu.is_some(),"menu":menu,"revision":revision,"error":error,
            "windowId":window.as_ref().map(|win|win["id"].clone()).unwrap_or(json!(0)),"outputName":output_name,
            "contextAppName":context_app,"contextWindowId":context_window,"mainWindowId":null,"appName":app});
        state.last_refresh=Instant::now();drop(state);self.publish().await;Ok(())
    }
    async fn guarded(&self,conn:&Connection,token:&str,read_only:bool)->Result<Registration> {
        let (window,item,current,_,_)=self.current(conn).await?;
        let state=self.state.lock().await;
        if read_only && window.is_none() && !token.is_empty() && token==state.held_token && state.snapshot["token"]==token {
            let parts=token.split(':').collect::<Vec<_>>();
            if parts.len()==3 {
                if let Some(reg)=parts[1].parse::<u32>().ok().and_then(|id|state.registrations.get(&id).cloned()) {
                    if Some(reg.epoch)==parts[2].parse().ok(){drop(state);
                        if Self::name_has_owner(conn,&reg.owner).await{return Ok(reg);}
                        return Err("Exporter disconnected".into());
                    }
                }
            }
        }
        if token.is_empty() || token!=current || state.snapshot["token"]!=token{return Err("Stale or unfocused menu".into());}
        drop(state);let item=item.ok_or("Stale or unfocused menu")?;
        if !Self::name_has_owner(conn,&item.owner).await{return Err("Exporter disconnected".into());}
        Ok(item)
    }
    async fn open(&self,conn:&Connection,token:&str,id:i32)->bool {
        let Ok(reg)=self.guarded(conn,token,true).await else{return false;};
        let node={let state=self.state.lock().await;actionable(&state.snapshot["menu"],id).cloned()};
        let Some(node)=node else{return false;};
        if node["enabled"]!=true || node["visible"]!=true || node["submenu"]!=true{return false;}
        let Ok(proxy)=zbus::Proxy::new(conn,reg.owner.as_str(),reg.path.as_str(),MENU).await else{return false;};
        if !matches!(tokio::time::timeout(Duration::from_secs(2),proxy.call::<_,_,bool>("AboutToShow",&(id,))).await,Ok(Ok(_))){return false;}
        let _=self.refresh(conn).await;true
    }
    async fn activate(&self,conn:&Connection,token:&str,id:i32,revision:Option<u32>)->bool {
        if self.guarded(conn,token,false).await.is_err(){return false;}
        if self.refresh(conn).await.is_err(){return false;}
        let Ok(reg)=self.guarded(conn,token,false).await else{return false;};
        let node={let state=self.state.lock().await;
            if revision.is_some_and(|revision|state.snapshot["revision"].as_u64()!=Some(revision as u64)){return false;}
            actionable(&state.snapshot["menu"],id).cloned()};
        let Some(node)=node else{return false;};
        if node["enabled"]!=true || node["visible"]!=true || node["submenu"]==true || node["separator"]==true{return false;}
        let Ok(proxy)=zbus::Proxy::new(conn,reg.owner.as_str(),reg.path.as_str(),MENU).await else{return false;};
        let result=tokio::time::timeout(Duration::from_secs(2),proxy.call::<_,_,()>("Event",&(id,"clicked",OwnedValue::from(0_i32),0_u32))).await;
        if !matches!(result,Ok(Ok(()))){return false;}
        self.state.lock().await.last_refresh=Instant::now()-Duration::from_secs(5);true
    }
    async fn activate_when_focused(&self,conn:Connection,token:String,id:i32,revision:Option<u32>) {
        let generation={let mut state=self.state.lock().await;state.pending_action+=1;state.pending_action};
        let runtime=self.clone();tokio::spawn(async move {
            let deadline=Instant::now()+Duration::from_millis(500);
            loop {
                if runtime.state.lock().await.pending_action!=generation{return;}
                match runtime.current(&conn).await {
                    Ok((Some(_),_,current,_,_))=>{
                        if current==token {let _=runtime.refresh(&conn).await;let _=runtime.activate(&conn,&token,id,revision).await;}
                        return;
                    }
                    Err(_)=>return,
                    _=>{},
                }
                if Instant::now()>=deadline{return;}
                tokio::time::sleep(Duration::from_millis(16)).await;
            }
        });
    }
    async fn input(&self,conn:&Connection,data:Value) {
        let action=data["action"].as_str().unwrap_or("");
        let token=data["token"].as_str().unwrap_or("");
        let id=data["id"].as_i64().and_then(|id|i32::try_from(id).ok()).unwrap_or(0);
        match action {
            "hold"=>{
                let mut state=self.state.lock().await;state.pending_action+=1;
                if state.snapshot["token"]==token{state.held_token=token.into();}
            }
            "release"=>{self.state.lock().await.held_token.clear();self.queue_refresh();}
            "open"=>{let _=self.open(conn,token,id).await;}
            "activate"=>{let _=self.activate(conn,token,id,None).await;}
            "activate_when_focused"=>{
                let revision=if data["revision"].is_null(){None}else{match data["revision"].as_u64().and_then(|value|u32::try_from(value).ok()){Some(value)=>Some(value),None=>return}};
                self.activate_when_focused(conn.clone(),token.into(),id,revision).await
            },
            _=>eprintln!("Invalid menu action"),
        }
    }
    async fn owner_changed(&self,conn:&Connection,name:&str,old:&str,new:&str) {
        if old.is_empty() || !new.is_empty(){return;}
        let removed={let mut state=self.state.lock().await;
            let windows=state.registrations.iter().filter_map(|(id,reg)|(reg.owner==name).then_some(*id)).collect::<Vec<_>>();
            for id in &windows{state.registrations.remove(id);}state.last_refresh=Instant::now()-Duration::from_secs(5);windows};
        for id in removed {let _=conn.emit_signal(None::<&str>,PATH,REG,"WindowUnregistered",&(id,)).await;}
        self.queue_refresh();
    }
}
#[interface(name="com.canonical.AppMenu.Registrar")]
impl Registrar {
    async fn register_window(&self,window:u32,path:OwnedObjectPath,#[zbus(header)] header:Header<'_>,#[zbus(connection)] conn:&Connection) {
        let Some(sender)=header.sender().map(|sender|sender.to_string()) else{return;};
        {let mut state=self.0.state.lock().await;state.epoch+=1;let epoch=state.epoch;
            state.registrations.insert(window,Registration{owner:sender.clone(),path:path.to_string(),epoch});
            state.last_refresh=Instant::now()-Duration::from_secs(5);}
        let _=conn.emit_signal(None::<&str>,PATH,REG,"WindowRegistered",&(window,sender,path)).await;
        self.0.queue_refresh();
    }
    async fn unregister_window(&self,window:u32,#[zbus(header)] header:Header<'_>,#[zbus(connection)] conn:&Connection) {
        let sender=header.sender().map(|sender|sender.to_string()).unwrap_or_default();
        let removed={let mut state=self.0.state.lock().await;
            if state.registrations.get(&window).is_some_and(|reg|reg.owner==sender){state.registrations.remove(&window);state.last_refresh=Instant::now()-Duration::from_secs(5);true}else{false}};
        if removed{let _=conn.emit_signal(None::<&str>,PATH,REG,"WindowUnregistered",&(window,)).await;}
        self.0.queue_refresh();
    }
    async fn get_menu_for_window(&self,window:u32)->(String,OwnedObjectPath) {
        let state=self.0.state.lock().await;
        let item=state.registrations.get(&window);
        (item.map(|reg|reg.owner.clone()).unwrap_or_default(),OwnedObjectPath::try_from(item.map(|reg|reg.path.as_str()).unwrap_or("/")).unwrap())
    }
    async fn get_menus(&self)->Vec<(u32,String,OwnedObjectPath)> {
        let state=self.0.state.lock().await;
        state.registrations.iter().filter_map(|(id,reg)|OwnedObjectPath::try_from(reg.path.as_str()).ok().map(|path|(*id,reg.owner.clone(),path))).collect()
    }
}
#[interface(name="org.lumendesktop.GlobalMenuPrototype")]
impl Ui {
    async fn state(&self)->String{self.0.state.lock().await.snapshot.to_string()}
    async fn open(&self,token:String,id:i32,#[zbus(connection)] conn:&Connection)->bool{self.0.open(conn,&token,id).await}
    async fn activate(&self,token:String,id:i32,#[zbus(connection)] conn:&Connection)->bool{self.0.activate(conn,&token,id,None).await}
}
async fn watch_sway(runtime:Runtime) {
    loop {
        let child=Command::new("swaymsg").args(["-m","-r","-t","subscribe","[\"window\",\"workspace\"]"])
            .stdout(Stdio::piped()).spawn();
        if let Ok(mut child)=child {
            if let Some(stdout)=child.stdout.take() {
                let mut lines=BufReader::new(stdout).lines();
                while let Ok(Some(line))=lines.next_line().await {
                    if let Ok(value)=serde_json::from_str::<Value>(&line) {
                        if ["focus","close","new","title","move","empty","init"].contains(&value["change"].as_str().unwrap_or("")) {runtime.queue_refresh();}
                    }
                }
            }
            let _=child.wait().await;
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}
async fn watch_bus(conn:Connection,runtime:Runtime) {
    if let Ok(bus)=zbus::Proxy::new(&conn,"org.freedesktop.DBus","/org/freedesktop/DBus","org.freedesktop.DBus").await {
        if let Ok(mut owners)=bus.receive_signal("NameOwnerChanged").await {
            let conn=conn.clone();let runtime=runtime.clone();
            tokio::spawn(async move {while let Some(message)=owners.next().await {
                if let Ok((name,old,new))=message.body().deserialize::<(String,String,String)>() {
                    runtime.owner_changed(&conn,&name,&old,&new).await;
                }
            }});
        }
    }
    if let Ok(rule)=MatchRule::builder().msg_type(Type::Signal).interface(MENU) {
        if let Ok(mut changes)=MessageStream::for_match_rule(rule.build(),&conn,Some(64)).await {
            while let Some(message)=changes.next().await {
                if message.is_ok(){runtime.queue_refresh();}
            }
        }
    }
}
async fn run()->Result<()> {
    if env::var("GLOBAL_MENU_ENABLED").as_deref()!=Ok("1") && env::var("GLOBAL_MENU_PROTOTYPE").as_deref()!=Ok("1") {
        return Err("Use the isolated prototype runner; live installation is not enabled.".into());
    }
    let (events,mut receiver)=mpsc::unbounded_channel();
    let runtime=Runtime{state:Arc::new(Mutex::new(State::new())),refresh:Arc::new(AtomicBool::new(false)),events};
    let conn=zbus::connection::Builder::session().map_err(|e|e.to_string())?.name(REG).map_err(|e|e.to_string())?
        .serve_at(PATH,Registrar(runtime.clone())).map_err(|e|e.to_string())?
        .serve_at(PATH,Ui(runtime.clone())).map_err(|e|e.to_string())?.build().await.map_err(|e|e.to_string())?;
    tokio::spawn(watch_sway(runtime.clone()));tokio::spawn(watch_bus(conn.clone(),runtime.clone()));
    let input=runtime.events.clone();tokio::spawn(async move {
        let mut lines=BufReader::new(tokio::io::stdin()).lines();
        while let Ok(Some(line))=lines.next_line().await {
            match serde_json::from_str::<Value>(&line) {
                Ok(value)=>{let _=input.send(Event::Input(value));},
                Err(error)=>eprintln!("{error}"),
            }
        }
        let _=input.send(Event::Stop);
    });
    runtime.publish().await;runtime.queue_refresh();
    let mut poll=tokio::time::interval(Duration::from_secs(2));
    loop {tokio::select! {
        event=receiver.recv()=>match event {
            Some(Event::Refresh)=>{runtime.refresh.store(false,Ordering::SeqCst);if let Err(error)=runtime.refresh(&conn).await{eprintln!("{error}");}},
            Some(Event::Input(value))=>runtime.input(&conn,value).await,
            Some(Event::Stop)|None=>break,
        },
        _=poll.tick()=>if let Err(error)=runtime.refresh(&conn).await{eprintln!("{error}");},
    }}
    Ok(())
}
#[tokio::main]
async fn main(){if let Err(error)=run().await{eprintln!("{error}");std::process::exit(1);}}
