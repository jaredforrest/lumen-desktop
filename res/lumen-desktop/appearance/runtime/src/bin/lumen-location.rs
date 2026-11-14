//! Refresh only the current approximate location; preserve it when offline or using a VPN route.
use serde_json::{Value,json};
use std::{fs, path::Path, process::Command};

fn run() -> Result<(),String> {
    let route=Command::new("ip").args(["-json","route","get","1.1.1.1"]).output().map_err(|e|e.to_string())?;
    let routes:Value=serde_json::from_slice(&route.stdout).map_err(|e|e.to_string())?;
    let device=routes[0]["dev"].as_str().ok_or("No usable network route")?;
    if ["wg","tun","tailscale","mullvad"].iter().any(|prefix|device.starts_with(prefix)) {
        println!("Keeping last location while using a VPN route"); return Ok(());
    }
    let response=Command::new("curl").args(["--fail","--silent","--show-error","--max-time","20","--max-filesize","65536","https://ipwho.is/"]).output().map_err(|e|e.to_string())?;
    if !response.status.success() {return Err("Location lookup unavailable; keeping last location".into());}
    let input:Value=serde_json::from_slice(&response.stdout).map_err(|_|"Invalid location response")?;
    let current=validate(&input)?;
    let zone=current["timezone"].as_str().ok_or("Missing timezone")?;
    // NixOS with time.timeZone=null leaves this symlink mutable. Do not follow it into the store.
    let target=Path::new("/etc/zoneinfo").join(zone);
    if !target.is_file() {return Err("Timezone is not installed".into());}
    let link=Path::new("/etc/localtime.lumen-new");
    if link.symlink_metadata().is_ok() {fs::remove_file(link).map_err(|e|e.to_string())?;}
    std::os::unix::fs::symlink(&target,link).map_err(|e|e.to_string())?;
    fs::rename(link,"/etc/localtime").map_err(|e|e.to_string())?;
    fs::create_dir_all("/var/lib/lumen-location").map_err(|e|e.to_string())?;
    fs::write("/var/lib/lumen-location/current.json.new",current.to_string()).map_err(|e|e.to_string())?;
    fs::rename("/var/lib/lumen-location/current.json.new","/var/lib/lumen-location/current.json").map_err(|e|e.to_string())?;
    println!("Local timezone refreshed: {zone}"); Ok(())
}
fn validate(input:&Value)->Result<Value,String> {
    if input["success"]!=true {return Err("Location provider could not determine location".into());}
    let zone=input["timezone"]["id"].as_str().ok_or("Missing timezone")?;
    let _:chrono_tz::Tz=zone.parse().map_err(|_|"Unknown timezone")?;
    let latitude=input["latitude"].as_f64().filter(|v|v.is_finite()&&(-90.0..=90.0).contains(v)).ok_or("Invalid latitude")?;
    let longitude=input["longitude"].as_f64().filter(|v|v.is_finite()&&(-180.0..=180.0).contains(v)).ok_or("Invalid longitude")?;
    Ok(json!({"timezone":zone,"latitude":latitude,"longitude":longitude,"city":input["city"].as_str().unwrap_or("Current location").chars().take(100).collect::<String>(),"source":"ipwho.is","updatedAt":chrono::Utc::now().to_rfc3339()}))
}
fn main(){if let Err(error)=run(){eprintln!("{error}");std::process::exit(1);}}
#[cfg(test)] mod tests {
    use super::*;
    #[test] fn rejects_invalid_location(){
        for input in [json!({"success":false}),json!({"success":true,"timezone":{"id":"../../etc/passwd"},"latitude":43.7,"longitude":-79.4}),json!({"success":true,"timezone":{"id":"America/Toronto"},"latitude":200,"longitude":0})] {assert!(validate(&input).is_err());}
    }
    #[test] fn retains_only_needed_fields(){let value=validate(&json!({"success":true,"timezone":{"id":"America/Toronto"},"latitude":43.7,"longitude":-79.4,"city":"Toronto","ip":"private"})).unwrap();assert_eq!(value["timezone"],"America/Toronto");assert!(value.get("ip").is_none());}
}
