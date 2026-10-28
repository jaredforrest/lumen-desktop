//! Run the calendar card and window fixtures in private D-Bus and headless Sway.
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::env;
use std::ffi::{OsStr, OsString};
use std::fs::{self, File};
use std::io::{self, BufRead};
use std::os::unix::fs::{PermissionsExt, symlink};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

#[derive(Clone, Copy)]
enum Mode {
    Card,
    Window,
    Popover,
    Toggle,
}

struct Args {
    mode: Mode,
    assets: PathBuf,
    output: PathBuf,
    launcher: PathBuf,
    font_config: Option<PathBuf>,
    inside: bool,
}

fn parse_args() -> Result<Args> {
    let mut input = env::args_os().skip(1);
    let mut mode = None;
    let mut assets = None;
    let mut output = None;
    let mut launcher = None;
    let mut font_config = None;
    let mut inside = false;
    while let Some(arg) = input.next() {
        match arg.to_str() {
            Some("card") => mode = Some(Mode::Card),
            Some("window") => mode = Some(Mode::Window),
            Some("popover") => mode = Some(Mode::Popover),
            Some("toggle") => mode = Some(Mode::Toggle),
            Some("--assets") => {
                assets = Some(PathBuf::from(input.next().ok_or("--assets needs a path")?))
            }
            Some("--output") => {
                output = Some(PathBuf::from(input.next().ok_or("--output needs a path")?))
            }
            Some("--launcher") => {
                launcher = Some(PathBuf::from(
                    input.next().ok_or("--launcher needs a path")?,
                ))
            }
            Some("--font-config") => {
                font_config = Some(PathBuf::from(
                    input.next().ok_or("--font-config needs a path")?,
                ));
            }
            Some("--inside") => inside = true,
            _ => return Err(format!("unknown argument: {}", arg.to_string_lossy()).into()),
        }
    }
    Ok(Args {
        mode: mode.ok_or(
            "usage: calendar-qa card|window|popover|toggle --assets PATH --output NEW-DIRECTORY [--launcher PATH] [--font-config PATH]",
        )?,
        assets: assets.ok_or("--assets is required")?,
        output: output.ok_or("--output is required")?,
        launcher: launcher.unwrap_or_else(|| "lumen-desktop-shell".into()),
        font_config,
        inside,
    })
}

fn which(command: &Path) -> Result<PathBuf> {
    if command.components().count() > 1 {
        return Ok(fs::canonicalize(command)?);
    }
    for directory in env::split_paths(&env::var_os("PATH").unwrap_or_default()) {
        let path = directory.join(command);
        if path.is_file() {
            return Ok(fs::canonicalize(path)?);
        }
    }
    Err(format!("command not found: {}", command.display()).into())
}

