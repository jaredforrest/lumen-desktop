//! Bitwarden owns enrolled Wi-Fi passwords; NetworkManager is the offline cache.
//! Only explicitly marked Wi-Fi entries are read. No secrets leave process pipes.
use super::{Result, Value, json};
use std::{env, fs, io::Write, os::{fd::AsRawFd, unix::fs::{OpenOptionsExt, PermissionsExt}}, path::PathBuf, process::{Command, Stdio}, time::{SystemTime, UNIX_EPOCH}};
const FOLDER:&str="Wi-Fi";
const MARKER:&str="lumen.wifi.v1:";

fn command(program:&str,args:&[&str],input:Option<&str>)->Result<String>{
    let mut child=Command::new("timeout").arg("45").arg(program).args(args).env("LC_ALL","C")
        .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn().map_err(|_|"Wi-Fi password service is unavailable")?;
    if let Some(input)=input {if let Some(mut pipe)=child.stdin.take(){pipe.write_all(input.as_bytes()).map_err(|_|"Unable to send credentials privately")?;}}
    drop(child.stdin.take());
    let output=child.wait_with_output().map_err(|_|"Wi-Fi password service did not finish")?;
    if !output.status.success(){return Err("Wi-Fi password sync failed. Check vault unlock, internet access and network permissions; cached connections remain available.".into());}
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}
fn nm(args:&[&str])->Result<String>{command("nmcli",args,None)}
fn unlocked()->bool {Command::new("timeout").args(["2","rbw","unlocked"]).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).status().is_ok_and(|s|s.success())}
fn directory()->Result<PathBuf>{
    let base=env::var_os("XDG_STATE_HOME").map(PathBuf::from).or_else(||env::var_os("HOME").map(|h|PathBuf::from(h).join(".local/state"))).ok_or("Home directory is unavailable")?;
    let path=base.join("lumen-wifi-vault");fs::create_dir_all(&path).map_err(|_|"Unable to save Wi-Fi sync status")?;
    if fs::symlink_metadata(&path).map_err(|_|"Wi-Fi sync status is unavailable")?.file_type().is_symlink(){return Err("Wi-Fi sync status directory redirects elsewhere".into());}
    fs::set_permissions(&path,fs::Permissions::from_mode(0o700)).map_err(|_|"Unable to protect Wi-Fi sync status")?;Ok(path)
}
fn report(state:&str,message:&str,added:usize,updated:usize)->Result<Value>{
    let value=json!({"state":state,"message":message,"uploaded":added,"cached":updated,"checkedAt":SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs()});
    let path=directory()?.join("status.json");let next=path.with_extension("new");
    let mut file=fs::OpenOptions::new().write(true).create(true).truncate(true).mode(0o600).custom_flags(libc::O_NOFOLLOW).open(&next).map_err(|_|"Unable to save Wi-Fi sync status")?;
    file.write_all(value.to_string().as_bytes()).map_err(|_|"Unable to save Wi-Fi sync status")?;fs::rename(next,path).map_err(|_|"Unable to save Wi-Fi sync status")?;Ok(value)
}
pub fn status()->Value{
    if !directory().is_ok_and(|p|p.join("enabled").exists()){return json!({"state":"disabled","message":"Keep Wi-Fi passwords in Bitwarden, with an offline cache"});}
    if !unlocked(){return json!({"state":"locked","message":"Unlock Bitwarden to sync. Saved Wi-Fi works offline."});}
    directory().ok().and_then(|p|fs::read(p.join("status.json")).ok()).and_then(|s|serde_json::from_slice(&s).ok()).unwrap_or(json!({"state":"ready","message":"Ready to sync Wi-Fi passwords"}))
}
fn name(ssid:&str,key:&str)->String{format!("Wi-Fi — {ssid} [{key}]")}
fn valid(ssid:&str,key:&str,password:&str)->bool{
    !ssid.is_empty() && ssid.len()<=32 && !ssid.chars().any(char::is_control) && !password.chars().any(char::is_control) && password.trim()==password
        && match key {"wpa-psk"=>(8..=63).contains(&password.len()) || (password.len()==64 && password.bytes().all(|b|b.is_ascii_hexdigit())),"sae"=>!password.is_empty() && password.len()<=256,_=>false}
}
struct Entry {ssid:String,key:String,password:String}
fn entries()->Result<Vec<Entry>>{
    let text=command("rbw",&["list","--fields","id,folder"],None)?;
    let mut result=Vec::new();let mut identities=std::collections::HashSet::new();
    for line in text.lines(){
        let Some((id,folder))=line.split_once('\t') else {continue;};
        if folder!=FOLDER || id.len()!=36 || !id.bytes().all(|b|b.is_ascii_hexdigit() || b==b'-'){continue;}
        let raw=command("rbw",&["get","--raw",id],None)?;
        let item:Value=serde_json::from_str(&raw).map_err(|_|"Unable to read a Wi-Fi vault item")?;
        let Some(notes)=item["notes"].as_str().and_then(|s|s.strip_prefix(MARKER)) else {continue;};
        let metadata:Value=serde_json::from_str(notes).map_err(|_|"A Wi-Fi vault item has invalid network metadata")?;
        let ssid=metadata["ssid"].as_str().unwrap_or("");let key=metadata["security"].as_str().unwrap_or("");
        let password=item["data"]["password"].as_str().unwrap_or("");
        if item["id"]!=id || item["folder"]!=FOLDER || item["name"]!=name(ssid,key) || !valid(ssid,key,password){return Err("A Wi-Fi vault item is invalid. Correct it in Bitwarden before syncing.".into());}
        if !identities.insert((ssid.to_owned(),key.to_owned())){return Err("Duplicate Wi-Fi vault items need resolving in Bitwarden before syncing.".into());}
        result.push(Entry{ssid:ssid.into(),key:key.into(),password:password.into()});
    }
    Ok(result)
}
struct Profile {uuid:String,ssid:String,key:String,password:String}
fn property(uuid:&str,key:&str)->Result<String>{
    nm(&["--escape","no","--show-secrets","--get-values",key,"connection","show","uuid",uuid]).map(|s|s.strip_suffix('\n').unwrap_or(&s).to_owned())
}
fn profiles()->Result<Vec<Profile>>{
    let text=nm(&["--terse","--fields","UUID,TYPE","connection","show"])?;
    let mut result=Vec::new();
    for line in text.lines(){
        let Some((uuid,kind))=line.split_once(':') else {continue;};if kind!="802-11-wireless"{continue;}
        let ssid=property(uuid,"802-11-wireless.ssid")?;let key=property(uuid,"802-11-wireless-security.key-mgmt").unwrap_or_default();
        if !["wpa-psk","sae"].contains(&key.as_str()){continue;}
        let password=property(uuid,"802-11-wireless-security.psk").unwrap_or_default();
        result.push(Profile{uuid:uuid.into(),ssid,key,password});
    }Ok(result)
}
fn cache(profile:&Profile,password:&str)->Result<()>{
    // The interactive editor accepts its commands over stdin. Never pass a PSK in argv.
    command("nmcli",&["--wait","10","connection","edit","uuid",&profile.uuid],Some(&format!("set 802-11-wireless-security.psk {password}\nsave persistent\nquit\n")))?;
    if property(&profile.uuid,"802-11-wireless-security.psk")?!=password{return Err("NetworkManager did not save the vault password. Existing connection was left active.".into());}Ok(())
}
fn add(entry:&Entry)->Result<()>{
    let metadata=json!({"ssid":entry.ssid,"security":entry.key});
    command("rbw",&["add","--folder",FOLDER,"--",&name(&entry.ssid,&entry.key)],Some(&format!("{}\n{MARKER}{metadata}\n",entry.password)))?;Ok(())
}
fn sync_inner()->Result<Value>{
    command("rbw",&["sync"],None)?;
    let mut vault=entries()?;let local=profiles()?;let mut uploaded=0;let mut cached=0;
    // Validate every enrolled vault entry before changing any cache.
    for profile in &local {
        if let Some(entry)=vault.iter().find(|e|e.ssid==profile.ssid && e.key==profile.key){
            if entry.password!=profile.password{cache(profile,&entry.password)?;cached+=1;}
        } else if valid(&profile.ssid,&profile.key,&profile.password){
            // Never overwrite an existing vault password with a local value.
            let entry=Entry{ssid:profile.ssid.clone(),key:profile.key.clone(),password:profile.password.clone()};add(&entry)?;vault.push(entry);uploaded+=1;
        }
    }
    for entry in &vault {
        if local.iter().any(|p|p.ssid==entry.ssid && p.key==entry.key){continue;}
        // Create an inactive cache profile; do not switch or reconnect the radio.
        let identity=fs::read_to_string("/proc/sys/kernel/random/uuid").map_err(|_|"Unable to identify the new offline Wi-Fi cache profile")?;
        let uuid=identity.trim();
        if uuid.len()!=36 || !uuid.bytes().all(|b|b.is_ascii_hexdigit() || b==b'-'){return Err("Unable to identify the new offline Wi-Fi cache profile".into());}
        nm(&["connection","add","type","wifi","connection.uuid",uuid,"con-name",&name(&entry.ssid,&entry.key),"ssid",&entry.ssid,"wifi-sec.key-mgmt",&entry.key,"connection.autoconnect","no"])?;
        let profile=Profile{uuid:uuid.into(),ssid:entry.ssid.clone(),key:entry.key.clone(),password:String::new()};cache(&profile,&entry.password)?;
        nm(&["connection","modify","uuid",uuid,"connection.autoconnect","yes"])?;cached+=1;
    }
    command("rbw",&["sync"],None)?;
    report("synced","Wi-Fi passwords synced with Bitwarden",uploaded,cached)
}
pub fn sync()->Result<Value>{
    fs::write(directory()?.join("enabled"),"1\n").map_err(|_|"Unable to enable Wi-Fi password sync")?;
    let lock=fs::OpenOptions::new().create(true).append(true).mode(0o600).custom_flags(libc::O_NOFOLLOW).open(directory()?.join("sync.lock")).map_err(|_|"Unable to lock Wi-Fi sync")?;
    if unsafe{libc::flock(lock.as_raw_fd(),libc::LOCK_EX|libc::LOCK_NB)}!=0{return Ok(json!({"state":"busy","message":"Wi-Fi sync is already running"}));}
    if !unlocked(){return report("locked","Unlock Bitwarden to sync. Saved Wi-Fi works offline.",0,0);}
    match sync_inner(){Ok(value)=>Ok(value),Err(message)=>{let _=report("error",&message,0,0);Err(message)}}
}
pub fn credential(ssid:&str,security:&str)->Result<Option<String>>{
    if !directory().is_ok_and(|p|p.join("enabled").exists()) || !unlocked(){return Ok(None);}
    let family=if security.contains("WPA3") && !security.contains("WPA2"){"sae"}else{"wpa-psk"};
    Ok(entries()?.into_iter().find(|e|e.ssid==ssid && e.key==family).map(|e|e.password))
}
pub fn schedule(){
    if directory().is_ok_and(|p|p.join("enabled").exists()){
        let _=Command::new("systemctl").args(["--user","start","--no-block","lumen-wifi-vault-sync.service"])
            .stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).status();
    }
}
pub fn unlock()->Result<Value>{
    let exe=env::current_exe().map_err(|_|"Unable to open vault unlock")?;
    // Detach the terminal from the short-lived menu helper. Keep the vault agent
    // alive when the terminal closes so subsequent background sync can use it.
    let mut launcher=Command::new("systemd-run");
    launcher.args(["--user","--collect","--property=KillMode=process"]);
    for key in ["PATH","WAYLAND_DISPLAY","DISPLAY","SWAYSOCK"] {
        if let Ok(value)=env::var(key){launcher.arg(format!("--setenv={key}={value}"));}
    }
    let result=launcher.arg("foot").args(["--app-id=lumen-vault-unlock","--title=Unlock Bitwarden for Wi-Fi sync"]).arg(exe).arg("wifi-vault-login")
        .stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).status().map_err(|_|"Unable to open vault unlock")?;
    if !result.success(){return Err("Unable to open vault unlock".into());}
    Ok(json!({"ok":true}))
}
pub fn login()->Result<Value>{
    // A pre-existing unpatched agent cannot handle the verification prompt.
    // Replace it only when locked, without interrupting an unlocked vault.
    if !unlocked(){let _=command("rbw",&["stop-agent"],None);}
    let ok=Command::new("rbw").arg("unlock").status().map_err(|_|"Bitwarden is unavailable")?;
    if !ok.success(){return Err("Vault unlock was cancelled. Saved Wi-Fi remains available.".into());}sync()
}
#[cfg(test)]mod tests{
    use super::*;
    #[test]fn credential_identity_and_validation(){
        assert!(valid("Cafe: Upstairs","wpa-psk","correct horse battery"));
        assert!(!valid("Cafe\nquit","wpa-psk","password123"));assert!(!valid("Cafe","wpa-psk","pw\nsave persistent"));
        assert!(!valid("Cafe","wpa-eap","password123"));assert!(!valid("Cafe","wpa-psk","short"));
        assert!(!valid("Cafe","wpa-psk"," leading space"));assert!(!valid("Cafe","wpa-psk","trailing space "));
        assert_ne!(name("Cafe","sae"),name("Cafe","wpa-psk"));
    }
}
