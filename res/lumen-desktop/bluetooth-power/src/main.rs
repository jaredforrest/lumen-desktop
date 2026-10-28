use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::process::Command;

type Result<T> = std::result::Result<T, String>;
fn numbered(value: &str, prefix: &str) -> bool {
    value.strip_prefix(prefix).is_some_and(|rest| !rest.is_empty() && rest.bytes().all(|c| c.is_ascii_digit()))
}
fn status(root: &Path) -> BTreeMap<String, Value> {
    let mut radios = BTreeMap::new();
    let Ok(items) = fs::read_dir(root) else { return radios; };
    for item in items.flatten() {
        let name = item.file_name().to_string_lossy().to_string();
        if !numbered(&name, "rfkill") { continue; }
        let read = |key: &str| fs::read_to_string(item.path().join(key)).ok().map(|s|s.trim().to_owned());
        if read("type").as_deref() != Some("bluetooth") { continue; }
        let Some(adapter) = read("name") else { continue; };
        if !numbered(&adapter,"hci") { continue; }
        let (Some(soft), Some(hard)) = (read("soft"), read("hard")) else { continue; };
        if !["0","1"].contains(&soft.as_str()) || !["0","1"].contains(&hard.as_str()) { continue; }
        radios.insert(format!("/org/bluez/{adapter}"),json!({"id":&name[6..],"soft":soft=="1","hard":hard=="1"}));
    }
    radios
}
fn prepare(path: &str, radios: &BTreeMap<String, Value>, unblock: impl FnOnce(&str)->Result<()>) -> Result<()> {
    let adapter = path.strip_prefix("/org/bluez/").ok_or("Invalid Bluetooth adapter")?;
    if !numbered(adapter,"hci") { return Err("Invalid Bluetooth adapter".into()); }
    if let Some(radio) = radios.get(path) {
        if radio["hard"] == true { return Err("Bluetooth is blocked by a hardware switch. Turn the switch on first.".into()); }
        if radio["soft"] == true {
            return unblock(radio["id"].as_str().ok_or("Invalid radio ID")?)
                .map_err(|_|"Bluetooth could not be turned on. Check your radio controls or try Bluetooth Settings.".into());
        }
    }
    Ok(())
}
fn main() {
    let args: Vec<String> = std::env::args().collect();
    let radios = status(Path::new("/sys/class/rfkill"));
    match args.get(1).map(String::as_str) {
        Some("status") => println!("{}",json!(radios)),
        Some("prepare") => {
            let result = args.get(2).ok_or("Missing adapter".to_owned()).and_then(|path| prepare(path,&radios,|id| {
                let result = Command::new("rfkill").args(["unblock",id]).output().map_err(|e|e.to_string())?;
                if result.status.success() { Ok(()) } else { Err("rfkill failed".into()) }
            }));
            if let Err(error)=result { eprintln!("{error}"); std::process::exit(1); }
        }
        _ => { eprintln!("Use status or prepare ADAPTER"); std::process::exit(2); }
    }
}
#[cfg(test)] mod tests {
    use super::*;
    #[test] fn only_selected_soft_block_is_removed() {
        let root = std::env::temp_dir().join(format!("radios-{}",std::process::id()));
        fs::create_dir(&root).unwrap();
        for (index,kind,name,soft,hard) in [(0,"bluetooth","hci0","1","0"),(1,"wlan","phy0","1","0"),(2,"bluetooth","hci1","1","1")] {
            let item=root.join(format!("rfkill{index}")); fs::create_dir(&item).unwrap();
            for (key,value) in [("type",kind),("name",name),("soft",soft),("hard",hard)] { fs::write(item.join(key),value).unwrap(); }
        }
        let radios=status(&root);
        assert_eq!(radios.len(),2);
        assert!(prepare("/org/bluez/hci0",&radios,|id|{assert_eq!(id,"0");Ok(())}).is_ok());
        assert!(prepare("/org/bluez/hci1",&radios,|_|panic!("Hardware block must not run rfkill")).unwrap_err().contains("hardware switch"));
        assert!(prepare("/org/bluez/hci0/dev_00",&radios,|_|panic!("Invalid path")).is_err());
        assert!(prepare("/org/bluez/hci3",&radios,|_|panic!("Unknown adapter must not run rfkill")).is_ok());
        fs::remove_dir_all(root).unwrap();
    }
}
