//! Window-independent file operations with no overwrite and private recovery history.
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::env;
use std::ffi::CString;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt, symlink};
use std::path::{Path, PathBuf};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

pub type JobResult<T> = Result<T, String>;
pub mod trash;
mod copy_undo;

fn now() -> JobResult<f64> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| error.to_string())?
        .as_secs_f64())
}

fn uuid() -> JobResult<String> {
    Ok(fs::read_to_string("/proc/sys/kernel/random/uuid")
        .map_err(|error| error.to_string())?
        .trim()
        .replace('-', ""))
}

fn python_json(value: &Value) -> String {
    match value {
        Value::String(value) => serde_json::to_string(value)
            .unwrap()
            .chars()
            .map(|ch| {
                if ch.is_ascii() {
                    ch.to_string()
                } else {
                    ch.encode_utf16(&mut [0; 2])
                        .iter()
                        .map(|code| format!("\\u{code:04x}"))
                        .collect::<String>()
                }
            })
            .collect(),
        Value::Array(values) => format!(
            "[{}]",
            values
                .iter()
                .map(python_json)
                .collect::<Vec<_>>()
                .join(", ")
        ),
        Value::Object(values) => format!(
            "{{{}}}",
            values
                .iter()
                .map(|(key, value)| format!("{}: {}", python_json(&json!(key)), python_json(value)))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        _ => value.to_string(),
    }
}

fn request_fingerprint(
    operation: &str,
    paths: &[String],
    destination: Value,
    conflict: &str,
) -> String {
    let payload =
        json!({"operation":operation,"paths":paths,"destination":destination,"conflict":conflict});
    format!("{:x}", Sha256::digest(python_json(&payload).as_bytes()))
}

pub fn local_path(value: &str) -> JobResult<PathBuf> {
    if value.is_empty() || value.contains('\0') {
        return Err("A local path is required".into());
    }
    let path = if let Some(rest) = value.strip_prefix("~/") {
        PathBuf::from(env::var_os("HOME").ok_or("HOME is missing")?).join(rest)
    } else if value == "~" {
        PathBuf::from(env::var_os("HOME").ok_or("HOME is missing")?)
    } else {
        PathBuf::from(value)
    };
    let path = if path.is_absolute() {
        path
    } else {
        env::current_dir()
            .map_err(|error| error.to_string())?
            .join(path)
    };
    let mut normal = PathBuf::from("/");
    for part in path.components() {
        match part {
            std::path::Component::Normal(name) => normal.push(name),
            std::path::Component::ParentDir => {
                normal.pop();
            }
            _ => (),
        }
    }
    Ok(normal)
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Signature {
    dev: u64,
    ino: u64,
    mode: u32,
    size: u64,
    mtime_sec: i64,
    mtime_ns: i64,
}

fn signature(path: &Path) -> JobResult<Signature> {
    let info = fs::symlink_metadata(path).map_err(|error| error.to_string())?;
    Ok(metadata_signature(&info))
}

fn metadata_signature(info: &fs::Metadata) -> Signature {
    Signature {
        dev: info.dev(),
        ino: info.ino(),
        mode: info.mode(),
        size: info.size(),
        mtime_sec: info.mtime(),
        mtime_ns: info.mtime_nsec(),
    }
}

fn snapshot(root: &Path) -> JobResult<BTreeMap<PathBuf, Signature>> {
    snapshot_metadata(root).map(|(signatures, _)| signatures)
}

fn snapshot_metadata(
    root: &Path,
) -> JobResult<(
    BTreeMap<PathBuf, Signature>,
    BTreeMap<PathBuf, fs::Metadata>,
)> {
    let mut result = BTreeMap::new();
    let mut metadata = BTreeMap::new();
    fn visit(
        path: &Path,
        result: &mut BTreeMap<PathBuf, Signature>,
        metadata: &mut BTreeMap<PathBuf, fs::Metadata>,
    ) -> JobResult<()> {
        let info = fs::symlink_metadata(path).map_err(|error| error.to_string())?;
        let state = metadata_signature(&info);
        let directory = state.mode & libc::S_IFMT == libc::S_IFDIR;
        result.insert(path.to_path_buf(), state);
        // Capture times before enumerating: readdir can update directory atime.
        metadata.insert(path.to_path_buf(), info);
        if directory {
            for child in fs::read_dir(path).map_err(|error| error.to_string())? {
                visit(
                    &child.map_err(|error| error.to_string())?.path(),
                    result,
                    metadata,
                )?;
            }
        }
        Ok(())
    }
    visit(root, &mut result, &mut metadata)?;
    Ok((result, metadata))
}

fn move_exact(source: &Path, destination: &Path) -> std::io::Result<()> {
    let source = CString::new(source.as_os_str().as_bytes())?;
    let destination = CString::new(destination.as_os_str().as_bytes())?;
    let result = unsafe {
        libc::renameat2(
            libc::AT_FDCWD,
            source.as_ptr(),
            libc::AT_FDCWD,
            destination.as_ptr(),
            libc::RENAME_NOREPLACE,
        )
    };
    if result != 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    }
}

fn copy_tag_metadata(source: &Path, destination: &Path) -> JobResult<()> {
    let name = CString::new("user.lumen.tags.v1").unwrap();
    let source = CString::new(source.as_os_str().as_bytes()).map_err(|error| error.to_string())?;
    let destination =
        CString::new(destination.as_os_str().as_bytes()).map_err(|error| error.to_string())?;
    let mut raw = [0_u8; 4097];
    let size = unsafe {
        libc::lgetxattr(
            source.as_ptr(),
            name.as_ptr(),
            raw.as_mut_ptr().cast(),
            raw.len(),
        )
    };
    if size < 0 {
        let error = std::io::Error::last_os_error();
        if matches!(
            error.raw_os_error(),
            Some(libc::ENODATA) | Some(libc::ENOTSUP)
        ) {
            return Ok(());
        }
        return Err(error.to_string());
    }
    if size as usize > 4096 {
        return Err("Tag metadata exceeds its size limit".into());
    }
    if unsafe {
        libc::lsetxattr(
            destination.as_ptr(),
            name.as_ptr(),
            raw.as_ptr().cast(),
            size as usize,
            0,
        )
    } != 0
    {
        return Err(format!(
            "Destination cannot preserve colour tags; source data was kept: {}",
            std::io::Error::last_os_error()
        ));
    }
    Ok(())
}

fn sync_directory(path: &Path) -> JobResult<()> {
    let directory = File::open(path).map_err(|error| error.to_string())?;
    directory.sync_all().map_err(|error| error.to_string())
}

fn sync_tree(path: &Path) -> JobResult<()> {
    let info = fs::symlink_metadata(path).map_err(|error| error.to_string())?;
    if info.file_type().is_symlink() {
        return Ok(());
    }
    if info.is_dir() {
        for entry in fs::read_dir(path).map_err(|error| error.to_string())? {
            sync_tree(&entry.map_err(|error| error.to_string())?.path())?;
        }
        sync_directory(path)
    } else {
        OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW)
            .open(path)
            .and_then(|file| file.sync_all())
            .map_err(|error| error.to_string())
    }
}

