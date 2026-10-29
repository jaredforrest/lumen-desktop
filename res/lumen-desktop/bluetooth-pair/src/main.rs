//! A single-device BlueZ pairing agent with explicit UI approval.
use serde_json::{Value, json};
use std::env;
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::sync::{Notify, oneshot};
use zbus::{Connection, DBusError, interface, zvariant::OwnedObjectPath};

const AGENT_PATH:&str="/org/lumendesktop/Pairing";
#[derive(Debug, DBusError)]
#[zbus(prefix="org.bluez.Error")]
enum BluezError {
    #[zbus(error)] ZBus(zbus::Error),
    Rejected(String),
}
#[derive(Clone,Copy,Debug,Eq,PartialEq)]
enum Kind {Pin,Passkey,Confirm,Authorize}
impl Kind {fn label(self)->&'static str {match self {Self::Pin=>"pin",Self::Passkey=>"passkey",Self::Confirm=>"confirm",Self::Authorize=>"authorize"}}}
#[derive(Debug)]
enum Answer {Pin(String),Passkey(u32),Accept,Reject}
struct Pending {kind:Kind, reply:oneshot::Sender<Answer>}
struct State {device:String,pending:Option<Pending>}
impl State {
    fn new(device:String)->Self {Self{device,pending:None}}
    fn request(&mut self, device:&str, kind:Kind)->Result<oneshot::Receiver<Answer>,BluezError> {
        if device!=self.device || self.pending.is_some() {
            return Err(BluezError::Rejected("Unexpected pairing request".into()));
        }
        let (reply,receive)=oneshot::channel();self.pending=Some(Pending{kind,reply});Ok(receive)
    }
    fn respond(&mut self, data:&Value)->Result<(),String> {
        let Some(pending)=self.pending.as_ref() else{return Ok(());};
        let answer=if data["accept"]!=true {Answer::Reject}else{
            let value=data["value"].as_str().unwrap_or("");
            match pending.kind {
                Kind::Pin=>{
                    if value.chars().count()<1 || value.chars().count()>16 {return Err("Enter 1–16 characters".into());}
                    Answer::Pin(value.to_owned())
                }
                Kind::Passkey=>{
                    if value.is_empty() || value.len()>6 || !value.bytes().all(|b|b.is_ascii_digit()) {return Err("Enter a passkey of up to six digits".into());}
                    Answer::Passkey(value.parse().map_err(|_|"Enter a passkey of up to six digits")?)
                }
                _=>Answer::Accept,
            }
        };
        if let Some(pending)=self.pending.take(){let _=pending.reply.send(answer);}
        Ok(())
    }
    fn cancel(&mut self) {if let Some(pending)=self.pending.take(){let _=pending.reply.send(Answer::Reject);}}
}
#[derive(Clone)]
struct Agent {state:Arc<Mutex<State>>, released:Arc<Notify>}
fn emit(value:Value) {println!("{value}");}
impl Agent {
    async fn ask(&self,device:OwnedObjectPath,kind:Kind,code:Option<String>)->Result<Answer,BluezError> {
        let receive=self.state.lock().map_err(|_|BluezError::Rejected("Pairing unavailable".into()))?.request(device.as_str(),kind)?;
        let mut prompt=json!({"event":"prompt","kind":kind.label()});
        if let Some(code)=code {prompt["code"]=json!(code);}
        emit(prompt);
        let answer=receive.await.map_err(|_|BluezError::Rejected("Pairing interrupted".into()))?;
        if matches!(answer,Answer::Reject){return Err(BluezError::Rejected("Pairing rejected".into()));}
        Ok(answer)
    }
    fn display(&self,device:&OwnedObjectPath,code:String,entered:Option<u16>) {
        if self.state.lock().is_ok_and(|state|state.device==device.as_str()) {
            let mut prompt=json!({"event":"prompt","kind":"display","code":code});
            if let Some(entered)=entered {prompt["entered"]=json!(entered);}
            emit(prompt);
        }
    }
}
#[interface(name="org.bluez.Agent1")]
impl Agent {
    #[zbus(name="RequestPinCode")]
    async fn request_pin_code(&self,device:OwnedObjectPath)->Result<String,BluezError> {
        match self.ask(device,Kind::Pin,None).await? {Answer::Pin(value)=>Ok(value),_=>Err(BluezError::Rejected("Invalid PIN reply".into()))}
    }
    #[zbus(name="RequestPasskey")]
    async fn request_passkey(&self,device:OwnedObjectPath)->Result<u32,BluezError> {
        match self.ask(device,Kind::Passkey,None).await? {Answer::Passkey(value)=>Ok(value),_=>Err(BluezError::Rejected("Invalid passkey reply".into()))}
    }
    #[zbus(name="RequestConfirmation")]
    async fn request_confirmation(&self,device:OwnedObjectPath,passkey:u32)->Result<(),BluezError> {
        self.ask(device,Kind::Confirm,Some(format!("{passkey:06}"))).await.map(|_|())
    }
    #[zbus(name="RequestAuthorization")]
    async fn request_authorization(&self,device:OwnedObjectPath)->Result<(),BluezError> {
        self.ask(device,Kind::Authorize,None).await.map(|_|())
    }
    #[zbus(name="AuthorizeService")]
    async fn authorize_service(&self,device:OwnedObjectPath,_uuid:String)->Result<(),BluezError> {
        self.ask(device,Kind::Authorize,None).await.map(|_|())
    }
    #[zbus(name="DisplayPinCode")]
    fn display_pin_code(&self,device:OwnedObjectPath,pin:String){self.display(&device,pin,None);}
    #[zbus(name="DisplayPasskey")]
    fn display_passkey(&self,device:OwnedObjectPath,passkey:u32,entered:u16){self.display(&device,format!("{passkey:06}"),Some(entered));}
    #[zbus(name="Cancel")]
    fn cancel(&self){if let Ok(mut state)=self.state.lock(){state.cancel();}emit(json!({"event":"cancelled"}));}
    #[zbus(name="Release")]
    fn release(&self){if let Ok(mut state)=self.state.lock(){state.cancel();}self.released.notify_one();}
}
fn valid_device(path:&str)->bool {
    let Some(rest)=path.strip_prefix("/org/bluez/hci") else{return false;};
    let count=rest.bytes().take_while(u8::is_ascii_digit).count();
    if count==0{return false;}
    rest[count..].strip_prefix("/dev_").is_some_and(|id|id.len()==17 && id.bytes().all(|b|b.is_ascii_hexdigit() || b==b'_'))
}
async fn cancel_pair(conn:&Connection,device:&str) {
    if let Ok(proxy)=zbus::Proxy::new(conn,"org.bluez",device,"org.bluez.Device1").await {
        let _=proxy.call::<_,_,()>("CancelPairing",&()).await;
    }
}
async fn run(device:&str)->Result<(),String> {
    if !valid_device(device){return Err("Invalid Bluetooth device".into());}
    let conn=zbus::connection::Builder::system().map_err(|e|e.to_string())?
        .serve_at(AGENT_PATH,Agent{state:Arc::new(Mutex::new(State::new(device.into()))),released:Arc::new(Notify::new())})
        .map_err(|e|e.to_string())?.build().await.map_err(|e|e.to_string())?;
    let agent=conn.object_server().interface::<_,Agent>(AGENT_PATH).await.map_err(|e|e.to_string())?;
    let state=agent.get().await.state.clone();let released=agent.get().await.released.clone();
    let manager=zbus::Proxy::new(&conn,"org.bluez","/org/bluez","org.bluez.AgentManager1").await.map_err(|e|e.to_string())?;
    let path=OwnedObjectPath::try_from(AGENT_PATH).map_err(|e|e.to_string())?;
    manager.call::<_,_,()>("RegisterAgent",&(path,"KeyboardDisplay")).await.map_err(|e|e.to_string())?;
    let outcome=async {
        let remote=zbus::Proxy::new(&conn,"org.bluez",device,"org.bluez.Device1").await.map_err(|e|e.to_string())?;
        let pair=remote.call::<_,_,()>("Pair",&());tokio::pin!(pair);
        let mut lines=BufReader::new(tokio::io::stdin()).lines();
        let deadline=tokio::time::sleep(std::time::Duration::from_secs(90));tokio::pin!(deadline);
        loop {tokio::select! {
            result=&mut pair=>{
                match result {Ok(())=>emit(json!({"event":"paired"})),Err(error)=>emit(json!({"error":error.to_string()}))}
                break;
            }
            line=lines.next_line()=>match line.map_err(|e|e.to_string())? {
                Some(line)=>match serde_json::from_str::<Value>(&line) {
                    Ok(data) if data["action"]=="cancel"=>{let _=tokio::time::timeout(std::time::Duration::from_secs(3),cancel_pair(&conn,device)).await;break;}
                    Ok(data)=>{
                        if let Err(error)=state.lock().map_err(|_|"Pairing unavailable".to_owned())?.respond(&data) {emit(json!({"error":error}));}
                    }
                    Err(error)=>emit(json!({"error":error.to_string()})),
                },
                None=>{let _=tokio::time::timeout(std::time::Duration::from_secs(3),cancel_pair(&conn,device)).await;break;}
            },
            _=&mut deadline=>{let _=tokio::time::timeout(std::time::Duration::from_secs(3),cancel_pair(&conn,device)).await;emit(json!({"error":"Pairing timed out"}));break;}
            _=released.notified()=>break,
        }}
        Ok::<(),String>(())
    }.await;
    let path=OwnedObjectPath::try_from(AGENT_PATH).map_err(|e|e.to_string())?;
    let _=manager.call::<_,_,()>("UnregisterAgent",&(path,)).await;
    outcome
}
#[tokio::main]
async fn main() {
    let device=env::args().nth(1).unwrap_or_default();
    if let Err(error)=run(&device).await {emit(json!({"error":error}));std::process::exit(1);}
}
#[cfg(test)] mod tests {
    use super::*;
    #[tokio::test] async fn requires_confirmation_and_rejects_other_device() {
        let device="/org/bluez/hci0/dev_00_11_22_33_44_55";
        assert!(valid_device(device));assert!(!valid_device("/wrong"));
        let mut state=State::new(device.into());
        let receive=state.request(device,Kind::Confirm).unwrap();
        assert!(state.request("/wrong",Kind::Confirm).is_err());
        assert!(state.request(device,Kind::Confirm).is_err());
        state.respond(&json!({"accept":true})).unwrap();
        assert!(matches!(receive.await.unwrap(),Answer::Accept));
        assert!(state.pending.is_none());
    }
    #[tokio::test] async fn invalid_code_is_retryable_and_cancel_is_explicit() {
        let mut state=State::new("device".into());
        let receive=state.request("device",Kind::Passkey).unwrap();
        assert!(state.respond(&json!({"accept":true,"value":"not-a-number"})).is_err());
        assert!(state.pending.is_some());
        state.respond(&json!({"accept":true,"value":"000123"})).unwrap();
        assert!(matches!(receive.await.unwrap(),Answer::Passkey(123)));
        let receive=state.request("device",Kind::Pin).unwrap();
        state.respond(&json!({"accept":false})).unwrap();
        assert!(matches!(receive.await.unwrap(),Answer::Reject));
    }
}
