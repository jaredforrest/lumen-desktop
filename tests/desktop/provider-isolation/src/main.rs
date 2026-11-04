//! Replace unrelated provider seams only in disposable fixture copies.
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::env;
use std::ffi::OsStr;
use std::fs::{self, OpenOptions};
use std::io::{self, BufRead, Write};
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::{Path, PathBuf};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

struct Provider {
    filename: &'static str,
    needle: &'static str,
    kind: &'static str,
}

const PROVIDERS: [Provider; 2] = [
    Provider {
        filename: "VpnState.qml",
        needle: "\"lumen-vpn-control\"",
        kind: "vpn",
    },
    Provider {
        filename: "SystemState.qml",
        needle: "\"lumen-system-controls\"",
        kind: "system",
    },
];

fn digest(data: &[u8]) -> String {
    format!("{:x}", Sha256::digest(data))
}

fn payload(kind: &str) -> Result<Value> {
    match kind {
        "vpn" => Ok(
            json!({"mullvad":{"installed":false,"state":"unavailable","account":false},
            "tailscale":{"installed":false,"state":"unavailable","online":false,"peers":0,"exitNodes":[]}}),
        ),
        "system" => Ok(
            json!({"wifi":{"available":false,"networks":[]},"bluetoothRadios":{},"power":{"available":false}}),
        ),
        _ => Err("Unknown fixture provider".into()),
    }
}

fn serve(kind: &str, output: &Path, command: Option<&str>, input: &str) -> Result<String> {
    let request: Value = if kind == "vpn" {
        serde_json::from_str(input)?
    } else {
        json!({"action":command.unwrap_or("")})
    };
    let mut log = OpenOptions::new()
        .create(true)
        .append(true)
        .open(output.join(format!("{kind}-provider-requests.jsonl")))?;
    writeln!(log, "{}", request)?;
    if kind=="system" && request["action"]=="displays" && env::var("LUMEN_REVIEW_POPULATED").as_deref()==Ok("1") {
        return Ok(format!("{}\n",json!({"displays":[{"bus":"review","name":"Studio Display","level":0.72,"maximum":100}]})));
    }
    if request["action"] != "status" {
        return Err("Unexpected fixture provider action".into());
    }
    let data=if kind=="system" && env::var("LUMEN_REVIEW_POPULATED").as_deref()==Ok("1") {
        json!({"wifi":{"available":true,"phase":"connected","connectivity":"full","name":"Studio Wi-Fi","networks":[{"name":"Studio Wi-Fi","connected":true,"signal":92}]},"bluetoothRadios":{},"power":{"available":true,"present":true,"percentage":82,"state":"discharging","secondsToEmpty":14400}})
    } else { payload(kind)? };
    Ok(format!("{data}\n"))
}

fn calendar(input: &str) -> Result<String> {
    let request: Value = serde_json::from_str(input)?;
    if !["list", "load", "sync"]
        .iter()
        .any(|action| request["action"] == *action)
    {
        return Err("Unexpected fixture calendar action".into());
    }
    Ok(format!(
        "{}\n",
        json!({
            "configured": false, "events": [], "monthEvents": [], "days": [],
            "total": 0, "providers": [], "month": request["month"],
            "date": request["selectedDate"],
        })
    ))
}

