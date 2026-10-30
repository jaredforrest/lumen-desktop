//! Actual switcher function routing with real QObjects; no compositor required.
use serde_json::json;
use sha2::{Digest, Sha256};
use std::os::unix::fs::PermissionsExt;
use std::{
    env,
    error::Error,
    fs,
    path::PathBuf,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};
type Result<T> = std::result::Result<T, Box<dyn Error>>;
fn immutable(path: PathBuf) -> Result<PathBuf> {
    let path = path.canonicalize()?;
    if !path.starts_with("/nix/store") || !path.is_file() {
        return Err("Immutable executable/configuration required".into());
    }
    Ok(path)
}
fn section(source: &str, start: &str, end: &str) -> Result<String> {
    let rest = source
        .split_once(start)
        .ok_or("Production function start missing")?
        .1;
    let body = rest
        .split_once(end)
        .ok_or("Production function boundary missing")?
        .0;
    Ok(format!("{start}{body}"))
}
fn run() -> Result<()> {
    let mut quickshell = None;
    let mut source = None;
    let mut output = None;
    let mut font = None;
    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        let value = PathBuf::from(args.next().ok_or("Missing option value")?);
        match arg.as_str() {
            "--quickshell" => quickshell = Some(immutable(value)?),
            "--fontconfig" => font = Some(immutable(value)?),
            "--source" => source = Some(value.canonicalize()?),
            "--output" => output = Some(value),
            _ => return Err("Unknown coverage fixture option".into()),
        }
    }
    let quickshell = quickshell.ok_or("--quickshell required")?;
    let source = source.ok_or("--source required")?;
    let output = output.ok_or("--output required")?;
    if !output.is_absolute() {
        return Err("Absolute fresh output path required".into());
    }
    let font = font.ok_or("--fontconfig required")?;
    fs::create_dir(&output)?;
    let production = fs::read_to_string(source.join("AppSwitcher.qml"))?;
    let model = fs::read(source.join("AppSwitcherModel.js"))?;
    let fixture = include_str!("../../mixed_coverage.qml")
        .replace(
            "__PRODUCTION_IDENTITY__",
            &section(
                &production,
                "function identity(window) {",
                "    function command(action) {",
            )?,
        )
        .replace(
            "__PRODUCTION_COMMAND__",
            &section(
                &production,
                "function command(action) {",
                "    function commit()",
            )?,
        )
        .replace(
            "__PRODUCTION_COMMIT__",
            &section(&production, "function commit(){", "    function status()")?,
        );
    fs::write(output.join("shell.qml"), fixture)?;
    fs::write(output.join("AppSwitcherModel.js"), &model)?;
    for name in ["home", "runtime", "config", "cache", "state"] {
        fs::create_dir(output.join(name))?;
        fs::set_permissions(output.join(name), fs::Permissions::from_mode(0o700))?;
    }
    let log = output.join("quickshell.log");
    let file = fs::File::create(&log)?;
    let mut command = Command::new(&quickshell);
    command
        .args(["-p"])
        .arg(output.join("shell.qml"))
        .env_clear()
        .env("HOME", output.join("home"))
        .env("XDG_RUNTIME_DIR", output.join("runtime"))
        .env("XDG_CONFIG_HOME", output.join("config"))
        .env("XDG_CACHE_HOME", output.join("cache"))
        .env("XDG_STATE_HOME", output.join("state"))
        .env("QT_QPA_PLATFORM", "offscreen")
        .env("QT_QUICK_BACKEND", "software")
        .env("QT_QPA_PLATFORMTHEME", "")
        .env("FONTCONFIG_FILE", font)
        .env(
            "DBUS_SESSION_BUS_ADDRESS",
            format!("unix:path={}/absent-bus", output.display()),
        )
        .env("NO_AT_BRIDGE", "1")
        .env("GTK_A11Y", "none")
        .stdin(Stdio::null())
        .stdout(file.try_clone()?)
        .stderr(file);
    let mut child = command.spawn()?;
    let deadline = Instant::now() + Duration::from_secs(10);
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if Instant::now() >= deadline {
            child.kill()?;
            child.wait()?;
            return Err("Coverage fixture timed out; child killed and reaped".into());
        }
        thread::sleep(Duration::from_millis(20));
    };
    let log = fs::read_to_string(log)?;
    if !status.success()
        || !log.contains("SWITCHER_COVERAGE_PASS ")
        || log.contains("SWITCHER_COVERAGE_FAILED")
        || log.contains("SWITCHER_COVERAGE_TIMEOUT")
        || log
            .lines()
            .filter(|line| !line.contains("SWITCHER_COVERAGE_PASS "))
            .any(|line| {
                ["WARN", "TypeError", "ReferenceError", "Binding loop"]
                    .iter()
                    .any(|marker| line.contains(marker))
            })
    {
        return Err("Actual QML coverage fixture failed; inspect quickshell.log".into());
    }
    let value = log
        .lines()
        .find_map(|line| {
            line.split_once("SWITCHER_COVERAGE_PASS ")
                .map(|(_, value)| value)
        })
        .ok_or("Missing actual QML result")?;
    let cases: serde_json::Value = serde_json::from_str(value)?;
    if cases.as_array().is_none_or(|rows| rows.len() != 20) {
        return Err("Incomplete actual QML coverage cases".into());
    }
    fs::write(
        output.join("result.json"),
        serde_json::to_vec_pretty(
            &json!({"passed":true,"cases":cases,"production":source,"quickshell":quickshell,"productionSha256":format!("{:x}",Sha256::digest(production.as_bytes())),"modelSha256":format!("{:x}",Sha256::digest(model)),"scope":"Unmodified production identity/command/commit bodies and model; actual offscreen QML with real QObjects. Synthetic broker and desktop-entry resolution; no native/XWayland client or compositor acceptance."}),
        )?,
    )?;
    Ok(())
}
fn main() {
    if let Err(error) = run() {
        eprintln!("Switcher coverage: {error}");
        std::process::exit(1);
    }
}
