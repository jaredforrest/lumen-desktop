//! Captured personal-Trash confirmation. All inspection is relative to opened directories.
use crate::{JobResult, python_json, trash_root, unquote_path};
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};
use std::ffi::{CString, OsStr};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::os::fd::{AsRawFd, FromRawFd};
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::thread;
use std::time::{Duration, Instant};

const MAX_ITEMS: usize = 1000;
const MAX_NODES: usize = 4096;
const MAX_BYTES: u64 = 8 * 1024 * 1024 * 1024;
const MAX_DEPTH: usize = 64;
const MAX_MANIFEST_BYTES: usize = 4 * 1024 * 1024;
const MAX_JOURNAL_BYTES: usize = 32 * 1024 * 1024;
const MAX_PUBLIC_BYTES: usize = 1024 * 1024;
const MAX_ERROR_BYTES: usize = 1024;
const PREPARE_SECONDS: u64 = 2;
const DELETE_SECONDS: u64 = 5;
const CONFIRM_SECONDS: u64 = 120;
const PUBLIC_ENVELOPE_BYTES: usize = 128;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Facts {
    pub(crate) dev: u64,
    pub(crate) ino: u64,
    pub(crate) mode: u32,
    pub(crate) size: i64,
    pub(crate) mtime_ns: i128,
    pub(crate) ctime_ns: i128,
}