fn populated_calendar(input: &str) -> Result<String> {
    let request: Value = serde_json::from_str(input)?;
    if !["list", "load", "sync"].iter().any(|action| request["action"] == *action) {
        return Err("Review calendar rejects mutations".into());
    }
    let month = request["month"].as_str().ok_or("Missing review month")?;
    let date = request["selectedDate"].as_str().ok_or("Missing review date")?;
    if !month.is_ascii() || !date.is_ascii() || month.len() < 7 || date.len() != 10 || !date.starts_with(&month[..7]) {
        return Err("Review calendar requires a selected date in the requested month".into());
    }
    let prefix = &month[..7];
    let mut days = vec![2, 4, 8, 12, 18, 22, 27];
    days.push(date[8..].parse::<u32>()?);
    days.sort_unstable();
    days.dedup();
    let mut events = Vec::new();
    for day in days {
        for (slot, (title, calendar, start, end, all_day)) in [
            ("Design review", "Studio", "09:30", "10:30", false),
            ("Sketch workshop", "Studio", "10:00", "11:00", false),
            ("Lunch with Sam", "Personal", "12:30", "13:30", false),
            ("Release planning", "Studio", "14:00", "15:00", false),
            ("Focus day", "Personal", "00:00", "23:59", true),
        ].into_iter().enumerate() {
            let day = format!("{prefix}-{day:02}");
            events.push(json!({"id":format!("review-{day}-{slot}"),"title":title,"calendar":calendar,
                "calendarId":calendar,"provider":"Sample","date":day,"endDate":day,
                "startTime":start,"endTime":end,"allDay":all_day,"location":"Studio",
                "notes":"Synthetic release-review event. No personal account is connected.","editable":false}));
        }
    }
    events.push(json!({"id":"review-residency","title":"Studio residency","calendar":"Personal","calendarId":"Personal","provider":"Sample","date":format!("{prefix}-02"),"endDate":format!("{prefix}-05"),"startTime":"00:00","endTime":"23:59","allDay":true,"location":"Studio","notes":"Synthetic multi-day event.","editable":false}));
    let selected: Vec<_> = events.iter().filter(|event| event["date"].as_str().unwrap_or("") <= date && event["endDate"].as_str().unwrap_or("") >= date).cloned().collect();
    Ok(format!("{}\n", json!({"configured":true,"events":selected,"monthEvents":events,
        "total":selected.len(),"month":month,"date":date,"timezone":"Europe/London",
        "lastSuccess":format!("{date}T08:00:00Z"),"providers":[{"id":"sample","name":"Sample calendars","configured":true}]})))
}

fn review_settings(path: &Path, args: &[String]) -> Result<String> {
    if fs::symlink_metadata(path)?.file_type().is_symlink() { return Err("Review settings cannot follow symlinks".into()); }
    let mut data:Value=serde_json::from_slice(&fs::read(path)?)?;
    match args.first().map(String::as_str) {
        Some("read") if args.len()==1 => {},
        Some("set" | "patch") if args.len()==3 && data.get(&args[1]).is_some() => {
            let next:Value=serde_json::from_str(&args[2])?;
            if args[0]=="patch" {
                let patch=next.as_object().ok_or("Review patch must be an object")?;
                let section=data[&args[1]].as_object_mut().ok_or("Review section must be an object")?;
                for (key,value) in patch {section.insert(key.clone(),value.clone());}
            } else {data[&args[1]]=next;}
            let pending=path.with_extension("pending");
            fs::write(&pending,serde_json::to_vec(&data)?)?;
            fs::rename(pending,path)?;
        },
        _ => return Err("Unsupported private review settings request".into()),
    }
    Ok(format!("{data}\n"))
}

fn review_displays(input:&str) -> Result<String> {
    let request:Value=serde_json::from_str(input)?;
    if request["action"]!="status" {return Err("Review display discovery rejects mutations".into());}
    Ok(format!("{}\n",json!({"outputs":[
        {"name":"DP-1","label":"Studio Display","mode":"2560x1440@60.000Hz","modes":["2560x1440@60.000Hz","1920x1080@60.000Hz"],"scale":1.25,"x":0,"y":0,"enabled":true},
        {"name":"eDP-1","label":"Laptop Display","mode":"1920x1080@60.000Hz","modes":["1920x1080@60.000Hz"],"scale":1,"x":2048,"y":360,"enabled":true}
    ],"pending":"","seconds":0})))
}

struct Prepared {
    target: PathBuf,
    helper: PathBuf,
    original: String,
    modified: String,
}

