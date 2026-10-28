//! Private offscreen acceptance for the production Files share menu.
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::env;
use std::error::Error;
use std::ffi::OsStr;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

type Result<T> = std::result::Result<T, Box<dyn Error>>;
fn fail(message: impl Into<String>) -> Box<dyn Error> {
    message.into().into()
}
fn require(ok: bool, message: &str) -> Result<()> {
    if ok {
        Ok(())
    } else {
        Err(fail(message))
    }
}
struct Options {
    launcher: PathBuf,
    fontconfig: PathBuf,
    output: PathBuf,
    window_source: Option<PathBuf>,
    inside: bool,
    run: bool,
}
fn options() -> Result<Options> {
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
fn immutable(path: &Path) -> Result<PathBuf> {
    let value = fs::canonicalize(path)?;
    require(
        value.starts_with("/nix/store") && value.is_file(),
        "Existing immutable fixture dependency required",
    )?;
    Ok(value)
}
fn digest(data: impl AsRef<[u8]>) -> String {
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
fn copy_tree(source: &Path, target: &Path) -> Result<()> {
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
fn launcher_environment(launcher: &Path) -> Result<BTreeMap<String, String>> {
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
fn outer(options: &Options) -> Result<()> {
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
    let provenance = json!({"sourceRevision":revision,"sourceAssetsSha256":before,"launcherSha256":file_hash(&launcher)?,"fontConfigSha256":file_hash(&fontconfig)?,"fixtureSha256":digest([include_bytes!("main.rs").as_slice(),include_bytes!("../fixture.qml").as_slice()].concat()),"launcher":launcher,"fontConfig":fontconfig,"exitCode":status.code().unwrap_or(128),"sourceUnchanged":unchanged});
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
fn private_environment() -> Result<BTreeMap<String, String>> {
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
fn inner(options: &Options) -> Result<()> {
    let private = private_environment()?;
    let mut command = Command::new("timeout");
    command.arg("35s").arg("dbus-run-session");
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
fn replace_once(
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
fn run(options: &Options) -> Result<()> {
    let mut values = private_environment()?;
    require(
        env::var("DBUS_SESSION_BUS_ADDRESS")?.starts_with("unix:"),
        "Private session bus missing",
    )?;
    let installed = launcher_environment(&options.launcher)?;
    for key in [
        "PATH",
        "QML_IMPORT_PATH",
        "QT_PLUGIN_PATH",
        "LUMEN_FILES_BIN",
        "LUMEN_FILE_JOBS_BIN",
        "LUMEN_DESKTOP_ASSETS",
    ] {
        if let Some(value) = installed.get(key) {
            values.insert(key.into(), value.clone());
        }
    }
    values.insert(
        "FONTCONFIG_FILE".into(),
        options.fontconfig.display().to_string(),
    );
    let out = &options.output;
    let assets = out.join("assets");
    copy_tree(Path::new("/source/res/lumen-desktop"), &assets)?;
    let mut seams = Vec::new();
    if let Some(window) = &options.window_source {
        let mut baseline = fs::read_to_string(immutable(window)?)?;
        for (old, new) in [
            (
                "id:actions;onOpened:",
                "id:actions;objectName:\"files-actions-menu\";onOpened:",
            ),
            (
                "required property var modelData;width:208;",
                "objectName:\"files-action-\"+modelData.action;required property var modelData;width:208;",
            ),
        ] {
            require(
                baseline.matches(old).count() == 1,
                "Window source marker missing",
            )?;
            baseline = baseline.replacen(old, new, 1);
        }
        let target = assets.join("shell/FilesWindow.qml");
        let original = fs::read_to_string(&target)?;
        replace_once(
            &target,
            &original,
            &baseline,
            "Immutable candidate FilesWindow source",
            &mut seams,
        )?;
    }
    replace_once(
        &assets.join("shell/NearbySharingState.qml"),
        "readonly property bool configured:!!(Quickshell.env(\"LUMEN_NEARBY_SHARING_PROVIDER\") || \"\")",
        "readonly property bool configured:true",
        "Enable private sharing row",
        &mut seams,
    )?;
    replace_once(
        &assets.join("shell/NearbySharingState.qml"),
        "function refresh(){if(configured && !sharingReader.running)sharingReader.running=true;}",
        "function refresh(){}",
        "Prevent transport launch",
        &mut seams,
    )?;
    replace_once(
        &assets.join("shell/AppLaunch.qml"),
        "QtObject {",
        "QtObject {\n    property var recordedCommands:[]",
        "Record application commands",
        &mut seams,
    )?;
    replace_once(
        &assets.join("shell/AppLaunch.qml"),
        "function run(command, workingDirectory) {",
        "function run(command, workingDirectory) {\n        recordedCommands=recordedCommands.concat([command]);return;",
        "Record without dispatch",
        &mut seams,
    )?;
    fs::write(
        out.join("fixture-seams.json"),
        serde_json::to_vec_pretty(&seams)?,
    )?;
    let folder = Path::new("/home/fixture/Documents");
    fs::create_dir(folder)?;
    let first = folder.join("a $(literal).txt");
    let second = folder.join("b.txt");
    fs::write(&first, "first unchanged")?;
    fs::write(&second, "second unchanged")?;
    let palette: Value =
        serde_json::from_slice(&fs::read(assets.join("appearance/palette.json"))?)?;
    let appearance = out.join("appearance.json");
    fs::write(
        &appearance,
        json!({"font":palette["font"],"palette":palette["dark"],"preferences":{}}).to_string(),
    )?;
    values.insert(
        "LUMEN_APPEARANCE_STATE".into(),
        appearance.display().to_string(),
    );
    let qml = out.join("fixture.qml");
    fs::write(&qml, include_str!("../fixture.qml"))?;
    let output = Command::new("timeout")
        .args(["15s", "quickshell", "-p"])
        .arg(&qml)
        .env_clear()
        .envs(&values)
        .output()?;
    let log = String::from_utf8_lossy(&output.stdout).to_string()
        + &String::from_utf8_lossy(&output.stderr);
    fs::write(out.join("qml.log"), &log)?;
    require(
        output.status.success(),
        &format!("Files share QML failed: {log}"),
    )?;
    require(
        ![
            "ReferenceError",
            "TypeError",
            "binding loop",
            "FILES_SHARE_MENU_FAIL",
        ]
        .iter()
        .any(|marker| log.to_lowercase().contains(&marker.to_lowercase())),
        &format!("QML errors: {log}"),
    )?;
    let marker = "FILES_SHARE_MENU_PASS ";
    let record = log
        .lines()
        .find_map(|line| line.split_once(marker).map(|(_, value)| value))
        .ok_or("QML success record missing")?;
    let mut result: Value = serde_json::from_str(record)?;
    require(
        fs::read_to_string(&first)? == "first unchanged"
            && fs::read_to_string(&second)? == "second unchanged",
        "Disposable file bytes changed",
    )?;
    result["passed"] = json!(true);
    result["scope"] = json!(
        "Actual production Files menu signal dispatch; no physical pointer, transport or delivery claim"
    );
    result["namespace"] = json!({"status":fs::read_to_string("/proc/self/status")?,"mounts":fs::read_to_string("/proc/self/mountinfo")?});
    fs::write(out.join("result.json"), serde_json::to_vec_pretty(&result)?)?;
    println!("PASS private Files share menu dispatch");
    Ok(())
}
fn main() {
    let result = (|| -> Result<()> {
        let options = options()?;
        if options.run {
            run(&options)
        } else if options.inside {
            inner(&options)
        } else {
            outer(&options)
        }
    })();
    if let Err(error) = result {
        eprintln!("Files share menu fixture failed: {error}");
        std::process::exit(1)
    }
}
