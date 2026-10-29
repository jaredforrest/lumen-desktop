//! Real disposable systemd user-manager restart-policy acceptance.
use std::{env, fs, io::{self, Write}, os::{fd::AsRawFd, unix::fs::{FileExt, MetadataExt}}, path::Path, process::{Command, Output}, thread, time::{Duration, Instant}};
unsafe extern "C" { fn tcdrain(fd: i32) -> i32; fn geteuid() -> u32; }
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
const HOME: &str = "/home/fixture";
fn require(ok: bool, message: &str) -> Result<()> { if ok { Ok(()) } else { Err(message.into()) } }
fn user(arguments: &[&str]) -> Result<Output> {
    Ok(Command::new(env!("VM_RUNUSER")).args(["-u", "fixture", "--", env!("VM_ENV"),
        "XDG_RUNTIME_DIR=/run/user/1000", "DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/1000/bus",
        env!("VM_SYSTEMCTL"), "--user"]).args(arguments).output()?)
}
fn checked(arguments: &[&str]) -> Result<String> {
    let output=user(arguments)?;
    require(output.status.success(), &format!("User command {arguments:?} failed: {}",String::from_utf8_lossy(&output.stderr)))?;
    Ok(String::from_utf8(output.stdout)?.trim().into())
}
fn property(unit: &str, key: &str) -> Result<String> { checked(&["show", unit, "--property", key, "--value"]) }
fn wait(label: &str, mut predicate: impl FnMut()->Result<bool>) -> Result<()> {
    let deadline=Instant::now()+Duration::from_secs(30);
    while Instant::now()<deadline { if predicate().unwrap_or(false) {return Ok(());} thread::sleep(Duration::from_millis(50)); }
    Err(format!("Timed out: {label}").into())
}
fn count(path: &str) -> u32 { fs::read_to_string(path).ok().and_then(|s|s.parse().ok()).unwrap_or(0) }
fn start_time(pid: &str) -> Result<String> {
    let stat=fs::read_to_string(format!("/proc/{pid}/stat"))?;
    Ok(stat.rsplit_once(") ").ok_or("Process status malformed")?.1.split_whitespace().nth(19).ok_or("Missing process start time")?.into())
}
fn inspect_filesystem() -> Result<()> {
    println!("FILESYSTEM_DIAGNOSTIC uid={}",unsafe {geteuid()});
    for name in ["/etc", "/etc/static", "/etc/systemd", "/etc/systemd/user", "/etc/systemd/user/recovery-test-session.target", "/etc/systemd/user/graphical-session.target", "/nix", "/nix/store"] {
        match fs::metadata(name) {
            Ok(info)=>println!("FILESYSTEM_DIAGNOSTIC {name}: uid={} gid={} mode={:o} link={:?}",info.uid(),info.gid(),info.mode(),fs::read_link(name)),
            Err(error)=>println!("FILESYSTEM_DIAGNOSTIC {name}: {error}"),
        }
        if name.ends_with(".target") {println!("FILESYSTEM_READ {name}: {:?}",fs::read_to_string(name));}
    }
    Ok(())
}
fn driver() -> Result<()> {
    wait("actual user manager", || Ok(user(&["show-environment"])?.status.success()))?;
    let paths=checked(&["show", "--property=UnitPath", "--value"])?;
    require(paths.split_whitespace().any(|path|path=="/etc/systemd/user"), "Acceptance did not reach the actual user manager")?;
    println!("RECOVERY_MANAGER_PATHS: {paths}");
    checked(&["start", "recovery-test-session.target"])?;
    require(property("lumen-desktop-shell.service", "OnFailure")?=="lumen-desktop-recovery.service", "Enabled fallback is not connected in the real manager")?;
    require(property("recovery-disabled.service", "OnFailure")?.is_empty(), "Disabled fallback still has an actual OnFailure handler")?;
    checked(&["start", "recovery-client.service"])?;
    let client=property("recovery-client.service","MainPID")?;
    require(client!="0","Client process missing")?;
    let identity=start_time(&client)?;
    checked(&["start", "lumen-desktop-shell.service"])?;
    wait("restart limit", || Ok(property("lumen-desktop-shell.service","Result")?=="start-limit-hit"))?;
    wait("independent fallback terminal", || Ok(Path::new("/home/fixture/fallback-shell").is_file()))?;
    let fallback=property("lumen-desktop-recovery.service", "MainPID")?;
    require(fallback!="0", "Fallback process missing")?;
    let fallback_identity=start_time(&fallback)?;
    require(count("/home/fixture/attempts")==3,"Restart budget did not allow exactly three starts")?;
    require(property("lumen-desktop-shell.service","ActiveState")?=="failed","Failed shell still active")?;
    let receipt=fs::metadata("/home/fixture/fallback-terminal")?.modified()?;
    thread::sleep(Duration::from_secs(3));
    require(count("/home/fixture/attempts")==3 && fs::metadata("/home/fixture/fallback-terminal")?.modified()?==receipt,"Restart or fallback continued after the limit")?;
    require(property("recovery-client.service","MainPID")?==client && start_time(&client)?==identity,"Independent client died or restarted")?;
    println!("RECOVERY_CHECK: three bounded starts, independent fallback, client identity preserved");
    fs::write(format!("{HOME}/healthy"),b"healthy")?;
    checked(&["reset-failed","lumen-desktop-shell.service"])?;
    checked(&["start","lumen-desktop-shell.service"])?;
    wait("manual recovered shell", || Ok(property("lumen-desktop-shell.service","ActiveState")?=="active" && count("/home/fixture/attempts")==4))?;
    require(fs::metadata("/home/fixture/fallback-terminal")?.modified()?==receipt,"Manual retry opened another fallback")?;
    require(property("lumen-desktop-recovery.service", "ActiveState")?=="active" && property("lumen-desktop-recovery.service", "MainPID")?==fallback && start_time(&fallback)?==fallback_identity, "Independent fallback did not survive manual shell retry")?;
    checked(&["start","recovery-disabled.service"])?;
    wait("disabled fallback restart limit", || Ok(property("recovery-disabled.service","Result")?=="start-limit-hit"))?;
    require(count("/home/fixture/disabled-attempts")==3 && fs::metadata("/home/fixture/fallback-terminal")?.modified()?==receipt,"Disabled fallback changed terminal or restart budget")?;
    require(property("recovery-client.service", "MainPID")?==client && start_time(&client)?==identity, "Client did not survive disabled-fallback failure")?;
    checked(&["stop","graphical-session.target"])?;
    wait("graphical session stop", || Ok(property("lumen-desktop-shell.service","ActiveState")?=="inactive" && property("recovery-client.service","ActiveState")?=="inactive" && property("lumen-desktop-recovery.service", "ActiveState")?=="inactive"))?;
    println!("RECOVERY_CHECK: explicit retry, disabled fallback and PartOf stop passed");
    Ok(())
}
fn run() -> Result<()> {
    let arguments=env::args().skip(1).collect::<Vec<_>>();
    match arguments.as_slice() {
        [mode,path] if mode=="crash" => {
            fs::write(path,(count(path)+1).to_string())?;
            if path.ends_with("/attempts") && Path::new("/home/fixture/healthy").exists() { loop {thread::sleep(Duration::from_secs(1));} }
            std::process::exit(7);
        }
        [mode] if mode=="client" => loop {thread::sleep(Duration::from_secs(1));},
        [mode] if mode=="filesystem" => inspect_filesystem(),
        [mode] if mode=="driver" => {
            require(fs::read_to_string("/etc/lumen-recovery-policy-fixture")?=="disposable-recovery-policy\n", "Disposable VM marker missing")?;
            let result=driver();
            match &result {
                Ok(())=>println!("LUMEN_RECOVERY_POLICY_PASS"),
                Err(error)=>{
                    eprintln!("LUMEN_RECOVERY_POLICY_FAIL: {error}");
                    for name in ["recovery-test-session.target", "graphical-session.target", "lumen-desktop-shell.service"] {
                        if let Ok(output)=user(&["show", name, "--property=RefuseManualStart,ActiveState,LoadState,LoadError,Result"]) { eprintln!("UNIT_DIAGNOSTIC {name}: {}{}",String::from_utf8_lossy(&output.stdout),String::from_utf8_lossy(&output.stderr)); }
                    }
                    if let Ok(output)=user(&["show", "--property=UnitPath", "--value"]) {eprintln!("UNIT_PATH_DIAGNOSTIC: {}",String::from_utf8_lossy(&output.stdout));}
                    if let Ok(output)=Command::new(env!("VM_RUNUSER")).args(["-u", "fixture", "--"]).arg(env::current_exe()?).arg("filesystem").output() {eprintln!("{}{}",String::from_utf8_lossy(&output.stdout),String::from_utf8_lossy(&output.stderr));}
                    if let Ok(output)=Command::new(env!("VM_JOURNALCTL")).args(["-b", "--no-pager", "-n", "100"]).output() { eprintln!("JOURNAL_DIAGNOSTIC:\n{}",String::from_utf8_lossy(&output.stdout)); }
                }
            }
            io::stdout().flush()?;io::stderr().flush()?;
            let mut serial=fs::OpenOptions::new().write(true).open("/dev/ttyS0")?;
            serial.write_all(if result.is_ok() {b"\nLUMEN_RECOVERY_POLICY_PASS\n"} else {b"\nLUMEN_RECOVERY_POLICY_FAIL\n"})?;
            if unsafe {tcdrain(serial.as_raw_fd())} != 0 {return Err(io::Error::last_os_error().into());}
            fs::OpenOptions::new().write(true).open("/dev/port")?.write_at(&0u32.to_le_bytes(),0xf4)?;
            Err("Disposable VM exit device did not terminate execution".into())
        }
        _=>Err("Expected driver, client or crash counter".into()),
    }
}
fn main() { if let Err(error)=run(){eprintln!("Recovery policy fixture: {error}");std::process::exit(1);} }