fn quote_path(value: &str) -> String {
    let mut output = String::new();
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || b"-._~/".contains(&byte) {
            output.push(byte as char);
        } else {
            output.push_str(&format!("%{byte:02X}"));
        }
    }
    output
}

fn unquote_path(value: &str) -> JobResult<String> {
    let bytes = value.as_bytes();
    let mut result = Vec::new();
    let mut offset = 0;
    while offset < bytes.len() {
        if bytes[offset] == b'%' {
            let text = std::str::from_utf8(
                bytes
                    .get(offset + 1..offset + 3)
                    .ok_or("Invalid Trash path")?,
            )
            .map_err(|_| "Invalid Trash path")?;
            result.push(u8::from_str_radix(text, 16).map_err(|_| "Invalid Trash path")?);
            offset += 3;
        } else {
            result.push(bytes[offset]);
            offset += 1;
        }
    }
    String::from_utf8(result).map_err(|_| "Invalid Trash path".into())
}

fn trash_root() -> JobResult<PathBuf> {
    let data = env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env::var_os("HOME").unwrap_or_default()).join(".local/share")
        });
    Ok(data.join("Trash"))
}

fn mount_root(mut path: PathBuf) -> JobResult<PathBuf> {
    loop {
        let Some(parent) = path.parent().map(Path::to_path_buf) else {
            return Ok(path);
        };
        if parent == path
            || fs::metadata(&path)
                .map_err(|error| error.to_string())?
                .dev()
                != fs::metadata(&parent)
                    .map_err(|error| error.to_string())?
                    .dev()
        {
            return Ok(path);
        }
        path = parent;
    }
}

pub fn trash_items() -> JobResult<Vec<Value>> {
    let root = trash_root()?;
    let mut rows = Vec::new();
    let info_dir = root.join("info");
    let entries = match fs::read_dir(&info_dir) {
        Ok(value) => value,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(rows),
        Err(error) => return Err(error.to_string()),
    };
    let mut files = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|part| part == "trashinfo"))
        .collect::<Vec<_>>();
    files.sort();
    for info in files {
        let record = (|| -> JobResult<Value> {
            let raw = fs::read_to_string(&info).map_err(|error| error.to_string())?;
            let mut section = false;
            let mut original = None;
            let mut deleted = String::new();
            for line in raw.lines() {
                if line.starts_with('[') {
                    section = line.trim() == "[Trash Info]";
                    continue;
                }
                if !section {
                    continue;
                }
                if let Some(path) = line.strip_prefix("Path=") {
                    original = Some(unquote_path(path)?);
                }
                if let Some(date) = line.strip_prefix("DeletionDate=") {
                    deleted = date.to_owned();
                }
            }
            let mut original = original.ok_or("Missing Trash path")?;
            if !original.starts_with('/') {
                original = mount_root(fs::canonicalize(&root).map_err(|error| error.to_string())?)?
                    .join(original)
                    .to_string_lossy()
                    .to_string();
            }
            let original = local_path(&original)?;
            let name = info
                .file_name()
                .and_then(|part| part.to_str())
                .ok_or("Invalid Trash record")?
                .strip_suffix(".trashinfo")
                .ok_or("Invalid Trash record")?;
            let stored = root.join("files").join(name);
            let meta = fs::symlink_metadata(&stored).map_err(|error| error.to_string())?;
            let directory = if meta.file_type().is_symlink() {
                fs::metadata(&stored).is_ok_and(|info| info.is_dir())
            } else {
                meta.is_dir()
            };
            Ok(
                json!({"identity":{"device":meta.dev().to_string(),"inode":meta.ino().to_string()},
                "renameFollow":!meta.is_dir() && meta.nlink() == 1,
                "name":original.file_name().unwrap_or_default().to_string_lossy(),
                "path":stored,"uri":format!("file://{}", quote_path(&stored.to_string_lossy())),
                "original":original,"trashInfo":info,"directory":directory,
                "link":meta.file_type().is_symlink(),"size":meta.len(),
                "mtime":meta.mtime() as f64 + meta.mtime_nsec() as f64 / 1e9,
                "kind":"Trashed item","deleted":deleted}),
            )
        })();
        if let Ok(row) = record {
            rows.push(row);
        }
    }
    Ok(rows)
}

fn restore_info(path: &Path) -> JobResult<Option<Value>> {
    Ok(trash_items()?
        .into_iter()
        .find(|row| row["path"].as_str() == path.to_str()))
}

#[derive(Clone)]
struct UndoRecord {
    source: PathBuf,
    destination: PathBuf,
    expected: BTreeMap<PathBuf, Signature>,
}

#[derive(Clone)]
enum UndoEntry { Move(UndoRecord), Copy(copy_undo::Record) }
impl UndoEntry {
    fn kind(&self) -> &str {match self {Self::Move(_)=>"move",Self::Copy(_)=>"copy"}}
    fn label(&self) -> String {match self {Self::Move(_)=>"Undo Last Move".into(),Self::Copy(record)=>record.label()}}
}
struct Transfer { destination: PathBuf, undo: Option<copy_undo::Record>, warning: Option<String> }
struct Inner {
    jobs: Vec<Value>,
    tokens: HashMap<String, Arc<AtomicBool>>,
    undo: Vec<UndoEntry>,
    warning: String,
}

pub struct Jobs {
    inner: Mutex<Inner>,
    journal: Option<PathBuf>,
}

impl Jobs {
    pub fn new(journal: Option<PathBuf>) -> Self {
        let mut jobs = Vec::new();
        let mut warning = String::new();
        if let Some(path) = &journal {
            if path.exists() {
                match fs::read(path).ok().and_then(|bytes| serde_json::from_slice::<Vec<Value>>(&bytes).ok()) {
                Some(rows) => for mut job in rows.into_iter().rev().take(40).collect::<Vec<_>>().into_iter().rev() {
                    if !["id","state","operation","errors","started"].iter().all(|key| job.get(*key).is_some()) { continue; }
                    if ["running","cancelling"].contains(&job["state"].as_str().unwrap_or("")) {
                        job["state"] = json!("interrupted");
                        if let Some(errors) = job["errors"].as_array_mut() {
                            errors.push(json!("Interrupted by a service or system exit. Check source and destination before retrying; no files were automatically changed during recovery."));
                        }
                    }
                    jobs.push(job);
                },
                None => warning = "File-operation history could not be read; existing files were not changed.".into(),
            }
            }
        }
        Self {
            inner: Mutex::new(Inner {
                jobs,
                tokens: HashMap::new(),
                undo: Vec::new(),
                warning,
            }),
            journal,
        }
    }

