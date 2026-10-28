//! Installed Rust 2024 Files JSON-lines command for the QML surface.
use chrono::{DateTime, Local, Utc};
use gio::prelude::*;
use lumen_file_surfaces::{
    captured::{Captured, Facts},
    files,
    line_service::LineService,
};
use notify::{RecursiveMode, Watcher as _};
use serde_json::{Value, json};
use std::env;
use std::ffi::CString;
use std::fs;
use std::io::{self, BufRead, Write};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{MetadataExt, symlink};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

fn required<'a>(request: &'a Value, key: &str) -> Result<&'a str, String> {
    request[key]
        .as_str()
        .ok_or_else(|| format!("Missing {key}"))
}

fn identity(number: u32, group: bool) -> String {
    let mut buffer = vec![0_i8; 16_384];
    // SAFETY: libc writes only inside the supplied scratch buffer and result structure.
    unsafe {
        if group {
            let mut value: libc::group = std::mem::zeroed();
            let mut found = std::ptr::null_mut();
            if libc::getgrgid_r(
                number,
                &mut value,
                buffer.as_mut_ptr(),
                buffer.len(),
                &mut found,
            ) == 0
                && !found.is_null()
            {
                return std::ffi::CStr::from_ptr(value.gr_name)
                    .to_string_lossy()
                    .into_owned();
            }
        } else {
            let mut value: libc::passwd = std::mem::zeroed();
            let mut found = std::ptr::null_mut();
            if libc::getpwuid_r(
                number,
                &mut value,
                buffer.as_mut_ptr(),
                buffer.len(),
                &mut found,
            ) == 0
                && !found.is_null()
            {
                return std::ffi::CStr::from_ptr(value.pw_name)
                    .to_string_lossy()
                    .into_owned();
            }
        }
    }
    number.to_string()
}

fn filemode(mode: u32) -> String {
    let kind = match mode & libc::S_IFMT {
        libc::S_IFDIR => 'd',
        libc::S_IFLNK => 'l',
        libc::S_IFCHR => 'c',
        libc::S_IFBLK => 'b',
        libc::S_IFIFO => 'p',
        libc::S_IFSOCK => 's',
        _ => '-',
    };
    let mut text = String::from(kind);
    for (index, (bit, normal)) in [
        (0o400, 'r'),
        (0o200, 'w'),
        (0o100, 'x'),
        (0o040, 'r'),
        (0o020, 'w'),
        (0o010, 'x'),
        (0o004, 'r'),
        (0o002, 'w'),
        (0o001, 'x'),
    ]
    .into_iter()
    .enumerate()
    {
        let mut letter = if mode & bit != 0 { normal } else { '-' };
        if index == 2 && mode & libc::S_ISUID != 0 {
            letter = if letter == 'x' { 's' } else { 'S' };
        }
        if index == 5 && mode & libc::S_ISGID != 0 {
            letter = if letter == 'x' { 's' } else { 'S' };
        }
        if index == 8 && mode & libc::S_ISVTX != 0 {
            letter = if letter == 'x' { 't' } else { 'T' };
        }
        text.push(letter);
    }
    text
}

struct Service {
    tags: Mutex<LineService>,
    rename: Mutex<LineService>,
    actions: Captured,
    watch: Mutex<WatchState>,
    latest_list: Mutex<Value>,
}

type Output = Arc<Mutex<io::Stdout>>;

fn emit(output: &Output, value: &Value) {
    if let Ok(mut writer) = output.lock() {
        let _ = writeln!(writer, "{value}");
        let _ = writer.flush();
    }
}

struct WatchState {
    watcher: Option<notify::RecommendedWatcher>,
    current: Arc<Mutex<Option<PathBuf>>>,
    output: Output,
}

impl WatchState {
    fn new(output: Output) -> Self {
        Self {
            watcher: None,
            current: Arc::new(Mutex::new(None)),
            output,
        }
    }