fn isolate(shell: &Path, output: &Path, executable: &Path) -> Result<Value> {
    let shell = fs::canonicalize(shell)?;
    let output = fs::canonicalize(output)?;
    if shell == output || !shell.starts_with(&output) {
        return Err("Provider fixtures require a private copied shell".into());
    }
    let mut prepared = Vec::new();
    for provider in &PROVIDERS {
        let target = shell.join(provider.filename);
        if fs::symlink_metadata(&target)?.file_type().is_symlink()
            || !target.is_file()
            || !fs::canonicalize(&target)?.starts_with(&output)
        {
            return Err("Provider fixture target must be a private regular copy".into());
        }
        let original = fs::read_to_string(&target)?;
        if original.matches(provider.needle).count() != 1 {
            return Err(format!("Provider command seam changed: {}", provider.filename).into());
        }
        let helper = output.join(format!("{}-unavailable-provider", provider.kind));
        if helper.exists() || fs::symlink_metadata(&helper).is_ok() {
            return Err(format!("Provider helper already exists: {}", helper.display()).into());
        }
        let replacement = serde_json::to_string(&helper.to_string_lossy())?;
        let modified = original.replacen(provider.needle, &replacement, 1);
        if modified.replacen(&replacement, provider.needle, 1) != original {
            return Err("Provider seam roundtrip failed".into());
        }
        prepared.push(Prepared {
            target,
            helper,
            original,
            modified,
        });
    }
    let helper_hash = digest(&fs::read(executable)?);
    let mut records = Vec::new();
    for item in prepared {
        symlink(executable, &item.helper)?;
        fs::set_permissions(&item.target, fs::Permissions::from_mode(0o600))?;
        fs::write(&item.target, &item.modified)?;
        records.push(json!({
            "file":item.target,"originalSha256":digest(item.original.as_bytes()),
            "modifiedSha256":digest(item.modified.as_bytes()),"helper":item.helper,
            "helperSha256":helper_hash,"roundtrip":true,"originalMatchCount":1,
            "scope":"Only unrelated provider command target; non-status requests rejected"
        }));
    }
    let value = Value::Array(records);
    fs::write(
        output.join("provider-isolation.json"),
        format!("{}\n", serde_json::to_string_pretty(&value)?),
    )?;
    Ok(value)
}

