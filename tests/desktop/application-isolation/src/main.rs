//! Private filesystem, network and process boundary for native desktop fixtures.
use serde_json::json;
use std::collections::BTreeMap;
use std::env;
use std::error::Error;
use std::ffi::{CString, OsStr, OsString};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::os::fd::{AsRawFd, FromRawFd};
use std::os::unix::fs::{FileTypeExt, MetadataExt, PermissionsExt};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{self, Command};
use std::thread;

type Result<T> = std::result::Result<T, Box<dyn Error>>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct DeviceIdentity {
    dev: u64,
    ino: u64,
    rdev: u64,
}

fn device_identity_matches(
    kind_is_character: bool,
    observed: DeviceIdentity,
    expected: DeviceIdentity,
) -> bool {
    kind_is_character && observed == expected
}

fn sysfs_device_matches(expected: &str, dev: &str, uevent: &str) -> bool {
    dev.trim() == expected && uevent.lines().any(|line| line == "DEVNAME=udmabuf")
}

fn malformed(message: &'static str) -> Box<dyn Error> {
    message.into()
}

fn next_path(args: &mut impl Iterator<Item = OsString>, message: &'static str) -> Result<PathBuf> {
    Ok(PathBuf::from(
        args.next().ok_or_else(|| malformed(message))?,
    ))
}

fn arguments(
    mut args: impl Iterator<Item = OsString>,
) -> Result<(PathBuf, PathBuf, bool, bool, Vec<OsString>)> {
    let mut source = None;
    let mut output = None;
    let mut software = false;
    let mut pty = false;
    let mut command = Vec::new();
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("--source") => source = Some(next_path(&mut args, "Missing source path")?),
            Some("--output") => output = Some(next_path(&mut args, "Missing output path")?),
            Some("--software-graphics") => software = true,
            Some("--private-pty") => pty = true,
            Some("--") => {
                command.extend(args);
                break;
            }
            _ => return Err(malformed("Unknown isolation option")),
        }
    }
    Ok((
        source.ok_or("Missing source")?,
        output.ok_or("Missing output")?,
        software,
        pty,
        command,
    ))
}

fn validate(source: &Path, output: &Path) -> Result<(PathBuf, PathBuf)> {
    if unsafe { libc::geteuid() } == 0 {
        return Err(malformed("Fixture must retain a non-root identity"));
    }
    let source = fs::canonicalize(source)?;
    let output = fs::canonicalize(output)?;
    if !source.is_dir() || !output.is_dir() {
        return Err(malformed(
            "Existing source and artifact directories required",
        ));
    }
    let info = fs::metadata(&output)?;
    if info.uid() != unsafe { libc::getuid() }
        || info.mode() & 0o022 != 0
        || fs::read_dir(&output)?.next().is_some()
    {
        return Err(malformed(
            "Artifact directory must be empty and writable only by its owner",
        ));
    }
    Ok((source, output))
}

fn software_device() -> Result<(File, DeviceIdentity)> {
    let path = Path::new("/dev/udmabuf");
    let info = fs::symlink_metadata(path)?;
    if !info.file_type().is_char_device() {
        return Err(malformed("Software allocator must be a character device"));
    }
    let cpath = CString::new("/dev/udmabuf")?;
    if unsafe { libc::access(cpath.as_ptr(), libc::R_OK | libc::W_OK) } != 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    let rdev = info.rdev();
    let major = ((rdev >> 8) & 0xfff) | ((rdev >> 32) & 0xfffff000);
    let minor = (rdev & 0xff) | ((rdev >> 12) & 0xffffff00);
    let expected = format!("{major}:{minor}");
    if !sysfs_device_matches(
        &expected,
        &fs::read_to_string("/sys/class/misc/udmabuf/dev")?,
        &fs::read_to_string("/sys/class/misc/udmabuf/uevent")?,
    ) {
        return Err(malformed(
            "Software allocator identity does not match sysfs",
        ));
    }
    let fd = unsafe { libc::open(cpath.as_ptr(), libc::O_PATH | libc::O_NOFOLLOW) };
    if fd < 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    let file = unsafe { File::from_raw_fd(fd) };
    let pinned = file.metadata()?;
    let identity = DeviceIdentity {
        dev: pinned.dev(),
        ino: pinned.ino(),
        rdev: pinned.rdev(),
    };
    if !device_identity_matches(
        pinned.file_type().is_char_device(),
        identity,
        DeviceIdentity {
            dev: info.dev(),
            ino: info.ino(),
            rdev,
        },
    ) {
        return Err(malformed("Software allocator changed before binding"));
    }
    Ok((file, identity))
}