    fn persist(&self, inner: &Inner) -> JobResult<()> {
        let Some(path) = &self.journal else {
            return Ok(());
        };
        let parent = path.parent().ok_or("Invalid journal path")?;
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        let temp = parent.join(format!(".file-jobs-{}", uuid()?));
        let result = (|| {
            let mut file = OpenOptions::new()
                .create_new(true)
                .write(true)
                .mode(0o600)
                .open(&temp)
                .map_err(|error| error.to_string())?;
            serde_json::to_writer(
                &mut file,
                &inner.jobs[inner.jobs.len().saturating_sub(40)..],
            )
            .map_err(|error| error.to_string())?;
            file.flush().map_err(|error| error.to_string())?;
            file.sync_all().map_err(|error| error.to_string())?;
            fs::rename(&temp, path).map_err(|error| error.to_string())?;
            File::open(parent)
                .and_then(|file| file.sync_all())
                .map_err(|error| error.to_string())
        })();
        if temp.exists() {
            let _ = fs::remove_file(temp);
        }
        result
    }

    pub fn state(&self) -> Value {
        let inner = self.inner.lock().unwrap();
        json!({"jobs":inner.jobs,"canUndo":!inner.undo.is_empty(),"undoKind":inner.undo.last().map(UndoEntry::kind),"undoLabel":inner.undo.last().map(UndoEntry::label).unwrap_or_else(||"Undo Last Move".into()),"historyWarning":inner.warning})
    }

    fn update(&self, id: &str, fields: Value) {
        let mut inner = self.inner.lock().unwrap();
        if let Some(job) = inner.jobs.iter_mut().find(|job| job["id"] == id) {
            if let (Some(target), Some(source)) = (job.as_object_mut(), fields.as_object()) {
                for (key, value) in source {
                    target.insert(key.clone(), value.clone());
                }
            }
        }
        if self.persist(&inner).is_err() {
            inner.warning = "File-operation history could not be saved; check disk space.".into();
        }
    }

    fn recovery(&self,id:&str,path:&Path,message:&str)->JobResult<()> {
        let mut inner=self.inner.lock().unwrap();
        let job=inner.jobs.iter_mut().find(|job|job["id"]==id).ok_or("Copy Undo job is no longer available")?;
        job["recoveryPath"]=json!(path);job["undoMessage"]=json!(message);
        self.persist(&inner).map_err(|error|format!("Unable to save Copy Undo recovery details: {error}"))
    }

    fn cancelled(&self, id: &str) -> bool {
        self.inner
            .lock()
            .unwrap()
            .tokens
            .get(id)
            .is_some_and(|token| token.load(Ordering::SeqCst))
    }

    pub fn cancel(&self, id: &str) -> Value {
        if let Some(token) = self.inner.lock().unwrap().tokens.get(id) {
            token.store(true, Ordering::SeqCst);
        }
        self.state()
    }

    pub fn cancel_all(&self) {
        for token in self.inner.lock().unwrap().tokens.values() {
            token.store(true, Ordering::SeqCst);
        }
    }

    pub fn start(self: &Arc<Self>, mut data: Value) -> JobResult<Value> {
        if !data.is_object() {
            return Err("File operation must be an object".into());
        }
        let request_id = match data.get("requestId") {
            None | Some(Value::Null) => None,
            Some(Value::String(value))
                if (1..=128).contains(&value.len())
                    && value.bytes().all(|byte| (33..=126).contains(&byte)) =>
            {
                Some(value.to_owned())
            }
            _ => {
                return Err(
                    "Request ID must contain 1 to 128 printable characters without spaces".into(),
                );
            }
        };
        let action = data["operation"]
            .as_str()
            .ok_or("Unknown file operation")?
            .to_owned();
        if !["copy", "move", "trash", "restore", "undo"].contains(&action.as_str()) {
            return Err("Unknown file operation".into());
        }
        let mut paths = Vec::new();
        let mut seen = HashSet::new();
        if data.get("paths").is_some() && !data["paths"].is_array() {
            return Err("Selected paths must be a list".into());
        }
        if let Some(items) = data["paths"].as_array() {
            for item in items {
                let path = local_path(item.as_str().ok_or("A local path is required")?)?;
                if seen.insert(path.clone()) {
                    paths.push(path);
                }
            }
        }
        if action != "undo" && paths.is_empty() {
            return Err("Select at least one item".into());
        }
        if paths.len() > 5000 {
            return Err("Select no more than 5,000 items per operation".into());
        }
        paths.retain(|path| {
            !seen.iter().any(|other| {
                other != path
                    && path.starts_with(other)
                    && fs::symlink_metadata(other).is_ok_and(|info| info.is_dir())
            })
        });
        if ["copy", "move"].contains(&action.as_str()) {
            let destination = local_path(
                data["destination"]
                    .as_str()
                    .ok_or("A local path is required")?,
            )?;
            if !destination.is_dir() {
                return Err("The destination is not a folder".into());
            }
        }
        let conflict = if data.get("conflict").is_none() {
            "skip"
        } else {
            data["conflict"]
                .as_str()
                .ok_or("Choose Skip or Keep Both; replacement is not supported")?
        };
        if !["skip", "keep-both"].contains(&conflict) {
            return Err("Choose Skip or Keep Both; replacement is not supported".into());
        }
        let id = uuid()?;
        let paths_json: Vec<String> = paths
            .iter()
            .map(|path| path.to_string_lossy().to_string())
            .collect();
        let fingerprint = request_fingerprint(
            &action,
            &paths_json,
            data.get("destination").cloned().unwrap_or(Value::Null),
            conflict,
        );
        data["paths"] = json!(paths_json);
        let token = Arc::new(AtomicBool::new(false));
        {
            let mut inner = self.inner.lock().unwrap();
            if let Some(request_id) = &request_id {
                if let Some(existing) = inner
                    .jobs
                    .iter()
                    .find(|job| job["requestId"].as_str() == Some(request_id))
                {
                    if existing["requestFingerprint"].as_str() != Some(&fingerprint) {
                        return Err("Request ID was already used for a different operation".into());
                    }
                    let existing_id = existing["id"].clone();
                    drop(inner);
                    let mut state = self.state();
                    state["job"] = existing_id;
                    return Ok(state);
                }
            }
            if inner
                .jobs
                .iter()
                .any(|job| ["running", "cancelling"].contains(&job["state"].as_str().unwrap_or("")))
            {
                return Err("Wait for the current file operation or cancel it first".into());
            }
            inner.tokens.insert(id.clone(), token);
            inner.jobs.push(
                json!({"id":id,"requestId":request_id,"requestFingerprint":fingerprint,
                "operation":action,"state":"running","total":paths.len(),
                "completed":0,"bytes":0,"current":"","errors":[],"selected":"","started":now()?,
                "sources":paths_json,"destination":data["destination"].as_str().unwrap_or("")}),
            );
            if self.persist(&inner).is_err() {
                inner.jobs.pop();
                inner.tokens.remove(&id);
                return Err("Unable to save operation history; no files were changed".into());
            }
        }
        let jobs = Arc::clone(self);
        let run_id = id.clone();
        thread::spawn(move || jobs.run(&run_id, &data));
        let mut state = self.state();
        state["job"] = json!(id);
        Ok(state)
    }

    fn check(&self, id: &str) -> JobResult<()> {
        if self.cancelled(id) {
            Err("Cancelled; uncopied source data was kept".into())
        } else {
            Ok(())
        }
    }

