//! Optional private PulseAudio null sink, with no host sound devices.
use crate::{Result, provenance::digest};
use serde_json::json;
use std::{
    collections::BTreeMap,
    fs::{self, File},
    os::unix::process::CommandExt,
    path::Path,
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

pub fn start_null_sink(
    package: &Path,
    env: &mut BTreeMap<String, String>,
    output: &Path,
) -> Result<Child> {
    let package = package.canonicalize()?;
    let executable = package.join("bin/pulseaudio");
    let pactl = package.join("bin/pactl");
    if !package.starts_with("/nix/store") || !executable.is_file() || !pactl.is_file() {
        return Err("Immutable PulseAudio package required".into());
    }
    if std::env::var("HOME")? != "/home/fixture"
        || Path::new("/.old-root").exists()
        || Path::new("/dev/snd").exists()
    {
        return Err("Private device-free audio boundary required".into());
    }
    let runtime = env
        .get("XDG_RUNTIME_DIR")
        .ok_or("Private runtime path required")?;
    let socket = Path::new(runtime).join("private-pulse.sock");
    if socket.exists() {
        return Err("Private PulseAudio socket already exists".into());
    }
    let command = vec![
        executable.display().to_string(),
        "--daemonize=no".into(),
        "--use-pid-file=no".into(),
        "--exit-idle-time=-1".into(),
        "--disable-shm=yes".into(),
        "--disallow-exit=yes".into(),
        "--realtime=no".into(),
        "-n".into(),
        "--load=module-null-sink sink_name=fixture".into(),
        format!(
            "--load=module-native-protocol-unix socket={} auth-anonymous=1",
            socket.display()
        ),
    ];
    let server = format!("unix:{}", socket.display());
    env.insert("PULSE_SERVER".into(), server.clone());
    let log = File::create(output.join("audio.log"))?;
    let mut child = Command::new(&executable);
    child
        .args(&command[1..])
        .env_clear()
        .envs(&*env)
        .stdout(Stdio::from(log.try_clone()?))
        .stderr(Stdio::from(log));
    unsafe {
        child.pre_exec(|| {
            libc::setsid();
            Ok(())
        });
    }
    let mut child = child.spawn()?;
    let result = (|| -> Result<()> {
        let deadline = Instant::now() + Duration::from_secs(5);
        while !socket.exists() {
            if child.try_wait()?.is_some() || Instant::now() >= deadline {
                return Err("Private null-sink provider unavailable; see audio.log".into());
            }
            thread::sleep(Duration::from_millis(20));
        }
        let mut sinks = Command::new(&pactl)
            .arg(format!("--server={server}"))
            .args(["list", "short", "sinks"])
            .env_clear()
            .envs(&*env)
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;
        let end = Instant::now() + Duration::from_secs(2);
        loop {
            if let Some(status) = sinks.try_wait()? {
                if !status.success() {
                    return Err("Private sink query failed".into());
                }
                break;
            }
            if Instant::now() >= end {
                let _ = sinks.kill();
                return Err("Private sink query timed out".into());
            }
            thread::sleep(Duration::from_millis(20));
        }
        let output_bytes = sinks.wait_with_output()?.stdout;
        let listed = String::from_utf8(output_bytes)?;
        if !listed.contains("\tfixture\t") || child.try_wait()?.is_some() {
            return Err("Expected private fixture sink absent".into());
        }
        let record = json!({"executable":executable,"sha256":digest(&executable)?,"argv":command,"sinks":listed,"scope":"Private null sink with no sound devices; no audio functionality acceptance"});
        fs::write(
            output.join("audio-provider.json"),
            serde_json::to_vec_pretty(&record)?,
        )?;
        Ok(())
    })();
    if let Err(error) = result {
        let _ = child.kill();
        let _ = child.wait();
        return Err(error);
    }
    Ok(child)
}