impl Facts {
    pub(crate) fn from_stat(info: &libc::stat) -> Self {
        Self {
            dev: info.st_dev,
            ino: info.st_ino,
            mode: info.st_mode,
            size: info.st_size,
            mtime_ns: info.st_mtime as i128 * 1_000_000_000 + info.st_mtime_nsec as i128,
            ctime_ns: info.st_ctime as i128 * 1_000_000_000 + info.st_ctime_nsec as i128,
        }
    }
    pub(crate) fn directory(self) -> bool {
        self.mode & libc::S_IFMT == libc::S_IFDIR
    }
    pub(crate) fn regular(self) -> bool {
        self.mode & libc::S_IFMT == libc::S_IFREG
    }
    pub(crate) fn symlink(self) -> bool {
        self.mode & libc::S_IFMT == libc::S_IFLNK
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Identity {
    pub(crate) dev: u64,
    pub(crate) ino: u64,
    pub(crate) mode: u32,
    pub(crate) uid: u32,
    pub(crate) mount: u64,
}

fn name(value: &OsStr) -> JobResult<CString> {
    CString::new(value.as_bytes()).map_err(|_| "Invalid Trash path".into())
}

pub(crate) fn open_at(
    parent: libc::c_int,
    path: &OsStr,
    flags: libc::c_int,
    mode: libc::mode_t,
) -> JobResult<File> {
    let path = name(path)?;
    let fd = unsafe { libc::openat(parent, path.as_ptr(), flags | libc::O_CLOEXEC, mode) };
    if fd < 0 {
        return Err(std::io::Error::last_os_error().to_string());
    }
    Ok(unsafe { File::from_raw_fd(fd) })
}

pub(crate) fn stat_fd(file: &File) -> JobResult<libc::stat> {
    let mut info: libc::stat = unsafe { std::mem::zeroed() };
    if unsafe { libc::fstat(file.as_raw_fd(), &mut info) } != 0 {
        return Err(std::io::Error::last_os_error().to_string());
    }
    Ok(info)
}

pub(crate) fn mount_id(fd: libc::c_int) -> JobResult<u64> {
    let raw =
        fs::read_to_string(format!("/proc/self/fdinfo/{fd}")).map_err(|error| error.to_string())?;
    raw.lines()
        .find_map(|line| line.strip_prefix("mnt_id:")?.trim().parse().ok())
        .ok_or_else(|| "Native mount identity is unavailable".into())
}

pub(crate) fn identity(file: &File) -> JobResult<Identity> {
    let info = stat_fd(file)?;
    Ok(Identity {
        dev: info.st_dev,
        ino: info.st_ino,
        mode: info.st_mode,
        uid: info.st_uid,
        mount: mount_id(file.as_raw_fd())?,
    })
}

pub(crate) fn directory(parent: libc::c_int, path: &OsStr, secure: bool) -> JobResult<File> {
    let file = open_at(
        parent,
        path,
        libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW,
        0,
    )?;
    let info = stat_fd(&file)?;
    if secure && (info.st_uid != unsafe { libc::getuid() } || info.st_mode & 0o022 != 0) {
        return Err("Trash directories must be owned and writable only by this user".into());
    }
    Ok(file)
}

pub(crate) fn observed(parent: &File, path: &OsStr) -> JobResult<(Facts, u64)> {
    let file = open_at(parent.as_raw_fd(), path, libc::O_PATH | libc::O_NOFOLLOW, 0)?;
    Ok((
        Facts::from_stat(&stat_fd(&file)?),
        mount_id(file.as_raw_fd())?,
    ))
}

pub(crate) fn rename_exact(source: &str, source_dir: &File, target: &str, target_dir: &File) -> JobResult<()> {
    let source = name(OsStr::new(source))?;
    let target = name(OsStr::new(target))?;
    if unsafe {
        libc::renameat2(
            source_dir.as_raw_fd(),
            source.as_ptr(),
            target_dir.as_raw_fd(),
            target.as_ptr(),
            libc::RENAME_NOREPLACE,
        )
    } != 0
    {
        return Err(std::io::Error::last_os_error().to_string());
    }
    Ok(())
}

pub(crate) fn mkdir_at(parent: &File, item: &str) -> JobResult<()> {
    let item = name(OsStr::new(item))?;
    if unsafe { libc::mkdirat(parent.as_raw_fd(), item.as_ptr(), 0o700) } != 0 {
        return Err(std::io::Error::last_os_error().to_string());
    }
    Ok(())
}

pub(crate) fn remove_at(parent: &File, item: &str, expected: Facts, directory_entry: bool) -> JobResult<()> {
    let (current, _) = observed(parent, OsStr::new(item))?;
    if directory_entry {
        if (current.dev, current.ino, current.mode) != (expected.dev, expected.ino, expected.mode) {
            return Err("A captured folder changed".into());
        }
    } else if current != expected {
        return Err("A captured item changed".into());
    }
    let item = name(OsStr::new(item))?;
    let flags = if directory_entry {
        libc::AT_REMOVEDIR
    } else {
        0
    };
    if unsafe { libc::unlinkat(parent.as_raw_fd(), item.as_ptr(), flags) } != 0 {
        return Err(std::io::Error::last_os_error().to_string());
    }
    Ok(())
}

pub(crate) fn bounded_names(parent: &File, limit: usize, deadline: Instant) -> JobResult<Vec<String>> {
    let mut names = Vec::new();
    for entry in fs::read_dir(format!("/proc/self/fd/{}", parent.as_raw_fd()))
        .map_err(|error| error.to_string())?
    {
        if names.len() >= limit || Instant::now() > deadline {
            return Err("Trash inspection exceeds its declared limits; nothing was changed".into());
        }
        let value = entry
            .map_err(|error| error.to_string())?
            .file_name()
            .into_string()
            .map_err(|_| "Non-UTF-8 Trash names are unsupported")?;
        names.push(value);
    }
    names.sort();
    Ok(names)
}

fn encoded_size(value: &Value) -> usize {
    python_json(value).len()
}

fn bounded_public(value: Value) -> JobResult<Value> {
    if encoded_size(&value) + PUBLIC_ENVELOPE_BYTES > MAX_PUBLIC_BYTES {
        return Err(
            "Trash result exceeds the supported transport budget; retained journal was not changed"
                .into(),
        );
    }
    Ok(value)
}

fn limits() -> Value {
    json!({"items":MAX_ITEMS,"nodes":MAX_NODES,"bytes":MAX_BYTES,
        "prepareSeconds":PREPARE_SECONDS,"deleteSeconds":DELETE_SECONDS,
        "confirmationSeconds":CONFIRM_SECONDS,"manifestBytes":MAX_MANIFEST_BYTES,
        "resultBytes":MAX_PUBLIC_BYTES,"errorBytes":MAX_ERROR_BYTES})
}

fn token() -> JobResult<String> {
    let mut bytes = [0_u8; 24];
    File::open("/dev/urandom")
        .and_then(|mut file| file.read_exact(&mut bytes))
        .map_err(|error| error.to_string())?;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}

fn token_value(value: &Value) -> JobResult<&str> {
    value
        .as_str()
        .filter(|text| {
            text.len() == 48
                && text
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        })
        .ok_or_else(|| "Invalid Trash confirmation".into())
}

fn identity_component(value: &Value) -> JobResult<u64> {
    if let Some(text) = value.as_str() {
        if text.is_empty()
            || text.len() > 20
            || text.len() > 1 && text.starts_with('0')
            || !text.bytes().all(|byte| byte.is_ascii_digit())
        {
            return Err("Invalid or imprecise file identity".into());
        }
        return text
            .parse()
            .map_err(|_| "Invalid or imprecise file identity".into());
    }
    value
        .as_u64()
        .filter(|value| *value <= 9_007_199_254_740_991)
        .ok_or_else(|| "Invalid or imprecise file identity".into())
}

fn selected_identity(value: &Value) -> JobResult<(u64, u64)> {
    let fields = value
        .as_object()
        .filter(|fields| {
            fields.len() == 2 && fields.contains_key("device") && fields.contains_key("inode")
        })
        .ok_or("Invalid file identity")?;
    Ok((
        identity_component(&fields["device"])?,
        identity_component(&fields["inode"])?,
    ))
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct TreeRow {
    path: Vec<String>,
    facts: Facts,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Captured {
    name: String,
    label: String,
    metadata: String,
    metadata_facts: Facts,
    tree: Vec<TreeRow>,
}

struct Budget {
    nodes: usize,
    bytes: u64,
    manifest_bytes: usize,
    deadline: Instant,
}

fn capture_tree(
    parent: &File,
    item: &str,
    mount: u64,
    budget: &mut Budget,
    prefix: &mut Vec<String>,
    result: &mut Vec<TreeRow>,
) -> JobResult<()> {
    if Instant::now() > budget.deadline {
        return Err("Trash preparation exceeded its time limit; nothing was changed".into());
    }
    let (facts, current_mount) = observed(parent, OsStr::new(item))?;
    if current_mount != mount {
        return Err("Mounted contents cannot be permanently deleted through Trash".into());
    }
    if !(facts.directory() || facts.regular() || facts.symlink()) {
        return Err("Special files cannot be permanently deleted through this action".into());
    }
    budget.nodes += 1;
    if facts.regular() {
        budget.bytes = budget.bytes.saturating_add(facts.size.max(0) as u64);
    }
    budget.manifest_bytes = budget
        .manifest_bytes
        .saturating_add(prefix.iter().map(String::len).sum::<usize>() + 128);
    if budget.nodes > MAX_NODES
        || budget.bytes > MAX_BYTES
        || prefix.len() > MAX_DEPTH
        || budget.manifest_bytes > MAX_MANIFEST_BYTES
    {
        return Err("Trash selection exceeds the declared limits; nothing was changed".into());
    }
    result.push(TreeRow {
        path: prefix.clone(),
        facts,
    });
    if facts.directory() {
        let child = directory(parent.as_raw_fd(), OsStr::new(item), false)?;
        if Facts::from_stat(&stat_fd(&child)?) != facts {
            return Err("A selected Trash folder changed".into());
        }
        for name in bounded_names(
            &child,
            MAX_NODES.saturating_sub(budget.nodes),
            budget.deadline,
        )? {
            prefix.push(name.clone());
            capture_tree(&child, &name, mount, budget, prefix, result)?;
            prefix.pop();
        }
        if Facts::from_stat(&stat_fd(&child)?) != facts {
            return Err("A selected Trash folder changed while it was inspected".into());
        }
    }
    Ok(())
}

fn capture(
    files: &File,
    info: &File,
    item: &str,
    mount: u64,
    budget: &mut Budget,
    expected: Option<(u64, u64)>,
) -> JobResult<Captured> {
    if item.is_empty() || item == "." || item == ".." || item.contains('/') || item.contains('\0') {
        return Err("Invalid Trash entry".into());
    }
    let metadata = format!("{item}.trashinfo");
    let mut descriptor = open_at(
        info.as_raw_fd(),
        OsStr::new(&metadata),
        libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_NONBLOCK,
        0,
    )?;
    let meta = stat_fd(&descriptor)?;
    if meta.st_mode & libc::S_IFMT != libc::S_IFREG
        || meta.st_uid != unsafe { libc::getuid() }
        || meta.st_size > 65536
        || mount_id(descriptor.as_raw_fd())? != mount
    {
        return Err("Unsupported Trash recovery record".into());
    }
    let mut raw = Vec::new();
    Read::by_ref(&mut descriptor)
        .take(65537)
        .read_to_end(&mut raw)
        .map_err(|error| error.to_string())?;
    budget.manifest_bytes = budget.manifest_bytes.saturating_add(raw.len());
    if budget.manifest_bytes > MAX_MANIFEST_BYTES {
        return Err("Trash recovery records exceed the manifest limit".into());
    }
    if raw.len() > 65536 || Facts::from_stat(&stat_fd(&descriptor)?) != Facts::from_stat(&meta) {
        return Err("Trash recovery record changed".into());
    }
    let text = std::str::from_utf8(&raw).map_err(|error| error.to_string())?;
    let mut section = false;
    let mut original = None;
    for line in text.lines() {
        if line.starts_with('[') {
            section = line.trim() == "[Trash Info]";
            continue;
        }
        if section {
            if let Some(value) = line.strip_prefix("Path=") {
                original = Some(unquote_path(value)?);
            }
        }
    }
    let original = original.ok_or("Missing Trash recovery path")?;
    if original.contains('\0') {
        return Err("Invalid Trash recovery path".into());
    }
    let label = Path::new(&original)
        .file_name()
        .and_then(OsStr::to_str)
        .filter(|value| !value.is_empty())
        .unwrap_or(item)
        .to_owned();
    let mut tree = Vec::new();
    capture_tree(files, item, mount, budget, &mut Vec::new(), &mut tree)?;
    if expected.is_some_and(|(dev, ino)| tree[0].facts.dev != dev || tree[0].facts.ino != ino) {
        return Err("The selected Trash item changed".into());
    }
    Ok(Captured {
        name: item.into(),
        label,
        metadata,
        metadata_facts: Facts::from_stat(&meta),
        tree,
    })
}

fn error_fields(error: &str) -> Value {
    let mut value = String::new();
    let mut truncated = false;
    for character in error.chars() {
        value.push(character);
        if encoded_size(&json!(value)) > MAX_ERROR_BYTES {
            value.pop();
            truncated = true;
            break;
        }
    }
    json!({"error":value,"errorTruncated":truncated})
}

fn delete_tree(
    parent: &File,
    item: &str,
    manifest: &[TreeRow],
    stop: &AtomicBool,
    deadline: Instant,
    outcome: &mut Value,
) -> JobResult<()> {
    fn visit(
        parent: &File,
        item: &str,
        path: &mut Vec<String>,
        manifest: &[TreeRow],
        stop: &AtomicBool,
        deadline: Instant,
        outcome: &mut Value,
    ) -> JobResult<()> {
        if stop.load(Ordering::SeqCst) {
            return Err("Deletion stopped; remaining contents were kept".into());
        }
        if Instant::now() > deadline {
            return Err("Deletion reached its time limit; remaining contents were kept".into());
        }
        let expected = manifest
            .iter()
            .find(|row| row.path == *path)
            .ok_or("Missing captured Trash item")?
            .facts;
        if expected.directory() {
            let child = directory(parent.as_raw_fd(), OsStr::new(item), false)?;
            let current = Facts::from_stat(&stat_fd(&child)?);
            if (current.dev, current.ino, current.mode)
                != (expected.dev, expected.ino, expected.mode)
            {
                return Err("A captured folder was replaced".into());
            }
            let mut children = manifest
                .iter()
                .filter(|row| row.path.len() == path.len() + 1 && row.path.starts_with(path))
                .map(|row| row.path.last().unwrap().clone())
                .collect::<Vec<_>>();
            children.sort();
            children.reverse();
            for next in children {
                path.push(next.clone());
                visit(&child, &next, path, manifest, stop, deadline, outcome)?;
                path.pop();
            }
            drop(child);
            remove_at(parent, item, expected, true)?;
        } else {
            remove_at(parent, item, expected, false)?;
        }
        outcome["removedNodes"] = json!(outcome["removedNodes"].as_u64().unwrap_or(0) + 1);
        Ok(())
    }
    visit(
        parent,
        item,
        &mut Vec::new(),
        manifest,
        stop,
        deadline,
        outcome,
    )
}

#[derive(Clone)]
struct Plan {
    items: Vec<Captured>,
    outcomes: Vec<Value>,
    roots: [Identity; 3],
    expires: Instant,
    mode: String,
}

struct Inner {
    root_identity: Option<Identity>,
    recovery_identity: Option<Identity>,
    pending: HashMap<String, Plan>,
    history: Vec<Value>,
    active: Option<String>,
}

pub struct Trash {
    root: PathBuf,
    inner: Mutex<Inner>,
    stop: AtomicBool,
    worker: Mutex<Option<thread::JoinHandle<()>>>,
    #[cfg(test)]
    after_stage: Mutex<Option<Box<dyn Fn() -> JobResult<()> + Send>>>,
}

impl Trash {
    pub fn new() -> JobResult<Self> {
        Self::new_at(trash_root()?)
    }

    fn new_at(root: PathBuf) -> JobResult<Self> {
        if !root.is_absolute() {
            return Err("Personal Trash must be an absolute location".into());
        }
        let root_identity = match directory(libc::AT_FDCWD, root.as_os_str(), true) {
            Ok(root) => Some(identity(&root)?),
            Err(_)
                if fs::symlink_metadata(&root)
                    .is_err_and(|error| error.kind() == std::io::ErrorKind::NotFound) =>
            {
                None
            }
            Err(error) => return Err(error),
        };
        let engine = Self {
            root,
            inner: Mutex::new(Inner {
                root_identity,
                recovery_identity: None,
                pending: HashMap::new(),
                history: Vec::new(),
                active: None,
            }),
            stop: AtomicBool::new(false),
            worker: Mutex::new(None),
            #[cfg(test)]
            after_stage: Mutex::new(None),
        };
        {
            let mut inner = engine.inner.lock().unwrap();
            if let Some(_) = inner.root_identity {
                let root = directory(libc::AT_FDCWD, engine.root.as_os_str(), true)?;
                if fs::symlink_metadata(engine.root.join(".lumen-deletion")).is_ok() {
                    let recovery = engine.recovery(&mut inner, &root, false)?;
                    if fs::symlink_metadata(engine.root.join(".lumen-deletion/journal.json"))
                        .is_ok()
                    {
                        let mut journal = open_at(
                            recovery.as_raw_fd(),
                            OsStr::new("journal.json"),
                            libc::O_RDONLY | libc::O_NOFOLLOW,
                            0,
                        )?;
                        let info = stat_fd(&journal)?;
                        if info.st_mode & libc::S_IFMT != libc::S_IFREG
                            || info.st_uid != unsafe { libc::getuid() }
                            || info.st_mode & 0o077 != 0
                            || info.st_size > MAX_JOURNAL_BYTES as i64
                        {
                            return Err("Unsupported Trash recovery journal".into());
                        }
                        let mut raw = Vec::new();
                        Read::by_ref(&mut journal)
                            .take(MAX_JOURNAL_BYTES as u64 + 1)
                            .read_to_end(&mut raw)
                            .map_err(|error| error.to_string())?;
                        if raw.len() > MAX_JOURNAL_BYTES
                            || Facts::from_stat(&stat_fd(&journal)?) != Facts::from_stat(&info)
                        {
                            return Err("Trash recovery journal changed while it was read".into());
                        }
                        let saved: Value =
                            serde_json::from_slice(&raw).map_err(|error| error.to_string())?;
                        if saved["version"] != 1 || !saved["operations"].is_object() {
                            return Err("Unsupported Trash journal".into());
                        }
                        let mut operations = saved["operations"]
                            .as_object()
                            .unwrap()
                            .values()
                            .cloned()
                            .collect::<Vec<_>>();
                        operations.sort_by(|left, right| {
                            left["started"]
                                .as_f64()
                                .unwrap_or(0.0)
                                .partial_cmp(&right["started"].as_f64().unwrap_or(0.0))
                                .unwrap_or(std::cmp::Ordering::Equal)
                        });
                        for operation in &mut operations {
                            if operation["state"] == "running" {
                                operation["state"] = json!("interrupted");
                                operation["message"] = json!(
                                    "Deletion was interrupted; remaining contents were kept. No destructive action was retried."
                                );
                                if let Some(items) = operation["items"].as_array_mut() {
                                    for item in items {
                                        if ["pending", "staging", "deleting"]
                                            .contains(&item["state"].as_str().unwrap_or(""))
                                        {
                                            item["state"] = json!("interrupted");
                                            item["uncertain"] = json!(true);
                                        }
                                    }
                                }
                            }
                        }
                        inner.history = operations;
                    }
                }
            }
        }
        Ok(engine)
    }

    fn recovery(&self, inner: &mut Inner, root: &File, create: bool) -> JobResult<File> {
        let current = identity(root)?;
        if inner
            .root_identity
            .is_some_and(|previous| previous != current)
        {
            return Err("The personal Trash root changed".into());
        }
        inner.root_identity = Some(current);
        if create {
            let name = name(OsStr::new(".lumen-deletion"))?;
            if unsafe { libc::mkdirat(root.as_raw_fd(), name.as_ptr(), 0o700) } == 0 {
                root.sync_all().map_err(|error| error.to_string())?;
            } else if std::io::Error::last_os_error().raw_os_error() != Some(libc::EEXIST) {
                return Err(std::io::Error::last_os_error().to_string());
            }
        }
        let recovery = directory(root.as_raw_fd(), OsStr::new(".lumen-deletion"), true)?;
        let info = stat_fd(&recovery)?;
        let current = identity(&recovery)?;
        if info.st_mode & 0o077 != 0 || current.mount != inner.root_identity.unwrap().mount {
            return Err("Trash recovery directory must be private and on the same mount".into());
        }
        if inner
            .recovery_identity
            .is_some_and(|previous| previous != current)
        {
            return Err("The Trash recovery directory changed".into());
        }
        inner.recovery_identity = Some(current);
        Ok(recovery)
    }

    fn persist(&self, inner: &mut Inner) -> JobResult<()> {
        let mut operations = serde_json::Map::new();
        for operation in &inner.history {
            let id = operation["id"]
                .as_str()
                .ok_or("Invalid Trash journal operation")?;
            operations.insert(id.into(), operation.clone());
        }
        let saved = json!({"version":1,"operations":operations});
        let raw = serde_json::to_vec(&saved).map_err(|error| error.to_string())?;
        if raw.len() > MAX_JOURNAL_BYTES {
            return Err("Trash operation journal is full; no new deletion can be started".into());
        }
        let root = directory(libc::AT_FDCWD, self.root.as_os_str(), true)?;
        let recovery = self.recovery(inner, &root, true)?;
        if fs::symlink_metadata(self.root.join(".lumen-deletion/journal.json")).is_ok() {
            let journal = open_at(
                recovery.as_raw_fd(),
                OsStr::new("journal.json"),
                libc::O_PATH | libc::O_NOFOLLOW,
                0,
            )?;
            let info = stat_fd(&journal)?;
            if info.st_mode & libc::S_IFMT != libc::S_IFREG
                || info.st_uid != unsafe { libc::getuid() }
                || info.st_mode & 0o077 != 0
            {
                return Err("Unsupported Trash recovery journal".into());
            }
        }
        let temporary = format!(".journal.{}", token()?[..16].to_owned());
        let mut output = open_at(
            recovery.as_raw_fd(),
            OsStr::new(&temporary),
            libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL | libc::O_NOFOLLOW,
            0o600,
        )?;
        let result = (|| -> JobResult<()> {
            output.write_all(&raw).map_err(|error| error.to_string())?;
            output.flush().map_err(|error| error.to_string())?;
            output.sync_all().map_err(|error| error.to_string())?;
            let source = name(OsStr::new(&temporary))?;
            let target = name(OsStr::new("journal.json"))?;
            if unsafe {
                libc::renameat(
                    recovery.as_raw_fd(),
                    source.as_ptr(),
                    recovery.as_raw_fd(),
                    target.as_ptr(),
                )
            } != 0
            {
                return Err(std::io::Error::last_os_error().to_string());
            }
            recovery.sync_all().map_err(|error| error.to_string())?;
            Ok(())
        })();
        let temporary = name(OsStr::new(&temporary))?;
        unsafe {
            libc::unlinkat(recovery.as_raw_fd(), temporary.as_ptr(), 0);
        }
        result
    }

    fn roots(
        &self,
        inner: &mut Inner,
        expected: Option<[Identity; 3]>,
    ) -> JobResult<(File, File, File, [Identity; 3])> {
        let root = directory(libc::AT_FDCWD, self.root.as_os_str(), true)?;
        let files = directory(root.as_raw_fd(), OsStr::new("files"), true)?;
        let info = directory(root.as_raw_fd(), OsStr::new("info"), true)?;
        let values = [identity(&root)?, identity(&files)?, identity(&info)?];
        if let Some(previous) = inner.root_identity {
            if previous != values[0] {
                return Err("The personal Trash root changed".into());
            }
        }
        inner.root_identity = Some(values[0]);
        if values[1].mount != values[0].mount || values[2].mount != values[0].mount {
            return Err("Trash directories on separate mounts are unsupported".into());
        }
        if expected.is_some_and(|expected| expected != values) {
            return Err("The personal Trash location changed".into());
        }
        Ok((root, files, info, values))
    }

    fn status_payload(history: &[Value], busy: bool, operations: Option<Vec<Value>>) -> Value {
        let selected = operations.unwrap_or_else(|| history.last().into_iter().cloned().collect());
        json!({"available":true,"busy":busy,"operations":selected,
            "history":history.iter().rev().take(40).collect::<Vec<_>>().into_iter().rev().map(|entry|
                json!({"id":entry["id"],"state":entry["state"],"count":entry["items"].as_array().map_or(0,Vec::len),
                    "message":entry["message"].as_str().unwrap_or("")})).collect::<Vec<_>>(),
            "limits":limits(),"scope":"Recognized items already in personal Trash only"})
    }

    fn reserve(&self, token: &str, plan: &Plan, history: &[Value]) -> JobResult<()> {
        let mut items = Vec::new();
        for (index, outcome) in plan.outcomes.iter().enumerate() {
            let stage = self
                .root
                .join(".lumen-deletion")
                .join(format!("{token}-{index}"));
            let mut item = outcome.clone();
            item["state"] = json!("interrupted");
            item["removedNodes"] = json!(MAX_NODES);
            item["recoveryPath"] = json!(stage);
            item["uncertain"] = json!(true);
            item["error"] = json!("\\".repeat(MAX_ERROR_BYTES / 2 - 1));
            item["errorTruncated"] = json!(false);
            for (key, part) in [
                ("payloadPath", "data"),
                ("metadataPath", "record.trashinfo"),
            ] {
                let alternate = json!(stage.join(part));
                if encoded_size(&alternate) > encoded_size(&item[key]) {
                    item[key] = alternate;
                }
            }
            items.push(item);
        }
        let operation = json!({"id":token,"mode":plan.mode,"state":"interrupted",
            "started":-1.7976931348623157e308_f64,"items":items,"message":"\u{10ffff}".repeat(1024)});
        let mut candidate = history.to_vec();
        candidate.push(operation.clone());
        while candidate.len() > 40 {
            let index = candidate
                .iter()
                .position(|value| value["id"] != token && value["state"] == "complete")
                .ok_or(
                    "Review retained incomplete Trash operations before starting another deletion",
                )?;
            candidate.remove(index);
        }
        for value in &candidate {
            bounded_public(Self::status_payload(
                &candidate,
                false,
                Some(vec![value.clone()]),
            ))?;
        }
        bounded_public(json!({"operation":operation}))?;
        let mut operations = serde_json::Map::new();
        for operation in &candidate {
            operations.insert(
                operation["id"]
                    .as_str()
                    .ok_or("Invalid Trash journal operation")?
                    .into(),
                operation.clone(),
            );
        }
        if encoded_size(&json!({"version":1,"operations":operations})) > MAX_JOURNAL_BYTES {
            return Err(
                "Trash journal cannot reserve all possible outcomes; nothing was changed".into(),
            );
        }
        Ok(())
    }

    pub fn status(&self) -> JobResult<Value> {
        let inner = self.inner.lock().unwrap();
        bounded_public(Self::status_payload(
            &inner.history,
            inner.active.is_some(),
            None,
        ))
    }

    pub fn cancel(&self, value: &Value) -> JobResult<Value> {
        let token = token_value(value)?;
        self.inner.lock().unwrap().pending.remove(token);
        Ok(json!({"cancelled":true}))
    }

    pub fn inspect(&self, value: &Value) -> JobResult<Value> {
        let token = token_value(value)?;
        let inner = self.inner.lock().unwrap();
        let operation = inner
            .history
            .iter()
            .find(|entry| entry["id"] == token)
            .ok_or("Trash operation is no longer in the retained journal")?;
        bounded_public(json!({"operation":operation}))
    }

    pub fn prepare(&self, mode: &Value, references: Option<&Value>) -> JobResult<Value> {
        let mode = mode
            .as_str()
            .filter(|value| ["empty", "selected"].contains(value))
            .ok_or("Unsupported Trash confirmation action")?;
        if mode == "selected"
            && references
                .and_then(Value::as_array)
                .is_none_or(Vec::is_empty)
        {
            return Err("Select at least one Trash item".into());
        }
        let mut inner = self.inner.lock().unwrap();
        if inner.active.is_some() {
            return Err("Wait for the current deletion or stop it first".into());
        }
        let now = Instant::now();
        inner.pending.retain(|_, plan| plan.expires > now);
        if inner.pending.len() >= 32 {
            return Err("Too many pending Trash confirmations".into());
        }
        let mut budget = Budget {
            nodes: 0,
            bytes: 0,
            manifest_bytes: 0,
            deadline: now + Duration::from_secs(PREPARE_SECONDS),
        };
        let (_, files, info, roots) = self.roots(&mut inner, None)?;
        let selected: Vec<(String, Option<(u64, u64)>)> = if mode == "empty" {
            if references.is_some() {
                return Err("Empty Trash does not accept caller paths".into());
            }
            bounded_names(&files, MAX_ITEMS, budget.deadline)?
                .into_iter()
                .map(|name| (name, None))
                .collect()
        } else {
            let mut result = Vec::new();
            let mut unique = HashSet::new();
            for row in references
                .and_then(Value::as_array)
                .ok_or("Captured Trash identities are required")?
            {
                let fields = row
                    .as_object()
                    .filter(|fields| {
                        fields.len() == 2
                            && fields.contains_key("path")
                            && fields.contains_key("identity")
                    })
                    .ok_or("Captured Trash identities are required")?;
                let path = fields["path"]
                    .as_str()
                    .map(Path::new)
                    .ok_or("Captured Trash identities are required")?;
                if !path.is_absolute() || path.parent() != Some(self.root.join("files").as_path()) {
                    return Err("Permanent deletion is limited to personal Trash items".into());
                }
                let item = path
                    .file_name()
                    .and_then(OsStr::to_str)
                    .ok_or("Invalid Trash entry")?
                    .to_owned();
                if !unique.insert(item.clone()) {
                    return Err("Repeated Trash item".into());
                }
                result.push((item, Some(selected_identity(&fields["identity"])?)));
            }
            result
        };
        if selected.is_empty() {
            return Err("Trash is already empty".into());
        }
        if selected.len() > MAX_ITEMS {
            return Err("Trash selection exceeds the item limit; nothing was changed".into());
        }
        let items = selected
            .into_iter()
            .map(|(name, expected)| {
                capture(&files, &info, &name, roots[0].mount, &mut budget, expected)
            })
            .collect::<JobResult<Vec<_>>>()?;
        let outcomes = items
            .iter()
            .map(|item| {
                json!({"name":item.label,"storedName":item.name,
            "state":"pending","removedNodes":0,"recoveryPath":"",
            "payloadPath":self.root.join("files").join(&item.name),
            "metadataPath":self.root.join("info").join(&item.metadata)})
            })
            .collect::<Vec<_>>();
        let token = token()?;
        let plan = Plan {
            items,
            outcomes,
            roots,
            expires: now + Duration::from_secs(CONFIRM_SECONDS),
            mode: mode.into(),
        };
        self.reserve(&token, &plan, &inner.history)?;
        let result = json!({"token":token,"mode":mode,"count":plan.items.len(),
            "names":plan.items.iter().map(|item| &item.label).collect::<Vec<_>>(),
            "nodes":budget.nodes,"bytes":budget.bytes,
            "consequence":"These items will be permanently deleted. This action cannot be undone.",
            "expiresIn":CONFIRM_SECONDS,"limits":limits()});
        inner.pending.insert(token, plan);
        Ok(result)
    }

    fn update_outcome(&self, token: &str, index: usize, outcome: &Value) -> JobResult<()> {
        let mut inner = self.inner.lock().unwrap();
        let operation = inner
            .history
            .iter_mut()
            .find(|entry| entry["id"] == token)
            .ok_or("Trash operation is no longer in the retained journal")?;
        operation["items"][index] = outcome.clone();
        self.persist(&mut inner)
    }

    pub fn confirm(self: &Arc<Self>, value: &Value, jobs_busy: bool) -> JobResult<Value> {
        let token = token_value(value)?.to_owned();
        let mut inner = self.inner.lock().unwrap();
        if let Some(operation) = inner.history.iter().find(|entry| entry["id"] == token) {
            let status = Self::status_payload(
                &inner.history,
                inner.active.is_some(),
                Some(vec![operation.clone()]),
            );
            return bounded_public(status);
        }
        if inner.active.is_some() {
            return Err("Wait for the current deletion or stop it first".into());
        }
        if jobs_busy {
            return Err("Wait for the current file operation or cancel it first".into());
        }
        let plan = inner
            .pending
            .remove(&token)
            .ok_or("This confirmation expired; review the Trash again")?;
        if plan.expires <= Instant::now() {
            return Err("This confirmation expired; review the Trash again".into());
        }
        let (_root, _files, _info, _) = self.roots(&mut inner, Some(plan.roots))?;
        self.reserve(&token, &plan, &inner.history)?;
        let previous = inner.history.clone();
        inner.history.push(
            json!({"id":token,"mode":plan.mode,"state":"running","started":crate::now()?,
            "items":plan.outcomes,"message":"Permanent deletion started"}),
        );
        while inner.history.len() > 40 {
            let index = inner
                .history
                .iter()
                .position(|entry| entry["id"] != token && entry["state"] == "complete")
                .ok_or(
                    "Review retained incomplete Trash operations before starting another deletion",
                )?;
            inner.history.remove(index);
        }
        let status = match bounded_public(Self::status_payload(&inner.history, true, None)) {
            Ok(status) => status,
            Err(error) => {
                inner.history = previous;
                return Err(error);
            }
        };
        if let Err(error) = self.persist(&mut inner) {
            inner.history = previous;
            return Err(error);
        }
        inner.active = Some(token.clone());
        self.stop.store(false, Ordering::SeqCst);
        drop(inner);
        if let Some(previous) = self.worker.lock().unwrap().take() {
            let _ = previous.join();
        }
        let engine = Arc::clone(self);
        *self.worker.lock().unwrap() = Some(thread::spawn(move || engine.run(&token, plan)));
        Ok(status)
    }

    pub fn stop(&self) -> JobResult<Value> {
        self.stop.store(true, Ordering::SeqCst);
        self.status()
    }

    pub fn busy(&self) -> bool {
        self.inner.lock().unwrap().active.is_some()
    }

    pub fn close(&self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(worker) = self.worker.lock().unwrap().take() {
            let _ = worker.join();
        }
    }

    fn run(&self, token: &str, plan: Plan) {
        let deadline = Instant::now() + Duration::from_secs(DELETE_SECONDS);
        let result = (|| -> JobResult<()> {
            let (root, files, info, recovery) = {
                let mut inner = self.inner.lock().unwrap();
                let (root, files, info, _) = self.roots(&mut inner, Some(plan.roots))?;
                let recovery = self.recovery(&mut inner, &root, false)?;
                (root, files, info, recovery)
            };
            let _root = root;
            for (index, captured) in plan.items.iter().enumerate() {
                let stage_name = format!("{token}-{index}");
                let mut staged_data = false;
                let mut staged_info = false;
                let mut outcome = plan.outcomes[index].clone();
                let item_result = (|| -> JobResult<()> {
                    if self.stop.load(Ordering::SeqCst) {
                        return Err("Deletion stopped; remaining contents were kept".into());
                    }
                    let mut budget = Budget {
                        nodes: 0,
                        bytes: 0,
                        manifest_bytes: 0,
                        deadline,
                    };
                    let current = capture(
                        &files,
                        &info,
                        &captured.name,
                        plan.roots[0].mount,
                        &mut budget,
                        None,
                    )?;
                    if current != *captured {
                        return Err(
                            "Trash contents changed after confirmation; review them again".into(),
                        );
                    }
                    mkdir_at(&recovery, &stage_name)?;
                    let stage = directory(recovery.as_raw_fd(), OsStr::new(&stage_name), true)?;
                    outcome["state"] = json!("staging");
                    outcome["recoveryPath"] =
                        json!(self.root.join(".lumen-deletion").join(&stage_name));
                    self.update_outcome(token, index, &outcome)?;
                    rename_exact(&captured.name, &files, "data", &stage)?;
                    staged_data = true;
                    outcome["payloadPath"] = json!(
                        self.root
                            .join(".lumen-deletion")
                            .join(&stage_name)
                            .join("data")
                    );
                    let (moved, _) = observed(&stage, OsStr::new("data"))?;
                    let original = captured.tree[0].facts;
                    if (moved.dev, moved.ino, moved.mode, moved.size, moved.mtime_ns)
                        != (
                            original.dev,
                            original.ino,
                            original.mode,
                            original.size,
                            original.mtime_ns,
                        )
                    {
                        return Err("The staged Trash object changed".into());
                    }
                    let (meta, _) = observed(&info, OsStr::new(&captured.metadata))?;
                    if meta != captured.metadata_facts {
                        return Err("The recovery record changed before staging".into());
                    }
                    rename_exact(&captured.metadata, &info, "record.trashinfo", &stage)?;
                    staged_info = true;
                    outcome["metadataPath"] = json!(
                        self.root
                            .join(".lumen-deletion")
                            .join(&stage_name)
                            .join("record.trashinfo")
                    );
                    let (moved_meta, _) = observed(&stage, OsStr::new("record.trashinfo"))?;
                    let expected_meta = captured.metadata_facts;
                    if (
                        moved_meta.dev,
                        moved_meta.ino,
                        moved_meta.mode,
                        moved_meta.size,
                        moved_meta.mtime_ns,
                    ) != (
                        expected_meta.dev,
                        expected_meta.ino,
                        expected_meta.mode,
                        expected_meta.size,
                        expected_meta.mtime_ns,
                    ) {
                        return Err("The staged recovery record changed".into());
                    }
                    files.sync_all().map_err(|error| error.to_string())?;
                    info.sync_all().map_err(|error| error.to_string())?;
                    stage.sync_all().map_err(|error| error.to_string())?;
                    #[cfg(test)]
                    if let Some(hook) = self.after_stage.lock().unwrap().as_ref() {
                        hook()?;
                    }
                    let mut manifest = captured.tree.clone();
                    manifest[0].facts = moved;
                    outcome["state"] = json!("deleting");
                    self.update_outcome(token, index, &outcome)?;
                    delete_tree(
                        &stage,
                        "data",
                        &manifest,
                        &self.stop,
                        deadline,
                        &mut outcome,
                    )?;
                    outcome["payloadPath"] = json!("");
                    remove_at(&stage, "record.trashinfo", moved_meta, false)?;
                    outcome["metadataPath"] = json!("");
                    stage.sync_all().map_err(|error| error.to_string())?;
                    drop(stage);
                    let stage = name(OsStr::new(&stage_name))?;
                    if unsafe {
                        libc::unlinkat(recovery.as_raw_fd(), stage.as_ptr(), libc::AT_REMOVEDIR)
                    } != 0
                    {
                        return Err(std::io::Error::last_os_error().to_string());
                    }
                    recovery.sync_all().map_err(|error| error.to_string())?;
                    outcome["state"] = json!("deleted");
                    outcome["recoveryPath"] = json!("");
                    Ok(())
                })();
                if let Err(error) = item_result {
                    outcome["state"] =
                        json!(if outcome["removedNodes"].as_u64().unwrap_or(0) > 0 {
                            "partial"
                        } else {
                            "kept"
                        });
                    let fields = error_fields(&error);
                    outcome["error"] = fields["error"].clone();
                    outcome["errorTruncated"] = fields["errorTruncated"].clone();
                    if staged_data && outcome["removedNodes"] == 0 {
                        let restored = (|| -> JobResult<()> {
                            let stage =
                                directory(recovery.as_raw_fd(), OsStr::new(&stage_name), true)?;
                            rename_exact("data", &stage, &captured.name, &files)?;
                            outcome["payloadPath"] =
                                json!(self.root.join("files").join(&captured.name));
                            if staged_info {
                                rename_exact(
                                    "record.trashinfo",
                                    &stage,
                                    &captured.metadata,
                                    &info,
                                )?;
                                outcome["metadataPath"] =
                                    json!(self.root.join("info").join(&captured.metadata));
                            }
                            drop(stage);
                            let stage = name(OsStr::new(&stage_name))?;
                            if unsafe {
                                libc::unlinkat(
                                    recovery.as_raw_fd(),
                                    stage.as_ptr(),
                                    libc::AT_REMOVEDIR,
                                )
                            } != 0
                            {
                                return Err(std::io::Error::last_os_error().to_string());
                            }
                            files.sync_all().map_err(|error| error.to_string())?;
                            info.sync_all().map_err(|error| error.to_string())?;
                            recovery.sync_all().map_err(|error| error.to_string())?;
                            outcome["recoveryPath"] = json!("");
                            Ok(())
                        })();
                        if let Err(recovery_error) = restored {
                            let original_truncated = outcome["errorTruncated"] == true;
                            let fields = error_fields(&format!(
                                "{}; remaining item kept at recovery location: {recovery_error}",
                                outcome["error"].as_str().unwrap_or("")
                            ));
                            outcome["error"] = fields["error"].clone();
                            outcome["errorTruncated"] =
                                json!(original_truncated || fields["errorTruncated"] == true);
                        }
                    }
                }
                self.update_outcome(token, index, &outcome)?;
            }
            Ok(())
        })();
        let mut inner = self.inner.lock().unwrap();
        if let Some(operation) = inner.history.iter_mut().find(|entry| entry["id"] == token) {
            match result {
                Ok(()) => {
                    let complete = operation["items"]
                        .as_array()
                        .is_some_and(|items| items.iter().all(|item| item["state"] == "deleted"));
                    operation["state"] = json!(if complete { "complete" } else { "partial" });
                    operation["message"] = json!(if complete {
                        "Confirmed items were permanently deleted"
                    } else {
                        "Some items were kept or only partly deleted; inspect each result and recovery location"
                    });
                }
                Err(error) => {
                    operation["state"] = json!("failed");
                    operation["message"] = json!(error.chars().take(1024).collect::<String>());
                    if let Some(items) = operation["items"].as_array_mut() {
                        for item in items {
                            if item["state"] == "pending" {
                                item["state"] = json!("kept");
                                let fields = error_fields(&error);
                                item["error"] = fields["error"].clone();
                                item["errorTruncated"] = fields["errorTruncated"].clone();
                            }
                        }
                    }
                }
            }
        }
        inner.active = None;
        if self.persist(&mut inner).is_err() {
            if let Some(operation) = inner.history.iter_mut().find(|entry| entry["id"] == token) {
                operation["state"] = json!("interrupted");
                operation["message"] = json!(
                    "The operation result could not be saved; inspect Trash and recovery locations before any new confirmation"
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    use std::time::Duration;

    fn root() -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "lumen-trash-plan-{}-{}",
            std::process::id(),
            token().unwrap()
        ));
        fs::create_dir(&path).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
        fs::create_dir(path.join("files")).unwrap();
        fs::create_dir(path.join("info")).unwrap();
        fs::set_permissions(path.join("files"), fs::Permissions::from_mode(0o700)).unwrap();
        fs::set_permissions(path.join("info"), fs::Permissions::from_mode(0o700)).unwrap();
        path
    }

    fn item(root: &Path, item: &str) -> PathBuf {
        let path = root.join("files").join(item);
        fs::write(&path, b"kept").unwrap();
        fs::write(
            root.join("info").join(format!("{item}.trashinfo")),
            format!("[Trash Info]\nPath=/home/test/{item}\n"),
        )
        .unwrap();
        path
    }

    fn finished(engine: &Trash) -> Value {
        let deadline = Instant::now() + Duration::from_secs(5);
        while engine.busy() && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(10));
        }
        assert!(!engine.busy(), "Trash worker did not finish");
        engine.status().unwrap()["operations"][0].clone()
    }

    #[test]
    fn selected_capture_requires_exact_identity_and_preserves_files() {
        let root = root();
        let path = item(&root, "caf\u{e9}");
        let engine = Trash::new_at(root.clone()).unwrap();
        let info = fs::symlink_metadata(&path).unwrap();
        let reference = json!({"path":path,"identity":{"device":info.dev().to_string(),"inode":info.ino().to_string()}});
        let plan = engine
            .prepare(&json!("selected"), Some(&json!([reference])))
            .unwrap();
        assert_eq!(plan["names"], json!(["caf\u{e9}"]));
        assert_eq!(plan["count"], 1);
        assert_eq!(fs::read(&path).unwrap(), b"kept");
        assert!(
            engine.cancel(&plan["token"]).unwrap()["cancelled"]
                .as_bool()
                .unwrap()
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn stale_identity_and_unrecognized_items_fail_without_mutation() {
        let root = root();
        let path = item(&root, "one");
        let engine = Trash::new_at(root.clone()).unwrap();
        let reference = json!({"path":path,"identity":{"device":"1","inode":"1"}});
        assert!(
            engine
                .prepare(&json!("selected"), Some(&json!([reference])))
                .is_err()
        );
        let other = root.join("files/orphan");
        fs::write(&other, b"keep").unwrap();
        assert!(engine.prepare(&json!("empty"), None).is_err());
        assert_eq!(fs::read(&path).unwrap(), b"kept");
        assert_eq!(fs::read(&other).unwrap(), b"keep");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn private_journal_marks_interrupted_work_without_replaying_it() {
        let root = root();
        let path = item(&root, "one");
        let engine = Trash::new_at(root.clone()).unwrap();
        {
            let mut inner = engine.inner.lock().unwrap();
            inner
                .history
                .push(json!({"id":"a".repeat(48),"state":"running","started":1.0,
                "items":[{"state":"deleting"}]}));
            engine.persist(&mut inner).unwrap();
        }
        assert_eq!(
            fs::metadata(root.join(".lumen-deletion"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
        assert_eq!(
            fs::metadata(root.join(".lumen-deletion/journal.json"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        let restored = Trash::new_at(root.clone()).unwrap();
        let state = restored.status().unwrap();
        assert_eq!(state["operations"][0]["state"], "interrupted");
        assert_eq!(state["operations"][0]["items"][0]["uncertain"], true);
        assert_eq!(fs::read(&path).unwrap(), b"kept");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn symlinked_recovery_directory_is_rejected_before_journal_write() {
        let root = root();
        let path = item(&root, "one");
        let outside = root
            .parent()
            .unwrap()
            .join(format!("lumen-trash-outside-{}", token().unwrap()));
        fs::create_dir(&outside).unwrap();
        std::os::unix::fs::symlink(&outside, root.join(".lumen-deletion")).unwrap();
        assert!(Trash::new_at(root.clone()).is_err());
        assert_eq!(fs::read(&path).unwrap(), b"kept");
        assert!(fs::read_dir(&outside).unwrap().next().is_none());
        fs::remove_dir_all(root).unwrap();
        fs::remove_dir(outside).unwrap();
    }

    #[test]
    fn confirmed_item_is_deleted_once_and_restart_does_not_replay() {
        let root = root();
        let path = item(&root, "one");
        let engine = Arc::new(Trash::new_at(root.clone()).unwrap());
        let info = fs::symlink_metadata(&path).unwrap();
        let reference = json!({"path":path,"identity":{"device":info.dev().to_string(),"inode":info.ino().to_string()}});
        let plan = engine
            .prepare(&json!("selected"), Some(&json!([reference])))
            .unwrap();
        assert!(path.exists());
        engine.confirm(&plan["token"], false).unwrap();
        assert_eq!(finished(&engine)["state"], "complete");
        assert!(!path.exists());
        assert!(!root.join("info/one.trashinfo").exists());
        engine.close();
        drop(engine);
        fs::write(&path, b"later arrival").unwrap();
        let restored = Arc::new(Trash::new_at(root.clone()).unwrap());
        let duplicate = restored.confirm(&plan["token"], false).unwrap();
        assert_eq!(duplicate["operations"][0]["state"], "complete");
        assert_eq!(fs::read(&path).unwrap(), b"later arrival");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn changed_record_and_late_descendant_are_kept() {
        let root = root();
        let path = item(&root, "folder");
        fs::remove_file(&path).unwrap();
        fs::create_dir(&path).unwrap();
        fs::write(path.join("child"), b"captured").unwrap();
        let engine = Arc::new(Trash::new_at(root.clone()).unwrap());
        let plan = engine.prepare(&json!("empty"), None).unwrap();
        fs::write(path.join("later"), b"late").unwrap();
        engine.confirm(&plan["token"], false).unwrap();
        assert_eq!(finished(&engine)["state"], "partial");
        assert_eq!(fs::read(path.join("later")).unwrap(), b"late");
        let plan = engine.prepare(&json!("empty"), None).unwrap();
        let record = root.join("info/folder.trashinfo");
        fs::rename(&record, root.join("info/old.trashinfo")).unwrap();
        fs::write(&record, b"replacement").unwrap();
        engine.confirm(&plan["token"], false).unwrap();
        assert_eq!(finished(&engine)["state"], "partial");
        assert_eq!(fs::read(&record).unwrap(), b"replacement");
        assert!(path.exists());
        engine.close();
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn symlink_deletion_never_touches_its_target() {
        let root = root();
        let path = item(&root, "link");
        let outside = root.join("outside");
        fs::write(&outside, b"untouched").unwrap();
        fs::remove_file(&path).unwrap();
        std::os::unix::fs::symlink(&outside, &path).unwrap();
        let engine = Arc::new(Trash::new_at(root.clone()).unwrap());
        let plan = engine.prepare(&json!("empty"), None).unwrap();
        engine.confirm(&plan["token"], false).unwrap();
        assert_eq!(finished(&engine)["state"], "complete");
        assert_eq!(fs::read(&outside).unwrap(), b"untouched");
        assert!(fs::symlink_metadata(&path).is_err());
        engine.close();
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rollback_never_overwrites_a_later_arrival() {
        let root = root();
        let path = item(&root, "one");
        let engine = Arc::new(Trash::new_at(root.clone()).unwrap());
        let late_path = path.clone();
        *engine.after_stage.lock().unwrap() = Some(Box::new(move || {
            fs::write(&late_path, b"later arrival").unwrap();
            Err("synthetic deletion failure".into())
        }));
        let plan = engine.prepare(&json!("empty"), None).unwrap();
        engine.confirm(&plan["token"], false).unwrap();
        let result = finished(&engine);
        assert_eq!(result["state"], "partial");
        assert_eq!(result["items"][0]["state"], "kept");
        assert_eq!(fs::read(&path).unwrap(), b"later arrival");
        let recovery = PathBuf::from(result["items"][0]["recoveryPath"].as_str().unwrap());
        assert_eq!(fs::read(recovery.join("data")).unwrap(), b"kept");
        assert!(recovery.join("record.trashinfo").exists());
        engine.close();
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn root_replacement_blocks_confirmation_without_touching_replacement() {
        let root = root();
        let path = item(&root, "one");
        let engine = Arc::new(Trash::new_at(root.clone()).unwrap());
        let plan = engine.prepare(&json!("empty"), None).unwrap();
        let retired = root.with_file_name(format!("retired-{}", token().unwrap()));
        fs::rename(&root, &retired).unwrap();
        fs::create_dir(&root).unwrap();
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
        fs::create_dir(root.join("files")).unwrap();
        fs::create_dir(root.join("info")).unwrap();
        fs::set_permissions(root.join("files"), fs::Permissions::from_mode(0o700)).unwrap();
        fs::set_permissions(root.join("info"), fs::Permissions::from_mode(0o700)).unwrap();
        fs::write(root.join("files/one"), b"replacement").unwrap();
        assert!(engine.confirm(&plan["token"], false).is_err());
        assert_eq!(fs::read(root.join("files/one")).unwrap(), b"replacement");
        assert_eq!(fs::read(retired.join("files/one")).unwrap(), b"kept");
        assert!(!root.join(".lumen-deletion").exists());
        assert!(!path.exists() || fs::read(&path).unwrap() == b"replacement");
        fs::remove_dir_all(root).unwrap();
        fs::remove_dir_all(retired).unwrap();
    }

    #[test]
    fn recovery_directory_replacement_blocks_confirmation() {
        let root = root();
        let path = item(&root, "one");
        let engine = Arc::new(Trash::new_at(root.clone()).unwrap());
        {
            let mut inner = engine.inner.lock().unwrap();
            engine.persist(&mut inner).unwrap();
        }
        let retired = root.join("retired-recovery");
        fs::rename(root.join(".lumen-deletion"), &retired).unwrap();
        fs::create_dir(root.join(".lumen-deletion")).unwrap();
        fs::set_permissions(
            root.join(".lumen-deletion"),
            fs::Permissions::from_mode(0o700),
        )
        .unwrap();
        let plan = engine.prepare(&json!("empty"), None).unwrap();
        assert!(engine.confirm(&plan["token"], false).is_err());
        assert_eq!(fs::read(&path).unwrap(), b"kept");
        assert!(
            fs::read_dir(root.join(".lumen-deletion"))
                .unwrap()
                .next()
                .is_none()
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn child_arriving_after_staging_remains_in_recovery() {
        let root = root();
        let path = item(&root, "folder");
        fs::remove_file(&path).unwrap();
        fs::create_dir(&path).unwrap();
        fs::write(path.join("child"), b"captured").unwrap();
        let engine = Arc::new(Trash::new_at(root.clone()).unwrap());
        let plan = engine.prepare(&json!("empty"), None).unwrap();
        let stage = root
            .join(".lumen-deletion")
            .join(format!("{}-0", plan["token"].as_str().unwrap()));
        *engine.after_stage.lock().unwrap() = Some(Box::new(move || {
            fs::write(stage.join("data/later"), b"late").unwrap();
            Ok(())
        }));
        engine.confirm(&plan["token"], false).unwrap();
        let result = finished(&engine);
        assert_eq!(result["state"], "partial");
        assert_eq!(result["items"][0]["removedNodes"], 1);
        let recovery = PathBuf::from(result["items"][0]["recoveryPath"].as_str().unwrap());
        assert_eq!(fs::read(recovery.join("data/later")).unwrap(), b"late");
        assert!(recovery.join("record.trashinfo").exists());
        engine.close();
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn public_failure_budget_is_reserved_before_any_deletion() {
        let root = root();
        for index in 0..600 {
            item(&root, &format!("item-{index:04}-{}", "x".repeat(180)));
        }
        let engine = Trash::new_at(root.clone()).unwrap();
        let error = engine.prepare(&json!("empty"), None).unwrap_err();
        assert!(
            error.contains("budget") || error.contains("journal"),
            "{error}"
        );
        assert!(engine.inner.lock().unwrap().pending.is_empty());
        assert!(!root.join(".lumen-deletion").exists());
        assert_eq!(fs::read_dir(root.join("files")).unwrap().count(), 600);
        fs::remove_dir_all(root).unwrap();
    }
}