fn runtime_environment(
    environment: impl IntoIterator<Item = (OsString, OsString)>,
) -> BTreeMap<OsString, OsString> {
    let mut values = BTreeMap::new();
    for (key, value) in environment {
        let name = key.to_string_lossy();
        if name.starts_with("QT_")
            || name.starts_with("QML")
            || matches!(
                name.as_ref(),
                "PATH"
                    | "GLOBAL_MENU_PYTHON"
                    | "LOCALE_ARCHIVE"
                    | "TZDIR"
                    | "LANG"
                    | "LC_ALL"
                    | "GI_TYPELIB_PATH"
                    | "GIO_EXTRA_MODULES"
                    | "LD_LIBRARY_PATH"
                    | "FONTCONFIG_FILE"
                    | "FONTCONFIG_PATH"
            )
        {
            values.insert(key, value);
        }
    }
    let path = values.get(OsStr::new("PATH")).cloned().unwrap_or_default();
    let pinned = env::split_paths(&path)
        .filter(|folder| {
            folder
                .as_os_str()
                .to_string_lossy()
                .starts_with("/nix/store/")
        })
        .collect::<Vec<_>>();
    values.insert("PATH".into(), env::join_paths(pinned).unwrap_or_default());
    for (key, value) in [
        ("HOME", "/home/fixture"),
        ("USER", "fixture"),
        ("LOGNAME", "fixture"),
        ("XDG_RUNTIME_DIR", "/run/fixture"),
        ("XDG_CONFIG_HOME", "/home/fixture/.config"),
        ("XDG_DATA_HOME", "/home/fixture/.local/share"),
        ("XDG_STATE_HOME", "/home/fixture/.local/state"),
        ("XDG_CACHE_HOME", "/home/fixture/.cache"),
        ("XDG_DATA_DIRS", "/nonexistent"),
        (
            "DBUS_SESSION_BUS_ADDRESS",
            "unix:path=/run/fixture/inert-session",
        ),
        (
            "DBUS_SYSTEM_BUS_ADDRESS",
            "unix:path=/run/fixture/inert-system",
        ),
        ("PULSE_SERVER", "unix:/run/fixture/inert-audio"),
        ("PIPEWIRE_REMOTE", "inert-pipewire"),
        ("GTK_A11Y", "none"),
        ("NO_AT_BRIDGE", "1"),
        ("PYTHONWARNINGS", "error"),
    ] {
        values.insert(key.into(), value.into());
    }
    values
}

fn write_etc(root: &Path) -> Result<()> {
    let uid = unsafe { libc::getuid() };
    let gid = unsafe { libc::getgid() };
    for (name, content) in [
        (
            "passwd",
            format!("fixture:x:{uid}:{gid}:Disposable fixture:/home/fixture:/bin/sh\n"),
        ),
        ("group", format!("fixture:x:{gid}:\n")),
        (
            "hosts",
            "127.0.0.1 localhost fixture.test\n::1 localhost\n".into(),
        ),
        (
            "nsswitch.conf",
            "passwd: files\ngroup: files\nhosts: files\n".into(),
        ),
        ("resolv.conf", String::new()),
        ("machine-id", "0123456789abcdef0123456789abcdef\n".into()),
    ] {
        fs::write(root.join(name), content)?;
    }
    Ok(())
}

