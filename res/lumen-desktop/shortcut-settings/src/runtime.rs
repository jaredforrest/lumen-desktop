//! Informational compositor-load witness, never an authentication boundary.
use serde_json::{Value, json};
use std::{
    env, fs,
    io::{Read, Write},
    os::{
        fd::AsRawFd,
        unix::{
            fs::{DirBuilderExt, FileTypeExt, MetadataExt, OpenOptionsExt},
            net::UnixStream,
        },
    },
    path::{Path, PathBuf},
    time::Duration,
};
type Result<T> = std::result::Result<T, String>;
fn record_path() -> Result<PathBuf> {
    let dir =
        PathBuf::from(env::var("XDG_RUNTIME_DIR").map_err(|_| "Session runtime unavailable")?)
            .join("lumen-desktop");
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(&dir)
        .map_err(|e| e.to_string())?;
    let metadata = fs::symlink_metadata(&dir).map_err(|e| e.to_string())?;
    if !metadata.is_dir()
        || metadata.uid() != unsafe { libc::geteuid() }
        || metadata.mode() & 0o077 != 0
    {
        return Err("Unsafe shortcut runtime directory".into());
    }
    Ok(dir.join("shortcuts-runtime.json"))
}
fn connect() -> Result<(UnixStream, Value)> {
    let path = env::var("SWAYSOCK").map_err(|_| "No compositor session is connected")?;
    let before = fs::symlink_metadata(&path).map_err(|e| e.to_string())?;
    if !before.file_type().is_socket() || before.uid() != unsafe { libc::geteuid() } {
        return Err("Foreign or linked compositor socket".into());
    }
    let stream = UnixStream::connect(&path).map_err(|e| e.to_string())?;
    stream
        .set_read_timeout(Some(Duration::from_millis(500)))
        .map_err(|e| e.to_string())?;
    stream
        .set_write_timeout(Some(Duration::from_millis(500)))
        .map_err(|e| e.to_string())?;
    let mut peer = libc::ucred {
        pid: 0,
        uid: 0,
        gid: 0,
    };
    let mut size = std::mem::size_of::<libc::ucred>() as libc::socklen_t;
    if unsafe {
        libc::getsockopt(
            stream.as_raw_fd(),
            libc::SOL_SOCKET,
            libc::SO_PEERCRED,
            (&raw mut peer).cast(),
            &raw mut size,
        )
    } != 0
        || size as usize != std::mem::size_of::<libc::ucred>()
        || peer.uid != unsafe { libc::geteuid() }
        || peer.pid <= 0
    {
        return Err("Compositor peer identity unavailable".into());
    }
    let after = fs::symlink_metadata(&path).map_err(|e| e.to_string())?;
    if before.dev() != after.dev() || before.ino() != after.ino() || before.uid() != after.uid() {
        return Err("Compositor socket changed during observation".into());
    }
    let stat = fs::read_to_string(format!("/proc/{}/stat", peer.pid)).map_err(|e| e.to_string())?;
    let start = stat
        .rsplit_once(") ")
        .and_then(|(_, tail)| tail.split_whitespace().nth(19))
        .ok_or("Compositor process epoch unavailable")?;
    let boot = fs::read_to_string("/proc/sys/kernel/random/boot_id").map_err(|e| e.to_string())?;
    Ok((
        stream,
        json!({"socket":path,"dev":before.dev(),"ino":before.ino(),"owner":before.uid(),"pid":peer.pid,"start":start,"boot":boot.trim()}),
    ))
}
fn config(stream: &mut UnixStream) -> Result<Value> {
    let mut header = [0u8; 14];
    header[..6].copy_from_slice(b"i3-ipc");
    header[10..].copy_from_slice(&9u32.to_le_bytes());
    stream.write_all(&header).map_err(|e| e.to_string())?;
    stream.read_exact(&mut header).map_err(|e| e.to_string())?;
    let length = u32::from_le_bytes(header[6..10].try_into().unwrap()) as usize;
    if &header[..6] != b"i3-ipc"
        || u32::from_le_bytes(header[10..].try_into().unwrap()) != 9
        || length > 4 * 1024 * 1024
    {
        return Err("Invalid compositor configuration response".into());
    }
    let mut body = vec![0; length];
    stream.read_exact(&mut body).map_err(|e| e.to_string())?;
    serde_json::from_slice(&body).map_err(|e| e.to_string())
}
pub fn observe(source: &Path) -> Result<Value> {
    if !source.starts_with("/nix/store") || !source.is_file() {
        return Err("Immutable shortcut generation required".into());
    }
    let (mut stream, mut epoch) = connect()?;
    let config = config(&mut stream)?;
    if config.get("lumen_config_valid").is_none() {
        return Err("The running compositor lacks load validation. Start a new session with the updated compositor.".into());
    }
    if config["lumen_config_valid"] != true {
        return Err("The running compositor reports configuration errors; shortcut activation is unconfirmed".into());
    }
    let generation = source.to_str().ok_or("Invalid generation path")?;
    let present = config["config"].as_str().is_some_and(|text| {
        text.lines().any(|line| {
            let words = line.split_whitespace().collect::<Vec<_>>();
            words.first() == Some(&"exec_always")
                && words
                    .iter()
                    .any(|word| word.ends_with("/bin/lumen-shortcut-runtime-witness"))
                && words
                    .last()
                    .is_some_and(|word| word.trim_matches(['\'', '"']) == generation)
        })
    });
    if !present {
        return Err("Generated shortcuts have not been loaded by this compositor".into());
    }
    if connect()?.1 != epoch {
        return Err("Compositor epoch changed during configuration observation".into());
    }
    epoch["registrySource"] = json!(generation);
    Ok(epoch)
}
fn safe_record(path: &Path) -> Result<fs::File> {
    let file = fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)
        .map_err(|e| e.to_string())?;
    let metadata = file.metadata().map_err(|e| e.to_string())?;
    if !metadata.is_file()
        || metadata.uid() != unsafe { libc::geteuid() }
        || metadata.nlink() != 1
        || metadata.mode() & 0o7777 != 0o600
    {
        return Err("Unsafe shortcut runtime witness".into());
    }
    Ok(file)
}
pub fn confirmed(source: &Path) -> Result<Value> {
    let path = record_path()?;
    let mut bytes = Vec::new();
    safe_record(&path).map_err(|error|format!("Waiting for a valid compositor load witness ({error}). If this persists, start a new session with the updated compositor."))?
        .take(65537)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > 65536 {
        return Err("Shortcut runtime witness is too large".into());
    }
    let record: Value = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    let epoch = observe(source)?;
    if record != json!({"version":1,"epoch":epoch}) {
        return Err("Shortcut runtime witness belongs to another session or generation".into());
    }
    Ok(epoch)
}
pub fn publish(source: &Path) -> Result<Value> {
    let epoch = observe(source)?;
    let path = record_path()?;
    if path.symlink_metadata().is_ok() {
        safe_record(&path)?;
    }
    let next = path.with_file_name(format!(".shortcuts-runtime-{}.json", std::process::id()));
    let result = (|| -> Result<()> {
        let mut file = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .mode(0o600)
            .open(&next)
            .map_err(|e| e.to_string())?;
        file.write_all(json!({"version":1,"epoch":epoch}).to_string().as_bytes())
            .map_err(|e| e.to_string())?;
        file.sync_all().map_err(|e| e.to_string())?;
        fs::rename(&next, &path).map_err(|e| e.to_string())?;
        Ok(())
    })();
    let _ = fs::remove_file(next);
    result?;
    Ok(epoch)
}