    fn set(&mut self, path: PathBuf) -> Result<(), String> {
        if self.current.lock().unwrap().as_ref() == Some(&path) {
            return Ok(());
        }
        if self.watcher.is_none() {
            let current = self.current.clone();
            let output = self.output.clone();
            let watcher =
                notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
                    if let Ok(event) = event {
                        if event.kind.is_access() { return; }
                        let selected = current.lock().unwrap().clone();
                        if let Some(selected) = selected {
                            if event.paths.iter().any(|path| {
                                path == &selected || path.parent() == Some(selected.as_path())
                            }) {
                                emit(&output, &json!({"event":"changed","path":selected}));
                            }
                        }
                    }
                })
                .map_err(|_| "Directory changes could not be watched")?;
            self.watcher = Some(watcher);
        }
        let watcher = self.watcher.as_mut().unwrap();
        if let Some(old) = self.current.lock().unwrap().take() {
            let _ = watcher.unwatch(&old);
        }
        watcher
            .watch(&path, RecursiveMode::NonRecursive)
            .map_err(|_| "Directory changes could not be watched")?;
        *self.current.lock().unwrap() = Some(path);
        Ok(())
    }
}

fn next_line(input: &mut impl BufRead) -> io::Result<Option<Option<Vec<u8>>>> {
    let mut bytes = Vec::new();
    let mut oversized = false;
    loop {
        let available = input.fill_buf()?;
        if available.is_empty() {
            return if bytes.is_empty() && !oversized {
                Ok(None)
            } else {
                Ok(Some((!oversized).then_some(bytes)))
            };
        }
        let count = available
            .iter()
            .position(|byte| *byte == b'\n')
            .map_or(available.len(), |position| position + 1);
        let ended = available[count - 1] == b'\n';
        if bytes.len() + count > 65_537 {
            oversized = true;
        }
        if !oversized {
            bytes.extend_from_slice(&available[..count]);
        }
        input.consume(count);
        if ended {
            return Ok(Some((!oversized).then_some(bytes)));
        }
    }
}

impl Service {
    fn new(output: Output) -> Result<Self, String> {
        Ok(Self {
            tags: Mutex::new(LineService::new(
                env::var("LUMEN_FILE_TAGS_BIN").unwrap_or_else(|_| "lumen-file-tags".into()),
            )),
            rename: Mutex::new(LineService::new(
                env::var("LUMEN_BATCH_RENAME_BIN").unwrap_or_else(|_| "lumen-batch-rename".into()),
            )),
            actions: Captured::new().map_err(|error| error.to_string())?,
            watch: Mutex::new(WatchState::new(output)),
            latest_list: Mutex::new(Value::Null),
        })
    }