fn run_with_environment(
    source: &Path,
    output: &Path,
    command: &[OsString],
    software: bool,
    pty: bool,
    environment: impl IntoIterator<Item = (OsString, OsString)>,
) -> Result<i32> {
    if command.is_empty() {
        return Err(malformed("A fixture command is required"));
    }
    let (source, output) = validate(source, output)?;
    let etc = tempfile::tempdir()?;
    write_etc(etc.path())?;
    let device = if software {
        Some(software_device()?)
    } else {
        None
    };
    let mut bwrap = Command::new(env!("LUMEN_BWRAP"));
    bwrap.args([
        "--unshare-all",
        "--as-pid-1",
        "--die-with-parent",
        "--hostname",
        "fixture",
        "--cap-drop",
        "ALL",
        "--clearenv",
    ]);
    bwrap.args([
        "--size",
        "2147483648",
        "--tmpfs",
        "/",
        "--chmod",
        "0755",
        "/",
    ]);
    bwrap.arg("--ro-bind").arg("/nix/store").arg("/nix/store");
    bwrap.arg("--ro-bind").arg(&source).arg("/source");
    bwrap.arg("--bind").arg(&output).arg("/artifacts");
    bwrap.args(["--dir", "/etc", "--dir", "/sys", "--dev", "/dev"]);
    if !pty {
        bwrap.args(["--tmpfs", "/dev/pts"]);
    }
    if let Some((file, _)) = &device {
        bwrap
            .arg("--dev-bind")
            .arg(format!("/proc/self/fd/{}", file.as_raw_fd()))
            .arg("/dev/udmabuf");
    }
    bwrap.args([
        "--size",
        "268435456",
        "--tmpfs",
        "/dev/shm",
        "--chmod",
        "1777",
        "/dev/shm",
    ]);
    bwrap.args([
        "--proc",
        "/proc",
        "--size",
        "2147483648",
        "--tmpfs",
        "/tmp",
        "--chmod",
        "1777",
        "/tmp",
    ]);
    bwrap.args([
        "--tmpfs",
        "/run",
        "--dir",
        "/run/fixture",
        "--chmod",
        "0700",
        "/run/fixture",
    ]);
    bwrap.args([
        "--tmpfs",
        "/home",
        "--dir",
        "/home/fixture",
        "--chmod",
        "0700",
        "/home/fixture",
    ]);
    for name in [
        ".config",
        ".local",
        ".local/share",
        ".local/state",
        ".cache",
    ] {
        let path = format!("/home/fixture/{name}");
        bwrap.args(["--dir", &path, "--chmod", "0700", &path]);
    }
    for name in [
        "passwd",
        "group",
        "hosts",
        "nsswitch.conf",
        "resolv.conf",
        "machine-id",
    ] {
        bwrap
            .arg("--ro-bind")
            .arg(etc.path().join(name))
            .arg(format!("/etc/{name}"));
    }
    for (key, value) in runtime_environment(environment) {
        bwrap.arg("--setenv").arg(key).arg(value);
    }
    bwrap.args(["--chdir", "/source", "--"]);
    if let Some((_, identity)) = &device {
        bwrap
            .arg(env::current_exe()?)
            .arg("inside-graphics")
            .arg(identity.dev.to_string())
            .arg(identity.ino.to_string())
            .arg(identity.rdev.to_string())
            .arg("--");
    }
    bwrap.args(command);
    let status = bwrap.status()?;
    Ok(status.code().unwrap_or(128))
}

fn run(
    source: &Path,
    output: &Path,
    command: &[OsString],
    software: bool,
    pty: bool,
) -> Result<i32> {
    run_with_environment(source, output, command, software, pty, env::vars_os())
}

fn inside_graphics(mut args: impl Iterator<Item = OsString>) -> Result<()> {
    let number = |value: Option<OsString>| -> Result<u64> {
        Ok(value
            .ok_or("Missing device identity")?
            .to_string_lossy()
            .parse()?)
    };
    let expected = DeviceIdentity {
        dev: number(args.next())?,
        ino: number(args.next())?,
        rdev: number(args.next())?,
    };
    if args.next().as_deref() != Some(OsStr::new("--")) {
        return Err(malformed("Missing child command"));
    }
    let command = args.collect::<Vec<_>>();
    if command.is_empty() {
        return Err(malformed("Missing child command"));
    }
    let info = fs::symlink_metadata("/dev/udmabuf")?;
    if !device_identity_matches(
        info.file_type().is_char_device(),
        DeviceIdentity {
            dev: info.dev(),
            ino: info.ino(),
            rdev: info.rdev(),
        },
        expected,
    ) {
        return Err(malformed("Bound software allocator changed"));
    }
    Err(Command::new(&command[0]).args(&command[1..]).exec().into())
}

