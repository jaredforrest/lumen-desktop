//! Private empty/live-dialog compositor shutdown and restart acceptance.
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::env;
use std::error::Error;
use std::fs::{self, File};
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

type Result<T> = std::result::Result<T, Box<dyn Error>>;
fn fail(message: impl Into<String>) -> Box<dyn Error> {
    message.into().into()
}
fn require(condition: bool, message: &str) -> Result<()> {
    if condition {
        Ok(())
    } else {
        Err(fail(message))
    }
}

struct Options {
    sway: PathBuf,
    client: PathBuf,
    output: PathBuf,
    effects: bool,
    inside: bool,
}
fn options() -> Result<Options> {
    let mut sway = None;
    let mut client = None;
    let mut output = None;
    let mut effects = false;
    let mut inside = false;
    let mut args = env::args_os().skip(1);
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("--sway") => sway = Some(PathBuf::from(args.next().ok_or("Missing Sway path")?)),
            Some("--client") => {
                client = Some(PathBuf::from(args.next().ok_or("Missing client path")?))
            }
            Some("--output") => {
                output = Some(PathBuf::from(args.next().ok_or("Missing output path")?))
            }
            Some("--effects") => effects = true,
            Some("--inside") => inside = true,
            _ => return Err(fail("Unknown dialog lifetime fixture option")),
        }
    }
    Ok(Options {
        sway: sway.ok_or("--sway is required")?,
        client: client.ok_or("--client is required")?,
        output: output.ok_or("--output is required")?,
        effects,
        inside,
    })
}

fn environment(private: &Path, runtime: &Path, effects: bool) -> BTreeMap<String, String> {
    let mut envs = env::vars().collect::<BTreeMap<_, _>>();
    for name in [
        "SWAYSOCK",
        "WAYLAND_DISPLAY",
        "DISPLAY",
        "DBUS_SESSION_BUS_ADDRESS",
        "AT_SPI_BUS_ADDRESS",
    ] {
        envs.remove(name);
    }
    envs.insert("HOME".into(), private.display().to_string());
    envs.insert("XDG_RUNTIME_DIR".into(), runtime.display().to_string());
    envs.insert(
        "DBUS_SYSTEM_BUS_ADDRESS".into(),
        format!("unix:path={}/no-system-bus", runtime.display()),
    );
    envs.insert("WLR_BACKENDS".into(), "headless".into());
    envs.insert("WLR_HEADLESS_OUTPUTS".into(), "1".into());
    envs.insert(
        "WLR_RENDERER".into(),
        if effects { "gles2" } else { "pixman" }.into(),
    );
    envs.insert(
        "WLR_RENDERER_FORCE_SOFTWARE".into(),
        if effects { "1" } else { "0" }.into(),
    );
    envs.insert("WLR_RENDERER_ALLOW_SOFTWARE".into(), "1".into());
    envs.insert("LIBGL_ALWAYS_SOFTWARE".into(), "1".into());
    envs.insert("WLR_LIBINPUT_NO_DEVICES".into(), "1".into());
    envs
}

fn outer(options: &Options) -> Result<()> {
    fs::create_dir(&options.output)?;
    let output = fs::canonicalize(&options.output)?;
    let private = tempfile::tempdir()?;
    let runtime = private.path().join("runtime");
    fs::create_dir(&runtime)?;
    fs::set_permissions(&runtime, fs::Permissions::from_mode(0o700))?;
    let config = private.path().join("bus.conf");
    fs::write(
        &config,
        format!(
            "<busconfig><type>session</type><listen>unix:tmpdir={}</listen><policy context=\"default\"><allow own=\"*\"/><allow send_destination=\"*\"/><allow receive_sender=\"*\"/></policy></busconfig>",
            runtime.display()
        ),
    )?;
    let mut command = Command::new("timeout");
    command
        .arg("70s")
        .arg("dbus-run-session")
        .arg(format!("--config-file={}", config.display()))
        .arg("--")
        .arg(env::current_exe()?)
        .arg("--inside")
        .arg("--sway")
        .arg(&options.sway)
        .arg("--client")
        .arg(&options.client)
        .arg("--output")
        .arg(output)
        .env_clear()
        .envs(environment(private.path(), &runtime, options.effects));
    if options.effects {
        command.arg("--effects");
    }
    let status = command.status()?;
    require(
        status.success(),
        &format!("Private dialog lifetime exited {status}"),
    )
}