    fn run_json(
        &self,
        program: &str,
        args: &[&str],
        request: Option<&Value>,
        seconds: u64,
    ) -> Result<Value, String> {
        let mut child = Command::new("timeout")
            .arg(format!("{seconds}s"))
            .arg(program)
            .args(args)
            .stdin(if request.is_some() {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|_| "The file service is unavailable".to_owned())?;
        if let Some(value) = request {
            let mut input = child
                .stdin
                .take()
                .ok_or("The file service is unavailable")?;
            writeln!(input, "{value}").map_err(|_| "The file service stopped")?;
        }
        let output = child
            .wait_with_output()
            .map_err(|_| "The file service stopped")?;
        if !output.status.success() {
            return Err("The file service could not complete this request".into());
        }
        if output.stdout.len() > 8 * 1024 * 1024 {
            return Err("The file service response is too large".into());
        }
        let result: Value = serde_json::from_slice(&output.stdout)
            .map_err(|_| "The file service returned invalid JSON")?;
        if let Some(error) = result["error"].as_str() {
            return Err(error.to_owned());
        }
        Ok(result)
    }

    fn jobs(&self, request: &Value) -> Result<Value, String> {
        let command = env::var("LUMEN_FILE_JOBS_BIN").unwrap_or_else(|_| "lumen-file-jobs".into());
        self.run_json(&command, &["request"], Some(request), 8)
    }

    fn clipboard(&self, request: &Value) -> Result<Value, String> {
        self.run_json("lumen-file-clipboard-client", &[], Some(request), 8)
    }

    fn volumes(&self) -> Value {
        let rows: Vec<Value> = gio::VolumeMonitor::get().mounts().into_iter().filter_map(|mount| {
            let volume = mount.volume()?;
            let device = volume.identifier("unix-device")?;
            let path = mount.root().path()?;
            Some(json!({"name":mount.name().to_string(),"path":path,"device":device.to_string(),"eject":mount.can_unmount()}))
        }).collect();
        json!({"volumes":rows})
    }

    fn eject(&self, path: &Path) -> Result<Value, String> {
        let mount = gio::VolumeMonitor::get()
            .mounts()
            .into_iter()
            .find(|mount| mount.root().path().as_deref() == Some(path))
            .ok_or("This volume cannot be unmounted")?;
        if !mount.can_unmount() {
            return Err("This volume cannot be unmounted".into());
        }
        let device = mount
            .volume()
            .and_then(|volume| volume.identifier("unix-device"))
            .ok_or("This volume cannot be unmounted")?;
        let status = Command::new("timeout")
            .args([
                "20s",
                "udisksctl",
                "unmount",
                "--block-device",
                device.as_str(),
                "--no-user-interaction",
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map_err(|_| "Unable to unmount this volume")?;
        if !status.success() {
            return Err("Unable to unmount this volume".into());
        }
        Ok(json!({"ok":true}))
    }

    fn run_open(&self, path: &Path) -> Result<(), String> {
        let uri = files::uri(path);
        let mut command = Command::new("timeout");
        command.arg("12s");
        if env::var("LUMEN_LAUNCH_SCOPES").as_deref() == Ok("1") {
            command.args(["lumen-launch-app", "--"]);
        }
        let status = command
            .args(["xdg-open", &uri])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map_err(|_| "The application launcher is unavailable")?;
        if !status.success() {
            return Err("The application launcher reported a failure".into());
        }
        Ok(())
    }

    fn run_job(
        &self,
        operation: &str,
        source: &Path,
        destination: Option<&Path>,
    ) -> Result<(), String> {
        let response = self.jobs(&json!({"action":"job-start","operation":operation,"paths":[source],"destination":destination,"conflict":"skip"}))?;
        let id = response["job"].as_str().ok_or("Missing file job ID")?;
        let deadline = Instant::now() + Duration::from_secs(120);
        loop {
            let state = self.jobs(&json!({"action":"job-status"}))?;
            if let Some(job) = state["jobs"]
                .as_array()
                .and_then(|jobs| jobs.iter().find(|job| job["id"] == id))
            {
                match job["state"].as_str().unwrap_or("") {
                    "running" | "cancelling" | "queued" => (),
                    "complete" => {
                        if let Some(folder) = destination {
                            let expected =
                                folder.join(source.file_name().ok_or("Invalid source name")?);
                            if job["selected"].as_str() != expected.to_str() {
                                return Err(
                                    "An item with this name already exists; nothing was replaced"
                                        .into(),
                                );
                            }
                        }
                        return Ok(());
                    }
                    _ => {
                        return Err(job["errors"]
                            .as_array()
                            .and_then(|errors| errors.first())
                            .and_then(Value::as_str)
                            .unwrap_or("File operation failed")
                            .to_owned());
                    }
                }
            }
            if Instant::now() >= deadline {
                return Err(
                    "File operation did not finish in time; check its status before retrying"
                        .into(),
                );
            }
            thread::sleep(Duration::from_millis(50));
        }
    }

    fn info(&self, path: &Path) -> Result<Value, String> {
        let info = fs::symlink_metadata(path).map_err(|error| error.to_string())?;
        let state = self
            .tags
            .lock()
            .unwrap()
            .tag(&json!({"action":"describe-link","path":path,"mode":info.mode()}))?;
        let date = DateTime::<Utc>::from_timestamp(info.mtime(), info.mtime_nsec() as u32)
            .ok_or("Invalid modification time")?
            .with_timezone(&Local)
            .format("%B %d, %Y at %H:%M:%S %Z")
            .to_string();
        let mime = mime_guess::from_path(path)
            .first_raw()
            .unwrap_or("Document");
        let kind = if state["link"] == true {
            "Symbolic link"
        } else if state["directory"] == true {
            "Folder"
        } else {
            mime
        };
        let target = if state["link"] == true {
            fs::read_link(path)
                .map_err(|error| error.to_string())?
                .to_string_lossy()
                .to_string()
        } else {
            String::new()
        };
        Ok(
            json!({"info":{"name":path.file_name().unwrap_or_default().to_string_lossy().to_string(),
            "location":path.parent().unwrap_or(path),"kind":kind,"size":format!("{} bytes",info.len()),
            "modified":date,"permissions":filemode(info.mode()),"owner":identity(info.uid(),false),
            "group":identity(info.gid(),true),"target":target,"linkStatus":state["linkStatus"],"linkDetail":state["linkDetail"]}}),
        )
    }

    fn local_action(&self, request: &Value, action: &str) -> Result<Value, String> {
        let path = files::local_path(required(request, "path")?)?;
        match action {
            "set-tags" => {
                self.tags.lock().unwrap().tag(&json!({"action":"write","path":path,"tags":request["tags"],"identity":request["identity"]}))?;
                Ok(json!({"ok":true,"selected":path}))
            }
            "info" => self.info(&path),
            "make-link" => {
                fs::symlink_metadata(&path).map_err(|error| error.to_string())?;
                for index in 1..=10_001 {
                    let name = format!(
                        "{} link{}",
                        path.file_name().unwrap_or_default().to_string_lossy(),
                        if index == 1 {
                            String::new()
                        } else {
                            format!(" {index}")
                        }
                    );
                    let target = path.with_file_name(name);
                    match symlink(&path, &target) {
                        Ok(()) => return Ok(json!({"ok":true,"selected":target})),
                        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                        Err(error) => return Err(error.to_string()),
                    }
                }
                Err("No available link name".into())
            }
            "open" => {
                let info =
                    fs::symlink_metadata(&path).map_err(|_| "This item is no longer available")?;
                let state = self
                    .tags
                    .lock()
                    .unwrap()
                    .tag(&json!({"action":"describe-link","path":path,"mode":info.mode()}))?;
                if state["linkStatus"] == "broken" {
                    return Err(state["linkError"]
                        .as_str()
                        .unwrap_or("This item is no longer available")
                        .into());
                }
                if state["linkStatus"] == "permission-denied" {
                    return Err(state["linkError"]
                        .as_str()
                        .unwrap_or("Permission denied")
                        .into());
                }
                if let Some(error) = state["linkError"]
                    .as_str()
                    .filter(|error| !error.is_empty())
                {
                    return Err(error.into());
                }
                if !path.exists() {
                    return Err("This item is no longer available".into());
                }
                self.run_open(&path)?;
                Ok(json!({"ok":true}))
            }
            "mkdir" => {
                let target = path.join(files::local_name(required(request, "name")?)?);
                fs::create_dir(&target).map_err(|error| error.to_string())?;
                Ok(json!({"ok":true,"selected":target}))
            }
            "rename" => {
                let target = path.with_file_name(files::local_name(required(request, "name")?)?);
                if target != path {
                    let from = CString::new(path.as_os_str().as_bytes())
                        .map_err(|_| "Invalid source path")?;
                    let to = CString::new(target.as_os_str().as_bytes())
                        .map_err(|_| "Invalid target path")?;
                    // SAFETY: both C strings remain alive for the entire renameat2 call.
                    if unsafe {
                        libc::renameat2(
                            libc::AT_FDCWD,
                            from.as_ptr(),
                            libc::AT_FDCWD,
                            to.as_ptr(),
                            libc::RENAME_NOREPLACE,
                        )
                    } != 0
                    {
                        return Err(io::Error::last_os_error().to_string());
                    }
                }
                Ok(json!({"ok":true,"selected":target}))
            }
            "copy" | "move" => {
                let folder = files::local_path(required(request, "destination")?)?;
                if !folder.is_dir() {
                    return Err("The destination is not a folder".into());
                }
                let target = folder.join(path.file_name().ok_or("Invalid source name")?);
                let resolved_source = fs::canonicalize(&path).map_err(|error| error.to_string())?;
                let resolved_target = fs::canonicalize(&folder)
                    .map_err(|error| error.to_string())?
                    .join(path.file_name().ok_or("Invalid source name")?);
                if target == path
                    || resolved_target == resolved_source
                    || (target.exists()
                        && fs::canonicalize(&target).ok() == Some(resolved_source.clone()))
                {
                    return Err("Choose a different destination folder".into());
                }
                if path.is_dir()
                    && !fs::symlink_metadata(&path)
                        .map_err(|error| error.to_string())?
                        .file_type()
                        .is_symlink()
                    && resolved_target.starts_with(&resolved_source)
                {
                    return Err("A folder cannot be placed inside itself".into());
                }
                if fs::symlink_metadata(&target).is_ok() {
                    return Err(
                        "An item with this name already exists; nothing was replaced".into(),
                    );
                }
                self.run_job(action, &path, Some(&folder))?;
                Ok(json!({"ok":true,"selected":target}))
            }
            "trash" => {
                if path == path.parent().unwrap_or(&path)
                    || path == PathBuf::from(env::var_os("HOME").unwrap_or_default())
                {
                    return Err("This location cannot be moved to Trash".into());
                }
                self.run_job("trash", &path, None)?;
                Ok(json!({"ok":true}))
            }
            _ => Err("Unsupported Files action".into()),
        }
    }

    fn handle(&self, request: &Value) -> Result<Value, String> {
        match request["action"].as_str().unwrap_or("") {
            "preferences" | "save-preferences" => {
                let root = env::var_os("XDG_STATE_HOME")
                    .map(PathBuf::from)
                    .unwrap_or_else(|| {
                        PathBuf::from(env::var_os("HOME").unwrap_or_default()).join(".local/state")
                    });
                files::preferences(request, &root)
            }
            "list" | "column-list" => {
                let result = files::listing(request, |value| self.tags.lock().unwrap().tag(value), || {
                    let command = env::var("LUMEN_FILE_JOBS_BIN").unwrap_or_else(|_| "lumen-file-jobs".into());
                    self.run_json(&command, &["trash-items"], None, 8)
                })?;
                if request["action"] == "list" && *self.latest_list.lock().unwrap()==request["id"] {
                    let path = if result["path"] == "trash:" {
                        env::var_os("XDG_DATA_HOME").map(PathBuf::from)
                            .unwrap_or_else(|| PathBuf::from(env::var_os("HOME").unwrap_or_default()).join(".local/share"))
                            .join("Trash/files")
                    } else { PathBuf::from(result["path"].as_str().ok_or("Invalid listed folder")?) };
                    let _ = self.watch.lock().unwrap().set(path);
                }
                Ok(result)
            }
            "folder-stack-action" => {
                let path = PathBuf::from(required(request, "path")?);
                let expected = Facts::parse(&request["facts"]).map_err(|error| error.to_string())?;
                let key = self.actions.capture(&path, Some(expected), None).map_err(|error| error.to_string())?;
                let result = (|| {
                    let selected = self.actions.selected(&key).map_err(|error| error.to_string())?;
                    if (selected.facts.0[2] as u32 & libc::S_IFMT) == libc::S_IFDIR {
                        Ok(json!({"folder":selected.path,"expectedFacts":selected.facts.wire()}))
                    } else {
                        self.actions.dispatch(&key, "open", None, None).map_err(|error| error.to_string())
                    }
                })();
                self.actions.retire(&[key]);
                result
            }
            "job-start" | "job-status" | "job-cancel" => self.jobs(request),
            "clipboard-read" => self.clipboard(&json!({"action":"read"})),
            "clipboard-write" => self.clipboard(&json!({"action":"write","paths":request["paths"],"operation":request["operation"]})),
            "clipboard-paste" => {
                let mut value = self.clipboard(&json!({"action":"read"}))?;
                value["action"] = json!("job-start");
                value["destination"] = request["destination"].clone();
                value["conflict"] = request.get("conflict").cloned().unwrap_or(json!("skip"));
                value["requestId"] = request["requestId"].clone();
                self.jobs(&value)
            }
            "drop" => {
                let paths = self.clipboard(&json!({"action":"parse-uris","uris":request["uris"]}))?["paths"].clone();
                self.jobs(&json!({"action":"job-start","operation":"copy","paths":paths,"destination":request["destination"],"requestId":request["requestId"]}))
            }
            "preview" => self.run_json("lumen-file-preview", &[request["path"].as_str().ok_or("A local path is required")?, &request["page"].as_u64().unwrap_or(1).to_string()], None, 12),
            "volumes" => Ok(self.volumes()),
            "eject" => self.eject(Path::new(request["path"].as_str().ok_or("A local path is required")?)),
            "retire-actions" => {
                let keys: Vec<String> = request["capabilities"].as_array().into_iter().flatten().filter_map(|value| value.as_str().map(ToOwned::to_owned)).collect();
                self.actions.retire(&keys);
                Ok(json!({"ok":true}))
            }
            "applications" => {
                let path = files::local_path(request["path"].as_str().ok_or("A local path is required")?)?;
                let expected = Facts::parse(&request["facts"]).map_err(|error| error.to_string())?;
                let key = self.actions.capture(&path, Some(expected), None).map_err(|error| error.to_string())?;
                match self.actions.applications(&key) {
                    Ok(mut result) => { result["capability"] = json!(key); result["generation"] = json!(self.actions.generation()); Ok(result) },
                    Err(error) => { self.actions.retire(&[key]); Err(error.to_string()) },
                }
            }
            "open-with" | "set-default" => {
                let key = request["capability"].as_str().ok_or("This captured selection is no longer available")?;
                let result = self.actions.dispatch(key, request["action"].as_str().unwrap_or(""), request["application"].as_str(), request["choice"].as_str())
                    .map_err(|error| error.to_string());
                self.actions.retire(&[key.to_owned()]);
                result
            }
            "rename-preview" | "rename-confirm" | "rename-cancel" => {
                let mut value = request.clone();
                value["id"] = json!(1);
                let mut result = self.rename.lock().unwrap().request(&value)?;
                if let Some(error) = result["error"].as_str() { return Err(error.to_owned()); }
                if let Some(map) = result.as_object_mut() { map.remove("id"); map.remove("action"); }
                Ok(result)
            }
            action => self.local_action(request, action),
        }
    }
}

fn main() {
    let output: Output = Arc::new(Mutex::new(io::stdout()));
    let service = match Service::new(output.clone()) {
        Ok(service) => Arc::new(service),
        Err(error) => {
            emit(&output, &json!({"error":error}));
            return;
        }
    };
    let mut input = io::stdin().lock();
    let mut workers: Vec<std::thread::JoinHandle<()>> = Vec::new();
    while let Ok(Some(line)) = next_line(&mut input) {
        let request = match line {
            Some(line) => match serde_json::from_slice::<Value>(&line) {
                Ok(request) if request.is_object() => Some(request),
                _ => None,
            },
            _ => None,
        };
        let Some(request) = request else {
            emit(&output, &json!({"error":"Invalid request"}));
            continue;
        };
        let mut index = 0;
        while index < workers.len() {
            if workers[index].is_finished() {
                let _ = workers.remove(index).join();
            } else {
                index += 1;
            }
        }
        if workers.len() >= 4 {
            emit(
                &output,
                &json!({"id":request["id"],"action":request["action"],"error":"The Files service is busy"}),
            );
            continue;
        }
        if request["action"] == "list" {
            *service.latest_list.lock().unwrap() = request["id"].clone();
        }
        let output = output.clone();
        let service = service.clone();
        workers.push(thread::spawn(move || {
            let mut result = service
                .handle(&request)
                .unwrap_or_else(|error| json!({"error":error}));
            if let Some(object) = result.as_object_mut() {
                object.insert("id".into(), request["id"].clone());
                object.insert("action".into(), request["action"].clone());
            }
            emit(&output, &result);
        }));
    }
    for worker in workers {
        let _ = worker.join();
    }
}
