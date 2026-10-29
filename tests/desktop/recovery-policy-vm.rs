//! Real disposable systemd user-manager restart-policy acceptance.
use std::{env, fs, io::{self, Write}, path::Path, process::{Command, Output}, thread, time::{Duration, Instant}};
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
const HOME: &str = "/home/fixture";
fn require(ok: bool, message: &str) -> Result<()> { if ok { Ok(()) } else { Err(message.into()) } }
fn user(arguments: &[&str]) -> Result<Output> {
    Ok(Command::new(env!("VM_RUNUSER")).args(["-u", "fixture", "--", env!("VM_ENV"),
        "XDG_RUNTIME_DIR=/run/user/1000", "DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/1000/bus",
        env!("VM_SYSTEMCTL")]).args(arguments).output()?)
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
fn driver() -> Result<()> {
    wait("actual user manager", || Ok(user(&["show-environment"])?.status.success()))?;
    checked(&["start", "graphical-session.target"])?;
    checked(&["start", "recovery-client.service"])?;
    let client=property("recovery-client.service","MainPID")?;
    require(client!="0","Client process missing")?;
    let identity=start_time(&client)?;
    checked(&["start", "lumen-desktop-shell.service"])?;
    wait("restart limit", || Ok(property("lumen-desktop-shell.service","Result")?=="start-limit-hit"))?;
    wait("independent fallback terminal", || Ok(Path::new("/home/fixture/fallback-shell").is_file()))?;
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
    checked(&["start","recovery-disabled.service"])?;
    wait("disabled fallback restart limit", || Ok(property("recovery-disabled.service","Result")?=="start-limit-hit"))?;
    require(count("/home/fixture/disabled-attempts")==3 && fs::metadata("/home/fixture/fallback-terminal")?.modified()?==receipt,"Disabled fallback changed terminal or restart budget")?;
    checked(&["stop","graphical-session.target"])?;
    wait("graphical session stop", || Ok(property("lumen-desktop-shell.service","ActiveState")?=="inactive" && property("recovery-client.service","ActiveState")?=="inactive"))?;
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
        [mode] if mode=="driver" => {
            let result=driver();
            match &result {Ok(())=>println!("LUMEN_RECOVERY_POLICY_PASS"),Err(error)=>eprintln!("LUMEN_RECOVERY_POLICY_FAIL: {error}")}
            io::stdout().flush()?;io::stderr().flush()?;
            Command::new(env!("VM_SYSTEMCTL")).args(["poweroff","--no-block"]).status()?;
            result
        }
        _=>Err("Expected driver, client or crash counter".into()),
    }
}
fn main() { if let Err(error)=run(){eprintln!("Recovery policy fixture: {error}");std::process::exit(1);} }