fn wait<T>(description: &str, mut probe: impl FnMut() -> Result<Option<T>>) -> Result<T> {
    let deadline = Instant::now() + Duration::from_secs(12);
    loop {
        if let Some(value) = probe()? {
            return Ok(value);
        }
        if Instant::now() >= deadline {
            return Err(fail(format!("Dialog lifetime timeout: {description}")));
        }
        thread::sleep(Duration::from_millis(30));
    }
}
fn first_match(folder: &Path, prefix: &str) -> Result<Option<PathBuf>> {
    Ok(fs::read_dir(folder)?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .find(|path| {
            path.file_name()
                .is_some_and(|name| name.to_string_lossy().starts_with(prefix))
        }))
}
fn first_wayland(folder: &Path) -> Result<Option<PathBuf>> {
    Ok(fs::read_dir(folder)?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .find(|path| {
            path.file_name().is_some_and(|name| {
                let name = name.to_string_lossy();
                name.starts_with("wayland-") && !name.ends_with(".lock")
            })
        }))
}

struct Children(Vec<Child>);
impl Children {
    fn launch(
        &mut self,
        program: &Path,
        args: &[&str],
        envs: &BTreeMap<String, String>,
        log: &Path,
        interactive: bool,
    ) -> Result<usize> {
        let stream = File::create(log)?;
        let mut command = Command::new(program);
        command
            .args(args)
            .env_clear()
            .envs(envs)
            .stdout(Stdio::from(stream.try_clone()?))
            .stderr(Stdio::from(stream))
            .stdin(if interactive {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .process_group(0);
        unsafe {
            command.pre_exec(|| {
                let limit = libc::rlimit {
                    rlim_cur: 0,
                    rlim_max: 0,
                };
                if libc::setrlimit(libc::RLIMIT_CORE, &limit) != 0 {
                    Err(std::io::Error::last_os_error())
                } else {
                    Ok(())
                }
            });
        }
        self.0.push(command.spawn()?);
        Ok(self.0.len() - 1)
    }
    fn wait(&mut self, index: usize, seconds: u64) -> Result<std::process::ExitStatus> {
        let deadline = Instant::now() + Duration::from_secs(seconds);
        loop {
            if let Some(status) = self.0[index].try_wait()? {
                return Ok(status);
            }
            if Instant::now() >= deadline {
                return Err(fail("Child did not exit"));
            }
            thread::sleep(Duration::from_millis(25));
        }
    }
}
impl Drop for Children {
    fn drop(&mut self) {
        for child in self.0.iter_mut().rev() {
            if child.try_wait().ok().flatten().is_none() {
                unsafe { libc::killpg(child.id() as i32, libc::SIGTERM) };
            }
        }
        for child in self.0.iter_mut().rev() {
            let deadline = Instant::now() + Duration::from_secs(3);
            loop {
                if child.try_wait().ok().flatten().is_some() {
                    break;
                }
                if Instant::now() >= deadline {
                    unsafe { libc::killpg(child.id() as i32, libc::SIGKILL) };
                    let _ = child.wait();
                    break;
                }
                thread::sleep(Duration::from_millis(25));
            }
        }
    }
}

fn query(swaymsg: &Path, envs: &BTreeMap<String, String>, kind: &str) -> Result<Value> {
    let output = Command::new("timeout")
        .arg("4s")
        .arg(swaymsg)
        .args(["-r", "-t", kind])
        .env_clear()
        .envs(envs)
        .output()?;
    require(
        output.status.success(),
        &format!(
            "swaymsg {kind} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ),
    )?;
    Ok(serde_json::from_slice(&output.stdout)?)
}
fn nodes<'a>(root: &'a Value, output: &mut Vec<&'a Value>) {
    output.push(root);
    for key in ["nodes", "floating_nodes"] {
        if let Some(children) = root[key].as_array() {
            for child in children {
                nodes(child, output);
            }
        }
    }
}
fn snapshot(swaymsg: &Path, envs: &BTreeMap<String, String>) -> Result<Vec<Value>> {
    let tree = query(swaymsg, envs, "get_tree")?;
    let mut found = Vec::new();
    nodes(&tree, &mut found);
    Ok(found.into_iter().cloned().collect())
}

fn inner(options: &Options) -> Result<()> {
    let runtime = PathBuf::from(env::var_os("XDG_RUNTIME_DIR").ok_or("XDG_RUNTIME_DIR missing")?);
    let mut envs = env::vars().collect::<BTreeMap<_, _>>();
    let swaymsg = options
        .sway
        .parent()
        .ok_or("Sway parent missing")?
        .join("swaymsg");
    let config = PathBuf::from(env::var_os("HOME").ok_or("HOME missing")?).join("sway.conf");
    fs::write(
        &config,
        "output HEADLESS-1 resolution 800x600\nxwayland disable\nseat seat0 fallback true\n",
    )?;
    let mut children = Children(Vec::new());
    let mut checks = Vec::new();
    for cycle in ["empty", "live-dialog", "restart"] {
        envs.remove("SWAYSOCK");
        envs.remove("WAYLAND_DISPLAY");
        let sway_args = [
            "--unsupported-gpu",
            "-c",
            config.to_str().ok_or("Invalid config")?,
        ];
        let compositor = children.launch(
            &options.sway,
            &sway_args,
            &envs,
            &options.output.join(format!("{cycle}-sway.log")),
            false,
        )?;
        let socket = wait("private compositor IPC socket", || {
            first_match(&runtime, "sway-ipc")
        })?;
        let wayland = wait("private Wayland socket", || first_wayland(&runtime))?;
        envs.insert("SWAYSOCK".into(), socket.display().to_string());
        envs.insert(
            "WAYLAND_DISPLAY".into(),
            wayland
                .file_name()
                .ok_or("Wayland name missing")?
                .to_string_lossy()
                .into_owned(),
        );
        require(
            query(&swaymsg, &envs, "get_version")?["native_window_relationships"] == true,
            "Native relationship compositor unavailable",
        )?;
        if cycle == "live-dialog" {
            let client = children.launch(
                &options.client,
                &[],
                &envs,
                &options.output.join(format!("{cycle}-client.log")),
                true,
            )?;
            wait("four live relationship surfaces", || {
                let count = snapshot(&swaymsg, &envs)?
                    .iter()
                    .filter(|node| {
                        node["name"]
                            .as_str()
                            .is_some_and(|name| name.starts_with("Relationship "))
                    })
                    .count();
                Ok((count == 4).then_some(()))
            })?;
            let stdin = children.0[client]
                .stdin
                .as_mut()
                .ok_or("Client input missing")?;
            stdin.write_all(b"modal\n")?;
            stdin.flush()?;
            wait("modal relationship", || {
                let modal = snapshot(&swaymsg, &envs)?
                    .iter()
                    .any(|node| node["lumen_relationships"]["modal"] == true);
                Ok(modal.then_some(()))
            })?;
            unsafe { libc::kill(children.0[compositor].id() as i32, libc::SIGTERM) };
            let status = children.wait(compositor, 8)?;
            let socket_remains = socket.exists();
            let wayland_remains = wayland.exists();
            fs::write(
                options.output.join(format!("{cycle}-exit.json")),
                format!(
                    "{}\n",
                    serde_json::to_string_pretty(
                        &json!({"returncode":status.code(),"ipcSocketRemains":socket_remains,"waylandSocketRemains":wayland_remains})
                    )?
                ),
            )?;
            require(status.success(), "Live-dialog compositor shutdown failed")?;
            require(
                !socket_remains
                    && !wayland_remains
                    && !PathBuf::from(format!("{}.lock", wayland.display())).exists(),
                "Native sockets remained after shutdown",
            )?;
            children.0[client].stdin.take();
            children.wait(client, 4)?;
        } else {
            unsafe { libc::kill(children.0[compositor].id() as i32, libc::SIGTERM) };
            let status = children.wait(compositor, 8)?;
            let socket_remains = socket.exists();
            let wayland_remains = wayland.exists();
            fs::write(
                options.output.join(format!("{cycle}-exit.json")),
                format!(
                    "{}\n",
                    serde_json::to_string_pretty(
                        &json!({"returncode":status.code(),"ipcSocketRemains":socket_remains,"waylandSocketRemains":wayland_remains})
                    )?
                ),
            )?;
            require(status.success(), "Compositor shutdown failed")?;
            require(
                !socket_remains
                    && !wayland_remains
                    && !PathBuf::from(format!("{}.lock", wayland.display())).exists(),
                "Native sockets remained after shutdown",
            )?;
        }
        checks.push(format!(
            "{cycle}: clean exit, both native endpoints removed"
        ));
    }
    fs::write(
        options.output.join("result.json"),
        format!(
            "{}\n",
            serde_json::to_string_pretty(
                &json!({"passed":true,"checks":checks,"scope":"Private headless compositor; live native dialog; real SIGTERM shutdown and restart; no host session"})
            )?
        ),
    )?;
    println!("PASS clean native dialog-manager shutdown and restart");
    Ok(())
}

fn main() {
    let result = options().and_then(|options| {
        if options.inside {
            inner(&options)
        } else {
            outer(&options)
        }
    });
    if let Err(error) = result {
        eprintln!("dialog lifetime: {error}");
        std::process::exit(1);
    }
}