fn outside(args: &Args) -> Result<()> {
    let parent = args
        .output
        .parent()
        .unwrap_or(Path::new("."))
        .canonicalize()?;
    let output = parent.join(
        args.output
            .file_name()
            .ok_or("output needs a final component")?,
    );
    fs::create_dir(&output)?;
    let assets = fs::canonicalize(&args.assets)?;
    let launcher = which(&args.launcher)?;
    if matches!(args.mode, Mode::Toggle) {
        let source = assets
            .parent()
            .and_then(Path::parent)
            .and_then(Path::parent)
            .ok_or("assets must be under repository res/lumen-desktop/shell")?;
        let font_config = fs::canonicalize(
            args.font_config
                .as_ref()
                .ok_or("toggle requires --font-config")?,
        )?;
        let executable = fs::canonicalize(env::current_exe()?)?;
        let path = env::split_paths(&env::var_os("PATH").unwrap_or_default())
            .filter(|entry| entry.starts_with("/nix/store/"))
            .collect::<Vec<_>>();
        let path = env::join_paths(path)?;
        let status = Command::new("bwrap")
            .args(["--unshare-all", "--die-with-parent", "--clearenv"])
            .args(["--ro-bind", "/nix/store", "/nix/store"])
            .arg("--ro-bind")
            .arg(source)
            .arg("/source")
            .arg("--bind")
            .arg(&output)
            .arg("/artifacts")
            .args([
                "--dev",
                "/dev",
                "--proc",
                "/proc",
                "--tmpfs",
                "/tmp",
                "--tmpfs",
                "/run",
                "--dir",
                "/run/fixture",
                "--tmpfs",
                "/home",
                "--dir",
                "/home/fixture",
            ])
            .args(["--setenv", "PATH"])
            .arg(path)
            .args([
                "--setenv",
                "HOME",
                "/home/fixture",
                "--setenv",
                "XDG_RUNTIME_DIR",
                "/run/fixture",
                "--setenv",
                "DBUS_SESSION_BUS_ADDRESS",
                "unix:path=/run/fixture/inert-session",
                "--setenv",
                "DBUS_SYSTEM_BUS_ADDRESS",
                "unix:path=/run/fixture/inert-system",
            ])
            .args(["--chdir", "/source", "--"])
            .arg(executable)
            .args([
                "toggle",
                "--inside",
                "--assets",
                "/source/res/lumen-desktop/shell",
                "--output",
                "/artifacts",
            ])
            .arg("--launcher")
            .arg(launcher)
            .arg("--font-config")
            .arg(font_config)
            .status()?;
        return if status.success() {
            Ok(())
        } else {
            Err("private Calendar toggle fixture failed".into())
        };
    }
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let runtime = env::temp_dir().join(format!("calendar-qa-{}-{nonce}", std::process::id()));
    fs::create_dir(&runtime)?;
    fs::set_permissions(&runtime, fs::Permissions::from_mode(0o700))?;
    let mut command = Command::new("dbus-run-session");
    command
        .arg("--")
        .arg(env::current_exe()?)
        .arg(match args.mode {
            Mode::Card => "card",
            Mode::Window => "window",
            Mode::Popover => "popover",
            Mode::Toggle => "toggle",
        })
        .args(["--inside", "--assets"])
        .arg(assets)
        .arg("--output")
        .arg(&output)
        .arg("--launcher")
        .arg(launcher)
        .env("XDG_RUNTIME_DIR", &runtime)
        .env("XDG_CONFIG_HOME", output.join("config"))
        .env("XDG_STATE_HOME", output.join("state"))
        .env("XDG_CACHE_HOME", output.join("cache"))
        .env("XDG_DATA_HOME", output.join("data"))
        .env_remove("DISPLAY")
        .env_remove("WAYLAND_DISPLAY")
        .env_remove("SWAYSOCK")
        .env_remove("DBUS_SESSION_BUS_ADDRESS")
        .env_remove("AT_SPI_BUS_ADDRESS")
        .env("QT_ACCESSIBILITY", "0")
        .env("QT_LINUX_ACCESSIBILITY_ALWAYS_ON", "0")
        .env("NO_AT_BRIDGE", "1")
        .env("GTK_A11Y", "none")
        .env(
            "DBUS_SYSTEM_BUS_ADDRESS",
            format!("unix:path={}/no-system-bus", runtime.display()),
        );
    if let Some(font_config) = &args.font_config {
        command
            .arg("--font-config")
            .arg(fs::canonicalize(font_config)?);
    }
    let status = command.status();
    for _ in 0..10 {
        if fs::remove_dir_all(&runtime).is_ok() {
            break;
        }
        thread::sleep(Duration::from_millis(100));
    }
    if !status?.success() {
        return Err("private calendar session failed".into());
    }
    Ok(())
}

fn copy_tree(from: &Path, to: &Path) -> Result<()> {
    fs::create_dir(to)?;
    for item in fs::read_dir(from)? {
        let item = item?;
        let kind = item.file_type()?;
        let dest = to.join(item.file_name());
        if kind.is_dir() {
            copy_tree(&item.path(), &dest)?;
        } else if kind.is_file() {
            fs::copy(item.path(), dest)?;
        } else {
            return Err(format!("unsupported asset: {}", item.path().display()).into());
        }
    }
    Ok(())
}

fn backend() -> Result<()> {
    let mut line = String::new();
    io::stdin().lock().read_line(&mut line)?;
    let request: Value = serde_json::from_str(&line).unwrap_or(Value::Null);
    if env::var("CALENDAR_FIXTURE_MODE").ok().as_deref() == Some("popover") {
        if !matches!(request["action"].as_str(), Some("list" | "load" | "sync")) {
            return Err("Calendar UI regression attempted a mutation".into());
        }
        let month = request["month"].as_str().unwrap_or("2026-10-01");
        let day = request["selectedDate"].as_str().unwrap_or("2026-10-14");
        let events=(0..8).map(|index| json!({
            "id":index.to_string(),
            "title":if index==0 {String::from("A long project review title that must remain readable")} else {format!("Example appointment {index}")},
            "startTime":"09:00","endTime":"10:00","allDay":false,"provider":"Google",
            "calendar":if index%2==1 {"Work"} else {"Personal"},
            "color":if index%2==1 {"#aa44cc"} else {"#008855"},
            "date":day,"endDate":day,"notes":"Synthetic fixture","location":"Test room",
            "editable":true,"editStart":format!("{day}T09:00"),"editEnd":format!("{day}T10:00"),
            "accountGeneration":"fixture","calendarId":"work","eventId":index.to_string()
        })).collect::<Vec<_>>();
        println!(
            "{}",
            json!({
                "configured":true,"timezone":"Europe/London","lastSuccess":"2026-10-14T09:00:00Z",
                "month":month,"date":day,"events":events,"monthEvents":events,"days":[day],"total":8,
                "providers":[{"id":"google","name":"Google","configured":true,"generation":"fixture",
                    "calendars":[{"id":"work","name":"Work","accessRole":"writer"}]}]
            })
        );
        return Ok(());
    }
    if matches!(request["action"].as_str(), Some("create" | "update")) {
        println!(
            "{}",
            json!({"error":"Fixture save failed: reconnect and try again."})
        );
    } else {
        println!(
            "{}",
            fs::read_to_string(env::var("CALENDAR_FIXTURE_JSON")?)?
        );
    }
    Ok(())
}

