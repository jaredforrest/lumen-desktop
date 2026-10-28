use serde_json::{Value, json};
use std::env;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::fd::AsRawFd;
use std::os::unix::fs::{FileTypeExt, OpenOptionsExt, PermissionsExt};
use std::os::unix::net::UnixDatagram;
use std::path::{Path, PathBuf};
use std::process::{Command,Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

type Result<T> = std::result::Result<T, String>;

fn run(args:&[&str], seconds:u32) -> Result<String> {
    let output=Command::new("timeout").arg(seconds.to_string()).args(args).output().map_err(|e|e.to_string())?;
    if !output.status.success() {return Err("The service rejected the request".into());}
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}
fn bluetooth_power(action:&str, path:Option<&str>) -> Result<Value> {
    let mut args=vec!["lumen-bluetooth-power",action];
    if let Some(path)=path {args.push(path);}
    let output=Command::new("timeout").arg("5").args(args).output().map_err(|e|e.to_string())?;
    if !output.status.success() {
        let detail=String::from_utf8_lossy(&output.stderr).trim().to_owned();
        return Err(if detail.is_empty(){"Bluetooth radio control failed".into()}else{detail});
    }
    if action=="status" {serde_json::from_slice(&output.stdout).map_err(|e|e.to_string())} else {Ok(Value::Null)}
}
fn adapter_path(path:&str) -> bool {
    path.strip_prefix("/org/bluez/hci").is_some_and(|rest|{
        let digits=rest.bytes().take_while(u8::is_ascii_digit).count();
        if digits==0{return false;}
        let tail=&rest[digits..];
        tail.is_empty() || tail.strip_prefix("/dev_").is_some_and(|id|id.len()==17 && id.bytes().all(|b|b.is_ascii_hexdigit() || b==b'_'))
    })
}
fn bluetooth(action:&str, path:&str) -> Result<Value> {
    if !adapter_path(path){return Err("Invalid Bluetooth device".into());}
    match action {
        "bluetooth-on"|"bluetooth-off"=>{
            if action=="bluetooth-on" {bluetooth_power("prepare",Some(path))?;}
            run(&["busctl","--system","call","org.bluez",path,"org.freedesktop.DBus.Properties","Set","ssv","org.bluez.Adapter1","Powered","b",if action=="bluetooth-on"{"true"}else{"false"}],12)
                .map_err(|_|"Bluetooth could not complete the request. Check that the device is on and in range.".to_owned())?;
        }
        "bluetooth-connect"|"bluetooth-disconnect"=>{
            run(&["busctl","--system","call","org.bluez",path,"org.bluez.Device1",if action=="bluetooth-connect"{"Connect"}else{"Disconnect"}],12)
                .map_err(|_|"Bluetooth could not complete the request. Check that the device is on and in range.".to_owned())?;
        }
        _=>return Err("Unknown control".into()),
    }
    Ok(json!({"ok":true}))
}
fn wifi_path() -> Option<PathBuf> {
    let mut paths=fs::read_dir("/run/wpa_supplicant").ok()?.filter_map(|entry|entry.ok().map(|entry|entry.path())).collect::<Vec<_>>();
    paths.sort();
    paths.into_iter().find(|path|path.file_name().is_some_and(|name|name!="global") && fs::metadata(path).is_ok_and(|metadata|metadata.file_type().is_socket()))
}
fn wifi_request(path:&Path, command:&str) -> Result<String> {
    let runtime=env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from).unwrap_or_else(env::temp_dir);
    let stamp=SystemTime::now().duration_since(UNIX_EPOCH).map_err(|e|e.to_string())?.as_nanos();
    let directory=runtime.join(format!("lumen-wifi-{}-{stamp}",std::process::id()));
    fs::create_dir(&directory).map_err(|e|e.to_string())?;
    fs::set_permissions(&directory,fs::Permissions::from_mode(0o700)).map_err(|e|e.to_string())?;
    let result=(|| {
        let client=UnixDatagram::bind(directory.join("control")).map_err(|e|e.to_string())?;
        client.set_read_timeout(Some(std::time::Duration::from_secs(3))).map_err(|e|e.to_string())?;
        client.connect(path).map_err(|e|e.to_string())?;
        client.send(command.as_bytes()).map_err(|e|e.to_string())?;
        let mut buffer=[0_u8;65536];let count=client.recv(&mut buffer).map_err(|e|e.to_string())?;
        let answer=String::from_utf8_lossy(&buffer[..count]).trim().to_owned();
        if answer.starts_with("FAIL") || answer.starts_with("UNKNOWN") {Err("Wi-Fi request failed".into())}else{Ok(answer)}
    })();
    let _=fs::remove_file(directory.join("control"));let _=fs::remove_dir(&directory);
    result
}
fn wifi_status(path:Option<&Path>) -> Result<Value> {
    let Some(path)=path else{return Ok(json!({"available":false,"networks":[]}));};
    let status_text=wifi_request(path,"STATUS")?;
    let status=status_text.lines().filter_map(|line|line.split_once('=')).collect::<std::collections::HashMap<_,_>>();
    let networks=wifi_request(path,"LIST_NETWORKS")?.lines().skip(1).filter_map(|line|{
        let fields=line.split('\t').collect::<Vec<_>>();
        (fields.len()>=4).then(||json!({"id":fields[0],"name":fields[1],"connected":fields[3].contains("[CURRENT]") && status.get("wpa_state")==Some(&"COMPLETED")}))
    }).collect::<Vec<_>>();
    Ok(json!({"available":true,"state":status.get("wpa_state").copied().unwrap_or("UNKNOWN"),"name":status.get("ssid").copied().unwrap_or(""),"networks":networks}))
}
fn network_status(mut wireless:Value, path:Option<&Path>) -> Value {
    let mut command=Command::new("timeout");
    command.args(["5","lumen-network-connectivity"]);
    if let Some(name)=path.and_then(|path|path.file_name()).and_then(|name|name.to_str()) {command.arg(name);}
    let result=(|| -> Result<Value> {
        let mut child=command.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn().map_err(|e|e.to_string())?;
        child.stdin.take().ok_or("Network helper input unavailable")?.write_all(wireless.to_string().as_bytes()).map_err(|e|e.to_string())?;
        let output=child.wait_with_output().map_err(|e|e.to_string())?;
        if !output.status.success(){return Err("Network helper failed".into());}
        serde_json::from_slice(&output.stdout).map_err(|e|e.to_string())
    })();
    if let Ok(result)=result {return result;}
    let available=wireless["available"].as_bool().unwrap_or(false);
    wireless["controlAvailable"]=json!(available);
    wireless["phase"]=json!(if available {"unknown"}else{"service-unavailable"});
    wireless["connectivity"]=json!("unknown");
    wireless["statusSource"]=json!(if available {"supplicant"}else{"unavailable"});
    wireless
}
fn power_status() -> Value {
    run(&["lumen-power-status"],4).ok().and_then(|result|serde_json::from_str(&result).ok())
        .unwrap_or_else(||json!({"available":false,"present":false,"percentage":null,"state":"unavailable","low":false,"secondsRemaining":null}))
}
fn boottime() -> f64 {
    let mut time=libc::timespec{tv_sec:0,tv_nsec:0};
    unsafe {libc::clock_gettime(libc::CLOCK_BOOTTIME,&mut time);}
    time.tv_sec as f64 + time.tv_nsec as f64/1e9
}
fn cache_path() -> PathBuf {
    let runtime=env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from).unwrap_or_else(env::temp_dir);
    runtime.join(format!("lumen-ddc-{}.json",unsafe{libc::getuid()}))
}
fn locked<T>(file:&Path, action:impl FnOnce()->Result<T>) -> Result<T> {
    let lock=file.with_extension("lock");
    let handle=OpenOptions::new().create(true).append(true).mode(0o600).open(&lock).map_err(|e|e.to_string())?;
    fs::set_permissions(lock,fs::Permissions::from_mode(0o600)).map_err(|e|e.to_string())?;
    if unsafe{libc::flock(handle.as_raw_fd(),libc::LOCK_EX)}!=0 {return Err("Display cache is unavailable".into());}
    action()
}
fn valid_cache(value:&Value) -> bool {
    value.as_object().is_some() && value["devices"].as_array().is_some_and(|devices|devices.iter().all(|device|device["bus"].as_str().is_some_and(|bus|!bus.is_empty() && bus.bytes().all(|b|b.is_ascii_digit()))))
        && ["detected","read"].iter().all(|key|value[*key].as_f64().is_some())
}
fn topology_identity() -> String {
    if env::var_os("SWAYSOCK").is_none(){return String::new();}
    run(&["swaymsg","-r","-t","get_outputs"],12).ok().and_then(|text|serde_json::from_str::<Value>(&text).ok()).and_then(|value|{
        value.as_array().map(|outputs|outputs.iter().map(|output|json!([output["name"],output["make"],output["model"],output["serial"],output["active"]])).collect::<Vec<_>>())
    }).and_then(|identity|serde_json::to_string(&identity).ok()).unwrap_or_default()
}
fn detect_devices(output:&str) -> Vec<Value> {
    output.split("Display ").skip(1).filter_map(|block|{
        let bus=block.lines().find_map(|line|line.split_once("I2C bus:").and_then(|(_,tail)|tail.trim().strip_prefix("/dev/i2c-")).and_then(|number|number.split_whitespace().next()).filter(|number|!number.is_empty() && number.bytes().all(|b|b.is_ascii_digit())))?;
        let name=block.lines().find_map(|line|line.split_once("Monitor:").map(|(_,tail)|tail.trim())).filter(|name|!name.is_empty()).unwrap_or("Display");
        Some(json!({"bus":bus,"name":name}))
    }).collect()
}
fn vcp_level(output:&str) -> Option<(f64,u32)> {
    let marker=output.find("VCP 10 C ")?;let mut values=output[marker+9..].split_whitespace();
    let current=values.next()?.parse::<u32>().ok()?;let maximum=values.next()?.parse::<u32>().ok()?;
    (maximum>0).then_some((current as f64/maximum as f64,maximum))
}
fn displays() -> Result<Value> {
    let file=cache_path();locked(&file,|| {
        let mut state=fs::read(&file).ok().and_then(|raw|serde_json::from_slice::<Value>(&raw).ok()).filter(valid_cache).unwrap_or_else(||json!({}));
        let identity=topology_identity();let now=boottime();
        if state["identity"]!=identity || now-state["detected"].as_f64().unwrap_or(0.)>300. {
            let detected=run(&["ddcutil","detect","--brief"],12)?;
            state=json!({"identity":identity,"detected":now,"devices":detect_devices(&detected),"read":0});
        }
        if now-state["read"].as_f64().unwrap_or(0.)>2. {
            let mut values=Vec::new();
            let devices=state["devices"].as_array().cloned().unwrap_or_default();
            for device in &devices {
                let bus=device["bus"].as_str().unwrap_or("");
                if let Ok(output)=run(&["ddcutil","--bus",bus,"getvcp","10","--brief"],12) {
                    if let Some((level,maximum))=vcp_level(&output) {
                        let mut item=device.clone();item["level"]=json!(level);item["maximum"]=json!(maximum);values.push(item);
                    }
                }else{state["detected"]=json!(0);}
            }
            state["displays"]=json!(values);state["read"]=json!(now);
            fs::write(&file,state.to_string()).map_err(|e|e.to_string())?;
            fs::set_permissions(&file,fs::Permissions::from_mode(0o600)).map_err(|e|e.to_string())?;
        }
        Ok(json!({"displays":state["displays"].as_array().cloned().unwrap_or_default()}))
    })
}
fn brightness(bus:&str, value:&str) -> Result<Value> {
    if bus.is_empty() || !bus.bytes().all(|b|b.is_ascii_digit()) || value.parse::<u16>().is_err() {return Err("Invalid brightness request".into());}
    run(&["ddcutil","--bus",bus,"setvcp","10",value],12)?;
    let file=cache_path();let _=locked(&file,||{
        if let Ok(mut state)=fs::read(&file).ok().and_then(|raw|serde_json::from_slice::<Value>(&raw).ok()).ok_or(()) {
            if state.is_object(){state["read"]=json!(0);fs::write(&file,state.to_string()).map_err(|e|e.to_string())?;}
        }
        Ok(())
    });
    Ok(json!({"ok":true}))
}
fn main_result(args:&[String]) -> Result<Value> {
    let action=args.first().map(String::as_str).unwrap_or("status");
    let path=wifi_path();
    match action {
        "status"=>{
            let wireless=wifi_status(path.as_deref()).unwrap_or_else(|_|json!({"available":false,"networks":[]}));
            Ok(json!({"wifi":network_status(wireless,path.as_deref()),"bluetoothRadios":bluetooth_power("status",None)?,"power":power_status()}))
        },
        "displays"=>displays(),
        "brightness"=>brightness(args.get(1).ok_or("Missing display bus")?,args.get(2).ok_or("Missing brightness value")?),
        "bluetooth-on"|"bluetooth-off"|"bluetooth-connect"|"bluetooth-disconnect"=>bluetooth(action,args.get(1).ok_or("Missing Bluetooth path")?),
        "wifi-connect"=>{
            let id=args.get(1).ok_or("Missing network")?;
            if id.is_empty() || !id.bytes().all(|b|b.is_ascii_digit()) || !wifi_status(path.as_deref())?["networks"].as_array().is_some_and(|networks|networks.iter().any(|network|network["id"]==*id)) {return Err("Network is no longer configured".into());}
            wifi_request(path.as_deref().ok_or("Wi-Fi service unavailable")?,&format!("SELECT_NETWORK {id}"))?;Ok(json!({"ok":true}))
        }
        "wifi-disconnect"|"wifi-reconnect"=>{
            wifi_request(path.as_deref().ok_or("Wi-Fi service unavailable")?,if action=="wifi-disconnect"{"DISCONNECT"}else{"RECONNECT"})?;Ok(json!({"ok":true}))
        }
        _=>Err("Unknown control".into()),
    }
}
fn main() {
    match main_result(&env::args().skip(1).collect::<Vec<_>>()) {
        Ok(value)=>println!("{value}"),
        Err(error)=>{println!("{}",json!({"error":error}));std::process::exit(1);}
    }
}
#[cfg(test)] mod tests {
    use super::*;
    #[test] fn display_parser_handles_missing_and_valid_values() {
        assert_eq!(detect_devices("Display 1\n I2C bus: /dev/i2c-9\n Monitor: Test\n")[0]["bus"],"9");
        assert_eq!(vcp_level("VCP 10 C 30 80"),Some((0.375,80)));
        assert_eq!(vcp_level("VCP 10 ERR"),None);
    }
    #[test] fn bluetooth_paths_are_restricted() {
        assert!(adapter_path("/org/bluez/hci0"));
        assert!(adapter_path("/org/bluez/hci0/dev_00_11_22_33_44_55"));
        assert!(!adapter_path("/org/bluez/hci0/other"));
    }
}
