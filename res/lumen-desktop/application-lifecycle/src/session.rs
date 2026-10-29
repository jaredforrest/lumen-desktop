use crate::{Lifecycle,Result};
use serde_json::{Value,json};
use std::collections::HashMap;
use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::PathBuf;

#[derive(Clone)]
pub struct Prepared{pub app:String,pub identity:Value,pub owner:String}
pub trait QuitProtocol{
    fn prepare(&self,lifecycle:&mut Lifecycle,app:&str)->Result<Prepared>;
    fn dispatch(&self,lifecycle:&mut Lifecycle,prepared:&Prepared)->Result<()>;
}
fn random_token()->Result<String>{Ok(fs::read_to_string("/proc/sys/kernel/random/uuid").map_err(|error|error.to_string())?.trim().replace('-',""))}
fn shell_process(pid:u32)->Option<(u64,u64,u64)>{
    if pid==0{return None;}
    let root=PathBuf::from(format!("/proc/{pid}"));
    if fs::metadata(&root).ok()?.uid()!=unsafe{libc::getuid()}{return None;}
    let exe=fs::canonicalize(root.join("exe")).ok()?;
    if !exe.file_name()?.to_string_lossy().contains("quickshell"){return None;}
    let info=fs::metadata(&exe).ok()?;
    let raw=fs::read_to_string(root.join("stat")).ok()?;
    let start=raw.rsplit_once(')')?.1.split_whitespace().nth(19)?.parse().ok()?;
    Some((start,info.dev(),info.ino()))
}
pub struct SessionEnd{
    pub token:String,pub phase:String,pub action:String,pub error:String,
    entries:Vec<Prepared>,current:Option<Prepared>,epoch:Option<String>,
    identities:HashMap<u32,Value>,windows:HashMap<u64,(u32,String)>,checking:Vec<String>,
    shell_pid:u32,shell_identity:Option<(u64,u64,u64)>,
}
impl SessionEnd{
    pub fn new(shell_pid:u32)->Self{
        Self{token:String::new(),phase:"idle".into(),action:String::new(),error:String::new(),entries:Vec::new(),current:None,epoch:None,identities:HashMap::new(),windows:HashMap::new(),checking:Vec::new(),shell_pid,shell_identity:shell_process(shell_pid)}
    }
    pub fn cancel(&mut self){self.token.clear();self.phase="idle".into();self.action.clear();self.error.clear();self.entries.clear();self.current=None;self.epoch=None;self.identities.clear();self.windows.clear();self.checking.clear();}
    pub fn state(&self)->Value{json!({"token":self.token,"phase":self.phase,"action":self.action,"error":self.error,"waiting":self.current.as_ref().map(|item|item.app.as_str()).unwrap_or(""),"remaining":self.entries.len()+usize::from(self.current.is_some())})}
    pub fn fail(&mut self,message:&str){self.phase="blocked".into();self.error=message.into();self.entries.clear();self.current=None;}
    fn external<'a>(&self,nodes:&'a[Value])->Vec<&'a Value>{
        let shell_valid=self.shell_identity.is_some()&&shell_process(self.shell_pid)==self.shell_identity;
        nodes.iter().filter(|node|!(shell_valid&&node["pid"].as_u64()==Some(self.shell_pid as u64)&&node["app_id"]=="org.quickshell")).collect()
    }
    pub fn begin(&mut self,action:&str,nodes:&[Value],lifecycle:&mut Lifecycle,protocol:&impl QuitProtocol)->Result<Value>{
        self.cancel();
        if !matches!(action,"logout"|"reboot"|"poweroff"){return Err("Unsupported session action".into());}
        self.action=action.into();self.token=random_token()?;self.epoch=lifecycle.epoch.clone();self.phase="checking".into();
        let result=(||->Result<()>{
            let external=self.external(nodes);
            for node in &external{
                let id=node["id"].as_u64().ok_or("Invalid window")?;
                let pid=node["pid"].as_u64().filter(|pid|*pid<=u32::MAX as u64).ok_or("Invalid process")? as u32;
                let app=node["app_id"].as_str().ok_or("Invalid application")?.to_owned();
                self.windows.insert(id,(pid,app));
                if node["shell"]!="xdg_shell"||!node["app_id"].as_str().is_some_and(|app|!app.is_empty()){
                    return Err("An open application has no supported native cooperative protocol".into());
                }
            }
            if lifecycle.processes.values().any(|process|process.apps.len()!=1){return Err("Shared process ownership is ambiguous".into());}
            if external.iter().any(|node|{
                let Some(pid)=node["pid"].as_u64().filter(|pid|*pid<=u32::MAX as u64)else{return true;};
                let Some(app)=node["app_id"].as_str()else{return true;};
                !lifecycle.processes.get(&(pid as u32)).is_some_and(|process|process.apps.contains(app))
            }){return Err("An open application cannot be identified safely".into());}
            self.identities=lifecycle.processes.iter().map(|(pid,process)|(*pid,process.identity.clone())).collect();
            self.checking=lifecycle.rows().iter().filter_map(|row|row["appId"].as_str().map(str::to_owned)).collect();
            self.tick(nodes,lifecycle,protocol);
            Ok(())
        })();
        if result.is_err(){self.fail("Cooperative Quit is unavailable or ownership is ambiguous for an open application. No further applications will be asked to quit. Use application-owned Quit controls, then try again.");}
        Ok(self.state())
    }
    fn validate(&self,nodes:&[Value],lifecycle:&Lifecycle)->Result<()>{
        if lifecycle.epoch!=self.epoch{return Err("The compositor session changed. Start again.".into());}
        for (pid,process) in &lifecycle.processes{if self.identities.get(pid)!=Some(&process.identity){return Err("An application started or changed. Start again.".into());}}
        for node in self.external(nodes){
            let pid=node["pid"].as_u64().filter(|pid|*pid<=u32::MAX as u64).map(|pid|pid as u32).unwrap_or(0);
            let app=node["app_id"].as_str().unwrap_or("");
            let id=node["id"].as_u64().unwrap_or(0);
            if self.windows.get(&id).is_some_and(|(old_pid,old_app)|*old_pid==pid&&old_app==app){continue;}
            let relation=&node["lumen_relationships"];
            let parent=relation["parent"].as_u64().unwrap_or(0);
            if self.windows.get(&parent).is_some_and(|(old_pid,old_app)|*old_pid==pid&&old_app==app){continue;}
            if relation["modal"]==true&&lifecycle.processes.get(&pid).is_some_and(|process|process.apps.len()==1&&process.apps.contains(app)){continue;}
            return Err("A new application window appeared. Start again after resolving it.".into());
        }
        Ok(())
    }
    pub fn tick(&mut self,nodes:&[Value],lifecycle:&mut Lifecycle,protocol:&impl QuitProtocol){
        if !matches!(self.phase.as_str(),"checking"|"waiting"|"ready"){return;}
        let result=(||->Result<()>{
            self.validate(nodes,lifecycle)?;
            if self.phase=="checking"{
                if !self.checking.is_empty(){
                    let app=self.checking.remove(0);
                    match protocol.prepare(lifecycle,&app){Ok(entry)=>self.entries.push(entry),Err(_)=>{self.fail(&format!("Cooperative Quit is unavailable for {app}. Quit it using its own controls, then try again."));return Ok(());}}
                    if !self.checking.is_empty(){return Ok(());}
                }
                self.phase="waiting".into();
            }
            if let Some(current)=&self.current{
                if lifecycle.processes.values().any(|process|process.identity==current.identity){return Ok(());}
                self.current=None;
            }
            while !self.entries.is_empty(){
                let entry=self.entries.remove(0);
                if !lifecycle.processes.values().any(|process|process.identity==entry.identity){continue;}
                protocol.dispatch(lifecycle,&entry)?;self.current=Some(entry);return Ok(());
            }
            self.phase="ready".into();Ok(())
        })();
        if result.is_err(){self.fail("Applications or their windows changed, or a cooperative Quit failed. The session action was stopped. Resolve applications and start again.");}
    }
    pub fn authorize(&mut self,token:&str,nodes:&[Value],lifecycle:&mut Lifecycle,protocol:&impl QuitProtocol)->Result<Value>{
        if token.is_empty()||token!=self.token||self.phase!="ready"{return Err("Session confirmation is no longer ready".into());}
        self.tick(nodes,lifecycle,protocol);
        if self.phase!="ready"||!lifecycle.processes.is_empty(){return Err("Applications remain open; the session action was stopped".into());}
        let action=self.action.clone();let token=self.token.clone();self.cancel();Ok(json!({"action":action,"token":token}))
    }
}
