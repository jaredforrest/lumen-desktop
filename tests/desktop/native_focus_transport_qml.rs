//! Real production QML transport with owned disposable stdio fault providers.
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
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
unsafe extern "C" {
    fn kill(pid: i32, signal: i32) -> i32;
}
#[test]
fn native_focus_transport() {
    let source = PathBuf::from(file!()).canonicalize().unwrap();
    let repo = source.parent().unwrap().parent().unwrap().parent().unwrap();
    let component = env::var_os("LUMEN_NATIVE_FOCUS_COMPONENT")
        .map(PathBuf::from)
        .unwrap_or_else(|| repo.join("res/lumen-desktop/shell/ApplicationLifecycleState.qml"));
    let modes = env::var("LUMEN_NATIVE_FOCUS_MODES")
        .unwrap_or_else(|_| "success,malformed,unavailable,wrong-id,stall,exit".into());
    for mode in modes.split(',') {
        let root = env::temp_dir().join(format!(
            "lf-{}-{:x}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&root).unwrap();
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
                    unsafe {
                        kill(-(child.id() as i32), 9);
                    };
                    break child.wait()?;
                }
                thread::sleep(Duration::from_millis(10));
            };
            // All spawned processes belong to this private group; reap the shell before
            // checking provider PIDs and retain its complete bounded diagnostic output.
            unsafe {
                kill(-(child.id() as i32), 9);
            };
            let _ = child.wait();
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
                for name in ["starts", "requests", "overlapping-owned-child"] {
                    if root.join(name).exists() {
                        fs::copy(
                            root.join(name),
                            PathBuf::from(&artifacts).join(format!("{mode}-{name}")),
                        )?;
                    }
                }
            }
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