fn fixture_data() -> Value {
    json!({
        "events":[{"accountGeneration":"fixture-generation","id":"fixture-event","title":"Planning lunch",
            "location":"Local fixture","notes":"First line\nSecond line","allDay":false,
            "editStart":"2026-10-08T12:00","editEnd":"2026-10-08T13:00","calendarId":"fixture",
            "eventId":"fixture-event","etag":"etag","provider":"Google","calendar":"Personal",
            "editable":true,"recurring":false,"date":"2026-10-08","endDate":"2026-10-08",
            "startTime":"12:00","endTime":"13:00"}],
        "days":["2026-10-08"],"total":1,"configured":true,
        "providers":[{"id":"google","name":"Google","generation":"fixture-generation","configured":true,
            "lastSuccess":"2026-10-08T12:00:00Z","calendars":[{"id":"fixture","name":"Personal","accessRole":"owner"}]}],
        "lastSuccess":"2026-10-08T12:00:00Z","syncError":"","warning":""
    })
}

struct Session {
    mode: Mode,
    output: PathBuf,
    runtime: PathBuf,
    ui: PathBuf,
    env: Vec<(OsString, OsString)>,
    children: Vec<Child>,
}

impl Session {
    fn new(args: &Args) -> Result<Self> {
        let output = fs::canonicalize(&args.output)?;
        copy_tree(&args.assets, &output.join("source"))?;
        let ui = output.join("fixture.qml");
        let qml = match args.mode {
            Mode::Card => include_str!("../calendar-card.qml").to_owned(),
            Mode::Window => include_str!("../calendar-window.qml").to_owned(),
            Mode::Popover => include_str!("../calendar-popover.qml").to_owned(),
            Mode::Toggle => {
                let palettes = fs::read_to_string(
                    args.assets
                        .parent()
                        .ok_or("Missing appearance asset parent")?
                        .join("appearance/palette.json"),
                )?;
                include_str!("../calendar-toggle.qml")
                    .replace("PALETTES_JSON", &palettes)
                    .replace(
                        "OUTPUT_DIRECTORY_JSON",
                        &serde_json::to_string(&output.to_string_lossy())?,
                    )
            }
        };
        fs::write(&ui, qml)?;
        let bin = output.join("bin");
        fs::create_dir(&bin)?;
        if matches!(args.mode, Mode::Popover) {
            fs::create_dir_all(output.join("home"))?;
            fs::create_dir_all(output.join("state/lumen-desktop/session"))?;
        }
        symlink(env::current_exe()?, bin.join("lumen-calendar"))?;
        fs::write(
            output.join("fixture.json"),
            serde_json::to_vec(&fixture_data())?,
        )?;
        let runtime =
            PathBuf::from(env::var_os("XDG_RUNTIME_DIR").ok_or("XDG_RUNTIME_DIR missing")?);
        let mut env = Vec::new();
        let printed = Command::new(&args.launcher).arg("--print-env0").output()?;
        if !printed.status.success() {
            return Err("Lumen launcher could not export its runtime environment".into());
        }
        let mut installed_path = env::var_os("PATH").unwrap_or_default();
        for record in printed
            .stdout
            .split(|byte| *byte == 0)
            .filter(|record| !record.is_empty())
        {
            let entry = String::from_utf8(record.to_vec())?;
            if let Some((key, value)) = entry.split_once('=') {
                if key == "PATH" {
                    installed_path = value.into();
                }
                env.push((key.into(), value.into()));
            }
        }
        env.push((
            "PATH".into(),
            format!("{}:{}", bin.display(), installed_path.to_string_lossy()).into(),
        ));
        env.extend([
            ("WLR_BACKENDS".into(), "headless".into()),
            ("WLR_RENDERER".into(), "pixman".into()),
            ("WLR_LIBINPUT_NO_DEVICES".into(), "1".into()),
            ("QT_QPA_PLATFORM".into(), "wayland".into()),
            ("GDK_BACKEND".into(), "wayland".into()),
            (
                "LUMEN_APPEARANCE_STATE".into(),
                output.join("appearance.json").into(),
            ),
            (
                "CALENDAR_FIXTURE_JSON".into(),
                output.join("fixture.json").into(),
            ),
        ]);
        if matches!(args.mode, Mode::Popover) {
            env.extend([
                ("HOME".into(), output.join("home").into()),
                ("CALENDAR_FIXTURE_MODE".into(), "popover".into()),
                ("LUMEN_COMPOSITOR_EFFECTS".into(), "0".into()),
                ("GLOBAL_MENU_ENABLED".into(), "0".into()),
                ("LUMEN_FILE_SERVICE".into(), "0".into()),
                ("LUMEN_LAUNCH_SCOPES".into(), "0".into()),
            ]);
        }
        for (key, suffix) in [
            ("XDG_CONFIG_HOME", "config"),
            ("XDG_STATE_HOME", "state"),
            ("XDG_CACHE_HOME", "cache"),
            ("XDG_DATA_HOME", "data"),
        ] {
            env.push((key.into(), output.join(suffix).into()));
        }
        for (key, value) in [
            ("QT_ACCESSIBILITY", "0"),
            ("QT_LINUX_ACCESSIBILITY_ALWAYS_ON", "0"),
            ("NO_AT_BRIDGE", "1"),
            ("GTK_A11Y", "none"),
        ] {
            env.push((key.into(), value.into()));
        }
        if matches!(args.mode, Mode::Toggle) {
            env.extend([
                ("QT_QPA_PLATFORM".into(), "offscreen".into()),
                ("QT_QUICK_BACKEND".into(), "software".into()),
                ("QT_QPA_PLATFORMTHEME".into(), "generic".into()),
                ("LUMEN_NATIVE_WINDOW_CONTROLS".into(), "0".into()),
                ("LUMEN_UNIFIED_OWNED_HEADERS".into(), "1".into()),
                ("LUMEN_FILE_SERVICE".into(), "0".into()),
                ("LUMEN_LAUNCH_SCOPES".into(), "0".into()),
            ]);
            env.push((
                "FONTCONFIG_FILE".into(),
                fs::canonicalize(
                    args.font_config
                        .as_ref()
                        .ok_or("toggle requires --font-config")?,
                )?
                .into(),
            ));
        }
        Ok(Self {
            mode: args.mode,
            output,
            runtime,
            ui,
            env,
            children: Vec::new(),
        })
    }

