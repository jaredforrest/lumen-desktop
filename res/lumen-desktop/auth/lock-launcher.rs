use std::env;
use std::fs::OpenOptions;
use std::os::fd::AsRawFd;
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::{self, Command};
use std::thread;
use std::time::Duration;

unsafe extern "C" {
    fn flock(fd: i32, operation: i32) -> i32;
}

const LOCK_EX: i32 = 2;
const O_NOFOLLOW: i32 = 0o400000;

fn run(quickshell: &str, swaylock: &str, qml: &str) -> Result<i32, String> {
    let runtime = env::var_os("XDG_RUNTIME_DIR")
        .ok_or("XDG_RUNTIME_DIR is required for the session lock")?;
    let lock_path = PathBuf::from(runtime).join("lumen-lock-startup");
    let lock = OpenOptions::new()
        .write(true)
        .create(true)
        .mode(0o600)
        .custom_flags(O_NOFOLLOW)
        .open(&lock_path)
        .map_err(|error| format!("Cannot open lock startup file: {error}"))?;
    if unsafe { flock(lock.as_raw_fd(), LOCK_EX) } != 0 {
        return Err(format!("Cannot serialize lock startup: {}", std::io::Error::last_os_error()));
    }

    let started = Command::new(quickshell)
        .args(["--no-duplicate", "--daemonize", "--path", qml])
        .status()
        .is_ok_and(|status| status.success());
    if !started {
        let error = Command::new(swaylock).arg("-f").exec();
        return Err(format!("Cannot start fallback screen locker: {error}"));
    }

    for _ in 0..100 {
        if let Ok(output) = Command::new(quickshell)
            .args(["--path", qml, "ipc", "call", "auth", "secure"])
            .output()
        {
            if output.status.success() && output.stdout.as_slice().trim_ascii() == b"true" {
                return Ok(0);
            }
        }
        thread::sleep(Duration::from_millis(50));
    }
    Err("The session locker did not confirm secure coverage".to_owned())
}

fn main() {
    let mut args = env::args().skip(1);
    let (Some(quickshell), Some(swaylock), Some(qml), None) =
        (args.next(), args.next(), args.next(), args.next())
    else {
        eprintln!("Usage: lumen-lock-launcher QUICKSHELL SWAYLOCK LOCK_QML");
        process::exit(2);
    };
    match run(&quickshell, &swaylock, &qml) {
        Ok(code) => process::exit(code),
        Err(message) => {
            eprintln!("{message}");
            process::exit(1);
        }
    }
}