fn probe_inside(sentinel: &Path, pty: bool) -> Result<()> {
    if process::id() != 1
        || unsafe { libc::geteuid() } == 0
        || env::var_os("FIXTURE_SECRET").is_some()
        || env::var_os("LD_PRELOAD").is_some()
    {
        return Err(malformed("Host identity or secret escaped the boundary"));
    }
    if fs::read_to_string("/proc/sys/kernel/hostname")?.trim() != "fixture"
        || sentinel.exists()
        || Path::new("/persist").exists()
    {
        return Err(malformed("Private hostname or filesystem is incorrect"));
    }
    let homes = fs::read_dir("/home")?
        .map(|entry| entry.map(|item| item.file_name()))
        .collect::<std::io::Result<Vec<_>>>()?;
    if homes != [OsString::from("fixture")]
        || ["dri", "snd", "video0"]
            .iter()
            .any(|name| Path::new("/dev").join(name).exists())
    {
        return Err(malformed("Host home or hardware entered the fixture"));
    }
    if Path::new("/dev/pts/ptmx").exists() != pty {
        return Err(malformed("Private PTY policy differs"));
    }
    if OpenOptions::new()
        .append(true)
        .open("/source/README.md")
        .is_ok()
    {
        return Err(malformed("Source became writable"));
    }
    let status = fs::read_to_string("/proc/self/status")?;
    if !status.contains("CapEff:\t0000000000000000")
        || !status.contains("CapBnd:\t0000000000000000")
        || !status.contains("NoNewPrivs:\t1")
    {
        return Err(malformed("Namespace capabilities remain"));
    }
    if Path::new("/.old-root").exists()
        || fs::read_to_string("/proc/net/route")?
            .lines()
            .skip(1)
            .any(|line| line.split_whitespace().nth(1) == Some("00000000"))
    {
        return Err(malformed("Old root or default network route remains"));
    }
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let address = listener.local_addr()?;
    let worker = thread::spawn(move || -> std::io::Result<()> {
        let (mut stream, _) = listener.accept()?;
        stream.write_all(b"private loopback")
    });
    let mut reply = Vec::new();
    TcpStream::connect(address)?.read_to_end(&mut reply)?;
    worker
        .join()
        .map_err(|_| malformed("Loopback worker panicked"))??;
    if reply != b"private loopback" {
        return Err(malformed("Private loopback failed"));
    }
    if !Command::new(env::current_exe()?)
        .arg("nested-user-probe")
        .status()?
        .success()
    {
        return Err(malformed("Nested user namespace unavailable"));
    }
    fs::write("/artifacts/result.json", json!({"passed":true,"uid":unsafe{libc::getuid()},"pid":process::id(),"network":fs::read_to_string("/proc/net/dev")?,"privatePty":pty}).to_string())?;
    Ok(())
}

fn probe(source: &Path, output: &Path) -> Result<()> {
    fs::create_dir(output)?;
    fs::set_permissions(output, fs::Permissions::from_mode(0o700))?;
    let private = tempfile::tempdir()?;
    let sentinel = private.path().join("must-not-be-visible");
    fs::write(&sentinel, "synthetic outside boundary")?;
    let mut reports = Vec::new();
    for pty in [false, true] {
        let target = output.join(if pty { "private-pty" } else { "default" });
        fs::create_dir(&target)?;
        fs::set_permissions(&target, fs::Permissions::from_mode(0o700))?;
        let args = [
            env::current_exe()?.into_os_string(),
            "probe-inside".into(),
            sentinel.as_os_str().to_owned(),
            if pty { "true" } else { "false" }.into(),
        ];
        let supplied = env::vars_os().chain(std::iter::once((
            "FIXTURE_SECRET".into(),
            "synthetic omitted value".into(),
        )));
        let exit = run_with_environment(source, &target, &args, false, pty, supplied)?;
        if exit != 0 {
            return Err(format!("Isolation probe exited {exit}").into());
        }
        reports.push(serde_json::from_str::<serde_json::Value>(
            &fs::read_to_string(target.join("result.json"))?,
        )?);
    }
    if fs::read_to_string(sentinel)? != "synthetic outside boundary" {
        return Err(malformed("Host sentinel changed"));
    }
    fs::write(
        output.join("result.json"),
        serde_json::to_string_pretty(&json!({"passed":true,"probes":reports}))?,
    )?;
    Ok(())
}

