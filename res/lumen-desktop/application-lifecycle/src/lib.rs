use serde_json::{Value,json};
use sha2::{Digest,Sha256};
use std::collections::{BTreeMap,BTreeSet,HashMap,HashSet};
use std::fs::{self,File,OpenOptions};
use std::io::{Read,Write};
use std::os::fd::{AsRawFd,FromRawFd,OwnedFd};
use std::os::unix::fs::{DirBuilderExt,FileTypeExt,MetadataExt,OpenOptionsExt};
use std::os::unix::net::UnixStream;
use std::path::{Path,PathBuf};
pub mod session;
pub mod cooperative;
pub mod mru;
pub mod windows;
pub mod inventory;
pub mod events;

pub type Result<T>=std::result::Result<T,String>;
pub const MAX_PROCESSES:usize=512;
pub const MAX_APP_IDS:usize=32;
pub const MAX_FRAME:usize=16*1024*1024;

pub fn epoch(path:&Path)->Result<String>{
    let info=fs::symlink_metadata(path).map_err(|error|error.to_string())?;
    if !info.file_type().is_socket()||info.uid()!=unsafe{libc::getuid()}{return Err("Compositor endpoint is not an owned socket".into());}
    let raw=format!("{}:{}:{}",info.dev(),info.ino(),info.ctime() as i128*1_000_000_000+info.ctime_nsec() as i128);
    Ok(format!("{:x}",Sha256::digest(raw.as_bytes()))[..24].to_owned())
}
pub fn send(stream:&mut UnixStream,kind:u32,value:&str)->Result<()>{
    if value.len()>MAX_FRAME{return Err("Compositor request exceeds limit".into());}
    let mut packet=b"i3-ipc".to_vec();packet.extend_from_slice(&(value.len() as u32).to_le_bytes());packet.extend_from_slice(&kind.to_le_bytes());packet.extend_from_slice(value.as_bytes());
    stream.write_all(&packet).map_err(|error|error.to_string())
}
pub fn receive(stream:&mut UnixStream)->Result<(u32,Value)>{
    let mut header=[0_u8;14];stream.read_exact(&mut header).map_err(|error|error.to_string())?;
    if &header[..6]!=b"i3-ipc"{return Err("Invalid compositor protocol".into());}
    let size=u32::from_le_bytes(header[6..10].try_into().map_err(|_|"Invalid compositor protocol")?) as usize;
    if size>MAX_FRAME{return Err("Compositor response exceeds limit".into());}
    let kind=u32::from_le_bytes(header[10..14].try_into().map_err(|_|"Invalid compositor protocol")?);
    let mut bytes=vec![0_u8;size];stream.read_exact(&mut bytes).map_err(|error|error.to_string())?;
    Ok((kind,serde_json::from_slice(&bytes).map_err(|_|"Invalid compositor reply")?))
}
pub fn window_nodes(tree:&Value)->Result<Vec<Value>>{
    let mut result=Vec::new();let mut pending=vec![tree];let mut seen=HashSet::new();
    while let Some(node)=pending.pop(){
        if seen.len()>100000{return Err("Compositor tree exceeds the supported limit".into());}
        let obj=node.as_object().ok_or("Invalid compositor tree")?;
        if let Some(id)=obj.get("id").filter(|value|!value.is_null()){
            let id=id.as_i64().ok_or("Invalid compositor identity")?;
            if !seen.insert(id){return Err("Invalid compositor identity".into());}
        }
        for key in ["nodes","floating_nodes"]{if let Some(value)=obj.get(key){pending.extend(value.as_array().ok_or("Invalid compositor children")?);}}
        if !node["app_id"].is_null()||!node["window"].is_null()||node["shell"]=="xdg_shell"{result.push(node.clone());}
    }
    Ok(result)
}
pub fn process_identity(pid:u32)->Result<Value>{
    identify_process(pid,false)
}
pub fn source_process_identity(pid:u32)->Result<Value>{
    identify_process(pid,true)
}
fn identify_process(pid:u32,source_only:bool)->Result<Value>{
    if pid<2{return Err("Invalid application process".into());}
    let root=PathBuf::from(format!("/proc/{pid}"));
    if fs::metadata(&root).map_err(|error|error.to_string())?.uid()!=unsafe{libc::getuid()}{return Err("Application process has an unexpected owner".into());}
    let raw=fs::read_to_string(root.join("stat")).map_err(|error|error.to_string())?;
    let fields=raw.rsplit_once(')').ok_or("Invalid process identity")?.1.split_whitespace().collect::<Vec<_>>();
    if fields.len()<20||fields[0]=="Z"{return Err("Application process has already exited".into());}
    let start=fields[19].parse::<u64>().map_err(|_|"Invalid process identity")?;
    let status=fs::read_to_string(root.join("status")).map_err(|error|error.to_string())?;
    let uids=status.lines().find_map(|line|line.strip_prefix("Uid:")).map(|line|line.split_whitespace().collect::<Vec<_>>()).unwrap_or_default();
    if uids.len()!=4||uids.iter().any(|uid|uid.parse::<u32>().ok()!=Some(unsafe{libc::getuid()})){return Err("Privileged application processes cannot be observed here".into());}
    let executable=root.join("exe");let resolved=fs::canonicalize(&executable).map_err(|error|error.to_string())?;
    if !source_only&&resolved.file_name().is_some_and(|name|name.to_string_lossy().contains("quickshell")){return Err("The desktop shell is not an external application".into());}
    let info=fs::metadata(&executable).map_err(|error|error.to_string())?;
    Ok(json!({"pid":pid,"start":start,"device":info.dev(),"inode":info.ino()}))
}
fn pidfd(pid:u32)->Result<OwnedFd>{
    let fd=unsafe{libc::syscall(libc::SYS_pidfd_open,pid as libc::pid_t,0)};
    if fd<0{return Err(std::io::Error::last_os_error().to_string());}
    Ok(unsafe{OwnedFd::from_raw_fd(fd as i32)})
}
fn exited(fd:&OwnedFd)->bool{
    let mut entry=libc::pollfd{fd:fd.as_raw_fd(),events:libc::POLLIN,revents:0};
    (unsafe{libc::poll(&mut entry,1,0)})!=0
}
pub fn valid_app_id(value:&str)->bool{!value.is_empty()&&value.len()<=256&&value!="org.quickshell"&&!value.chars().any(|ch|ch<' '||ch=='\u{7f}')}
pub fn valid_identity(value:&Value)->bool{
    value.as_object().is_some_and(|row|row.len()==4&&["pid","start","device","inode"].iter().all(|key|row.get(*key).and_then(Value::as_u64).is_some())&&row["pid"].as_u64().is_some_and(|pid|pid>=2&&pid<=u32::MAX as u64))
}
pub struct Process{pub identity:Value,pub handle:OwnedFd,pub apps:BTreeSet<String>}
pub struct Lifecycle{pub processes:BTreeMap<u32,Process>,pub windows:HashMap<(u32,String),Vec<Value>>,pub epoch:Option<String>,pub activation_history:mru::History,pub window_history:windows::History,pub current_source_focus:Value,pub native_coverage_complete:bool,pub native_guard_available:bool,pub shell_owner:Option<Value>,pub inventory:inventory::Inventory,focus_witnesses:HashMap<i64,Option<(u32,Value,String)>>}
impl Default for Lifecycle{fn default()->Self{Self{processes:BTreeMap::new(),windows:HashMap::new(),epoch:None,activation_history:mru::History::default(),window_history:windows::History::default(),current_source_focus:Value::Null,native_coverage_complete:false,native_guard_available:false,shell_owner:None,inventory:inventory::Inventory::default(),focus_witnesses:HashMap::new()}}}
impl Lifecycle{
    pub fn native_observation(&self)->Option<Value>{self.inventory.observation(self).ok()}
    pub fn require_inventory(&self,witness:&Value)->Result<()>{self.inventory.require(self,witness)}
    pub fn bind_shell_owner(&mut self,pid:u32){self.shell_owner=source_process_identity(pid).ok();}
    pub fn close(&mut self){self.processes.clear();self.windows.clear();self.epoch=None;self.activation_history=mru::History::default();self.window_history=windows::History::default();self.current_source_focus=Value::Null;self.native_coverage_complete=false;self.native_guard_available=false;self.inventory.clear();self.focus_witnesses.clear();}
    pub fn observe(&mut self,node:&Value){
        if node["shell"]!="xdg_shell"||!node["window"].is_null(){return;}
        let Some(app)=node["app_id"].as_str().filter(|app|valid_app_id(app))else{return;};
        let Some(pid)=node["pid"].as_u64().filter(|pid|*pid>=2&&*pid<=u32::MAX as u64).map(|pid|pid as u32)else{return;};
        let Ok(identity)=process_identity(pid)else{return;};
        if self.processes.get(&pid).is_some_and(|process|exited(&process.handle)||process.identity!=identity){self.processes.remove(&pid);}
        if !self.processes.contains_key(&pid){
            if self.processes.len()>=MAX_PROCESSES{return;}
            let Ok(handle)=pidfd(pid)else{return;};
            if exited(&handle)||process_identity(pid).ok().as_ref()!=Some(&identity){return;}
            self.processes.insert(pid,Process{identity,handle,apps:BTreeSet::new()});
        }
        if let Some(process)=self.processes.get_mut(&pid){if process.apps.len()<MAX_APP_IDS||process.apps.contains(app){process.apps.insert(app.to_owned());}}
    }
    pub fn recover(&mut self,epoch:&str,rows:&[Value]){
        if self.epoch.as_deref()!=Some(epoch){self.close();self.epoch=Some(epoch.into());}
        for row in rows{
            let identity=&row["identity"];let Some(pid)=identity["pid"].as_u64().map(|pid|pid as u32)else{continue;};
            if process_identity(pid).ok().as_ref()!=Some(identity){continue;}
            let Ok(handle)=pidfd(pid)else{continue;};
            if exited(&handle)||process_identity(pid).ok().as_ref()!=Some(identity){continue;}
            let Some(apps)=row["apps"].as_array()else{continue;};
            self.processes.insert(pid,Process{identity:identity.clone(),handle,apps:apps.iter().filter_map(Value::as_str).map(str::to_owned).collect()});
        }
        self.prune();
    }
    pub fn refresh(&mut self,epoch:&str,nodes:&[Value])->Vec<Value>{
        if self.epoch.as_deref()!=Some(epoch){self.close();self.epoch=Some(epoch.into());}
        for node in nodes{self.observe(node);}
        self.windows.clear();
        for node in nodes{
            if node["shell"]=="xdg_shell"{
                if let (Some(pid),Some(app))=(node["pid"].as_u64().filter(|pid|*pid<=u32::MAX as u64),node["app_id"].as_str().filter(|app|!app.is_empty())){
                    self.windows.entry((pid as u32,app.to_owned())).or_default().push(node["id"].clone());
                }
            }
        }
        self.prune();
        for node in nodes{
            if node["shell"]!="xdg_shell"||!node["window"].is_null(){continue;}
            let (Some(id),Some(pid),Some(app))=(node["id"].as_i64().filter(|id|*id>0),node["pid"].as_u64().and_then(|pid|u32::try_from(pid).ok()),node["app_id"].as_str())else{continue;};
            let Some(process)=self.processes.get(&pid).filter(|p|p.apps.contains(app))else{continue;};
            let witness=(pid,process.identity.clone(),app.to_owned());
            if let Some(previous)=self.focus_witnesses.get_mut(&id){if previous.as_ref()!=Some(&witness){*previous=None;}}
            else if self.focus_witnesses.len()<4096{self.focus_witnesses.insert(id,Some(witness));}
        }
        let native=nodes.iter().filter_map(|node|self.native_record(node)).collect::<Vec<_>>();self.window_history.reconcile(&native);
        self.current_source_focus=self.source_focus(nodes);self.native_coverage_complete=self.native_coverage_complete(nodes);
        self.inventory.refresh(nodes);
        self.rows()
    }
    pub fn prune(&mut self){self.processes.retain(|pid,process|!exited(&process.handle)&&process_identity(*pid).ok().as_ref()==Some(&process.identity));for witness in self.focus_witnesses.values_mut(){if witness.as_ref().is_some_and(|(pid,identity,app)|!self.processes.get(pid).is_some_and(|p|p.identity==*identity&&p.apps.contains(app))){*witness=None;}}let rows=self.rows();self.activation_history.prune(&rows);self.window_history.retain_processes(&self.records());}
    pub fn observe_focus(&mut self,nodes:&[Value]){
        let Some(node)=nodes.iter().find(|node|node["focused"]==true&&node["shell"]=="xdg_shell"&&node["window"].is_null())else{return;};
        let Some(app)=node["app_id"].as_str()else{return;};let Some(pid)=node["pid"].as_u64().and_then(|n|u32::try_from(n).ok())else{return;};
        let Some(process)=self.processes.get(&pid)else{return;};
        if exited(&process.handle)||process_identity(pid).ok().as_ref()!=Some(&process.identity)||!process.apps.contains(app){return;}
        let rows=self.rows();self.activation_history.promote(app,&rows);
        if let Some(record)=self.native_record(node){self.window_history.promote(record);}
    }
    pub fn observe_focus_event(&mut self,event:&Value){
        if event["change"]!="focus"{return;}
        let focused=&event["container"];
        // A previously verified surface can close before its queued focus
        // receipt arrives. Retain only bounded in-memory process witnesses;
        // neither unknown surfaces nor new processes inherit that receipt.
        let Some(id)=focused["id"].as_i64()else{return;};
        let Some(Some((pid,identity,app)))=self.focus_witnesses.get(&id)else{return;};
        if focused["pid"].as_u64()!=Some(*pid as u64)||focused["app_id"].as_str()!=Some(app)||!self.processes.get(pid).is_some_and(|p|p.identity==*identity){return;}
        self.observe_focus(std::slice::from_ref(focused));
    }
    pub fn records(&self)->Vec<Value>{self.processes.values().map(|process|json!({"identity":process.identity,"apps":process.apps.iter().collect::<Vec<_>>()})).collect()}
    pub fn rows(&self)->Vec<Value>{
        let mut apps:BTreeMap<String,(usize,usize,Vec<String>,bool)>=BTreeMap::new();
        for (pid,process) in &self.processes{
            for app in &process.apps{
                let count=self.windows.get(&(*pid,app.clone())).map_or(0,Vec::len);
                if process.apps.len()>1&&count==0{continue;}
                let item=apps.entry(app.clone()).or_default();
                item.0+=count;item.1+=1;item.2.push(format!("{:x}",Sha256::digest(serde_json::to_vec(&process.identity).unwrap_or_default())));item.3|=process.apps.len()>1;
            }
        }
        apps.into_iter().map(|(app,(windows,processes,mut ids,shared))|{ids.sort();json!({"appId":app,"windowCount":windows,"processCount":processes,"instanceIds":ids,"sharedProcess":shared,"assurance":"native-process-bound-advertised-id","zeroWindows":windows==0})}).collect()
    }
    pub fn exact_process(&mut self,app:&str)->Result<&Process>{
        self.prune();let matches=self.processes.values().filter(|process|process.apps.contains(app)).collect::<Vec<_>>();
        if matches.len()!=1||matches[0].apps.len()!=1{return Err("Application ownership is ambiguous or no longer running".into());}
        Ok(matches[0])
    }
}