    fn add_bytes(&self, id: &str, count: usize) {
        let mut inner = self.inner.lock().unwrap();
        if let Some(job) = inner.jobs.iter_mut().find(|job| job["id"] == id) {
            job["bytes"] = json!(job["bytes"].as_u64().unwrap_or(0) + count as u64);
        }
    }

    fn copy(
        &self,
        id: &str,
        source: &Path,
        destination: &Path,
        observed: &BTreeMap<PathBuf, fs::Metadata>,
    ) -> JobResult<()> {
        self.check(id)?;
        let info = fs::symlink_metadata(source).map_err(|error| error.to_string())?;
        let original = observed.get(source).ok_or("Source changed while copying")?;
        if metadata_signature(&info) != metadata_signature(original) {
            return Err("Source changed while copying".into());
        }
        if info.is_dir() {
            fs::create_dir(destination).map_err(|error| error.to_string())?;
            fs::set_permissions(destination, fs::Permissions::from_mode(0o700))
                .map_err(|error| error.to_string())?;
            for child in fs::read_dir(source).map_err(|error| error.to_string())? {
                let child = child.map_err(|error| error.to_string())?;
                self.copy(
                    id,
                    &child.path(),
                    &destination.join(child.file_name()),
                    observed,
                )
                .map_err(|error| format!("{}: {error}", child.path().display()))?;
            }
            copy_tag_metadata(source, destination).map_err(|error| format!("tags: {error}"))?;
            fs::set_permissions(
                destination,
                fs::Permissions::from_mode(info.mode() & 0o7777),
            )
            .map_err(|error| format!("mode: {error}"))?;
            preserve_times(destination, original).map_err(|error| format!("times: {error}"))?;
        } else if info.file_type().is_symlink() {
            symlink(
                fs::read_link(source).map_err(|error| error.to_string())?,
                destination,
            )
            .map_err(|error| error.to_string())?;
        } else if info.is_file() {
            let mut input = OpenOptions::new()
                .read(true)
                .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
                .open(source)
                .map_err(|error| error.to_string())?;
            let opened = input.metadata().map_err(|error| error.to_string())?;
            if !opened.is_file() || opened.dev() != info.dev() || opened.ino() != info.ino() {
                return Err("Source changed while copying".into());
            }
            let mut output = OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(destination)
                .map_err(|error| error.to_string())?;
            let mut buffer = vec![0; 1024 * 1024];
            loop {
                self.check(id)?;
                let count = input.read(&mut buffer).map_err(|error| error.to_string())?;
                if count == 0 {
                    break;
                }
                output
                    .write_all(&buffer[..count])
                    .map_err(|error| error.to_string())?;
                self.add_bytes(id, count);
            }
            output.flush().map_err(|error| error.to_string())?;
            output.sync_all().map_err(|error| error.to_string())?;
            copy_tag_metadata(source, destination).map_err(|error| format!("tags: {error}"))?;
            fs::set_permissions(
                destination,
                fs::Permissions::from_mode(info.mode() & 0o7777),
            )
            .map_err(|error| format!("mode: {error}"))?;
            preserve_times(destination, original).map_err(|error| format!("times: {error}"))?;
        } else {
            return Err("Device files, sockets and pipes cannot be copied".into());
        }
        Ok(())
    }

    fn transfer(&self,id:&str,source:&Path,destination:&Path,moving:bool,conflict:&str)->JobResult<PathBuf> {
        self.transfer_captured(id,source,destination,moving,conflict,false).map(|value|value.destination)
    }
    fn transfer_captured(
        &self,
        id: &str,
        source: &Path,
        destination: &Path,
        moving: bool,
        conflict: &str,
        capture_undo: bool,
    ) -> JobResult<Transfer> {
        let mut undo_record=None;
        let mut undo_warning=None;
        let src = fs::canonicalize(source).map_err(|error| error.to_string())?;
        let parent = fs::canonicalize(destination.parent().ok_or("Invalid destination")?)
            .map_err(|error| error.to_string())?;
        let mut dst = parent.join(destination.file_name().ok_or("Invalid destination")?);
        if dst == src && (moving || conflict != "keep-both") {
            return Err("Choose a different destination".into());
        }
        if source.is_dir() && !source.is_symlink() && parent.starts_with(&src) {
            return Err("A folder cannot be placed inside itself".into());
        }
        if fs::symlink_metadata(&dst).is_ok() {
            if conflict != "keep-both" {
                return Err("Skipped: an item with this name already exists".into());
            }
            let original = dst.clone();
            let stem = original
                .file_stem()
                .and_then(|name| name.to_str())
                .ok_or("Invalid name")?;
            let suffix = original
                .extension()
                .and_then(|name| name.to_str())
                .map(|s| format!(".{s}"))
                .unwrap_or_default();
            for index in 2..10002 {
                dst = original.with_file_name(format!("{stem} copy {index}{suffix}"));
                if fs::symlink_metadata(&dst).is_err() {
                    break;
                }
            }
            if fs::symlink_metadata(&dst).is_ok() {
                return Err("No available copy name".into());
            }
        }
        let (before, observed) =
            snapshot_metadata(source).map_err(|error| format!("source snapshot: {error}"))?;
        if moving {
            match move_exact(source, &dst) {
                Ok(()) => {
                    sync_directory(dst.parent().ok_or("Invalid destination")?)?;
                    if source.parent() != dst.parent() {
                        sync_directory(source.parent().ok_or("Invalid source")?)?;
                    }
                    return Ok(Transfer {destination:dst,undo:None,warning:None});
                }
                Err(error) if error.raw_os_error() == Some(libc::EXDEV) => (),
                Err(error) => return Err(error.to_string()),
            }
        }
        let staging = parent.join(format!(".lumen-copy-{}", uuid()?));
        fs::create_dir(&staging).map_err(|error| error.to_string())?;
        let result = (|| {
            let partial = staging.join("item");
            self.copy(id, source, &partial, &observed)
                .map_err(|error| format!("copy: {error}"))?;
            self.check(id)?;
            if before != snapshot(source).map_err(|error| format!("source recheck: {error}"))? {
                return Err(
                    "Source changed during copying; source was kept and partial output discarded"
                        .into(),
                );
            }
            if moving {
                sync_tree(&partial)?;
            }
            if capture_undo {
                match copy_undo::Record::for_publication(&partial,&dst) {
                    Ok(record)=>undo_record=Some(record),
                    Err(error)=>undo_warning=Some(format!("Copy completed; Undo unavailable: {error}")),
                }
            }
            self.check(id)?;
            // Some filesystems refuse to rename a directory whose final mode
            // has no owner write bit. Keep the staging root writable through
            // publication, then restore its observed metadata at the target.
            let published_metadata =
                fs::symlink_metadata(&partial).map_err(|error| error.to_string())?;
            let restore_mode =
                if published_metadata.is_dir() && published_metadata.mode() & 0o200 == 0 {
                    fs::set_permissions(
                        &partial,
                        fs::Permissions::from_mode(published_metadata.mode() & 0o7777 | 0o200),
                    )
                    .map_err(|error| format!("prepare publish: {error}"))?;
                    Some(published_metadata.mode() & 0o7777)
                } else {
                    None
                };
            self.check(id)?;
            move_exact(&partial, &dst).map_err(|error| format!("publish: {error}"))?;
            if let Some(mode) = restore_mode {
                fs::set_permissions(&dst, fs::Permissions::from_mode(mode))
                    .map_err(|error| format!("restore mode: {error}"))?;
                preserve_times(&dst, &published_metadata)
                    .map_err(|error| format!("restore times: {error}"))?;
            }
            sync_directory(&parent).map_err(|error| format!("sync parent: {error}"))?;
            if moving {
                if before != snapshot(source)? {
                    return Err(
                        "Source changed; both source and completed destination were kept".into(),
                    );
                }
                let mut entries: Vec<_> = before.iter().collect();
                entries.sort_by_key(|(path, _)| std::cmp::Reverse(path.components().count()));
                for (path, expected) in entries {
                    let current = signature(path)?;
                    if expected.mode & libc::S_IFMT == libc::S_IFDIR {
                        if (current.dev, current.ino, current.mode)
                            != (expected.dev, expected.ino, expected.mode)
                        {
                            return Err(
                                "Source directory changed; remaining source data was kept".into()
                            );
                        }
                        fs::remove_dir(path).map_err(|error| error.to_string())?;
                    } else {
                        if current != *expected {
                            return Err("Source changed; remaining source data was kept".into());
                        }
                        fs::remove_file(path).map_err(|error| error.to_string())?;
                    }
                }
            }
            if let Some(record)=undo_record.take() {
                match record.published() {Ok(record)=>undo_record=Some(record),Err(error)=>undo_warning=Some(format!("Copy completed; Undo unavailable: {error}"))}
            }
            Ok(Transfer {destination:dst.clone(),undo:undo_record.take(),warning:undo_warning.take()})
        })();
        make_writable(&staging);
        let _ = fs::remove_dir_all(staging);
        result
    }