fn real_main() -> Result<i32> {
    let mut args = env::args_os().skip(1);
    match args.next().as_deref() {
        Some(value) if value == "run" => {
            let (source, output, software, pty, command) = arguments(args)?;
            run(&source, &output, &command, software, pty)
        }
        Some(value) if value == "inside-graphics" => {
            inside_graphics(args)?;
            Ok(0)
        }
        Some(value) if value == "probe" => {
            let (source, output, _, _, command) = arguments(args)?;
            if !command.is_empty() {
                return Err(malformed("Probe takes no child command"));
            }
            probe(&source, &output)?;
            Ok(0)
        }
        Some(value) if value == "probe-inside" => {
            let sentinel = next_path(&mut args, "Missing sentinel")?;
            let pty = args.next().as_deref() == Some(OsStr::new("true"));
            probe_inside(&sentinel, pty)?;
            Ok(0)
        }
        Some(value) if value == "nested-user-probe" => {
            if unsafe { libc::unshare(libc::CLONE_NEWUSER) } != 0 {
                return Err(std::io::Error::last_os_error().into());
            }
            Ok(0)
        }
        _ => Err(malformed("Expected run or probe")),
    }
}

fn main() {
    match real_main() {
        Ok(code) => process::exit(code),
        Err(error) => {
            eprintln!("application isolation: {error}");
            process::exit(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn output_must_be_empty_and_private() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("source");
        let output = root.path().join("output");
        fs::create_dir(&source).unwrap();
        fs::create_dir(&output).unwrap();
        fs::set_permissions(&output, fs::Permissions::from_mode(0o700)).unwrap();
        assert!(validate(&source, &output).is_ok());
        fs::write(output.join("sentinel"), "unchanged").unwrap();
        assert!(validate(&source, &output).is_err());
        fs::remove_file(output.join("sentinel")).unwrap();
        fs::set_permissions(&output, fs::Permissions::from_mode(0o777)).unwrap();
        assert!(validate(&source, &output).is_err());
    }
    #[test]
    fn forwarded_environment_excludes_secrets_and_host_paths() {
        let values = runtime_environment([("FIXTURE_SECRET".into(), "not forwarded".into())]);
        assert!(!values.contains_key(OsStr::new("FIXTURE_SECRET")));
        assert_eq!(
            values.get(OsStr::new("DBUS_SYSTEM_BUS_ADDRESS")).unwrap(),
            "unix:path=/run/fixture/inert-system"
        );
    }
    #[test]
    fn software_allocator_requires_character_identity_and_sysfs_name() {
        let expected = DeviceIdentity {
            dev: 7,
            ino: 5,
            rdev: 10,
        };
        assert!(device_identity_matches(true, expected, expected));
        assert!(!device_identity_matches(false, expected, expected));
        assert!(!device_identity_matches(
            true,
            DeviceIdentity { ino: 6, ..expected },
            expected
        ));
        assert!(!device_identity_matches(
            true,
            DeviceIdentity {
                rdev: 11,
                ..expected
            },
            expected
        ));
        assert!(sysfs_device_matches(
            "10:267",
            "10:267\n",
            "MAJOR=10\nMINOR=267\nDEVNAME=udmabuf\n"
        ));
        assert!(!sysfs_device_matches(
            "10:267",
            "10:268\n",
            "DEVNAME=udmabuf\n"
        ));
        assert!(!sysfs_device_matches(
            "10:267",
            "10:267\n",
            "DEVNAME=other\n"
        ));
    }
}
