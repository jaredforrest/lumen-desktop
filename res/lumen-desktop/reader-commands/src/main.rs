//! Typed Orca commands, pinned to one local owner and native unlock generation.
mod lock;
use lock::LockAuthority;
use serde_json::{Value, json};
use std::env;
use std::fs;
use std::os::unix::fs::MetadataExt;
use std::time::Duration;
use tokio::time::{Instant, timeout};
use zbus::Connection;

type Result<T> = std::result::Result<T, String>;
const SERVICE:&str="org.gnome.Orca.Service";
const INTERFACE:&str="org.gnome.Orca.Module";
const BASE:&str="/org/gnome/Orca/Service/";

fn monotonic() -> f64 {
    let mut stamp=libc::timespec{tv_sec:0,tv_nsec:0};
    unsafe {libc::clock_gettime(libc::CLOCK_MONOTONIC,&mut stamp);}
    stamp.tv_sec as f64+stamp.tv_nsec as f64/1e9
}
fn process_start(pid:u32) -> Result<u64> {
    if pid==0 {return Err("reader_unavailable".into());}
    let path=format!("/proc/{pid}");
    if fs::metadata(&path).map_err(|_|"reader_unavailable")?.uid()!=unsafe{libc::geteuid()} {
        return Err("reader_unavailable".into());
    }
    let stat=fs::read_to_string(format!("{path}/stat")).map_err(|_|"reader_unavailable")?;
    stat.rsplit_once(") ").and_then(|(_,tail)|tail.split_whitespace().nth(19))
        .and_then(|value|value.parse().ok()).ok_or("reader_unavailable".into())
}
fn command_kind(value:&str) -> Option<(&'static str,&'static str)> {
    match value {"where-am-i"=>Some(("WhereAmIPresenter","WhereAmIBasic")),
        "read-all"=>Some(("SayAllPresenter","SayAll")),
        "stop"=>Some(("SpeechManager","InterruptSpeech")),_=>None}
}
#[derive(PartialEq,Eq)]
struct Owner {name:String,uid:u32,pid:u32,start:u64}
struct Reader {connection:Connection,authority:LockAuthority,deadline:Instant,dispatched:bool}
impl Reader {
    fn remaining(&self) -> Result<Duration> {
        let remaining=self.deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {Err("provider_timeout".into())} else {Ok(remaining)}
    }
    async fn owner(&self) -> Result<Owner> {
        let proxy=zbus::Proxy::new(&self.connection,"org.freedesktop.DBus","/org/freedesktop/DBus","org.freedesktop.DBus")
            .await.map_err(|_|"reader_unavailable")?;
        let name:String=timeout(self.remaining()?,proxy.call("GetNameOwner",&(SERVICE,)))
            .await.map_err(|_|"provider_timeout")?.map_err(|_|"reader_unavailable")?;
        if !name.starts_with(':') {return Err("reader_unavailable".into());}
        let uid:u32=timeout(self.remaining()?,proxy.call("GetConnectionUnixUser",&(name.as_str(),)))
            .await.map_err(|_|"provider_timeout")?.map_err(|_|"reader_unavailable")?;
        let pid:u32=timeout(self.remaining()?,proxy.call("GetConnectionUnixProcessID",&(name.as_str(),)))
            .await.map_err(|_|"provider_timeout")?.map_err(|_|"reader_unavailable")?;
        if uid!=unsafe{libc::geteuid()} {return Err("reader_unavailable".into());}
        Ok(Owner {name,uid,pid,start:process_start(pid)?})
    }
    fn unlocked(&mut self) -> Result<String> {
        if self.authority.snapshot()!=json!({"known":true,"locked":false}) {
            return Err("session_unavailable".into());
        }
        self.authority.signature().ok_or("session_unavailable".into())
    }
    async fn execute(&mut self,command:&str) -> Result<Value> {
        let (module,action)=command_kind(command).ok_or("unsupported_command")?;
        let lock=if command=="stop" {None}else{Some(self.unlocked()?)};
        let captured=self.owner().await?;
        let path=format!("{BASE}{module}");
        let proxy=zbus::Proxy::new(&self.connection,captured.name.as_str(),path.as_str(),INTERFACE)
            .await.map_err(|_|"reader_unavailable")?;
        let available:Vec<(String,String)>=timeout(self.remaining()?,proxy.call("ListCommands",&()))
            .await.map_err(|_|"provider_timeout")?.map_err(|_|"reader_unavailable")?;
        if !available.iter().any(|(name,_)|name==action) {return Err("unsupported_command".into());}
        if self.owner().await?!=captured || lock.as_ref().is_some_and(|before|self.unlocked().as_ref()!=Ok(before)) {
            return Err("source_changed".into());
        }
        self.dispatched=true;
        let accepted:bool=timeout(self.remaining()?,proxy.call("ExecuteCommand",&(action,false)))
            .await.map_err(|_|"provider_timeout")?.map_err(|_|"reader_unavailable")?;
        if self.owner().await?!=captured || lock.as_ref().is_some_and(|before|self.unlocked().as_ref()!=Ok(before)) {
            return Err("dispatch_state_changed".into());
        }
        if !accepted {return Err("command_rejected".into());}
        Ok(json!({"accepted":true,"command":command,"dispatched":true}))
    }
}
async fn run(args:&[String]) -> (Value,i32) {
    if args.len()!=1 && args.len()!=2 {return (json!({"accepted":false,"reason":"unsupported_command","dispatched":false}),1);}
    let (enabled,command)=if args.len()==2 && args[0]=="--enabled" {(true,args[1].as_str())}
        else if args.len()==1 {(false,args[0].as_str())}
        else {return (json!({"accepted":false,"reason":"unsupported_command","dispatched":false}),1);};
    if command_kind(command).is_none() {return (json!({"accepted":false,"reason":"unsupported_command","dispatched":false}),1);}
    if !enabled {return (json!({"accepted":false,"reason":"disabled","dispatched":false}),1);}
    let address=env::var("DBUS_SESSION_BUS_ADDRESS").unwrap_or_default();
    if !address.starts_with("unix:") || address.contains(';') {
        return (json!({"accepted":false,"reason":"reader_unavailable","dispatched":false}),1);
    }
    let Ok(connection)=Connection::session().await else {
        return (json!({"accepted":false,"reason":"reader_unavailable","dispatched":false}),1);
    };
    let mut client=Reader {connection,authority:LockAuthority::new(),deadline:Instant::now()+Duration::from_millis(2500),dispatched:false};
    match client.execute(command).await {
        Ok(value)=>(value,0),
        Err(reason)=>{
            let reason=if client.dispatched && !matches!(reason.as_str(),"dispatch_state_changed" | "command_rejected") {
                "dispatch_unknown".to_owned()
            } else {reason};
            (json!({"accepted":false,"reason":reason,"dispatched":client.dispatched}),1)
        }
    }
}
#[tokio::main]
async fn main() {
    let (value,code)=run(&env::args().skip(1).collect::<Vec<_>>()).await;
    println!("{value}");std::process::exit(code);
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn typed_allowlist_is_closed() {
        assert_eq!(command_kind("where-am-i"),Some(("WhereAmIPresenter","WhereAmIBasic")));
        assert_eq!(command_kind("read-all"),Some(("SayAllPresenter","SayAll")));
        assert_eq!(command_kind("stop"),Some(("SpeechManager","InterruptSpeech")));
        assert!(command_kind("arbitrary").is_none());
    }
}
