//! Opt-in, ephemeral clipboard history with private persistent consent.
use serde_json::{Value, json};
use std::collections::HashSet;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::os::fd::AsRawFd;
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

pub type Result<T> = std::result::Result<T, String>;
pub const MAX_BYTES: usize = 256 * 1024;
pub const WARNING: &str = "Copied text may contain secrets. Marked sensitive offers are excluded, but unmarked secrets cannot always be detected. History is local, expires automatically and is cleared at logout.";

pub fn boottime() -> f64 {
    let mut stamp = libc::timespec { tv_sec: 0, tv_nsec: 0 };
    if unsafe { libc::clock_gettime(libc::CLOCK_BOOTTIME, &mut stamp) } != 0 { return 0.0; }
    let mut value = stamp.tv_sec as f64 + stamp.tv_nsec as f64 / 1_000_000_000.0;
    if let Some(path) = std::env::var_os("LUMEN_CLIPBOARD_TEST_CLOCK_OFFSET") {
        if let Ok(offset) = fs::read_to_string(path) { value += offset.trim().parse::<f64>().unwrap_or(0.0); }
    }
    value
}

pub fn walltime() -> f64 { SystemTime::now().duration_since(UNIX_EPOCH).map(|duration| duration.as_secs_f64()).unwrap_or(0.0) }

fn private_directory(path: &Path) -> Result<()> {
    fs::DirBuilder::new().recursive(true).mode(0o700).create(path).map_err(|error| error.to_string())?;
    let info = fs::symlink_metadata(path).map_err(|error| error.to_string())?;
    if !info.is_dir() || info.uid() != unsafe { libc::getuid() } { return Err("History directory is not a private owned directory".into()); }
    fs::set_permissions(path, fs::Permissions::from_mode(0o700)).map_err(|error| error.to_string())?;
    Ok(())
}

fn file(path: &Path, max: u64) -> Result<Option<File>> {
    let opened = OpenOptions::new().read(true).custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC).open(path);
    let opened = match opened { Ok(file) => file, Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None), Err(error) => return Err(error.to_string()) };
    let info = opened.metadata().map_err(|error| error.to_string())?;
    if !info.is_file() || info.uid() != unsafe { libc::getuid() } || info.mode() & 0o077 != 0 || info.len() > max { return Err("Invalid private history storage".into()); }
    Ok(Some(opened))
}

