//! Read-only security facts. Missing evidence never becomes an enabled badge.
use serde_json::{Value, json};
use std::{env, fs, path::Path, process::Command, time::Duration};

const EFI_GUID: &str = "8be4df61-93ca-11d2-aa0d-00e098032b8c";

fn command_json(program: &str, args: &[&str]) -> Result<Value, String> {
    let output = Command::new("timeout").arg("4s").arg(program).args(args)
        .output().map_err(|error| error.to_string())?;
    if !output.status.success() { return Err(format!("{program} failed")); }
    serde_json::from_slice(&output.stdout).map_err(|error| error.to_string())
}

fn encrypted(items: &[Value]) -> bool {
    !items.is_empty() && items.iter().all(|item| {
        item.get("type").and_then(Value::as_str) == Some("crypt")
            || item.get("children").and_then(Value::as_array).is_some_and(|children| encrypted(children))
    })
}

fn encryption_status_with(mut run: impl FnMut(&str, &[&str]) -> Result<Value, String>) -> Value {
    let result = (|| -> Result<bool, String> {
        let mounts = run("findmnt", &["--json", "--mountpoint", "/persist", "--output", "SOURCE,FSTYPE"])?;
        let source = mounts["filesystems"][0]["source"].as_str().ok_or("No source")?
            .split('[').next().ok_or("No source")?;
        if !source.starts_with("/dev/") { return Err("Not a block device".into()); }
        let devices = run("lsblk", &["--json", "--inverse", "--tree", "--output", "TYPE", source])?;
        let paths = devices["blockdevices"].as_array().ok_or("No block devices")?;
        Ok(encrypted(paths))
    })();
    match result {
        Ok(true) => json!({"state":"enabled","value":"Encrypted","detail":"Persistent storage is mounted through an encrypted device. Other disks and removable drives are separate."}),
        Ok(false) => json!({"state":"unknown","value":"Not confirmed","detail":"No active device-encryption layer was found for persistent storage. Other forms of encryption are not assessed here."}),
        Err(_) => json!({"state":"unknown","value":"Unavailable","detail":"The encryption status of persistent storage could not be read."}),
    }
}

fn firmware_flag(directory: &Path, name: &str) -> Result<u8, String> {
    let bytes = fs::read(directory.join(format!("{name}-{EFI_GUID}"))).map_err(|error| error.to_string())?;
    if bytes.len() != 5 || bytes[4] > 1 { return Err("Invalid firmware flag".into()); }
    Ok(bytes[4])
}

fn secure_boot_status(directory: &Path) -> Value {
    match (firmware_flag(directory, "SecureBoot"), firmware_flag(directory, "SetupMode")) {
        (Ok(1), Ok(0)) => json!({"state":"enabled","value":"On","detail":"Firmware reports Secure Boot is enabled for this boot."}),
        (Ok(0), Ok(_)) => json!({"state":"disabled","value":"Off","detail":"Firmware reports Secure Boot is disabled."}),
        _ => json!({"state":"unknown","value":"Unavailable","detail":"Firmware status could not be read. A configured bootloader alone does not confirm Secure Boot is enabled."}),
    }
}

const DETAIL: &str = "Enrollment and authentication policy are not assessed. Password authentication remains managed by the host.";

fn biometric_fact(state: &str, value: &str, detail: &str) -> Value {
    json!({"state":state,"value":value,"detail":format!("{detail} {DETAIL}")})
}

fn reported_devices(devices: &[String]) -> Option<Value> {
    if devices.len() > 64 || devices.iter().any(|path| path.len() > 256 || !path.starts_with("/net/reactivated/Fprint/Device/")) {
        return None;
    }
    let unique = devices.iter().collect::<std::collections::HashSet<_>>();
    if unique.len() != devices.len() { return None; }
    Some(if devices.is_empty() {
        biometric_fact("unavailable", "No reader reported", "The active fingerprint provider reports no reader. Other biometric providers are not assessed.")
    } else {
        biometric_fact("reported", "Reader reported", "The active fingerprint provider reports a reader; this does not establish that biometric unlock is configured or working.")
    })
}

