//! Private offscreen acceptance for the production Files share menu.
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::env;
use std::error::Error;
use std::ffi::OsStr;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

pub type Result<T> = std::result::Result<T, Box<dyn Error>>;
fn fail(message: impl Into<String>) -> Box<dyn Error> {
    message.into().into()
}
pub fn require(ok: bool, message: &str) -> Result<()> {
    if ok { Ok(()) } else { Err(fail(message)) }
}
pub struct Options {
    pub launcher: PathBuf,
    pub fontconfig: PathBuf,
    pub output: PathBuf,
    pub window_source: Option<PathBuf>,
    pub inside: bool,
    pub run: bool,
}
pub fn options() -> Result<Options> {
    let mut launcher = None;
    let mut fontconfig = None;
    let mut output = None;
    let mut window_source = None;
    let mut inside = false;
    let mut run = false;
    let mut args = env::args_os().skip(1);
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("--launcher") => {
                launcher = Some(PathBuf::from(args.next().ok_or("Missing launcher")?))
            }
            Some("--fontconfig") => {
                fontconfig = Some(PathBuf::from(args.next().ok_or("Missing fontconfig")?))
            }
            Some("--output") => output = Some(PathBuf::from(args.next().ok_or("Missing output")?)),
            Some("--window-source") => {
                window_source = Some(PathBuf::from(args.next().ok_or("Missing window source")?))
            }
            Some("--inside") => inside = true,
            Some("--run") => run = true,
            _ => return Err(fail("Unknown Files share menu option")),
        }
    }
    Ok(Options {
        launcher: launcher.ok_or("--launcher required")?,
        fontconfig: fontconfig.ok_or("--fontconfig required")?,
        output: output.ok_or("--output required")?,
        window_source,
        inside,
        run,
    })
}
pub fn immutable(path: &Path) -> Result<PathBuf> {
    let value = fs::canonicalize(path)?;
    require(
        value.starts_with("/nix/store") && value.is_file(),
        "Existing immutable fixture dependency required",
    )?;
    Ok(value)
}
pub fn digest(data: impl AsRef<[u8]>) -> String {
    hex::encode(Sha256::digest(data))
}
fn file_hash(path: &Path) -> Result<String> {
    Ok(digest(fs::read(path)?))
}
fn tree_hashes(root: &Path, folder: &Path, hashes: &mut BTreeMap<String, String>) -> Result<()> {
    for entry in fs::read_dir(folder)? {
        let path = entry?.path();
        if path.is_dir() {
            tree_hashes(root, &path, hashes)?;
        } else if path.is_file() {
            hashes.insert(
                path.strip_prefix(root)?.to_string_lossy().into_owned(),
                file_hash(&path)?,
            );
        }
    }
    Ok(())
}
fn tree_hash(root: &Path) -> Result<String> {
    let mut hashes = BTreeMap::new();
    tree_hashes(root, root, &mut hashes)?;
    Ok(digest(serde_json::to_vec(&hashes)?))
}
pub fn copy_tree(source: &Path, target: &Path) -> Result<()> {
    fs::create_dir(target)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let from = entry.path();
        let to = target.join(entry.file_name());
        if from.is_dir() {
            copy_tree(&from, &to)?;
        } else {
            fs::copy(&from, &to)?;
        }
    }
    Ok(())
}
pub fn launcher_environment(launcher: &Path) -> Result<BTreeMap<String, String>> {
    let output = Command::new(launcher).arg("--print-env0").output()?;
    require(
        output.status.success(),
        "Installed launcher environment failed",
    )?;
    let mut values = BTreeMap::new();
    for row in output
        .stdout
        .split(|b| *b == 0)
        .filter(|row| !row.is_empty())
    {
        if let Some((key, value)) = std::str::from_utf8(row)?.split_once('=') {
            values.insert(key.into(), value.into());
        }
    }
    Ok(values)
}
fn find_program(name: &str) -> Result<PathBuf> {
    for folder in env::split_paths(&env::var_os("PATH").unwrap_or_default()) {
        let path = folder.join(name);
        if path.is_file() {
            return Ok(path);
        }
    }
    Err(fail(format!("{name} not found in pinned PATH")))
}
fn command_args(options: &Options, inside: bool, run: bool) -> Vec<String> {
    let mut args = vec![
        "--launcher".into(),
        options.launcher.display().to_string(),
        "--fontconfig".into(),
        options.fontconfig.display().to_string(),
        "--output".into(),
        options.output.display().to_string(),
    ];
    if let Some(path) = &options.window_source {
        args.push("--window-source".into());
        args.push(path.display().to_string());
    }
    if inside {
        args.push("--inside".into());
    }
    if run {
        args.push("--run".into());
    }
    args
}
pub fn outer(options: &Options) -> Result<()> {
    let repo = fs::canonicalize(env::current_dir()?)?;
    let output = if options.output.is_absolute() {
        options.output.clone()
    } else {
        repo.join(&options.output)
    };
    require(!output.exists(), "Fresh artifact directory required")?;
    let launcher = immutable(&options.launcher)?;
    let fontconfig = immutable(&options.fontconfig)?;
    let window_source = options
        .window_source
        .as_ref()
        .map(|value| immutable(value))
        .transpose()?;
    let before = tree_hash(&repo.join("res/lumen-desktop"))?;
    fs::create_dir_all(&output)?;
    fs::set_permissions(&output, fs::Permissions::from_mode(0o700))?;
    let private = Options {
        launcher: launcher.clone(),
        fontconfig: fontconfig.clone(),
        output: PathBuf::from("/artifacts"),
        window_source,
        inside: true,
        run: false,
    };
    let environment = launcher_environment(&launcher)?;
    let mut command = Command::new(find_program("lumen-application-isolation")?);
    command
        .arg("run")
        .arg("--source")
        .arg(&repo)
        .arg("--output")
        .arg(&output)
        .arg("--")
        .arg(env::current_exe()?)
        .args(command_args(&private, true, false));
    command
        .env_clear()
        .envs(&environment)
        .env("FONTCONFIG_FILE", &fontconfig);
    let status = command.status()?;
    let after = tree_hash(&repo.join("res/lumen-desktop"))?;
    let unchanged = before == after;
    let revision = Command::new("git")
        .args([
            "-C",
            repo.to_str().ok_or("Source path invalid")?,
            "rev-parse",
            "HEAD",
        ])
        .output()?;
    let revision = String::from_utf8(revision.stdout)?.trim().to_owned();
    let provenance = json!({"sourceRevision":revision,"sourceAssetsSha256":before,"launcherSha256":file_hash(&launcher)?,"fontConfigSha256":file_hash(&fontconfig)?,"fixtureExecutableSha256":file_hash(&env::current_exe()?)?,"fixtureSha256":digest([include_bytes!("lib.rs").as_slice(),include_bytes!("main.rs").as_slice(),include_bytes!("../fixture.qml").as_slice()].concat()),"launcher":launcher,"fontConfig":fontconfig,"exitCode":status.code().unwrap_or(128),"sourceUnchanged":unchanged});
    fs::write(
        output.join("offscreen-boundary.json"),
        serde_json::to_vec_pretty(&provenance)?,
    )?;
    fs::write(
        output.join("provenance.json"),
        serde_json::to_vec_pretty(
            &json!({"revision":revision,"exitCode":status.code().unwrap_or(128)}),
        )?,
    )?;
    require(
        unchanged,
        "Fixture source assets changed during private Qt acceptance",
    )?;
    require(
        status.success(),
        &format!("Private Files share menu fixture exited {status}"),
    )?;
    Ok(())
}
pub fn private_environment() -> Result<BTreeMap<String, String>> {
    require(
        unsafe { libc::getuid() } != 0,
        "Private fixture must not run as root",
    )?;
    require(
        env::var("HOME")? == "/home/fixture",
        "Full private home required",
    )?;
    let homes = fs::read_dir("/home")?
        .filter_map(|row| row.ok().map(|row| row.file_name()))
        .collect::<Vec<_>>();
    require(
        homes == [OsStr::new("fixture")],
        "Only fixture home may be visible",
    )?;
    require(!Path::new("/.old-root").exists(), "Host root is visible")?;
    let network = fs::read_to_string("/proc/net/dev")?;
    let interfaces = network
        .lines()
        .skip(2)
        .filter_map(|line| line.split_once(':').map(|(name, _)| name.trim().to_owned()))
        .collect::<Vec<_>>();
    require(interfaces == ["lo"], "Private loopback network required")?;
    require(
        !Path::new("/dev/pts/ptmx").exists(),
        "Private PTY was exposed",
    )?;
    for name in ["dri", "snd", "video0", "udmabuf"] {
        require(
            !Path::new("/dev").join(name).exists(),
            &format!("Hardware device visible: /dev/{name}"),
        )?;
    }
    let mut values = env::vars().collect::<BTreeMap<_, _>>();
    for (key, value) in [
        ("QT_QPA_PLATFORM", "offscreen"),
        ("QT_QUICK_BACKEND", "software"),
        ("QT_QPA_PLATFORMTHEME", "generic"),
        ("QT_ACCESSIBILITY", "0"),
        ("QT_LINUX_ACCESSIBILITY_ALWAYS_ON", "0"),
        ("NO_AT_BRIDGE", "1"),
        ("GTK_A11Y", "none"),
        ("LUMEN_NATIVE_WINDOW_CONTROLS", "0"),
        ("LUMEN_UNIFIED_OWNED_HEADERS", "1"),
        ("LUMEN_NEARBY_SHARING_LABEL", "Nearby Sharing"),
        ("LUMEN_NEARBY_SHARING_PROVIDER", ""),
        ("LUMEN_FILE_SERVICE", "0"),
        ("LUMEN_LAUNCH_SCOPES", "0"),
    ] {
        values.insert(key.into(), value.into());
    }
    for key in [
        "WAYLAND_DISPLAY",
        "DISPLAY",
        "SWAYSOCK",
        "AT_SPI_BUS_ADDRESS",
    ] {
        values.remove(key);
    }
    Ok(values)
}
pub fn inner(options: &Options) -> Result<()> {
    inner_with_timeout(options, 35)
}
pub fn inner_with_timeout(options: &Options, seconds: u64) -> Result<()> {
    let private = private_environment()?;
    let mut command = Command::new("timeout");
    command
        .arg(format!("{}s", seconds.clamp(1, 60)))
        .arg("dbus-run-session");
    let config = options.output.join("bus.conf");
    fs::write(
        &config,
        format!(
            "<busconfig><type>session</type><listen>unix:tmpdir=/run/fixture</listen><policy user=\"{}\"><allow own=\"*\"/><allow send_destination=\"*\"/><allow receive_sender=\"*\"/></policy></busconfig>",
            unsafe { libc::getuid() }
        ),
    )?;
    command
        .arg(format!("--config-file={}", config.display()))
        .arg("--")
        .arg(env::current_exe()?)
        .args(command_args(options, true, true))
        .env_clear()
        .envs(private);
    let status = command.status()?;
    require(
        status.success(),
        &format!("Private D-Bus share menu fixture exited {status}"),
    )
}
pub fn replace_once(
    path: &Path,
    old: &str,
    new: &str,
    reason: &str,
    seams: &mut Vec<Value>,
) -> Result<()> {
    let before = fs::read_to_string(path)?;
    require(
        before.matches(old).count() == 1,
        &format!("Expected one seam: {}", path.display()),
    )?;
    let after = before.replacen(old, new, 1);
    require(
        after.replacen(new, old, 1) == before,
        "Fixture seam was not reversible",
    )?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    fs::write(path, &after)?;
    seams.push(json!({"path":path,"reason":reason,"beforeSha256":digest(before.as_bytes()),"afterSha256":digest(after.as_bytes()),"old":old,"new":new}));
    Ok(())
}