fn main() -> Result<()> {
    let executable = env::current_exe()?;
    let invoked = PathBuf::from(env::args_os().next().ok_or("Missing argv0")?);
    let name = invoked
        .file_name()
        .and_then(OsStr::to_str)
        .unwrap_or_default();
    if name=="lumen-fixture-settings" || name=="lumen-nix-settings" {
        let path=PathBuf::from(env::var_os("LUMEN_REVIEW_SETTINGS").ok_or("Missing private settings path")?);
        print!("{}",review_settings(&path,&env::args().skip(1).collect::<Vec<_>>())?);
        return Ok(());
    }
    if env::var("LUMEN_REVIEW_POPULATED").as_deref()==Ok("1") && name=="lumen-privacy-status" {
        println!("{}",json!({"encryption":{"value":"On","detail":"Synthetic encrypted test disk."},"secureBoot":{"value":"On","detail":"Synthetic verified boot state."},"biometrics":{"value":"Not configured","detail":"No fingerprint reader is connected to this test session."}}));return Ok(());
    }
    if env::var("LUMEN_REVIEW_POPULATED").as_deref()==Ok("1") && name=="lumen-portal-permissions" {
        let mut line=String::new();io::stdin().lock().read_line(&mut line)?;let request:Value=serde_json::from_str(&line)?;
        if request["action"]!="list" {return Err("Review permissions reject mutations".into());}
        println!("{}",json!({"available":true,"rows":[],"coverage":[{"capability":"screen capture","state":"available"}]}));return Ok(());
    }
    if env::var("LUMEN_REVIEW_POPULATED").as_deref()==Ok("1") && name=="lumen-system-controls" {
        let output=invoked.parent().and_then(Path::parent).ok_or("Missing review output")?;
        print!("{}",serve("system",output,env::args().nth(1).as_deref(),"")?);return Ok(());
    }
    if env::var("LUMEN_REVIEW_POPULATED").as_deref()==Ok("1") && name=="lumen-display-settings" {
        let mut line=String::new();io::stdin().lock().read_line(&mut line)?;
        print!("{}",review_displays(&line)?);return Ok(());
    }
    if env::var("LUMEN_REVIEW_POPULATED").as_deref()==Ok("1") && name=="lumen-nearby-sharing" {
        if env::args().nth(1).as_deref()!=Some("status") {return Err("Review sharing rejects actions".into());}
        println!("{}",json!({"available":true,"active":true,"canSend":true,"detail":"Contacts Only"}));return Ok(());
    }
    if env::var("LUMEN_REVIEW_POPULATED").as_deref()==Ok("1") && name=="desktop-network-diagnose" {
        if !matches!(env::args().nth(1).as_deref(),Some("--cached" | "--refresh")) {return Err("Review network discovery is read-only".into());}
        println!("{}",json!({"status":"online","reason":"healthy","message":"Connected via Studio Wi-Fi","routes":{"4":{"available":true,"interface":"wlan0"},"6":{"available":false,"interface":""}},"dns":{"status":"resolved"},"vpn":{"state":"disconnected","selected_route":false,"locked_down":false},"checks":[{"ok":true}]}));return Ok(());
    }
    if name == "lumen-fixture-calendar" || name == "lumen-calendar" {
        let mut line = String::new();
        io::stdin().lock().read_line(&mut line)?;
        print!("{}", if env::var("LUMEN_REVIEW_POPULATED").as_deref() == Ok("1") { populated_calendar(&line)? } else { calendar(&line)? });
        return Ok(());
    }
    if let Some(kind) = name.strip_suffix("-unavailable-provider") {
        let mut line = String::new();
        if kind == "vpn" {
            io::stdin().lock().read_line(&mut line)?;
        }
        let command = env::args().nth(1);
        let output = invoked.parent().ok_or("Provider helper has no parent")?;
        print!("{}", serve(kind, output, command.as_deref(), &line)?);
        return Ok(());
    }
    let mut args = env::args().skip(1);
    if args.next().as_deref() != Some("isolate") {
        return Err("Usage: lumen-fixture-providers isolate SHELL OUTPUT".into());
    }
    let shell = PathBuf::from(args.next().ok_or("Missing shell")?);
    let output = PathBuf::from(args.next().ok_or("Missing output")?);
    if args.next().is_some() {
        return Err("Unexpected argument".into());
    }
    println!("{}", isolate(&shell, &output, &executable)?);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn review_settings_persist_and_reject_unknown_sections() {
        let root=tempfile::tempdir().unwrap();let path=root.path().join("settings.json");
        fs::write(&path,r#"{"appearance":{"reduceMotion":false}}"#).unwrap();
        review_settings(&path,&["set".into(),"appearance".into(),r#"{"reduceMotion":true}"#.into()]).unwrap();
        let read:Value=serde_json::from_str(&review_settings(&path,&["read".into()]).unwrap()).unwrap();
        assert_eq!(read["appearance"]["reduceMotion"],true);
        review_settings(&path,&["patch".into(),"appearance".into(),r#"{"reduceTransparency":true}"#.into()]).unwrap();
        let read:Value=serde_json::from_str(&review_settings(&path,&["read".into()]).unwrap()).unwrap();
        assert_eq!(read["appearance"]["reduceMotion"],true);
        assert_eq!(read["appearance"]["reduceTransparency"],true);
        assert!(review_settings(&path,&["set".into(),"unknown".into(),"{}".into()]).is_err());
        let link=root.path().join("link.json");symlink(&path,&link).unwrap();
        assert!(review_settings(&link,&["read".into()]).is_err());
    }
    #[test]
    fn review_displays_are_multiple_and_cannot_change_real_outputs() {
        let result:Value=serde_json::from_str(&review_displays(r#"{"action":"status"}"#).unwrap()).unwrap();
        assert_eq!(result["outputs"].as_array().unwrap().len(),2);
        assert!(review_displays(r#"{"action":"apply"}"#).is_err());
    }
    #[test]
    fn review_calendar_is_populated_and_read_only() {
        let input=r#"{"action":"list","month":"2026-11-01","selectedDate":"2026-11-03"}"#;
        let response:Value=serde_json::from_str(&populated_calendar(input).unwrap()).unwrap();
        assert_eq!(response["configured"],true);
        assert_eq!(response["events"].as_array().unwrap().len(),6);
        assert!(response["events"].as_array().unwrap().iter().any(|e| e["date"] != e["endDate"]));
        assert!(response["events"].as_array().unwrap().iter().any(|e| e["startTime"] == "10:00"));
        assert!(response["monthEvents"].as_array().unwrap().len()>20);
        assert!(response["events"].as_array().unwrap().iter().any(|e|e["allDay"]==true));
        assert!(populated_calendar(r#"{"action":"create"}"#).is_err());
        assert!(populated_calendar(r#"{"action":"list","month":"2026-11-01","selectedDate":"2026-12-03"}"#).is_err());
    }
    #[test]
    fn status_and_rejection() {
        let root = tempfile::tempdir().unwrap();
        assert_eq!(
            serve("vpn", root.path(), None, "{\"action\":\"status\"}")
                .unwrap()
                .trim()
                .parse::<Value>()
                .unwrap(),
            payload("vpn").unwrap()
        );
        assert!(serve("vpn", root.path(), None, "{\"action\":\"disconnect\"}").is_err());
        assert_eq!(
            serve("system", root.path(), Some("status"), "")
                .unwrap()
                .trim()
                .parse::<Value>()
                .unwrap(),
            payload("system").unwrap()
        );
        assert!(serve("system", root.path(), Some("disconnect"), "").is_err());
        let log = fs::read_to_string(root.path().join("vpn-provider-requests.jsonl")).unwrap();
        assert_eq!(log.lines().count(), 2);
    }
    #[test]
    fn calendar_absent_account_contract() {
        let response: Value = serde_json::from_str(
            &calendar(r#"{"action":"load","month":"2026-10","selectedDate":"2026-10-25"}"#)
                .unwrap(),
        )
        .unwrap();
        assert_eq!(
            response,
            json!({
                "configured":false,"events":[],"monthEvents":[],"days":[],"total":0,
                "providers":[],"month":"2026-10","date":"2026-10-25"
            })
        );
        assert!(calendar(r#"{"action":"account-list"}"#).is_err());
        assert_eq!(
            serde_json::from_str::<Value>(&calendar(r#"{"action":"list"}"#).unwrap()).unwrap()["month"],
            Value::Null
        );
    }
    #[test]
    fn copy_and_seam_boundaries() {
        let root = tempfile::tempdir().unwrap();
        let shell = root.path().join("assets/shell");
        fs::create_dir_all(&shell).unwrap();
        for provider in &PROVIDERS {
            fs::write(
                shell.join(provider.filename),
                format!("command: [{}]", provider.needle),
            )
            .unwrap();
        }
        let binary = env::current_exe().unwrap();
        assert_eq!(
            isolate(&shell, root.path(), &binary)
                .unwrap()
                .as_array()
                .unwrap()
                .len(),
            2
        );
        assert!(shell.join("VpnState.qml").is_file());
        assert!(
            !fs::read_to_string(shell.join("VpnState.qml"))
                .unwrap()
                .contains("lumen-vpn-control")
        );
        let other = root.path().join("other");
        fs::create_dir(&other).unwrap();
        assert!(isolate(&shell, &other, &binary).is_err());
    }
    #[test]
    fn changed_seam_does_not_rewrite_either_target() {
        let root = tempfile::tempdir().unwrap();
        let shell = root.path().join("assets/shell");
        fs::create_dir_all(&shell).unwrap();
        fs::write(shell.join("VpnState.qml"), PROVIDERS[0].needle).unwrap();
        fs::write(shell.join("SystemState.qml"), "changed provider").unwrap();
        let before = fs::read(shell.join("VpnState.qml")).unwrap();
        assert!(isolate(&shell, root.path(), &env::current_exe().unwrap()).is_err());
        assert_eq!(fs::read(shell.join("VpnState.qml")).unwrap(), before);
        assert!(!root.path().join("vpn-unavailable-provider").exists());
    }
    #[test]
    fn symlink_target_is_rejected() {
        let root = tempfile::tempdir().unwrap();
        let shell = root.path().join("assets/shell");
        fs::create_dir_all(&shell).unwrap();
        let external = root.path().join("original.qml");
        fs::write(&external, PROVIDERS[0].needle).unwrap();
        symlink(&external, shell.join("VpnState.qml")).unwrap();
        fs::write(shell.join("SystemState.qml"), PROVIDERS[1].needle).unwrap();
        assert!(isolate(&shell, root.path(), &env::current_exe().unwrap()).is_err());
        assert_eq!(fs::read(&external).unwrap(), PROVIDERS[0].needle.as_bytes());
    }
}