fn read_json(path: &Path, max: u64) -> Result<Option<Value>> {
    let Some(file) = file(path, max)? else { return Ok(None); };
    let mut bytes = Vec::new();
    file.take(max + 1).read_to_end(&mut bytes).map_err(|error| error.to_string())?;
    if bytes.len() as u64 > max { return Err("Invalid private history storage".into()); }
    serde_json::from_slice(&bytes).map(Some).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    fn fixture() -> (PathBuf, History, Arc<AtomicU64>) {
        let root = std::env::temp_dir().join(format!("lumen-clipboard-test-{}-{}",std::process::id(),random_id().unwrap()));
        fs::create_dir(&root).unwrap();
        let clock = Arc::new(AtomicU64::new(100));
        let current = clock.clone();
        let history = History::new(root.join("runtime"),root.join("state"),2,60,Arc::new(move || current.load(Ordering::SeqCst) as f64),Arc::new(|| 123.0)).unwrap();
        (root, history, clock)
    }

    #[test]
    fn consent_expiry_clear_and_epoch_fence() {
        let (root, history, clock) = fixture();
        assert_eq!(history.ingest(Some("before consent"),false,None,None).unwrap()["recorded"],false);
        history.set_enabled(true).unwrap();
        let epoch = history.barrier().unwrap()["epoch"].as_str().unwrap().to_owned();
        assert_eq!(history.ingest(Some("sensitive"),true,None,None).unwrap()["recorded"],false);
        assert_eq!(history.ingest(Some("one"),false,Some(100.0),Some(&epoch)).unwrap()["recorded"],true);
        assert_eq!(history.ingest(Some("one"),false,None,None).unwrap()["recorded"],false);
        assert_eq!(history.list("").unwrap()["entries"].as_array().unwrap().len(),1);
        clock.store(101,Ordering::SeqCst);
        history.clear().unwrap();
        assert_eq!(history.ingest(Some("old"),false,Some(100.0),None).unwrap()["recorded"],false);
        assert_eq!(history.ingest(Some("old observer"),false,Some(101.0),Some(&epoch)).unwrap()["recorded"],false);
        assert_eq!(history.ingest(Some("new"),false,Some(101.0),None).unwrap()["recorded"],true);
        clock.store(162,Ordering::SeqCst);
        assert!(history.list("").unwrap()["entries"].as_array().unwrap().is_empty());
        history.prune().unwrap();
        assert_eq!(serde_json::from_slice::<Value>(&fs::read(&history.data).unwrap()).unwrap(),json!([]));
        history.set_enabled(false).unwrap();
        assert!(!history.consent.exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn bounded_copy_and_private_storage() {
        let (root, history, _clock) = fixture();
        history.set_enabled(true).unwrap();
        for text in ["one", "two", "three"] { history.ingest(Some(text),false,None,None).unwrap(); }
        assert_eq!(history.list("").unwrap()["entries"].as_array().unwrap().len(),2);
        assert_eq!(history.list("THR").unwrap()["entries"][0]["text"],"three");
        let id = history.list("").unwrap()["entries"][0]["id"].as_str().unwrap().to_owned();
        let mut copied = String::new();
        history.copy(&id, |text| { copied = text.to_owned(); Ok(()) }).unwrap();
        assert_eq!(copied,"three");
        assert_eq!(fs::metadata(&history.data).unwrap().mode() & 0o777,0o600);
        assert_eq!(fs::metadata(&history.runtime).unwrap().mode() & 0o777,0o700);
        assert_eq!(fs::read_dir(&history.state).unwrap().count(),1);
        assert_eq!(history.ingest(Some(&"x".repeat(MAX_BYTES+1)),false,None,None).unwrap()["recorded"],false);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn invalid_and_linked_storage_fail_closed() {
        let (root, history, _clock) = fixture();
        history.set_enabled(true).unwrap();
        history.ingest(Some("original"),false,None,None).unwrap();
        fs::write(&history.data,b"[{}]").unwrap();
        assert!(history.list("").is_err());
        fs::remove_file(&history.data).unwrap();
        let outside = root.join("outside");
        fs::write(&outside,b"unrelated").unwrap();
        std::os::unix::fs::symlink(&outside,&history.data).unwrap();
        assert!(history.list("").is_err());
        assert_eq!(fs::read(&outside).unwrap(),b"unrelated");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn malformed_consent_and_stale_copy_fail_closed() {
        let (root, history, clock) = fixture();
        for contents in ["", "{}", "{\"version\":1,\"enabled\":false}", "{\"version\":2,\"enabled\":true}", "not json"] {
            fs::write(&history.consent, contents).unwrap();
            assert!(!history.enabled());
            assert_eq!(history.ingest(Some("private"), false, None, None).unwrap()["recorded"], false);
        }
        fs::remove_file(&history.consent).unwrap();
        let outside = root.join("unrelated");
        fs::write(&outside, b"{\"version\":1,\"enabled\":true}").unwrap();
        std::os::unix::fs::symlink(&outside, &history.consent).unwrap();
        assert!(!history.enabled());
        fs::remove_file(&history.consent).unwrap();
        history.set_enabled(true).unwrap();
        history.ingest(Some("item"), false, None, None).unwrap();
        let id = history.list("").unwrap()["entries"][0]["id"].as_str().unwrap().to_owned();
        clock.store(161, Ordering::SeqCst);
        assert!(history.copy(&id, |_| panic!("expired item copied")).is_err());
        fs::remove_dir_all(root).unwrap();
    }
}

fn random_id() -> Result<String> {
    Ok(fs::read_to_string("/proc/sys/kernel/random/uuid").map_err(|error| error.to_string())?.trim().replace('-', ""))
}

fn atomic(path: &Path, value: &Value) -> Result<()> {
    let temporary = path.parent().ok_or("Invalid history path")?.join(format!(".pending-{}-{}", std::process::id(), random_id()?));
    let result = (|| -> Result<()> {
        let mut output = OpenOptions::new().write(true).create_new(true).mode(0o600).open(&temporary).map_err(|error| error.to_string())?;
        serde_json::to_writer(&mut output, value).map_err(|error| error.to_string())?;
        output.flush().map_err(|error| error.to_string())?;
        output.sync_all().map_err(|error| error.to_string())?;
        fs::rename(&temporary, path).map_err(|error| error.to_string())?;
        Ok(())
    })();
    let _ = fs::remove_file(&temporary);
    result
}

pub fn process_identity(pid: u32) -> Option<Value> {
    if pid == 0 { return None; }
    let root = PathBuf::from(format!("/proc/{pid}"));
    if fs::metadata(&root).ok()?.uid() != unsafe { libc::getuid() } { return None; }
    let info = fs::read_to_string(root.join("stat")).ok()?;
    let fields = info.rsplit_once(") ")?.1.split_whitespace().collect::<Vec<_>>();
    if fields.first() == Some(&"Z") { return None; }
    Some(json!({"pid":pid,"start":fields.get(19)?.parse::<u64>().ok()?}))
}

pub struct History {
    pub runtime: PathBuf,
    pub state: PathBuf,
    pub data: PathBuf,
    pub consent: PathBuf,
    pub limit: usize,
    pub retention: u64,
    now: Arc<dyn Fn() -> f64 + Send + Sync>,
    wall: Arc<dyn Fn() -> f64 + Send + Sync>,
}

impl History {
    pub fn new(runtime: PathBuf, state: PathBuf, limit: usize, retention: u64,
        now: Arc<dyn Fn() -> f64 + Send + Sync>, wall: Arc<dyn Fn() -> f64 + Send + Sync>) -> Result<Self> {
        if !(1..=1000).contains(&limit) || !(60..=86400).contains(&retention) { return Err("Invalid history policy".into()); }
        private_directory(&runtime)?;
        private_directory(&state)?;
        Ok(Self { data:runtime.join("entries.json"), consent:state.join("consent"), runtime, state, limit, retention, now, wall })
    }

    pub fn from_env() -> Result<Self> {
        let runtime = PathBuf::from(std::env::var_os("XDG_RUNTIME_DIR").ok_or("A private user runtime directory is required")?);
        let info = fs::symlink_metadata(&runtime).map_err(|error| error.to_string())?;
        if !info.is_dir() || info.uid() != unsafe { libc::getuid() } || info.mode() & 0o077 != 0 { return Err("User runtime root must be private and owned".into()); }
        let state = std::env::var_os("XDG_STATE_HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".local/state"));
        let limit = std::env::var("LUMEN_CLIPBOARD_MAX_ITEMS").unwrap_or_else(|_| "100".into()).parse().map_err(|_| "Invalid history policy")?;
        let retention = std::env::var("LUMEN_CLIPBOARD_RETENTION_SECONDS").unwrap_or_else(|_| "3600".into()).parse().map_err(|_| "Invalid history policy")?;
        Self::new(runtime.join("lumen-desktop/clipboard"),state.join("lumen-desktop/clipboard"),limit,retention,Arc::new(boottime),Arc::new(walltime))
    }

    fn locked<T>(&self, work: impl FnOnce() -> Result<T>) -> Result<T> {
        let lock = OpenOptions::new().read(true).write(true).create(true).mode(0o600).custom_flags(libc::O_NOFOLLOW).open(self.runtime.join("lock")).map_err(|error| error.to_string())?;
        if lock.metadata().map_err(|error| error.to_string())?.uid() != unsafe { libc::getuid() } { return Err("Invalid lock owner".into()); }
        if unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX) } != 0 { return Err(std::io::Error::last_os_error().to_string()); }
        work()
    }

    pub fn enabled(&self) -> bool {
        let Some(value) = read_json(&self.consent, 1024).ok().flatten() else { return false; };
        value.as_object().is_some_and(|object| object.len() == 2 && object["version"].as_u64() == Some(1) && object["enabled"] == true)
    }

    pub fn entries(&self, include_expired: bool) -> Result<Vec<Value>> {
        let max = ((MAX_BYTES * 6 + 1024) * self.limit) as u64;
        let Some(value) = read_json(&self.data, max)? else { return Ok(Vec::new()); };
        let rows = value.as_array().filter(|rows| rows.len() <= self.limit).ok_or("Invalid history data")?;
        let mut ids = HashSet::new();
        for row in rows {
            let object = row.as_object().filter(|row| row.len() == 4 && ["id","text","timestamp","created_monotonic"].iter().all(|key| row.contains_key(*key))).ok_or("Invalid history row")?;
            let id = object["id"].as_str().ok_or("Invalid history identity")?;
            if id.len() != 32 || !id.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)) || !ids.insert(id) { return Err("Invalid history identity".into()); }
            let text = object["text"].as_str().ok_or("Invalid history text")?;
            if text.is_empty() || text.contains('\0') || text.len() > MAX_BYTES { return Err("Invalid history text".into()); }
            for key in ["timestamp", "created_monotonic"] {
                if !object[key].as_f64().is_some_and(|value| value.is_finite() && value >= 0.0) { return Err("Invalid history time".into()); }
            }
        }
        let now = (self.now)();
        Ok(rows.iter().filter(|row| include_expired || { let age = now - row["created_monotonic"].as_f64().unwrap_or(f64::INFINITY); age >= 0.0 && age < self.retention as f64 }).take(self.limit).cloned().collect())
    }

    pub fn prune(&self) -> Result<usize> {
        self.locked(|| {
            let raw = self.entries(true)?;
            let now = (self.now)();
            let items = if self.enabled() { raw.iter().filter(|row| { let age = now - row["created_monotonic"].as_f64().unwrap_or(f64::INFINITY); age >= 0.0 && age < self.retention as f64 }).cloned().collect::<Vec<_>>() } else { Vec::new() };
            if items != raw { atomic(&self.data, &json!(items))?; }
            Ok(items.len())
        })
    }

    pub fn advance_barrier(&self) -> Result<()> { atomic(&self.runtime.join("clear-state.json"), &json!({"time":(self.now)(),"epoch":random_id()?})) }

    pub fn barrier(&self) -> Result<Value> {
        let Some(value) = read_json(&self.runtime.join("clear-state.json"), 128)? else { return Ok(json!({"time":0,"epoch":""})); };
        let object = value.as_object().filter(|object| object.len() == 2 && object.contains_key("time") && object.contains_key("epoch")).ok_or("Invalid clear barrier")?;
        let time = object["time"].as_f64().filter(|time| time.is_finite() && *time >= 0.0).ok_or("Invalid clear barrier")?;
        let epoch = object["epoch"].as_str().ok_or("Invalid clear barrier")?;
        if !time.is_finite() || epoch.len() != 32 || !epoch.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)) { return Err("Invalid clear barrier".into()); }
        Ok(value)
    }

    pub fn set_enabled(&self, enabled: bool) -> Result<Value> {
        self.locked(|| {
            if enabled {
                if !self.enabled() { self.advance_barrier()?; }
                atomic(&self.consent, &json!({"version":1,"enabled":true}))?;
            } else {
                self.advance_barrier()?;
                match fs::remove_file(&self.consent) { Ok(()) => (), Err(error) if error.kind() == std::io::ErrorKind::NotFound => (), Err(error) => return Err(error.to_string()) }
                atomic(&self.data, &json!([]))?;
            }
            Ok(())
        })?;
        self.status()
    }

    pub fn observer_status(&self) -> Option<Value> {
        let value = read_json(&self.runtime.join("observer.json"), 1024).ok().flatten()?;
        let object = value.as_object()?;
        if object.len() != 4 || !["supervisor","observer","ready","epoch"].iter().all(|key| object.contains_key(*key)) || !object["ready"].is_boolean() || !object["epoch"].is_string() { return None; }
        for key in ["supervisor", "observer"] {
            let info = object[key].as_object()?;
            if info.len() != 2 || !info["pid"].is_u64() || !info["start"].is_u64() { return None; }
        }
        Some(value)
    }

    pub fn collecting(&self) -> bool {
        let Some(value) = self.observer_status() else { return false; };
        if value["ready"] != true || self.barrier().ok().and_then(|barrier| barrier["epoch"].as_str().map(str::to_owned)).as_deref() != value["epoch"].as_str() { return false; }
        ["supervisor", "observer"].iter().all(|key| value[*key]["pid"].as_u64().and_then(|pid| process_identity(pid as u32)).as_ref() == Some(&value[*key]))
    }

    pub fn publish_observer(&self, supervisor: &Value, observer: &Value, ready: bool, epoch: &str) -> Result<()> {
        self.locked(|| atomic(&self.runtime.join("observer.json"), &json!({"supervisor":supervisor,"observer":observer,"ready":ready,"epoch":epoch})))
    }

    pub fn retire_observer(&self, supervisor: &Value) -> Result<()> {
        self.locked(|| {
            if self.observer_status().is_some_and(|value| value["supervisor"] == *supervisor) {
                match fs::remove_file(self.runtime.join("observer.json")) { Ok(()) => (), Err(error) if error.kind() == std::io::ErrorKind::NotFound => (), Err(error) => return Err(error.to_string()) }
            }
            Ok(())
        })
    }

    pub fn await_observer(&self) -> bool {
        let until = std::time::Instant::now() + std::time::Duration::from_secs(3);
        while self.enabled() && !self.collecting() && std::time::Instant::now() < until { std::thread::sleep(std::time::Duration::from_millis(30)); }
        self.collecting()
    }

    pub fn status(&self) -> Result<Value> {
        let enabled = self.enabled();
        Ok(json!({"enabled":enabled,"collecting":enabled && self.collecting(),"warning":WARNING,"entries":[],"retentionSeconds":self.retention,"maxItems":self.limit,"ephemeral":true}))
    }

    pub fn ingest(&self, text: Option<&str>, sensitive: bool, captured_at: Option<f64>, epoch: Option<&str>) -> Result<Value> {
        self.locked(|| {
            let now = (self.now)();
            let barrier = self.barrier()?;
            if epoch.is_some_and(|epoch| Some(epoch) != barrier["epoch"].as_str()) { return Ok(json!({"recorded":false})); }
            if captured_at.is_some_and(|at| !at.is_finite() || at < barrier["time"].as_f64().unwrap_or(0.0) || now-at < 0.0 || now-at >= self.retention as f64) { return Ok(json!({"recorded":false})); }
            let Some(text) = text.filter(|text| !text.is_empty() && text.len() <= MAX_BYTES && !text.contains('\0')) else { return Ok(json!({"recorded":false})); };
            if !self.enabled() || sensitive { return Ok(json!({"recorded":false})); }
            let mut items = self.entries(false)?;
            if items.first().is_some_and(|item| item["text"] == text) { return Ok(json!({"recorded":false})); }
            items.insert(0, json!({"id":random_id()?,"text":text,"timestamp":(self.wall)(),"created_monotonic":captured_at.unwrap_or(now)}));
            items.truncate(self.limit);
            atomic(&self.data, &json!(items))?;
            Ok(json!({"recorded":true}))
        })
    }

    pub fn list(&self, query: &str) -> Result<Value> {
        self.locked(|| {
            if !self.enabled() { return self.status(); }
            let items = self.entries(false)?;
            atomic(&self.data, &json!(items))?;
            let folded = query.to_lowercase();
            let entries = items.iter().filter_map(|item| {
                let text = item["text"].as_str()?;
                if !text.to_lowercase().contains(&folded) { return None; }
                Some(json!({"id":item["id"],"text":text.chars().take(4096).collect::<String>(),"timestamp":item["timestamp"],"truncated":text.chars().count()>4096}))
            }).collect::<Vec<_>>();
            let mut state = self.status()?;
            state["entries"] = json!(entries);
            Ok(state)
        })
    }

    pub fn clear(&self) -> Result<Value> {
        self.locked(|| { self.advance_barrier()?; atomic(&self.data, &json!([])) })?;
        self.status()
    }

    pub fn copy(&self, id: &str, mut writer: impl FnMut(&str) -> Result<()>) -> Result<Value> {
        self.locked(|| {
            if !self.enabled() { return Err("Clipboard history is disabled".into()); }
            let items = self.entries(false)?;
            let row = items.iter().find(|row| row["id"] == id).ok_or("Clipboard item expired or was cleared")?;
            writer(row["text"].as_str().ok_or("Invalid history text")?)?;
            let mut state = self.status()?;
            state["copied"] = json!(true);
            Ok(state)
        })
    }
}