pub struct ObservationStore{pub directory:PathBuf,pub epoch:String,pub boot_id:String,lock:File,previous:Option<Vec<u8>>,pub warning:String}
pub struct Recovery{pub processes:Vec<Value>,pub history:mru::History,pub windows:windows::History}
impl ObservationStore{
    pub fn new(runtime:&Path,epoch:&str,boot_id:&str)->Result<Self>{
        if epoch.len()!=24||!epoch.bytes().all(|byte|byte.is_ascii_digit()||(b'a'..=b'f').contains(&byte))||boot_id.len()!=36||!boot_id.bytes().enumerate().all(|(index,byte)|if matches!(index,8|13|18|23){byte==b'-'}else{byte.is_ascii_digit()||(b'a'..=b'f').contains(&byte)}){return Err("Invalid recovery scope".into());}
        let info=fs::symlink_metadata(runtime).map_err(|error|error.to_string())?;
        if !info.is_dir()||info.uid()!=unsafe{libc::getuid()}||info.mode()&0o077!=0{return Err("Recovery runtime must be private and owned".into());}
        let directory=runtime.join("lumen-application-observations");
        fs::DirBuilder::new().mode(0o700).create(&directory).or_else(|error|if error.kind()==std::io::ErrorKind::AlreadyExists{Ok(())}else{Err(error)}).map_err(|error|error.to_string())?;
        let observed=fs::symlink_metadata(&directory).map_err(|error|error.to_string())?;
        if !observed.is_dir()||observed.uid()!=unsafe{libc::getuid()}||observed.mode()&0o077!=0{return Err("Recovery directory must be private and owned".into());}
        let lock=OpenOptions::new().read(true).write(true).create(true).mode(0o600).custom_flags(libc::O_NOFOLLOW).open(directory.join(format!("{epoch}.lock"))).map_err(|error|error.to_string())?;
        let info=lock.metadata().map_err(|error|error.to_string())?;
        if !info.is_file()||info.uid()!=unsafe{libc::getuid()}||info.mode()&0o077!=0||info.nlink()!=1{return Err("Invalid recovery lease".into());}
        if unsafe{libc::flock(lock.as_raw_fd(),libc::LOCK_EX|libc::LOCK_NB)}!=0{return Err("Recovery lease is already held".into());}
        Ok(Self{directory,epoch:epoch.into(),boot_id:boot_id.into(),lock,previous:None,warning:String::new()})
    }
    fn path(&self)->PathBuf{self.directory.join(format!("{}.json",self.epoch))}
    pub fn load(&mut self)->Vec<Value>{
        self.load_state().processes
    }
    pub fn load_state(&mut self)->Recovery{
        match self.read(){Ok(value)=>value,Err(error)=>{if error!="missing"{self.warning="Previous application observations were not restored.".into();}Recovery{processes:Vec::new(),history:mru::History::default(),windows:windows::History::default()}}}
    }
    fn read(&self)->Result<Recovery>{
        let file=OpenOptions::new().read(true).custom_flags(libc::O_NOFOLLOW|libc::O_NONBLOCK).open(self.path()).map_err(|error|if error.kind()==std::io::ErrorKind::NotFound{"missing".into()}else{error.to_string()})?;
        let info=file.metadata().map_err(|error|error.to_string())?;
        if !info.is_file()||info.uid()!=unsafe{libc::getuid()}||info.mode()&0o077!=0||info.nlink()!=1||info.len()>1024*1024{return Err("Invalid recovery record".into());}
        let mut bytes=Vec::new();file.take(1024*1024+1).read_to_end(&mut bytes).map_err(|error|error.to_string())?;
        if bytes.len()>1024*1024{return Err("Recovery record exceeds limit".into());}
        let data:Value=serde_json::from_slice(&bytes).map_err(|_|"Invalid recovery record")?;
        let version=data["version"].as_u64().filter(|v|matches!(v,1..=3)).ok_or("Unsupported recovery record")?;
        let obj=data.as_object().filter(|obj|obj.len()==(version as usize+3)&&["version","epoch","bootId","processes"].iter().all(|key|obj.contains_key(*key))&&(version==1||obj.contains_key("activationHistory"))&&(version<3||obj.contains_key("windowHistory"))).ok_or("Invalid recovery record")?;
        if obj["epoch"]!=self.epoch||obj["bootId"]!=self.boot_id{return Err("Recovery scope changed".into());}
        let rows=obj["processes"].as_array().filter(|rows|rows.len()<=MAX_PROCESSES).ok_or("Invalid recovery process list")?;
        let mut pids=HashSet::new();
        for row in rows{
            let obj=row.as_object().filter(|obj|obj.len()==2&&obj.contains_key("identity")&&obj.contains_key("apps")).ok_or("Invalid recovery identity")?;
            if !valid_identity(&obj["identity"])||!pids.insert(obj["identity"]["pid"].as_u64().unwrap()){return Err("Invalid recovery identity".into());}
            let apps=obj["apps"].as_array().filter(|apps|!apps.is_empty()&&apps.len()<=MAX_APP_IDS).ok_or("Invalid recovered advertisement")?;
            let mut seen=HashSet::new();
            if !apps.iter().all(|value|value.as_str().is_some_and(|app|valid_app_id(app)&&seen.insert(app))){return Err("Invalid recovered advertisement".into());}
        }
        let history=if version>=2{mru::History::from_saved(&obj["activationHistory"],rows)?}else{mru::History::default()};
        let windows=if version==3{windows::History::from_saved(&obj["windowHistory"],rows)?}else{windows::History::default()};
        Ok(Recovery{processes:rows.clone(),history,windows})
    }
    pub fn invalidate(&mut self)->Result<()>{match fs::remove_file(self.path()){Ok(())=>(),Err(error)if error.kind()==std::io::ErrorKind::NotFound=>(),Err(error)=>return Err(error.to_string())};self.previous=None;self.sync_directory()}
    fn sync_directory(&self)->Result<()>{File::open(&self.directory).map_err(|error|error.to_string())?.sync_all().map_err(|error|error.to_string())}
    pub fn save(&mut self,rows:&[Value])->Result<()>{
        self.save_state(rows,&mru::History::default())
    }
    pub fn save_state(&mut self,rows:&[Value],history:&mru::History)->Result<()>{
        self.save_complete(rows,history,&windows::History::default())
    }
    pub fn save_complete(&mut self,rows:&[Value],history:&mru::History,windows:&windows::History)->Result<()>{
        let history=mru::History::from_saved(&history.value(),rows)?.value();
        let windows=windows::History::from_saved(&windows.value(),rows)?.value();
        let mut data=json!({"version":3,"epoch":self.epoch,"bootId":self.boot_id,"processes":rows,"activationHistory":[],"windowHistory":[]});
        let base=serde_json::to_vec(&data).map_err(|error|error.to_string())?.len();
        let mut used=base;let mut retained=Vec::new();
        for row in history.as_array().ok_or("Invalid application order")?{
            let size=serde_json::to_vec(row).map_err(|error|error.to_string())?.len()+usize::from(!retained.is_empty());
            if used+size>1024*1024{break;}used+=size;retained.push(row.clone());
        }
        data["activationHistory"]=Value::Array(retained);
        let mut retained=Vec::new();
        for row in windows.as_array().ok_or("Invalid native window order")?{
            let size=serde_json::to_vec(row).map_err(|error|error.to_string())?.len()+usize::from(!retained.is_empty());
            if used+size>1024*1024{break;}used+=size;retained.push(row.clone());
        }
        data["windowHistory"]=Value::Array(retained);
        let encoded=serde_json::to_vec(&data).map_err(|error|error.to_string())?;
        if self.previous.as_ref()==Some(&encoded){return Ok(());}
        if encoded.len()>1024*1024{return Err("Recovery record exceeds limit".into());}
        let random=fs::read_to_string("/proc/sys/kernel/random/uuid").map_err(|error|error.to_string())?.trim().replace('-',"");
        let temporary=self.directory.join(format!(".{}.json.{random}",self.epoch));
        let result=(||->Result<()> {
            let mut file=OpenOptions::new().write(true).create_new(true).mode(0o600).custom_flags(libc::O_NOFOLLOW).open(&temporary).map_err(|error|error.to_string())?;
            file.write_all(&encoded).map_err(|error|error.to_string())?;file.sync_all().map_err(|error|error.to_string())?;
            fs::rename(&temporary,self.path()).map_err(|error|error.to_string())?;self.sync_directory()?;self.previous=Some(encoded);Ok(())
        })();
        let _=fs::remove_file(&temporary);result
    }
    pub fn lock_fd(&self)->i32{self.lock.as_raw_fd()}
}
