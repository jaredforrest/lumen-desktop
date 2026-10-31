//! Real production QML transport with owned disposable stdio fault providers.
mod private_qml_root;
use std::{
    env, fs,
    io::Read,
    os::unix::{
        fs::{PermissionsExt, symlink},
        process::CommandExt,
    },
    path::PathBuf,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};
unsafe extern "C" {
    fn kill(pid: i32, signal: i32) -> i32;
    fn prctl(option: i32, ...) -> i32;
    fn waitpid(pid: i32, status: *mut i32, options: i32) -> i32;
}
struct Subreaper(i32);
impl Subreaper {
    fn new() -> std::io::Result<Self> {
        let mut previous = 0_i32;
        if unsafe { prctl(37, &mut previous as *mut i32) } != 0 || unsafe { prctl(36, 1_i32) } != 0
        {
            return Err(std::io::Error::last_os_error());
        }
        Ok(Self(previous))
    }
}
impl Drop for Subreaper {
    fn drop(&mut self) {
        unsafe {
            prctl(36, self.0);
        }
    }
}
fn reap_providers(root: &std::path::Path, group: u32) -> Result<(), Box<dyn std::error::Error>> {
    let identities = fs::read_to_string(root.join("identities")).unwrap_or_default();
    let mut diagnostics = String::new();
    for line in identities.lines() {
        let fields = line.split_whitespace().collect::<Vec<_>>();
        if fields.len() != 2 {
            return Err("Malformed owned provider identity ledger".into());
        }
        let pid = fields[0].parse::<i32>()?;
        let start = fields[1].parse::<u64>()?;
        let end = Instant::now() + Duration::from_secs(2);
        let mut signalled = false;
        loop {
            let stat = fs::read_to_string(format!("/proc/{pid}/stat")).unwrap_or_default();
            diagnostics.push_str(&format!("pid={pid} expectedStart={start} stat={stat}\n"));
            fs::write(root.join("teardown-proc-stat.log"), &diagnostics)?;
            let mut status = 0;
            let waited = unsafe { waitpid(pid, &mut status, 1) };
            if waited == pid {
                break;
            }
            if waited < 0 {
                if !std::path::Path::new(&format!("/proc/{pid}")).exists() {
                    break;
                }
                return Err(format!("Recorded provider {pid} still exists but is not an adopted child; refusing to signal it. {diagnostics}").into());
            }
            // An unreaped direct child cannot have its PID reused. Still bind
            // the adopted process to the stub's captured start time and group
            // before signalling; unrelated processes are never targeted.
            let values = stat
                .rsplit_once(')')
                .ok_or("Owned provider stat missing")?
                .1
                .split_whitespace()
                .collect::<Vec<_>>();
            if values.get(1).and_then(|s| s.parse::<u32>().ok()) != Some(std::process::id())
                || values.get(2).and_then(|s| s.parse::<u32>().ok()) != Some(group)
                || values.get(19).and_then(|s| s.parse::<u64>().ok()) != Some(start)
            {
                return Err(format!(
                    "Adopted provider identity changed; refusing to signal it. {diagnostics}"
                )
                .into());
            }
            if !signalled {
                if unsafe { kill(pid, 9) } != 0 {
                    return Err(std::io::Error::last_os_error().into());
                }
                signalled = true;
            }
            if Instant::now() >= end {
                return Err(format!("Owned provider reap deadline exceeded. {diagnostics}").into());
            }
            thread::sleep(Duration::from_millis(10));
        }
        assert!(
            !std::path::Path::new(&format!("/proc/{pid}")).exists(),
            "Owned provider remains after exact waitpid: {diagnostics}"
        );
    }
    Ok(())
}
#[test]
fn native_focus_transport() {
    let _subreaper = Subreaper::new().expect("Owned provider subreaper unavailable");
    let source = PathBuf::from(file!()).canonicalize().unwrap();
    let repo = source.parent().unwrap().parent().unwrap().parent().unwrap();
    let component = env::var_os("LUMEN_NATIVE_FOCUS_COMPONENT")
        .map(PathBuf::from)
        .unwrap_or_else(|| repo.join("res/lumen-desktop/shell/ApplicationLifecycleState.qml"));
    let modes = env::var("LUMEN_NATIVE_FOCUS_MODES")
        .unwrap_or_else(|_| "success,malformed,unavailable,wrong-id,stall,exit".into());
    for mode in modes.split(',') {
        let root = private_qml_root::create().unwrap();
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
        let result = (|| -> Result<(), Box<dyn std::error::Error>> {
            fs::copy(&component, root.join("ApplicationLifecycleState.qml"))?;
            fs::write(
                root.join("shell.qml"),
                include_str!("native_focus_transport_fixture.qml"),
            )?;
            for name in ["bin", "runtime", "home"] {
                fs::create_dir(root.join(name))?;
                fs::set_permissions(root.join(name), fs::Permissions::from_mode(0o700))?;
            }
            symlink(
                env::var("LUMEN_NATIVE_FOCUS_STUB")?,
                root.join("bin/lumen-application-lifecycle"),
            )?;
            let mut child = Command::new("quickshell")
                .arg("-p")
                .arg(&root)
                .process_group(0)
                .env("HOME", root.join("home"))
                .env("XDG_RUNTIME_DIR", root.join("runtime"))
                .env("QT_QPA_PLATFORM", "offscreen")
                .env("QT_QPA_PLATFORMTHEME", "")
                .env("QT_QUICK_BACKEND", "software")
                .env("XDG_DATA_HOME", root.join("data"))
                .env("XDG_CONFIG_HOME", root.join("config"))
                .env("XDG_CACHE_HOME", root.join("cache"))
                .env("XDG_STATE_HOME", root.join("state"))
                .env("QT_ACCESSIBILITY", "0")
                .env("QT_LINUX_ACCESSIBILITY_ALWAYS_ON", "0")
                .env("NO_AT_BRIDGE", "1")
                .env("GTK_A11Y", "none")
                .env(
                    "DBUS_SYSTEM_BUS_ADDRESS",
                    format!("unix:path={}/no-system", root.display()),
                )
                .env(
                    "DBUS_SESSION_BUS_ADDRESS",
                    format!("unix:path={}/no-session", root.display()),
                )
                .env("PULSE_SERVER", format!("unix:{}/no-audio", root.display()))
                .env("PIPEWIRE_REMOTE", "no-pipewire")
                .env(
                    "PATH",
                    format!("{}:{}", root.join("bin").display(), env::var("PATH")?),
                )
                .env("LUMEN_NATIVE_FOCUS_ROOT", &root)
                .env("LUMEN_NATIVE_FOCUS_MODE", mode)
                .env_remove("WAYLAND_DISPLAY")
                .env_remove("DISPLAY")
                .env_remove("AT_SPI_BUS_ADDRESS")
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()?;
            let capture = |mut pipe: Box<dyn Read + Send>| {
                thread::spawn(move || {
                    let mut data = Vec::new();
                    pipe.by_ref()
                        .take(1024 * 1024)
                        .read_to_end(&mut data)
                        .map(|_| data)
                })
            };
            let stdout = capture(Box::new(child.stdout.take().unwrap()));
            let stderr = capture(Box::new(child.stderr.take().unwrap()));
            let end = Instant::now() + Duration::from_secs(7);
            let status = loop {
                if let Some(status) = child.try_wait()? {
                    break status;
                }
                if Instant::now() > end {
                    child.kill()?;
                    break child.wait()?;
                }
                thread::sleep(Duration::from_millis(10));
            };
            // The shell can exit before its final worker. Adopt that exact
            // recorded child and reap it ourselves, independent of sandbox PID 1.
            let _ = child.wait();
            let teardown = reap_providers(&root, child.id());
            let stdout = stdout.join().unwrap()?;
            let stderr = stderr.join().unwrap()?;
            let log = format!(
                "{}\n{}",
                String::from_utf8_lossy(&stdout),
                String::from_utf8_lossy(&stderr)
            );
            if let Ok(artifacts) = env::var("LUMEN_NATIVE_FOCUS_ARTIFACTS") {
                fs::create_dir_all(&artifacts)?;
                fs::write(PathBuf::from(&artifacts).join(format!("{mode}.log")), &log)?;
                for name in [
                    "starts",
                    "requests",
                    "identities",
                    "teardown-proc-stat.log",
                    "overlapping-owned-child",
                ] {
                    if root.join(name).exists() {
                        fs::copy(
                            root.join(name),
                            PathBuf::from(&artifacts).join(format!("{mode}-{name}")),
                        )?;
                    }
                }
            }
            teardown?;
            assert!(
                status.success()
                    && log.contains(&format!("PASS native focus transport {mode}"))
                    && !log.contains("FAIL ")
                    && !log.contains("TypeError:")
                    && !log.contains("ReferenceError:")
                    && !log.contains("Failed to start IPC server"),
                "{mode}: {status}\n{log}"
            );
            assert!(
                !root.join("overlapping-owned-child").exists(),
                "New provider started before previous owned child was reaped"
            );
            let starts = fs::read_to_string(root.join("starts"))?;
            let requests = fs::read_to_string(root.join("requests"))?;
            let pids = starts.lines().collect::<Vec<_>>();
            assert_eq!(
                pids.len(),
                if mode == "success" { 1 } else { 2 },
                "Unexpected blind provider retry"
            );
            let rows = requests.lines().collect::<Vec<_>>();
            assert_eq!(rows.len(), 2, "A request was replayed automatically");
            for (index, row) in rows.iter().enumerate() {
                let fields = row.split_whitespace().collect::<Vec<_>>();
                assert_eq!(fields.len(), 2);
                assert_eq!(fields[0], pids[if mode == "success" { 0 } else { index }]);
            }
            assert_ne!(
                rows[0].split_whitespace().nth(1),
                rows[1].split_whitespace().nth(1),
                "Owned request identity reused after retirement"
            );
            for pid in pids {
                assert!(
                    !PathBuf::from(format!("/proc/{pid}")).exists(),
                    "Owned provider still alive after fixture exit"
                );
            }
            Ok(())
        })();
        fs::remove_dir_all(&root).unwrap();
        result.unwrap();
    }
}