async fn biometric_discover_with(conn: &zbus::Connection, trusted_uid: u32) -> Result<Value, String> {
    let bus = zbus::Proxy::new(conn,"org.freedesktop.DBus","/org/freedesktop/DBus","org.freedesktop.DBus").await.map_err(|e|e.to_string())?;
    let service = "net.reactivated.Fprint";
    let active: bool = bus.call("NameHasOwner", &(service,)).await.map_err(|e|e.to_string())?;
    if !active { return Ok(biometric_fact("unknown", "Provider inactive", "Fingerprint status is unavailable while its provider is inactive. This check does not start it.")); }
    let owner: String = bus.call("GetNameOwner", &(service,)).await.map_err(|e|e.to_string())?;
    if !owner.starts_with(':') { return Err("Untrusted provider owner".into()); }
    let uid: u32 = bus.call("GetConnectionUnixUser", &(owner.as_str(),)).await.map_err(|e|e.to_string())?;
    if uid != trusted_uid { return Err("Untrusted provider owner".into()); }
    let provider = zbus::Proxy::new(conn,owner.as_str(),"/net/reactivated/Fprint/Manager","net.reactivated.Fprint.Manager").await.map_err(|e|e.to_string())?;
    let paths: Vec<zbus::zvariant::OwnedObjectPath> = provider.call("GetDevices", &()).await.map_err(|e|e.to_string())?;
    let devices = paths.iter().map(ToString::to_string).collect::<Vec<_>>();
    let observed = reported_devices(&devices).ok_or("Invalid reader report")?;
    let current: String = bus.call("GetNameOwner", &(service,)).await.map_err(|e|e.to_string())?;
    if current != owner { return Err("Provider changed".into()); }
    Ok(observed)
}

async fn biometric_discover() -> Result<Value, String> {
    let conn = zbus::Connection::system().await.map_err(|e|e.to_string())?;
    biometric_discover_with(&conn, 0).await
}

async fn biometric_status() -> Value {
    tokio::time::timeout(Duration::from_millis(1500), biometric_discover()).await.ok()
        .and_then(Result::ok)
        .unwrap_or_else(||biometric_fact("unknown", "Unavailable", "Fingerprint reader status could not be established."))
}