    fn run(&self, id: &str, data: &Value) {
        let action = data["operation"].as_str().unwrap_or("");
        let mut errors = Vec::new();
        let mut records = Vec::new();
        let result = (|| -> JobResult<()> {
            if action == "undo" {
                let record = self
                    .inner
                    .lock()
                    .unwrap()
                    .undo
                    .last()
                    .cloned()
                    .ok_or("No reversible copy or move is available")?;
                match record {
                    UndoEntry::Move(record)=>{
                        if snapshot(&record.destination)? != record.expected {return Err("The moved item has changed; Undo cannot safely restore it".into());}
                        self.transfer(id,&record.destination,&record.source,true,"skip")?;
                        self.inner.lock().unwrap().undo.pop();
                        self.update(id,json!({"completed":1,"selected":record.source}));
                    }
                    UndoEntry::Copy(record)=>match record.undo(||self.check(id),|path,message|self.recovery(id,path,message)) {
                        copy_undo::Outcome::Complete(path)=>{
                            self.inner.lock().unwrap().undo.pop();
                            self.update(id,json!({"completed":1,"recoveryPath":path,"undoMessage":"Copy undone. The copied item is in Trash; use Put Back to restore it."}));
                        }
                        copy_undo::Outcome::Refused(error)=>{self.update(id,json!({"undoMessage":error}));return Err(error)},
                        copy_undo::Outcome::Preserved{path,error}=>{
                            self.inner.lock().unwrap().undo.pop();
                            self.update(id,json!({"recoveryPath":path,"undoMessage":error}));return Err(error);
                        }
                    }
                }
            } else {
                for value in data["paths"].as_array().ok_or("Missing paths")? {
                    self.check(id)?;
                    let path = local_path(value.as_str().ok_or("Invalid path")?)?;
                    self.update(
                        id,
                        json!({"current":path.file_name().unwrap_or_default().to_string_lossy()}),
                    );
                    let item = (|| -> JobResult<Option<PathBuf>> {
                        let home = PathBuf::from(env::var_os("HOME").unwrap_or_default());
                        if path == Path::new("/") || path == home {
                            return Err("This location cannot be modified".into());
                        }
                        match action {
                            "copy" | "move" => {
                                let destination = local_path(
                                    data["destination"].as_str().ok_or("Missing destination")?,
                                )?
                                .join(path.file_name().ok_or("Invalid source name")?);
                                let transfer = self.transfer_captured(
                                    id,&path,&destination,action=="move",data["conflict"].as_str().unwrap_or("skip"),action=="copy",
                                )?;
                                let target=transfer.destination;
                                if let Some(record)=transfer.undo {records.push(UndoEntry::Copy(record));if records.len()>20 {records.remove(0);}}
                                if let Some(warning)=transfer.warning {self.update(id,json!({"undoWarning":format!("{warning}. Earlier eligible Undo history, if any, remains available by its named item.")}));}
                                if action == "move" {
                                    records.push(UndoEntry::Move(UndoRecord {
                                        source: path.clone(),
                                        expected: snapshot(&target)?,
                                        destination: target.clone(),
                                    }));
                                }
                                Ok(Some(target))
                            }
                            "trash" => {
                                let root = trash_root()?;
                                fs::create_dir_all(root.join("files"))
                                    .map_err(|error| error.to_string())?;
                                fs::create_dir_all(root.join("info"))
                                    .map_err(|error| error.to_string())?;
                                let name = uuid()?;
                                let stored = root.join("files").join(&name);
                                let info = root.join("info").join(format!("{name}.trashinfo"));
                                let mut output = OpenOptions::new()
                                    .write(true)
                                    .create_new(true)
                                    .mode(0o600)
                                    .open(&info)
                                    .map_err(|error| error.to_string())?;
                                let date = unix_date()?;
                                write!(
                                    output,
                                    "[Trash Info]\nPath={}\nDeletionDate={date}\n",
                                    quote_path(&path.to_string_lossy())
                                )
                                .map_err(|error| error.to_string())?;
                                output.flush().map_err(|error| error.to_string())?;
                                output.sync_all().map_err(|error| error.to_string())?;
                                if let Err(error) = self.transfer(id, &path, &stored, true, "skip")
                                {
                                    if fs::symlink_metadata(&stored).is_err() {
                                        let _ = fs::remove_file(info);
                                    }
                                    return Err(error);
                                }
                                Ok(None)
                            }
                            "restore" => {
                                let row = restore_info(&path)?
                                    .ok_or("Trash record is no longer available")?;
                                let destination = local_path(
                                    row["original"]
                                        .as_str()
                                        .ok_or("Invalid Trash destination")?,
                                )?;
                                if !destination.parent().is_some_and(Path::is_dir) {
                                    return Err(
                                        "The original parent folder no longer exists".into()
                                    );
                                }
                                let target = self.transfer(
                                    id,
                                    &path,
                                    &destination,
                                    true,
                                    data["conflict"].as_str().unwrap_or("skip"),
                                )?;
                                fs::remove_file(
                                    row["trashInfo"].as_str().ok_or("Invalid Trash record")?,
                                )
                                .map_err(|error| error.to_string())?;
                                Ok(Some(target))
                            }
                            _ => Err("Unknown file operation".into()),
                        }
                    })();
                    match item {
                        Ok(target) => {
                            let completed = self
                                .inner
                                .lock()
                                .unwrap()
                                .jobs
                                .iter()
                                .find(|job| job["id"] == id)
                                .and_then(|job| job["completed"].as_u64())
                                .unwrap_or(0)
                                + 1;
                            self.update(id, json!({"completed":completed,"selected":target.map(|p| p.to_string_lossy().to_string()).unwrap_or_default()}));
                        }
                        Err(error) if error.starts_with("Cancelled;") => return Err(error),
                        Err(error) => errors.push(format!(
                            "{}: {error}",
                            path.file_name().unwrap_or_default().to_string_lossy()
                        )),
                    }
                }
            }
            Ok(())
        })();
        {
            let mut inner = self.inner.lock().unwrap();
            inner.undo.extend(records);
            let surplus = inner.undo.len().saturating_sub(20);
            if surplus > 0 {
                inner.undo.drain(..surplus);
            }
        }
        match result {
            Ok(()) => self.update(
                id,
                json!({"state":if errors.is_empty() {"complete"} else {"partial"},"errors":errors}),
            ),
            Err(error) => {
                errors.push(error.clone());
                self.update(id, json!({"state":if error.starts_with("Cancelled;") {"cancelled"} else {"failed"},"errors":errors}));
            }
        }
        let mut inner = self.inner.lock().unwrap();
        while inner.jobs.len() > 40 {
            inner.jobs.remove(0);
        }
        inner.tokens.remove(id);
    }
}