    fn command(&self, program: impl AsRef<OsStr>) -> Command {
        let mut command = Command::new(program);
        command
            .env_remove("AT_SPI_BUS_ADDRESS")
            .env_remove("DISPLAY");
        command.envs(self.env.iter().cloned());
        command
    }

    fn launch(&mut self, program: impl AsRef<OsStr>, args: &[&str], name: &str) -> Result<()> {
        let log = File::create(self.output.join(format!("{name}.log")))?;
        let child = self
            .command(program)
            .args(args)
            .stdout(Stdio::from(log.try_clone()?))
            .stderr(Stdio::from(log))
            .process_group(0)
            .spawn()?;
        self.children.push(child);
        Ok(())
    }

    fn stop_last(&mut self) -> Result<()> {
        let mut child = self.children.pop().ok_or("No child to stop")?;
        unsafe {
            libc::kill(-(child.id() as i32), libc::SIGTERM);
        }
        let start = Instant::now();
        while child.try_wait()?.is_none() && start.elapsed() < Duration::from_secs(5) {
            thread::sleep(Duration::from_millis(50));
        }
        if child.try_wait()?.is_none() {
            unsafe {
                libc::kill(-(child.id() as i32), libc::SIGKILL);
            }
        }
        let _ = child.wait();
        Ok(())
    }