#[tokio::main]
async fn main() {
    let directory = env::var("LUMEN_EFIVARS_DIR").unwrap_or_else(|_| "/sys/firmware/efi/efivars".into());
    println!("{}", json!({
        "encryption": encryption_status_with(command_json),
        "secureBoot": secure_boot_status(Path::new(&directory)),
        "biometrics": biometric_status().await,
    }));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;
    use std::sync::{Arc, atomic::{AtomicBool, AtomicUsize, Ordering}};
    #[derive(Clone)] struct Reader {present:Arc<AtomicBool>, calls:Arc<AtomicUsize>}
    #[zbus::interface(name="net.reactivated.Fprint.Manager")]
    impl Reader {
        fn get_devices(&self) -> Vec<zbus::zvariant::OwnedObjectPath> {
            self.calls.fetch_add(1,Ordering::SeqCst);
            if self.present.load(Ordering::SeqCst) {
                vec![zbus::zvariant::OwnedObjectPath::try_from("/net/reactivated/Fprint/Device/0").unwrap()]
            } else { Vec::new() }
        }
    }
    #[test]
    fn private_reader_bus_only_queries_devices() {
        if env::var_os("LUMEN_PRIVACY_BUS_CHILD").is_none() {
            let mut runner=Command::new("dbus-run-session");
            if let Ok(config)=env::var("LUMEN_PRIVACY_TEST_DBUS_CONFIG") {runner.arg(format!("--config-file={config}"));}
            let output=runner.arg("--").arg(env::current_exe().unwrap())
                .args(["--exact","tests::private_reader_bus_only_queries_devices","--nocapture"])
                .env("LUMEN_PRIVACY_BUS_CHILD","1").output().unwrap();
            assert!(output.status.success(),"{}{}",String::from_utf8_lossy(&output.stdout),String::from_utf8_lossy(&output.stderr));
            return;
        }
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            let conn=zbus::Connection::session().await.unwrap();
            let uid=unsafe{libc::geteuid()};
            let inactive=biometric_discover_with(&conn,uid).await.unwrap();
            assert_eq!(inactive["value"],"Provider inactive");
            let present=Arc::new(AtomicBool::new(false));
            let calls=Arc::new(AtomicUsize::new(0));
            let _provider=zbus::connection::Builder::session().unwrap().name("net.reactivated.Fprint").unwrap()
                .serve_at("/net/reactivated/Fprint/Manager",Reader{present:present.clone(),calls:calls.clone()}).unwrap().build().await.unwrap();
            assert_eq!(biometric_discover_with(&conn,uid).await.unwrap()["state"],"unavailable");
            present.store(true,Ordering::SeqCst);
            assert_eq!(biometric_discover_with(&conn,uid).await.unwrap()["state"],"reported");
            assert_eq!(calls.load(Ordering::SeqCst),2);
        });
    }
    #[test]
    fn all_backing_paths_must_be_encrypted() {
        assert!(encrypted(&[json!({"type":"lvm","children":[{"type":"crypt"}]})]));
        assert!(!encrypted(&[json!({"type":"lvm","children":[{"type":"crypt"},{"type":"part"}]})]));
        assert!(!encrypted(&[]));
    }
    #[test]
    fn strips_btrfs_suffix_and_fails_closed() {
        let mut seen = String::new();
        let status = encryption_status_with(|program, args| {
            if program == "findmnt" { Ok(json!({"filesystems":[{"source":"/dev/mapper/main[/persist]"}]})) }
            else { seen = args.last().unwrap().to_string(); Ok(json!({"blockdevices":[{"type":"crypt"}]})) }
        });
        assert_eq!(status["state"], "enabled");
        assert_eq!(seen, "/dev/mapper/main");
        assert_eq!(encryption_status_with(|_, _| Err("failed".into()))["state"], "unknown");
    }
    #[test]
    fn firmware_requires_consistent_flags() {
        let directory = env::temp_dir().join(format!("lumen-privacy-{}", std::process::id()));
        fs::create_dir_all(&directory).unwrap();
        let write = |name: &str, value: u8| fs::write(directory.join(format!("{name}-{EFI_GUID}")), [7, 0, 0, 0, value]).unwrap();
        assert_eq!(secure_boot_status(&directory)["state"], "unknown");
        write("SecureBoot", 1); write("SetupMode", 0);
        assert_eq!(secure_boot_status(&directory)["state"], "enabled");
        write("SetupMode", 1);
        assert_eq!(secure_boot_status(&directory)["state"], "unknown");
        write("SecureBoot", 0);
        assert_eq!(secure_boot_status(&directory)["state"], "disabled");
        write("SecureBoot", 2);
        assert_eq!(secure_boot_status(&directory)["state"], "unknown");
        fs::remove_dir_all(directory).unwrap();
    }
    #[test]
    fn reader_reports_are_bounded_and_do_not_claim_authentication() {
        assert_eq!(reported_devices(&[]).unwrap()["state"],"unavailable");
        let one=vec!["/net/reactivated/Fprint/Device/0".to_owned()];
        let reported=reported_devices(&one).unwrap();
        assert_eq!(reported["state"],"reported");
        assert!(reported["detail"].as_str().unwrap().contains("does not establish"));
        assert!(!reported.to_string().contains("/Device/"));
        assert!(reported_devices(&[one[0].clone(),one[0].clone()]).is_none());
        assert!(reported_devices(&["not-a-reader".to_owned()]).is_none());
        assert!(reported_devices(&vec![one[0].clone();65]).is_none());
    }
}