fn preserve_times(path: &Path, info: &fs::Metadata) -> JobResult<()> {
    let times = fs::FileTimes::new()
        .set_accessed(info.accessed().map_err(|error| error.to_string())?)
        .set_modified(info.modified().map_err(|error| error.to_string())?);
    File::open(path)
        .map_err(|error| error.to_string())?
        .set_times(times)
        .map_err(|error| error.to_string())
}

fn make_writable(path: &Path) {
    if !path.exists() {
        return;
    }
    let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o700));
    if let Ok(entries) = fs::read_dir(path) {
        for entry in entries.flatten() {
            if entry.file_type().is_ok_and(|kind| kind.is_dir()) {
                make_writable(&entry.path());
            }
        }
    }
}

fn unix_date() -> JobResult<String> {
    let time = now()? as libc::time_t;
    let mut local: libc::tm = unsafe { std::mem::zeroed() };
    if unsafe { libc::localtime_r(&time, &mut local) }.is_null() {
        return Err("Cannot read local time".into());
    }
    Ok(format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}",
        local.tm_year + 1900,
        local.tm_mon + 1,
        local.tm_mday,
        local.tm_hour,
        local.tm_min,
        local.tm_sec
    ))
}

pub fn default_journal() -> JobResult<PathBuf> {
    let base = env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env::var_os("HOME").unwrap_or_default()).join(".local/state")
        });
    Ok(base.join("lumen-desktop/operations/file-jobs.json"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, UNIX_EPOCH};

    #[test]
    fn copy_uses_times_captured_before_source_enumeration() {
        let root = env::temp_dir().join(format!("jobs-observed-times-{}", uuid().unwrap()));
        let source = root.join("source");
        let destination = root.join("copy");
        fs::create_dir_all(&source).unwrap();
        fs::write(source.join("payload"), b"contents").unwrap();
        let old = UNIX_EPOCH + Duration::from_nanos(1_512_345_678_123_456_789);
        for path in [&source, &source.join("payload")] {
            File::open(path)
                .unwrap()
                .set_times(fs::FileTimes::new().set_accessed(old))
                .unwrap();
        }
        let (_, observed) = snapshot_metadata(&source).unwrap();
        // Deterministically model an atime-updating filesystem on every host.
        for path in [&source, &source.join("payload")] {
            File::open(path)
                .unwrap()
                .set_times(
                    fs::FileTimes::new()
                        .set_accessed(UNIX_EPOCH + Duration::from_secs(1_700_000_000)),
                )
                .unwrap();
        }
        Jobs::new(None)
            .copy("test", &source, &destination, &observed)
            .unwrap();
        for path in [&destination, &destination.join("payload")] {
            assert_eq!(fs::metadata(path).unwrap().accessed().unwrap(), old);
        }
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn recursive_copy_preserves_directory_modes_times_and_literal_symlinks() {
        let root = env::temp_dir().join(format!("jobs-metadata-{}", uuid().unwrap()));
        let source = root.join("source");
        let target = root.join("target");
        let outside = root.join("outside");
        fs::create_dir_all(source.join("private")).unwrap();
        fs::create_dir(source.join("public")).unwrap();
        fs::create_dir(&target).unwrap();
        fs::create_dir(&outside).unwrap();
        fs::write(source.join("private/payload"), b"private").unwrap();
        fs::write(outside.join("keep"), b"external").unwrap();
        symlink(&outside, source.join("linked")).unwrap();
        symlink(source.join("absent"), source.join("broken")).unwrap();
        let private = source.join("private");
        let public = source.join("public");
        let directories = [(&source, 0o555), (&private, 0o500), (&public, 0o755)];
        let expected: Vec<_> = directories
            .iter()
            .enumerate()
            .map(|(index, (path, mode))| {
                fs::set_permissions(path, fs::Permissions::from_mode(*mode)).unwrap();
                let atime = 1_512_345_678_123_456_789_u64 + index as u64;
                let mtime = 1_612_345_678_987_654_321_u64 + index as u64;
                File::open(path)
                    .unwrap()
                    .set_times(
                        fs::FileTimes::new()
                            .set_accessed(UNIX_EPOCH + Duration::from_nanos(atime))
                            .set_modified(UNIX_EPOCH + Duration::from_nanos(mtime)),
                    )
                    .unwrap();
                (
                    path.strip_prefix(&source).unwrap().to_path_buf(),
                    *mode,
                    atime,
                    mtime,
                )
            })
            .collect();
        let jobs = Arc::new(Jobs::new(None));
        let copied_job = run(&jobs, "copy", &[&source], &target, "skip");
        assert_eq!(copied_job["state"], "complete", "{copied_job}");
        let copied = target.join("source");
        for (relative, mode, atime, mtime) in expected {
            let info = fs::metadata(copied.join(relative)).unwrap();
            assert_eq!(info.mode() & 0o7777, mode);
            assert_eq!(
                info.atime() as u64 * 1_000_000_000 + info.atime_nsec() as u64,
                atime
            );
            assert_eq!(
                info.mtime() as u64 * 1_000_000_000 + info.mtime_nsec() as u64,
                mtime
            );
        }
        assert_eq!(fs::read_link(copied.join("linked")).unwrap(), outside);
        assert_eq!(
            fs::read_link(copied.join("broken")).unwrap(),
            source.join("absent")
        );
        assert_eq!(fs::read(outside.join("keep")).unwrap(), b"external");
        let collision = run(&jobs, "copy", &[&source], &target, "skip");
        assert_eq!(collision["state"], "partial", "{collision}");
        assert_eq!(
            fs::read(copied.join("private/payload")).unwrap(),
            b"private"
        );
        for directory in [
            source.join("private"),
            source.clone(),
            copied.join("private"),
            copied,
        ] {
            fs::set_permissions(directory, fs::Permissions::from_mode(0o700)).unwrap();
        }
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn request_fingerprint_matches_existing_python_journal_format() {
        assert_eq!(
            request_fingerprint("move", &["/tmp/é".into()], json!("/tmp/a"), "skip"),
            "29240a871840c442e8d940e4bf6600a64d1339661cb8c726e16d30f07b68a260"
        );
    }

    #[test]
    fn duplicate_request_returns_original_receipt_across_restart() {
        let root = env::temp_dir().join(format!("jobs-request-{}", uuid().unwrap()));
        let source = root.join("source");
        let target = root.join("target");
        fs::create_dir_all(&source).unwrap();
        fs::create_dir_all(&target).unwrap();
        let item = source.join("one");
        fs::write(&item, "content").unwrap();
        let journal = root.join("journal.json");
        let jobs = Arc::new(Jobs::new(Some(journal.clone())));
        let request = json!({"operation":"copy","paths":[item],"destination":target,
            "conflict":"keep-both","requestId":"fixture-copy-1"});
        let id = jobs.start(request.clone()).unwrap()["job"]
            .as_str()
            .unwrap()
            .to_owned();
        assert_eq!(wait(&jobs, &id)["state"], "complete");
        assert_eq!(jobs.start(request.clone()).unwrap()["job"], id);
        assert_eq!(fs::read_dir(&target).unwrap().count(), 1);
        let restarted = Arc::new(Jobs::new(Some(journal)));
        assert_eq!(restarted.start(request.clone()).unwrap()["job"], id);
        let mut changed = request;
        changed["conflict"] = json!("skip");
        assert!(
            restarted
                .start(changed)
                .unwrap_err()
                .contains("different operation")
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn copied_files_and_directories_preserve_colour_tags() {
        let root = env::temp_dir().join(format!("jobs-tags-{}", uuid().unwrap()));
        let source = root.join("source");
        let target = root.join("target");
        fs::create_dir_all(&source).unwrap();
        fs::create_dir_all(&target).unwrap();
        let file = source.join("item");
        fs::write(&file, "content").unwrap();
        let name = CString::new("user.lumen.tags.v1").unwrap();
        let value = br#"{"version":1,"tags":["green"]}"#;
        for path in [&source, &file] {
            let path = CString::new(path.as_os_str().as_bytes()).unwrap();
            if unsafe {
                libc::lsetxattr(
                    path.as_ptr(),
                    name.as_ptr(),
                    value.as_ptr().cast(),
                    value.len(),
                    0,
                )
            } != 0
            {
                if std::io::Error::last_os_error().raw_os_error() == Some(libc::ENOTSUP) {
                    fs::remove_dir_all(root).unwrap();
                    return;
                }
                panic!(
                    "Could not create tag fixture: {}",
                    std::io::Error::last_os_error()
                );
            }
        }
        let jobs = Arc::new(Jobs::new(None));
        assert_eq!(
            run(&jobs, "copy", &[&source], &target, "skip")["state"],
            "complete"
        );
        for path in [target.join("source"), target.join("source/item")] {
            let path = CString::new(path.as_os_str().as_bytes()).unwrap();
            let mut read = [0_u8; 128];
            let size = unsafe {
                libc::lgetxattr(
                    path.as_ptr(),
                    name.as_ptr(),
                    read.as_mut_ptr().cast(),
                    read.len(),
                )
            };
            assert_eq!(&read[..size as usize], value);
        }
        fs::remove_dir_all(root).unwrap();
    }

    fn wait(jobs: &Arc<Jobs>, id: &str) -> Value {
        for _ in 0..1000 {
            if let Some(job) = jobs.state()["jobs"]
                .as_array()
                .unwrap()
                .iter()
                .find(|job| job["id"] == id)
            {
                if !["running", "cancelling"].contains(&job["state"].as_str().unwrap_or("")) {
                    return job.clone();
                }
            }
            thread::sleep(Duration::from_millis(10));
        }
        panic!("File job did not finish");
    }

    fn run(
        jobs: &Arc<Jobs>,
        operation: &str,
        paths: &[&Path],
        destination: &Path,
        conflict: &str,
    ) -> Value {
        let request = json!({"operation":operation,"paths":paths.iter().map(|p| p.to_string_lossy().to_string()).collect::<Vec<_>>(),
            "destination":destination,"conflict":conflict});
        let id = jobs.start(request).unwrap()["job"]
            .as_str()
            .unwrap()
            .to_owned();
        wait(jobs, &id)
    }

    #[test]
    fn copies_moves_and_undoes_without_overwrite() {
        let root = env::temp_dir().join(format!("jobs-transfer-{}", uuid().unwrap()));
        let source = root.join("source");
        let target = root.join("target");
        fs::create_dir_all(&source).unwrap();
        fs::create_dir_all(&target).unwrap();
        fs::write(source.join("a"), b"payload").unwrap();
        let private = source.join("private");
        fs::create_dir(&private).unwrap();
        fs::write(private.join("inside"), b"retained").unwrap();
        let stamp = UNIX_EPOCH + Duration::from_nanos(1_612_345_678_987_654_321);
        File::open(&private)
            .unwrap()
            .set_times(fs::FileTimes::new().set_accessed(stamp).set_modified(stamp))
            .unwrap();
        symlink("/nonexistent-job-fixture", source.join("broken")).unwrap();
        let jobs = Arc::new(Jobs::new(None));
        assert_eq!(
            run(&jobs, "copy", &[&source], &target, "skip")["state"],
            "complete"
        );
        assert_eq!(fs::read(target.join("source/a")).unwrap(), b"payload");
        assert!(target.join("source/broken").is_symlink());
        assert_eq!(
            fs::metadata(target.join("source/private"))
                .unwrap()
                .modified()
                .unwrap(),
            stamp
        );
        assert_eq!(
            run(&jobs, "copy", &[&source], &target, "skip")["state"],
            "partial"
        );
        assert_eq!(
            run(&jobs, "copy", &[&source], &target, "keep-both")["state"],
            "complete"
        );
        assert!(target.join("source copy 2").exists());
        let item = source.join("a");
        assert_eq!(
            run(&jobs, "move", &[&item], &target, "skip")["state"],
            "complete"
        );
        assert!(!item.exists());
        assert_eq!(
            run(&jobs, "undo", &[], &target, "skip")["state"],
            "complete"
        );
        assert_eq!(fs::read(&item).unwrap(), b"payload");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn changed_destination_blocks_undo() {
        let root = env::temp_dir().join(format!("jobs-undo-{}", uuid().unwrap()));
        let source = root.join("source");
        let target = root.join("target");
        fs::create_dir_all(&source).unwrap();
        fs::create_dir_all(&target).unwrap();
        let item = source.join("a");
        fs::write(&item, b"first").unwrap();
        let jobs = Arc::new(Jobs::new(None));
        assert_eq!(
            run(&jobs, "move", &[&item], &target, "skip")["state"],
            "complete"
        );
        fs::write(target.join("a"), b"new work").unwrap();
        assert_eq!(run(&jobs, "undo", &[], &target, "skip")["state"], "failed");
        assert_eq!(fs::read(target.join("a")).unwrap(), b"new work");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn partial_batch_can_undo_only_completed_moves() {
        let root = env::temp_dir().join(format!("jobs-batch-{}", uuid().unwrap()));
        let source = root.join("source");
        let target = root.join("target");
        fs::create_dir_all(&source).unwrap();
        fs::create_dir_all(&target).unwrap();
        let first = source.join("a");
        let second = source.join("b");
        fs::write(&first, b"first").unwrap();
        fs::write(&second, b"second").unwrap();
        fs::write(target.join("b"), b"existing").unwrap();
        let jobs = Arc::new(Jobs::new(None));
        let result = run(&jobs, "move", &[&first, &second], &target, "skip");
        assert_eq!(result["state"], "partial");
        assert_eq!(result["completed"], 1);
        assert_eq!(fs::read(&second).unwrap(), b"second");
        assert_eq!(fs::read(target.join("b")).unwrap(), b"existing");
        assert_eq!(
            run(&jobs, "undo", &[], &target, "skip")["state"],
            "complete"
        );
        assert_eq!(fs::read(&first).unwrap(), b"first");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn trash_restore_preserves_collisions_and_long_names() {
        let root = env::temp_dir().join(format!("jobs-trash-{}", uuid().unwrap()));
        let source = root.join("source");
        fs::create_dir_all(&source).unwrap();
        let previous = env::var_os("XDG_DATA_HOME");
        unsafe {
            env::set_var("XDG_DATA_HOME", root.join("data"));
        }
        let jobs = Arc::new(Jobs::new(None));
        let item = source.join("a");
        fs::write(&item, b"recover").unwrap();
        assert_eq!(
            run(&jobs, "trash", &[&item], &source, "skip")["state"],
            "complete"
        );
        let rows = trash_items().unwrap();
        assert!(rows[0]["identity"]["inode"].as_str().is_some());
        assert_eq!(rows[0]["renameFollow"], true);
        let stored = PathBuf::from(rows[0]["path"].as_str().unwrap());
        fs::write(&item, b"new").unwrap();
        assert_eq!(
            run(&jobs, "restore", &[&stored], &source, "skip")["state"],
            "partial"
        );
        assert_eq!(fs::read(&item).unwrap(), b"new");
        assert!(stored.exists());
        fs::remove_file(&item).unwrap();
        assert_eq!(
            run(&jobs, "restore", &[&stored], &source, "skip")["state"],
            "complete"
        );
        assert_eq!(fs::read(&item).unwrap(), b"recover");
        let long = source.join("x".repeat(255));
        fs::write(&long, b"long").unwrap();
        assert_eq!(
            run(&jobs, "trash", &[&long], &source, "skip")["state"],
            "complete"
        );
        let stored = PathBuf::from(trash_items().unwrap()[0]["path"].as_str().unwrap());
        assert_eq!(
            run(&jobs, "restore", &[&stored], &source, "skip")["state"],
            "complete"
        );
        assert_eq!(fs::read(&long).unwrap(), b"long");
        if let Some(value) = previous {
            unsafe {
                env::set_var("XDG_DATA_HOME", value);
            }
        } else {
            unsafe {
                env::remove_var("XDG_DATA_HOME");
            }
        }
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn cross_device_move_can_be_undone() {
        let external = Path::new("/dev/shm");
        if !external.is_dir()
            || fs::metadata(external).unwrap().dev() == fs::metadata(env::temp_dir()).unwrap().dev()
        {
            return;
        }
        let root = env::temp_dir().join(format!("jobs-cross-{}", uuid().unwrap()));
        let target = external.join(format!("jobs-cross-{}", uuid().unwrap()));
        let source = root.join("source");
        fs::create_dir_all(&source).unwrap();
        fs::create_dir(&target).unwrap();
        fs::write(source.join("a"), b"cross device").unwrap();
        let jobs = Arc::new(Jobs::new(None));
        assert_eq!(
            run(&jobs, "move", &[&source], &target, "skip")["state"],
            "complete"
        );
        assert!(!source.exists());
        assert_eq!(fs::read(target.join("source/a")).unwrap(), b"cross device");
        assert_eq!(
            run(&jobs, "undo", &[], &target, "skip")["state"],
            "complete"
        );
        assert_eq!(fs::read(source.join("a")).unwrap(), b"cross device");
        fs::remove_dir_all(root).unwrap();
        fs::remove_dir_all(target).unwrap();
    }

    #[test]
    fn cancellation_token_stops_copy_before_publish() {
        let root = env::temp_dir().join(format!("jobs-cancel-{}", uuid().unwrap()));
        let source = root.join("source");
        let target = root.join("target");
        fs::create_dir_all(&source).unwrap();
        fs::create_dir_all(&target).unwrap();
        fs::write(source.join("a"), b"retained").unwrap();
        let jobs = Jobs::new(None);
        jobs.inner
            .lock()
            .unwrap()
            .tokens
            .insert("cancelled".into(), Arc::new(AtomicBool::new(true)));
        assert!(jobs.check("cancelled").unwrap_err().contains("Cancelled"));
        assert!(
            jobs.transfer("cancelled", &source, &target.join("source"), false, "skip")
                .unwrap_err()
                .contains("Cancelled")
        );
        assert!(source.join("a").exists());
        assert!(fs::read_dir(&target).unwrap().next().is_none());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn special_files_are_rejected_without_partial_output() {
        let root = env::temp_dir().join(format!("jobs-special-{}", uuid().unwrap()));
        let source = root.join("source");
        let target = root.join("target");
        fs::create_dir_all(&source).unwrap();
        fs::create_dir_all(&target).unwrap();
        let pipe = source.join("pipe");
        let path = CString::new(pipe.as_os_str().as_bytes()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(path.as_ptr(), 0o600) }, 0);
        let jobs = Arc::new(Jobs::new(None));
        assert_eq!(
            run(&jobs, "copy", &[&pipe], &target, "skip")["state"],
            "partial"
        );
        assert!(fs::read_dir(&target).unwrap().next().is_none());
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn interrupted_history_does_not_replay() {
        let root = env::temp_dir().join(format!("jobs-history-{}", uuid().unwrap()));
        fs::create_dir(&root).unwrap();
        let journal = root.join("history.json");
        fs::write(
            &journal,
            r#"[{"id":"old","state":"running","operation":"move","errors":[],"started":1}]"#,
        )
        .unwrap();
        let jobs = Jobs::new(Some(journal.clone()));
        assert_eq!(jobs.state()["jobs"][0]["state"], "interrupted");
        assert_eq!(jobs.state()["canUndo"], false);
        fs::write(journal, b"{").unwrap();
        assert_ne!(
            Jobs::new(Some(root.join("history.json"))).state()["historyWarning"],
            ""
        );
        fs::remove_dir_all(root).unwrap();
    }
}