    fn call(&self, program: &str, args: &[&str]) -> Result<String> {
        let mut child = self
            .command(program)
            .args(args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        let start = Instant::now();
        loop {
            if child.try_wait()?.is_some() {
                break;
            }
            if start.elapsed() > Duration::from_secs(12) {
                child.kill()?;
                return Err(format!("{program} timed out").into());
            }
            thread::sleep(Duration::from_millis(10));
        }
        let output = child.wait_with_output()?;
        if !output.status.success() {
            return Err(format!(
                "{program} failed: {}",
                String::from_utf8_lossy(&output.stderr)
            )
            .into());
        }
        Ok(String::from_utf8(output.stdout)?)
    }

    fn ipc(&self, method: &str, args: &[&str]) -> Result<String> {
        let mut all = vec![
            "-p",
            self.ui.to_str().unwrap(),
            "ipc",
            "call",
            if matches!(self.mode, Mode::Popover) {
                "audit"
            } else {
                "calendarTest"
            },
            method,
        ];
        all.extend_from_slice(args);
        self.call("quickshell", &all)
    }

    fn state(&self) -> Result<Value> {
        Ok(serde_json::from_str(self.ipc("status", &[])?.trim())?)
    }

    fn wait(&self, label: &str, mut predicate: impl FnMut() -> bool) -> Result<()> {
        let start = Instant::now();
        while start.elapsed() < Duration::from_secs(25) {
            if predicate() {
                return Ok(());
            }
            thread::sleep(Duration::from_millis(150));
        }
        Err(format!("{label} timed out; inspect {}", self.output.display()).into())
    }

    fn screenshot(&self, name: &str) -> Result<()> {
        self.call(
            "grim",
            &[self.output.join(format!("{name}.png")).to_str().unwrap()],
        )?;
        Ok(())
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        for child in self.children.iter_mut().rev() {
            if child.try_wait().ok().flatten().is_none() {
                unsafe {
                    libc::kill(-(child.id() as i32), libc::SIGTERM);
                }
                for _ in 0..50 {
                    if child.try_wait().ok().flatten().is_some() {
                        break;
                    }
                    thread::sleep(Duration::from_millis(100));
                }
                if child.try_wait().ok().flatten().is_none() {
                    unsafe {
                        libc::kill(-(child.id() as i32), libc::SIGKILL);
                    }
                }
            }
            let _ = child.wait();
        }
    }
}

fn appearance(session: &Session, dark: bool) -> Result<()> {
    let palette = if dark {
        json!({"background":"#28282b","surface":"#343437","field":"#1e1e21","text":"#f5f5f7","muted":"#aeaeb5","border":"#55555b","hover":"#45454b","accent":"#0a84ff","selectedText":"#ffffff"})
    } else {
        json!({"background":"#f2f2f4","surface":"#ffffff","field":"#e5e5e8","text":"#202023","muted":"#6b6b70","border":"#c8c8ce","hover":"#dedee3","accent":"#007aff","selectedText":"#ffffff"})
    };
    fs::write(
        session.output.join("appearance.json"),
        serde_json::to_vec(
            &json!({"font":"Interlude Variable","palette":palette,"preferences":{}}),
        )?,
    )?;
    Ok(())
}

fn setup(args: &Args) -> Result<Session> {
    let mut session = Session::new(args)?;
    appearance(&session, false)?;
    let config = session.output.join("sway.conf");
    let extra = match args.mode {
        Mode::Window => {
            "for_window [title=\"Calendar\"] floating enable, border none, move position center\n"
        }
        Mode::Popover => "for_window [app_id=\"org.quickshell\"] floating enable, border none\n",
        Mode::Toggle => "",
        Mode::Card => "",
    };
    let resolution = if matches!(args.mode, Mode::Popover) {
        "900x1000"
    } else {
        "1280x900"
    };
    fs::write(
        &config,
        format!("output HEADLESS-1 resolution {resolution}\nxwayland disable\n{extra}"),
    )?;
    if matches!(args.mode, Mode::Popover) {
        session.launch(
            "sway",
            &["--unsupported-gpu", "-c", config.to_str().unwrap()],
            "sway",
        )?;
    } else {
        session.launch("sway", &["-c", config.to_str().unwrap()], "sway")?;
    }
    let runtime = session.runtime.clone();
    session.wait("Sway socket", || {
        fs::read_dir(&runtime).ok().is_some_and(|entries| {
            entries
                .flatten()
                .any(|item| item.file_name().to_string_lossy().starts_with("sway-ipc"))
        })
    })?;
    let entries = fs::read_dir(&runtime)?
        .flatten()
        .map(|entry| entry.file_name())
        .collect::<Vec<_>>();
    let sway = entries
        .iter()
        .find(|name| name.to_string_lossy().starts_with("sway-ipc"))
        .ok_or("Sway socket missing")?;
    let wayland = entries
        .iter()
        .find(|name| {
            name.to_string_lossy().starts_with("wayland-")
                && !name.to_string_lossy().ends_with(".lock")
        })
        .ok_or("Wayland socket missing")?;
    session
        .env
        .push(("SWAYSOCK".into(), runtime.join(sway).into()));
    session
        .env
        .push(("WAYLAND_DISPLAY".into(), wayland.clone()));
    let ui = session.ui.to_string_lossy().into_owned();
    if matches!(args.mode, Mode::Popover) {
        return Ok(session);
    }
    session.launch("quickshell", &["-p", &ui], "shell")?;
    session.wait("Calendar load", || {
        session
            .state()
            .ok()
            .and_then(|state| state["loaded"].as_bool())
            .unwrap_or(false)
    })?;
    Ok(session)
}

fn check_log(session: &Session, window: bool) -> Result<()> {
    let log = fs::read_to_string(session.output.join("shell.log"))?;
    let mut bad = vec![
        "Failed to load configuration",
        "ReferenceError:",
        "TypeError:",
        "Binding loop",
        "Cannot assign [undefined]",
    ];
    if window {
        bad.push("Error: left-hand");
    }
    for term in bad {
        if log.contains(term) {
            return Err(format!("shell log contains {term}").into());
        }
    }
    Ok(())
}

fn card(args: &Args) -> Result<()> {
    let session = setup(args)?;
    let mut checks = Vec::new();
    for (scale, factor) in [("1", 1.0_f64), ("1.25", 1.25), ("2", 2.0)] {
        for height in [900, 600, 450] {
            session.call(
                "swaymsg",
                &[&format!(
                    "output HEADLESS-1 resolution {}x{} scale {scale}",
                    (1280.0 * factor) as i32,
                    (f64::from(height) * factor) as i32
                )],
            )?;
            thread::sleep(Duration::from_millis(500));
            for dark in [false, true] {
                appearance(&session, dark)?;
                session.ipc("edit", &[if dark { "true" } else { "false" }])?;
                thread::sleep(Duration::from_millis(500));
                session.call(
                    "wtype",
                    &[
                        "-s",
                        "150",
                        "-M",
                        "ctrl",
                        "-k",
                        "a",
                        "-m",
                        "ctrl",
                        "Edited event title",
                    ],
                )?;
                session.wait("title edit", || {
                    session
                        .state()
                        .ok()
                        .is_some_and(|s| s["title"] == "Edited event title")
                })?;
                session.ipc("notes", &[])?;
                session.call(
                    "wtype",
                    &[
                        "-s",
                        "150",
                        "-M",
                        "ctrl",
                        "-k",
                        "a",
                        "-m",
                        "ctrl",
                        "Retained notes",
                    ],
                )?;
                session.wait("notes edit", || {
                    session
                        .state()
                        .ok()
                        .is_some_and(|s| s["notes"] == "Retained notes")
                })?;
                session.ipc("save", &[])?;
                session.wait("save error", || {
                    session.state().ok().is_some_and(|s| {
                        s["error"].as_str().is_some_and(|value| !value.is_empty())
                            && s["busy"] == false
                    })
                })?;
                let state = session.state()?;
                if state["editing"] != true
                    || state["title"] != "Edited event title"
                    || state["notes"] != "Retained notes"
                {
                    return Err(format!("editor lost content: {state}").into());
                }
                for name in ["save", "cancel", "notesBounds"] {
                    let bounds = &state[name];
                    let y = bounds["y"].as_f64().ok_or("missing control bounds")?;
                    let h = bounds["height"].as_f64().ok_or("missing control height")?;
                    let panel = state["height"].as_f64().ok_or("missing panel height")?;
                    if bounds["visible"] != true || y < 0.0 || y + h > panel {
                        return Err(format!("{name} outside panel: {state}").into());
                    }
                }
                let name = format!("{scale}-{height}-{}", if dark { "dark" } else { "light" });
                session.screenshot(&name)?;
                checks.push(json!({"name":name,"state":state}));
                session.ipc("cancel", &[])?;
                session.wait("cancel edit", || {
                    session.state().ok().is_some_and(|s| s["editing"] == false)
                })?;
            }
        }
    }
    check_log(&session, false)?;
    fs::write(
        session.output.join("result.json"),
        serde_json::to_vec_pretty(&json!({"passed":true,"checks":checks}))?,
    )?;
    println!("{}", session.output.join("result.json").display());
    Ok(())
}

fn window(args: &Args) -> Result<()> {
    let session = setup(args)?;
    let mut checks = Vec::new();
    for (scale, factor) in [("1", 1.0_f64), ("1.25", 1.25), ("2", 2.0)] {
        session.call(
            "swaymsg",
            &[&format!(
                "output HEADLESS-1 resolution {}x{} scale {scale}",
                (1280.0 * factor) as i32,
                (900.0 * factor) as i32
            )],
        )?;
        for dark in [false, true] {
            appearance(&session, dark)?;
            for view in ["Month", "Week", "Day", "Year"] {
                session.ipc("view", &[view])?;
                session.wait("calendar view", || {
                    session.state().ok().is_some_and(|s| s["view"] == view)
                })?;
                thread::sleep(Duration::from_millis(500));
                let name = format!("{scale}-{}-{view}", if dark { "dark" } else { "light" });
                session.screenshot(&name)?;
                checks.push(json!({"name":name,"state":session.state()?}));
            }
            session.ipc("view", &["Month"])?;
            session.ipc("step", &["1"])?;
            session.wait("next month", || {
                session
                    .state()
                    .ok()
                    .is_some_and(|s| s["month"] == "2026-11-01")
            })?;
            session.ipc("step", &["-1"])?;
            session.wait("previous month", || {
                session
                    .state()
                    .ok()
                    .is_some_and(|s| s["month"] == "2026-10-01")
            })?;
            session.ipc("edit", &[])?;
            session.wait("calendar edit", || {
                session.state().ok().is_some_and(|s| s["editing"] == true)
            })?;
            session.ipc("closeEditor", &[])?;
        }
    }
    check_log(&session, true)?;
    fs::write(
        session.output.join("result.json"),
        serde_json::to_vec_pretty(&json!({"passed":true,"checks":checks}))?,
    )?;
    println!("{}", session.output.join("result.json").display());
    Ok(())
}

fn popover(args: &Args) -> Result<()> {
    let mut session = setup(args)?;
    let palettes: Value = serde_json::from_slice(&fs::read(
        args.assets
            .parent()
            .ok_or("Missing appearance asset parent")?
            .join("appearance/palette.json"),
    )?)?;
    let mut checks = Vec::new();
    for theme in ["light", "dark"] {
        session.call(
            "swaymsg",
            &["output HEADLESS-1 resolution 900x1000 scale 1"],
        )?;
        thread::sleep(Duration::from_millis(200));
        let appearance = session.output.join(format!("{theme}.json"));
        fs::write(
            &appearance,
            serde_json::to_vec(&json!({
                "font":palettes["font"],"palette":palettes[theme],
                "preferences":{"mode":theme,"reduceMotion":true}
            }))?,
        )?;
        session
            .env
            .push(("LUMEN_APPEARANCE_STATE".into(), appearance.into()));
        let ui = session.ui.to_string_lossy().into_owned();
        session.launch("quickshell", &["-p", &ui], theme)?;
        session.wait("calendar events", || {
            session
                .state()
                .ok()
                .is_some_and(|s| s["events"] == 8 && s["busy"] == false)
        })?;
        session.ipc("check", &[])?;
        session.wait("date logic", || {
            session.state().ok().is_some_and(|s| s["busy"] == false)
        })?;
        let mut keyboard = session
            .command("wtype")
            .args(["-s", "500", "-k", "Right"])
            .spawn()?;
        thread::sleep(Duration::from_millis(200));
        session.ipc("focusGrid", &[])?;
        expect(keyboard.wait()?.success(), "Physical calendar key failed")?;
        session.wait("keyboard date navigation", || {
            session
                .state()
                .ok()
                .is_some_and(|s| s["selected"] == "2026-10-17")
        })?;
        checks.push(format!("{theme} real keyboard date navigation"));
        session.ipc("today", &[])?;
        session.wait("today", || {
            session.state().ok().is_some_and(|s| s["busy"] == false)
        })?;
        session.screenshot(&format!("{theme}-collapsed"))?;
        session.ipc("notificationCheck", &[])?;
        thread::sleep(Duration::from_millis(200));
        session.ipc("notificationOpen", &[])?;
        session.ipc("notificationPolicy", &[])?;
        session.screenshot(&format!("{theme}-panel"))?;
        for (resolution, scale) in [("1920x1080", "1"), ("1920x1080", "2"), ("1280x720", "2")] {
            session.call(
                "swaymsg",
                &[&format!(
                    "output HEADLESS-1 resolution {resolution} scale {scale}"
                )],
            )?;
            thread::sleep(Duration::from_millis(250));
            let state = session.state()?;
            expect(
                state["scale"] == 1
                    && state["contentHeight"]
                        .as_f64()
                        .is_some_and(|height| height > 0.)
                    && state["width"].as_f64().is_some_and(|width| width <= 344.),
                &format!("Calendar panel clipped at {resolution} scale {scale}: {state}"),
            )?;
            session.screenshot(&format!("{theme}-{resolution}-{scale}"))?;
            checks.push(format!(
                "{theme} readable panel at {resolution} scale {scale}"
            ));
        }
        session.ipc("preferences", &["agenda", "true", "false"])?;
        session.wait("agenda preference", || {
            session.state().ok().is_some_and(|s| {
                s["calendarVisible"] == false
                    && s["style"] == "agenda"
                    && s["calendarFirst"] == true
            })
        })?;
        let preference = session
            .output
            .join("state/lumen-desktop/session/notification-centre.json");
        session.wait("saved calendar preference", || {
            fs::read(&preference)
                .ok()
                .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
                .is_some_and(|value| value["calendarVisible"] == false)
        })?;
        session.call(
            "swaymsg",
            &["output HEADLESS-1 resolution 900x1000 scale 1"],
        )?;
        thread::sleep(Duration::from_millis(200));
        session.ipc("preferences", &["month", "false", "true"])?;
        session.wait("month preference", || {
            session
                .state()
                .ok()
                .is_some_and(|s| s["calendarVisible"] == true)
        })?;
        session.ipc("editWidgets", &[])?;
        session.screenshot(&format!("{theme}-widget-editor"))?;
        session.ipc("editor", &[])?;
        session.wait("event editor", || {
            session
                .state()
                .ok()
                .is_some_and(|s| s["editorVisible"] == true)
        })?;
        thread::sleep(Duration::from_millis(500));
        session.ipc("editorCheck", &[])?;
        session.screenshot(&format!("{theme}-event-editor"))?;
        session.stop_last()?;
        session.launch("quickshell", &["-p", &ui], &format!("{theme}-restart"))?;
        session.wait("saved preferences after restart", || {
            session.state().ok().is_some_and(|s| {
                s["style"] == "month" && s["calendarVisible"] == true && s["calendarFirst"] == false
            })
        })?;
        checks.push(format!("{theme} saved preferences survive restart"));
        session.stop_last()?;
        for suffix in ["", "-restart"] {
            let log = fs::read_to_string(session.output.join(format!("{theme}{suffix}.log")))?;
            for bad in [
                "Failed to load configuration",
                "ReferenceError:",
                "TypeError:",
                "Binding loop",
            ] {
                expect(!log.contains(bad), &format!("Calendar log contains {bad}"))?;
            }
        }
    }
    fs::write(
        session.output.join("results.json"),
        serde_json::to_vec_pretty(&json!({"passed":true,"checks":checks}))?,
    )?;
    println!("PASS calendar/notification regression and capture matrix");
    Ok(())
}

fn expect(ok: bool, detail: &str) -> Result<()> {
    if ok { Ok(()) } else { Err(detail.into()) }
}

fn painted_pixels(image: &image::RgbImage, color: &Value) -> Result<usize> {
    let mut expected = [0_i32; 3];
    for (index, channel) in ["r", "g", "b"].iter().enumerate() {
        expected[index] =
            (color[*channel].as_f64().ok_or("missing sample color")? * 255.0).round() as i32;
    }
    Ok(image
        .pixels()
        .filter(|pixel| (0..3).all(|index| (i32::from(pixel[index]) - expected[index]).abs() <= 3))
        .count())
}

fn toggle(args: &Args) -> Result<()> {
    let mut session = Session::new(args)?;
    let ui = session.ui.to_string_lossy().into_owned();
    session.launch("quickshell", &["-p", &ui], "qt")?;
    let start = Instant::now();
    let status = loop {
        if let Some(status) = session
            .children
            .last_mut()
            .ok_or("Qt process missing")?
            .try_wait()?
        {
            break status;
        }
        if start.elapsed() >= Duration::from_secs(25) {
            return Err(format!(
                "Calendar toggle Qt timed out; inspect {}",
                session.output.display()
            )
            .into());
        }
        thread::sleep(Duration::from_millis(50));
    };
    let log = fs::read_to_string(session.output.join("qt.log"))?;
    expect(
        status.success() && !log.contains("CALENDAR_TOGGLE_FAILURE"),
        &format!("Calendar toggle QML failed: {log}"),
    )?;
    let marker = "CALENDAR_TOGGLE_RESULT ";
    let record = log
        .lines()
        .find_map(|line| line.split_once(marker).map(|(_, value)| value))
        .ok_or("Calendar toggle result missing")?;
    let mut cases: Vec<Value> = serde_json::from_str(record)?;
    expect(cases.len() == 4, "Calendar toggle expected four cases")?;
    for case in &mut cases {
        let mode = case["mode"].as_str().ok_or("missing mode")?.to_owned();
        let contrast = case["increaseContrast"]
            .as_bool()
            .ok_or("missing contrast")?;
        let capture = PathBuf::from(case["capture"].as_str().ok_or("missing capture")?);
        let image = image::open(&capture)?.to_rgb8();
        for (index, sample) in case["samples"]
            .as_array_mut()
            .ok_or("missing samples")?
            .iter_mut()
            .enumerate()
        {
            let rect = &sample["rect"];
            let x = (rect["x"].as_f64().ok_or("missing x")? * 2.0).round() as u32;
            let y = (rect["y"].as_f64().ok_or("missing y")? * 2.0).round() as u32;
            let right = ((rect["x"].as_f64().ok_or("missing x")?
                + rect["width"].as_f64().ok_or("missing width")?)
                * 2.0)
                .round() as u32;
            let bottom = ((rect["y"].as_f64().ok_or("missing y")?
                + rect["height"].as_f64().ok_or("missing height")?)
                * 2.0)
                .round() as u32;
            expect(
                right > x && bottom > y && right <= image.width() && bottom <= image.height(),
                "Calendar mark crop outside frame",
            )?;
            let crop = image::imageops::crop_imm(&image, x, y, right - x, bottom - y).to_image();
            crop.save(
                session
                    .output
                    .join(format!("{mode}-{contrast}-{index}-mark.png")),
            )?;
            let ink = painted_pixels(&crop, &sample["foreground"])?;
            let fill = painted_pixels(&crop, &sample["fill"])?;
            sample["paintedInk"] = json!(ink);
            sample["paintedFill"] = json!(fill);
            expect(
                ink >= 1 && fill >= 12,
                &format!("Calendar mark lacks painted colors: {sample}"),
            )?;
        }
    }
    let source = fs::read(args.assets.join("CalendarWindow.qml"))?;
    let result = json!({
        "passed": true,
        "cases": cases,
        "sourceSha256": format!("{:x}", Sha256::digest(source)),
        "scope": "Actual CalendarWindow visibility marks, all five tints in light/dark with both contrast preferences; genuine Qt software frames and unchecked state, not physical-input or reference-exact typography"
    });
    fs::write(
        session.output.join("result.json"),
        serde_json::to_vec_pretty(&result)?,
    )?;
    println!("PASS Calendar visibility marks in four Qt software frames");
    Ok(())
}

fn main() -> Result<()> {
    if env::args_os()
        .next()
        .as_deref()
        .map(Path::new)
        .and_then(|path| path.file_name())
        == Some(OsStr::new("lumen-calendar"))
    {
        return backend();
    }
    let args = parse_args()?;
    if !args.inside {
        outside(&args)
    } else {
        match args.mode {
            Mode::Card => card(&args),
            Mode::Window => window(&args),
            Mode::Popover => popover(&args),
            Mode::Toggle => toggle(&args),
        }
    }
}
