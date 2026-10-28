use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::os::unix::fs::{DirBuilderExt, FileTypeExt, MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::Command;

type Result<T> = std::result::Result<T, String>;
const MAX_WINDOWS: usize = 100;
const LIFETIME: f64 = 60.0;
const MAX_FRAME: usize = 1024 * 1024;

fn now() -> f64 {
    let mut time = libc::timespec {tv_sec:0,tv_nsec:0};
    if unsafe{libc::clock_gettime(libc::CLOCK_BOOTTIME,&mut time)}!=0 {return 0.0;}
    time.tv_sec as f64 + time.tv_nsec as f64/1_000_000_000.0
}
fn epoch(path:&Path) -> Result<String> {
    let info=fs::symlink_metadata(path).map_err(|error|error.to_string())?;
    if !info.file_type().is_socket() || info.uid()!=unsafe{libc::getuid()} {return Err("Compositor endpoint is not an owned socket".into());}
    let raw=format!("{}:{}:{}",info.dev(),info.ino(),info.ctime() as i128*1_000_000_000+info.ctime_nsec() as i128);
    Ok(format!("{:x}",Sha256::digest(raw.as_bytes()))[..24].to_owned())
}
fn window_nodes(tree:&Value) -> Result<Vec<Value>> {
    let mut pending=vec![tree];let mut seen=HashSet::new();let mut result=Vec::new();
    while let Some(node)=pending.pop() {
        if seen.len()>100000{return Err("Compositor tree exceeds the supported limit".into());}
        let object=node.as_object().ok_or("Invalid compositor tree")?;
        if let Some(ident)=object.get("id").filter(|id|!id.is_null()) {
            let id=ident.as_i64().ok_or("Invalid compositor identity")?;
            if !seen.insert(id){return Err("Invalid compositor identity".into());}
        }
        for key in ["nodes","floating_nodes"] {
            if let Some(children)=object.get(key) {
                let children=children.as_array().ok_or("Invalid compositor children")?;
                pending.extend(children);
            }
        }
        if !node["app_id"].is_null()||!node["window"].is_null(){result.push(node.clone());}
    }
    Ok(result)
}
fn tree() -> Result<(String,Vec<Value>)> {
    let path=PathBuf::from(std::env::var_os("SWAYSOCK").ok_or("No supported compositor session")?);
    let first=epoch(&path)?;
    let output=Command::new("timeout").args(["5","swaymsg","-r","-t","get_tree"]).output().map_err(|error|error.to_string())?;
    if !output.status.success()||output.stdout.is_empty()||output.stdout.len()>MAX_FRAME{return Err("Unable to read compositor tree".into());}
    let value:Value=serde_json::from_slice(&output.stdout).map_err(|_|"Invalid compositor tree")?;
    if epoch(&path)?!=first{return Err("Compositor changed; select the application again".into());}
    Ok((first,window_nodes(&value)?))
}
fn identity(pid:u64) -> Result<Value> {
    if pid<2||pid>i32::MAX as u64{return Err("Invalid application process".into());}
    let root=PathBuf::from(format!("/proc/{pid}"));
    if fs::metadata(&root).map_err(|error|error.to_string())?.uid()!=unsafe{libc::getuid()}{return Err("Application process has an unexpected owner".into());}
    let raw=fs::read_to_string(root.join("stat")).map_err(|error|error.to_string())?;
    let fields=raw.rsplit_once(')').ok_or("Invalid process identity")?.1.split_whitespace().collect::<Vec<_>>();
    if fields.len()<20||fields[0]=="Z"{return Err("Application process has already exited".into());}
    let start=fields[19].parse::<u64>().map_err(|_|"Invalid process identity")?;
    let status=fs::read_to_string(root.join("status")).map_err(|error|error.to_string())?;
    let uids=status.lines().find_map(|line|line.strip_prefix("Uid:")).map(|line|line.split_whitespace().collect::<Vec<_>>()).unwrap_or_default();
    if uids.len()!=4||uids.iter().any(|uid|uid.parse::<u32>().ok()!=Some(unsafe{libc::getuid()})){return Err("Privileged application processes cannot be terminated here".into());}
    let executable=root.join("exe");
    let resolved=fs::canonicalize(&executable).map_err(|error|error.to_string())?;
    let info=fs::metadata(&executable).map_err(|error|error.to_string())?;
    if resolved.file_name().is_some_and(|name|name.to_string_lossy().contains("quickshell")){return Err("The desktop shell cannot be force quit".into());}
    Ok(json!({"pid":pid,"start":start,"device":info.dev(),"inode":info.ino()}))
}
fn pidfd(pid:u64) -> Result<OwnedFd> {
    let fd=unsafe{libc::syscall(libc::SYS_pidfd_open,pid as libc::pid_t,0)};
    if fd<0{return Err("Safe process handles are unavailable on this system".into());}
    Ok(unsafe{OwnedFd::from_raw_fd(fd as i32)})
}
fn terminate(fd:&OwnedFd) -> Result<bool> {
    let status=unsafe{libc::syscall(libc::SYS_pidfd_send_signal,fd.as_raw_fd(),libc::SIGKILL,0 as *const libc::siginfo_t,0)};
    if status==0{return Ok(true);}
    let error=std::io::Error::last_os_error();
    if error.raw_os_error()==Some(libc::ESRCH){return Ok(false);}
    Err(error.to_string())
}
fn token() -> Result<String> {Ok(fs::read_to_string("/proc/sys/kernel/random/uuid").map_err(|error|error.to_string())?.trim().replace('-',""))}
fn valid_token(value:&str) -> bool {value.len()==32&&value.bytes().all(|byte|byte.is_ascii_digit()||(b'a'..=b'f').contains(&byte))}
struct Control {directory:PathBuf}
impl Control {
    fn new(directory:PathBuf) -> Result<Self> {
        fs::DirBuilder::new().recursive(true).mode(0o700).create(&directory).map_err(|error|error.to_string())?;
        let info=fs::symlink_metadata(&directory).map_err(|error|error.to_string())?;
        if !info.is_dir()||info.uid()!=unsafe{libc::getuid()}{return Err("Invalid confirmation directory".into());}
        fs::set_permissions(&directory,fs::Permissions::from_mode(0o700)).map_err(|error|error.to_string())?;
        Ok(Self{directory})
    }
    fn locked<T>(&self,work:impl FnOnce()->Result<T>) -> Result<T> {
        let lock=OpenOptions::new().read(true).write(true).create(true).mode(0o600).custom_flags(libc::O_NOFOLLOW).open(self.directory.join("lock")).map_err(|error|error.to_string())?;
        let info=lock.metadata().map_err(|error|error.to_string())?;
        if !info.is_file()||info.uid()!=unsafe{libc::getuid()}||info.mode()&0o077!=0{return Err("Invalid confirmation lock".into());}
        if unsafe{libc::flock(lock.as_raw_fd(),libc::LOCK_EX)}!=0{return Err(std::io::Error::last_os_error().to_string());}
        work()
    }
    fn receipt(&self,value:&str) -> Result<PathBuf> {if !valid_token(value){return Err("Invalid confirmation identity".into());}Ok(self.directory.join(value))}
    fn read(&self,value:&str) -> Result<Value> {
        let mut file=OpenOptions::new().read(true).custom_flags(libc::O_NOFOLLOW).open(self.receipt(value)?).map_err(|error|error.to_string())?;
        let info=file.metadata().map_err(|error|error.to_string())?;
        if !info.is_file()||info.uid()!=unsafe{libc::getuid()}||info.mode()&0o077!=0||info.len()>131072{return Err("Invalid confirmation receipt".into());}
        let mut bytes=Vec::new();file.read_to_end(&mut bytes).map_err(|error|error.to_string())?;
        let receipt:Value=serde_json::from_slice(&bytes).map_err(|_|"Invalid confirmation receipt")?;
        let obj=receipt.as_object().filter(|obj|obj.len()==5&&["version","created","epoch","windows","processes"].iter().all(|key|obj.contains_key(*key))).ok_or("Invalid confirmation receipt")?;
        if obj["version"].as_u64()!=Some(1){return Err("Invalid confirmation receipt".into());}
        let created=obj["created"].as_f64().ok_or("Invalid confirmation time or session")?;
        let age=now()-created;
        if !age.is_finite()||age<0.0||age>=LIFETIME{return Err("Confirmation expired; select the application again".into());}
        if !obj["epoch"].as_str().is_some_and(|epoch|!epoch.is_empty()&&epoch.len()<=256){return Err("Invalid confirmation time or session".into());}
        let windows=obj["windows"].as_array().filter(|rows|!rows.is_empty()&&rows.len()<=MAX_WINDOWS).ok_or("Invalid confirmation processes")?;
        let processes=obj["processes"].as_array().filter(|rows|!rows.is_empty()&&rows.len()<=MAX_WINDOWS).ok_or("Invalid confirmation processes")?;
        let mut ids=HashSet::new();let mut window_pids=HashSet::new();
        for window in windows {
            let row=window.as_object().filter(|row|row.len()==3&&["id","pid","app_id"].iter().all(|key|row.contains_key(*key))).ok_or("Invalid confirmation window")?;
            let id=row["id"].as_u64().filter(|value|*value>0).ok_or("Invalid confirmation window")?;
            let pid=row["pid"].as_u64().filter(|value|*value>0).ok_or("Invalid confirmation window")?;
            let app=row["app_id"].as_str().filter(|value|!value.is_empty()&&value.len()<=256&&*value!="org.quickshell").ok_or("Invalid confirmation window")?;
            if app.is_empty()||!ids.insert(id){return Err("Invalid confirmation window".into());}
            window_pids.insert(pid);
        }
        let mut process_pids=HashSet::new();
        for process in processes {
            let row=process.as_object().filter(|row|row.len()==4&&["pid","start","device","inode"].iter().all(|key|row.contains_key(*key))).ok_or("Invalid confirmation process")?;
            let pid=row["pid"].as_u64().filter(|pid|*pid>=2).ok_or("Invalid confirmation process")?;
            for key in ["start","device","inode"] {row[key].as_u64().ok_or("Invalid confirmation process")?;}
            if !process_pids.insert(pid){return Err("Invalid confirmation process".into());}
        }
        if window_pids!=process_pids{return Err("Confirmation process membership is inconsistent".into());}
        Ok(receipt)
    }
    fn prepare(&self,descriptors:&Value) -> Result<Value> {
        let descriptors=descriptors.as_array().filter(|rows|!rows.is_empty()&&rows.len()<=MAX_WINDOWS).ok_or("Select a running application with native windows")?;
        let (epoch,nodes)=tree()?;let mut selected=Vec::new();let mut app_ids=HashSet::new();let mut selected_ids=HashSet::new();let mut pids=HashSet::new();
        for descriptor in descriptors {
            let row=descriptor.as_object().filter(|row|row.len()==2&&row.contains_key("appId")&&row.contains_key("title")).ok_or("Invalid selected window")?;
            let app=row["appId"].as_str().filter(|app|!app.is_empty()&&app.len()<=256).ok_or("Invalid selected window")?;
            let title=row["title"].as_str().filter(|title|title.len()<=8192).ok_or("Invalid selected window")?;
            let matches=nodes.iter().filter(|node|node["app_id"]==app&&node["name"]==title).collect::<Vec<_>>();
            if matches.len()!=1{return Err("The selected window changed or is ambiguous; select it again".into());}
            let node=matches[0];
            if node["shell"]!="xdg_shell"||app=="org.quickshell"{return Err("Force Quit is available only for external native applications".into());}
            let pid=node["pid"].as_u64().filter(|pid|*pid>=2).ok_or("Application process identity is unavailable")?;
            let id=node["id"].as_u64().filter(|id|*id>0).ok_or("Invalid compositor identity")?;
            if !selected_ids.insert(id){return Err("A window was selected more than once".into());}
            selected.push(json!({"id":id,"pid":pid,"app_id":app}));app_ids.insert(app);pids.insert(pid);
        }
        if app_ids.len()!=1{return Err("Select windows from one application at a time".into());}
        if nodes.iter().any(|node|node["pid"].as_u64().is_some_and(|pid|pids.contains(&pid))&&!node["id"].as_u64().is_some_and(|id|selected_ids.contains(&id))){return Err("This process also owns unselected windows; select the whole application or close those windows first".into());}
        let mut sorted=pids.into_iter().collect::<Vec<_>>();sorted.sort_unstable();
        let processes=sorted.into_iter().map(identity).collect::<Result<Vec<_>>>()?;
        let receipt=json!({"version":1,"created":now(),"epoch":epoch,"windows":selected,"processes":processes});
        self.locked(||{
            for item in fs::read_dir(&self.directory).map_err(|error|error.to_string())? {
                let item=item.map_err(|error|error.to_string())?;
                let name=item.file_name();let name=name.to_string_lossy();
                if valid_token(&name) && item.metadata().is_ok_and(|info|info.mtime()< (std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs() as i64)-LIFETIME as i64){let _=fs::remove_file(item.path());}
            }
            let name=token()?;let path=self.receipt(&name)?;
            let mut file=OpenOptions::new().write(true).create_new(true).mode(0o600).custom_flags(libc::O_NOFOLLOW).open(path).map_err(|error|error.to_string())?;
            serde_json::to_writer(&mut file,&receipt).map_err(|error|error.to_string())?;
            file.flush().map_err(|error|error.to_string())?;
            Ok(json!({"token":name,"processCount":receipt["processes"].as_array().unwrap().len(),"windowCount":receipt["windows"].as_array().unwrap().len(),"expiresIn":LIFETIME as u64}))
        })
    }
    fn cancel(&self,value:&str) -> Result<Value> {
        self.locked(||{match fs::remove_file(self.receipt(value)?){Ok(())=>(),Err(error)if error.kind()==std::io::ErrorKind::NotFound=>(),Err(error)=>return Err(error.to_string())}Ok(json!({"cancelled":true}))})
    }
    fn confirm(&self,value:&str) -> Result<Value> {
        self.locked(||{
            let receipt=self.read(value)?;
            fs::remove_file(self.receipt(value)?).map_err(|error|error.to_string())?;
            let (epoch,nodes)=tree()?;
            if receipt["epoch"]!=epoch{return Err("The desktop session changed; select the application again".into());}
            let windows=receipt["windows"].as_array().unwrap();let processes=receipt["processes"].as_array().unwrap();
            let selected_ids=windows.iter().filter_map(|window|window["id"].as_u64()).collect::<HashSet<_>>();
            let selected_pids=processes.iter().filter_map(|process|process["pid"].as_u64()).collect::<HashSet<_>>();
            let by_id=nodes.iter().filter_map(|node|node["id"].as_u64().map(|id|(id,node))).collect::<HashMap<_,_>>();
            for window in windows {
                let node=window["id"].as_u64().and_then(|id|by_id.get(&id)).ok_or("The selected application windows changed; select them again")?;
                if node["pid"]!=window["pid"]||node["app_id"]!=window["app_id"]||node["shell"]!="xdg_shell"{return Err("The selected application windows changed; select them again".into());}
            }
            if nodes.iter().any(|node|node["pid"].as_u64().is_some_and(|pid|selected_pids.contains(&pid))&&!node["id"].as_u64().is_some_and(|id|selected_ids.contains(&id))){return Err("The application process now owns other windows; review the selection again".into());}
            let mut handles=Vec::new();
            for process in processes {
                let pid=process["pid"].as_u64().unwrap();let handle=pidfd(pid)?;
                handles.push(handle);
                if identity(pid)?!=*process{return Err("Application process identity changed; nothing was terminated".into());}
            }
            let mut terminated=0;let mut exited=0;
            for handle in &handles {if terminate(handle)?{terminated+=1}else{exited+=1}}
            Ok(json!({"terminationRequested":terminated,"alreadyExited":exited}))
        })
    }
}
fn main() {
    let args=std::env::args().skip(1).collect::<Vec<_>>();
    let (request_id,action,value)=match args.as_slice(){[flag,id,action,value] if flag=="--request-id"=>(id.as_str(),action.as_str(),value.as_str()),[action,value]=>("",action.as_str(),value.as_str()),_=>("","","")};
    let result=(||->Result<Value>{
        let root=PathBuf::from(std::env::var_os("XDG_RUNTIME_DIR").ok_or("No private desktop runtime directory")?);
        let control=Control::new(root.join("lumen-desktop/force-quit"))?;
        match action {"prepare"=>control.prepare(&serde_json::from_str(value).map_err(|_|"Invalid selected window")?),"confirm"=>control.confirm(value),"cancel"=>control.cancel(value),_=>Err("Choose prepare, confirm or cancel".into())}
    })();
    match result{Ok(mut value)=>{value["requestId"]=json!(request_id);println!("{value}");},Err(error)=>{println!("{}",json!({"error":error,"requestId":request_id}));std::process::exit(1);}}
}

#[cfg(test)]
mod tests{
    use super::*;
    #[test]
    fn invalid_tokens_and_tree_facts_fail_closed(){
        for value in ["", "../../etc/passwd", "1234", "g".repeat(32).as_str()] {assert!(!valid_token(value));}
        assert!(window_nodes(&json!({"id":1,"nodes":[{"id":2},{"id":2}]})).is_err());
        assert!(window_nodes(&json!({"id":1,"nodes":{}})).is_err());
    }
    #[test]
    fn receipt_is_private_bounded_and_one_use() {
        let root=std::env::temp_dir().join(format!("lumen-force-quit-test-{}-{}",std::process::id(),token().unwrap()));
        let control=Control::new(root.clone()).unwrap();
        let name=token().unwrap();
        let path=control.receipt(&name).unwrap();
        let receipt=json!({"version":1,"created":now(),"epoch":"fixture","windows":[{"id":4,"pid":5,"app_id":"org.example.App"}],"processes":[{"pid":5,"start":1,"device":2,"inode":3}]});
        let mut file=OpenOptions::new().write(true).create_new(true).mode(0o600).open(&path).unwrap();
        serde_json::to_writer(&mut file,&receipt).unwrap();drop(file);
        assert_eq!(control.read(&name).unwrap(),receipt);
        let mut corrupt=receipt.clone();corrupt["processes"][0]["pid"]=json!(6);
        fs::write(&path,serde_json::to_vec(&corrupt).unwrap()).unwrap();
        assert!(control.read(&name).is_err());
        fs::remove_file(&path).unwrap();
        let outside=root.join("outside");fs::write(&outside,serde_json::to_vec(&receipt).unwrap()).unwrap();
        std::os::unix::fs::symlink(&outside,&path).unwrap();
        assert!(control.read(&name).is_err());
        assert!(control.cancel(&name).is_ok());
        assert_eq!(fs::read(&outside).unwrap(),serde_json::to_vec(&receipt).unwrap());
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn pidfd_targets_only_disposable_child() {
        let mut child=Command::new("sleep").arg("30").spawn().unwrap();
        let observed=identity(child.id() as u64).unwrap();
        assert_eq!(observed["pid"],child.id());
        let handle=pidfd(child.id() as u64).unwrap();
        assert_eq!(identity(child.id() as u64).unwrap(),observed);
        assert!(terminate(&handle).unwrap());
        let status=child.wait().unwrap();
        assert!(!status.success());
    }
}
