//! A scoped, persistent local filename index.
use crate::line_service::LineService;
use crate::preview::{PreviewSession, token as preview_token};
use chrono::{Local, NaiveDate, TimeZone};
use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use rusqlite::{Connection, params, params_from_iter, types::Value as SqlValue};
use serde_json::{Value, json};
use std::collections::HashSet;
use std::env;
use std::fs;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use unicode_casefold::UnicodeCaseFold;
use unicode_normalization::{UnicodeNormalization, char::is_combining_mark};

pub type Result<T> = std::result::Result<T, String>;
const LIMIT: usize = 150000;
const SKIP: &[&str] = &[
    "node_modules",
    "vendor",
    "target",
    "build",
    "dist",
    "__pycache__",
    "venv",
    "Cache",
    "Code Cache",
    "GPUCache",
    "component_crx_cache",
    "blob_storage",
];
fn skipped(name: &str) -> bool {
    name.starts_with('.') || SKIP.contains(&name)
}
pub fn normalize(value: &str) -> String {
    value
        .nfkd()
        .case_fold()
        .filter(|c| !is_combining_mark(*c))
        .collect()
}
fn now_ns() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
        .min(i64::MAX as u128) as i64
}
fn uri(path: &Path) -> String {
    use std::os::unix::ffi::OsStrExt;
    let mut result = "file://".to_owned();
    for byte in path.as_os_str().as_bytes() {
        if byte.is_ascii_alphanumeric() || b"/-._~".contains(byte) {
            result.push(*byte as char);
        } else {
            result.push_str(&format!("%{byte:02X}"));
        }
    }
    result
}
fn absolute(path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        env::current_dir().unwrap_or_default().join(path)
    }
}
pub fn configured_roots() -> Result<Vec<PathBuf>> {
    let home = PathBuf::from(env::var_os("HOME").unwrap_or_default());
    let names = if let Ok(override_roots) = env::var("LUMEN_SEARCH_ROOTS") {
        serde_json::from_str::<Vec<String>>(&override_roots).map_err(|e| e.to_string())?
    } else {
        [
            "Desktop",
            "Documents",
            "Downloads",
            "Pictures",
            "Music",
            "Videos",
            "data",
        ]
        .iter()
        .map(|name| home.join(name).to_string_lossy().into_owned())
        .collect()
    };
    let mut roots = Vec::new();
    for name in names {
        let path = if name == "~" {
            home.clone()
        } else if let Some(rest) = name.strip_prefix("~/") {
            home.join(rest)
        } else {
            PathBuf::from(name)
        };
        let path = absolute(&path);
        let path = path.canonicalize().unwrap_or(path);
        if roots.iter().any(|root: &PathBuf| path.starts_with(root)) {
            continue;
        }
        roots.retain(|root| !root.starts_with(&path));
        roots.push(path);
    }
    Ok(roots)
}
#[derive(Default)]
struct Status {
    count: usize,
    limited: bool,
    unreadable: usize,
    last_scan: Option<Instant>,
}
pub struct Index {
    roots: Vec<PathBuf>,
    database: PathBuf,
    status: Mutex<Status>,
    scanning: AtomicBool,
    closed: AtomicBool,
    watched: Mutex<HashSet<PathBuf>>,
    watcher: Mutex<RecommendedWatcher>,
    changes: Sender<PathBuf>,
    emit: Arc<dyn Fn(Value) + Send + Sync>,
    tags: Mutex<LineService>,
    source_generation: String,
}
impl Index {
    pub fn new(
        roots: Vec<PathBuf>,
        database: PathBuf,
        changes: Sender<PathBuf>,
        emit: Arc<dyn Fn(Value) + Send + Sync>,
    ) -> Result<Self> {
        let parent = database.parent().ok_or("Invalid search cache path")?;
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        fs::set_permissions(parent, fs::Permissions::from_mode(0o700))
            .map_err(|e| e.to_string())?;
        let conn = Connection::open(&database).map_err(|e| e.to_string())?;
        conn.busy_timeout(Duration::from_secs(3))
            .map_err(|e| e.to_string())?;
        conn.execute_batch("PRAGMA journal_mode=WAL; CREATE TABLE IF NOT EXISTS files(path TEXT PRIMARY KEY, name TEXT, norm TEXT, basename TEXT, folder INTEGER, size INTEGER, mtime REAL, generation INTEGER); CREATE TABLE IF NOT EXISTS settings(key TEXT PRIMARY KEY, value TEXT);").map_err(|e|e.to_string())?;
        let mut columns = conn
            .prepare("PRAGMA table_info(files)")
            .map_err(|e| e.to_string())?;
        let names = columns
            .query_map([], |row| row.get::<_, String>(1))
            .map_err(|e| e.to_string())?
            .filter_map(|row| row.ok())
            .collect::<HashSet<_>>();
        if !names.contains("tags") {
            conn.execute(
                "ALTER TABLE files ADD COLUMN tags TEXT NOT NULL DEFAULT ''",
                [],
            )
            .map_err(|e| e.to_string())?;
        }
        drop(columns);
        let scope = serde_json::to_string(
            &roots
                .iter()
                .map(|root| root.to_string_lossy().into_owned())
                .collect::<Vec<_>>(),
        )
        .map_err(|e| e.to_string())?;
        let old: Option<String> = conn
            .query_row("SELECT value FROM settings WHERE key='scope'", [], |row| {
                row.get(0)
            })
            .ok();
        if old.as_deref() != Some(scope.as_str()) {
            conn.execute("DELETE FROM files", [])
                .map_err(|e| e.to_string())?;
            conn.execute(
                "INSERT OR REPLACE INTO settings VALUES ('scope', ?)",
                params![scope],
            )
            .map_err(|e| e.to_string())?;
        }
        let count = conn
            .query_row("SELECT count(*) FROM files", [], |row| {
                row.get::<_, usize>(0)
            })
            .map_err(|e| e.to_string())?;
        fs::set_permissions(&database, fs::Permissions::from_mode(0o600))
            .map_err(|e| e.to_string())?;
        let observer_changes = changes.clone();
        let watcher = notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
            if let Ok(event) = event {
                for path in event.paths {
                    let directory = if path.is_dir() {
                        path
                    } else {
                        path.parent().unwrap_or(Path::new("/")).to_path_buf()
                    };
                    let _ = observer_changes.send(directory);
                }
            }
        })
        .map_err(|e| e.to_string())?;
        let mut random = [0_u8; 16];
        getrandom::fill(&mut random).map_err(|e| e.to_string())?;
        Ok(Self {
            roots,
            database,
            status: Mutex::new(Status {
                count,
                ..Status::default()
            }),
            scanning: AtomicBool::new(false),
            closed: AtomicBool::new(false),
            watched: Mutex::new(HashSet::new()),
            watcher: Mutex::new(watcher),
            changes,
            emit,
            tags: Mutex::new(LineService::new(
                env::var("LUMEN_FILE_TAGS_BIN").unwrap_or_else(|_| "lumen-file-tags".into()),
            )),
            source_generation: hex::encode(random),
        })
    }
    fn connect(&self) -> Result<Connection> {
        let conn = Connection::open(&self.database).map_err(|e| e.to_string())?;
        conn.busy_timeout(Duration::from_secs(3))
            .map_err(|e| e.to_string())?;
        Ok(conn)
    }
    pub fn status(&self) -> Value {
        let status = self.status.lock().unwrap();
        json!({"event":"index","scanning":self.scanning.load(Ordering::SeqCst),"count":status.count,
            "limited":status.limited,"unreadable":status.unreadable,"roots":self.roots})
    }
    pub fn begin_scan(&self, force: bool) -> bool {
        let recent = self
            .status
            .lock()
            .unwrap()
            .last_scan
            .is_some_and(|time| time.elapsed() < Duration::from_secs(60));
        if !force && recent {
            return false;
        }
        if self
            .scanning
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return false;
        }
        (self.emit)(self.status());
        true
    }
    fn set_watches(&self, next: HashSet<PathBuf>) {
        let Ok(mut watched) = self.watched.lock() else {
            return;
        };
        let Ok(mut watcher) = self.watcher.lock() else {
            return;
        };
        let mut failed = 0;
        for path in watched.difference(&next) {
            let _ = watcher.unwatch(path);
        }
        let mut valid = watched.intersection(&next).cloned().collect::<HashSet<_>>();
        for path in next.difference(&watched) {
            if watcher.watch(path, RecursiveMode::NonRecursive).is_ok() {
                valid.insert(path.clone());
                // Enumeration happened before registration, so reconcile files
                // created in that gap through the ordinary bounded dirty pass.
                let _ = self.changes.send(path.clone());
            } else {
                failed += 1;
            }
        }
        *watched = valid;
        if failed > 0 {
            let mut status = self.status.lock().unwrap();
            status.limited = true;
            status.unreadable += failed;
            drop(status);
            (self.emit)(self.status());
        }
    }
    fn tag_text(&self, path: &str) -> Result<String> {
        let value = self
            .tags
            .lock()
            .unwrap()
            .tag(&json!({"action":"index","path":path}))?;
        Ok(value["text"].as_str().unwrap_or("").to_owned())
    }
    fn write_batch(
        conn: &mut Connection,
        batch: &mut Vec<(String, String, String, String, i64, i64, f64, i64, String)>,
    ) -> Result<()> {
        let transaction = conn.transaction().map_err(|e| e.to_string())?;
        for (path, name, norm, basename, folder, size, mtime, generation, tags) in batch.drain(..) {
            transaction
                .execute(
                    "INSERT OR REPLACE INTO files VALUES (?,?,?,?,?,?,?,?,?)",
                    params![
                        path, name, norm, basename, folder, size, mtime, generation, tags
                    ],
                )
                .map_err(|e| e.to_string())?;
        }
        transaction.commit().map_err(|e| e.to_string())
    }
    pub fn scan(&self) {
        let generation = now_ns();
        let mut count = 0;
        let mut unreadable = 0;
        let mut limited = false;
        let mut watched = HashSet::new();
        let mut stack = self
            .roots
            .iter()
            .rev()
            .filter(|root| root.is_dir())
            .cloned()
            .collect::<Vec<_>>();
        let result = (|| -> Result<()> {
            let mut db = self.connect()?;
            let mut batch = Vec::new();
            while let Some(directory) = stack.pop() {
                if self.closed.load(Ordering::SeqCst) {
                    return Ok(());
                }
                if watched.len() < 8192 {
                    watched.insert(directory.clone());
                }
                let entries = match fs::read_dir(&directory) {
                    Ok(entries) => entries,
                    Err(_) => {
                        unreadable += 1;
                        continue;
                    }
                };
                for entry in entries {
                    let entry = match entry {
                        Ok(entry) => entry,
                        Err(_) => {
                            unreadable += 1;
                            continue;
                        }
                    };
                    let name = entry.file_name().to_string_lossy().into_owned();
                    if name.starts_with('.') {
                        continue;
                    }
                    let kind = match entry.file_type() {
                        Ok(kind) => kind,
                        Err(_) => {
                            unreadable += 1;
                            continue;
                        }
                    };
                    if kind.is_symlink()
                        || (kind.is_dir() && SKIP.contains(&name.as_str()))
                        || (!kind.is_dir() && !kind.is_file())
                    {
                        continue;
                    }
                    let metadata = match entry.metadata() {
                        Ok(value) => value,
                        Err(_) => {
                            unreadable += 1;
                            continue;
                        }
                    };
                    let path = absolute(&entry.path());
                    let path_text = path.to_string_lossy().into_owned();
                    let tags = self.tag_text(&path_text)?;
                    batch.push((
                        path_text.clone(),
                        name.clone(),
                        normalize(&path_text),
                        normalize(&name),
                        i64::from(kind.is_dir()),
                        metadata.len().min(i64::MAX as u64) as i64,
                        metadata.mtime() as f64 + metadata.mtime_nsec() as f64 / 1e9,
                        generation,
                        tags,
                    ));
                    count += 1;
                    if kind.is_dir() {
                        stack.push(path);
                    }
                    if batch.len() >= 500 {
                        Self::write_batch(&mut db, &mut batch)?;
                        self.status.lock().unwrap().count = count;
                        (self.emit)(self.status());
                    }
                    if count >= LIMIT {
                        limited = true;
                        break;
                    }
                }
                if limited {
                    break;
                }
            }
            if self.closed.load(Ordering::SeqCst) {
                return Ok(());
            }
            Self::write_batch(&mut db, &mut batch)?;
            if !limited {
                db.execute(
                    "DELETE FROM files WHERE generation != ?",
                    params![generation],
                )
                .map_err(|e| e.to_string())?;
            }
            Ok(())
        })();
        if result.is_err() {
            unreadable += 1;
        }
        {
            let mut status = self.status.lock().unwrap();
            status.count = count;
            status.unreadable = unreadable;
            status.limited = limited;
            status.last_scan = Some(Instant::now());
        }
        self.set_watches(watched);
        self.scanning.store(false, Ordering::SeqCst);
        (self.emit)(self.status());
    }
    pub fn is_scanning(&self) -> bool {
        self.scanning.load(Ordering::SeqCst)
    }
    pub fn refresh_dirty(&self, pending: HashSet<PathBuf>) {
        if self
            .scanning
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return;
        }
        let mut watched = self.watched.lock().unwrap().clone();
        let mut visited = 0;
        let mut limited = false;
        let result = (|| -> Result<usize> {
            let db = self.connect()?;
            let mut stack = pending.into_iter().collect::<Vec<_>>();
            while let Some(directory) = stack.pop() {
                if visited >= 10000 || self.closed.load(Ordering::SeqCst) {
                    limited = !stack.is_empty();
                    break;
                }
                let prefix = format!("{}/", directory.display());
                let mut query=db.prepare("SELECT path FROM files WHERE substr(path,1,?)=? AND instr(substr(path,?),'/')=0").map_err(|e|e.to_string())?;
                let existing = query
                    .query_map(params![prefix.len(), prefix, prefix.len() + 1], |row| {
                        row.get::<_, String>(0)
                    })
                    .map_err(|e| e.to_string())?
                    .filter_map(|value| value.ok())
                    .collect::<HashSet<_>>();
                let mut seen = HashSet::new();
                if self.allowed(directory.to_string_lossy().as_ref()).is_ok() {
                    if let Ok(entries) = fs::read_dir(&directory) {
                        for entry in entries.flatten() {
                            let name = entry.file_name().to_string_lossy().into_owned();
                            if name.starts_with('.') {
                                continue;
                            }
                            let Ok(kind) = entry.file_type() else {
                                continue;
                            };
                            if kind.is_symlink()
                                || (kind.is_dir() && SKIP.contains(&name.as_str()))
                                || (!kind.is_dir() && !kind.is_file())
                            {
                                continue;
                            }
                            let Ok(meta) = entry.metadata() else {
                                continue;
                            };
                            let path = absolute(&entry.path());
                            let path_text = path.to_string_lossy().into_owned();
                            seen.insert(path_text.clone());
                            let tags = self.tag_text(&path_text)?;
                            db.execute(
                                "INSERT OR REPLACE INTO files VALUES (?,?,?,?,?,?,?,?,?)",
                                params![
                                    path_text,
                                    name,
                                    normalize(&path_text),
                                    normalize(&entry.file_name().to_string_lossy()),
                                    i64::from(kind.is_dir()),
                                    meta.len().min(i64::MAX as u64) as i64,
                                    meta.mtime() as f64 + meta.mtime_nsec() as f64 / 1e9,
                                    now_ns(),
                                    tags
                                ],
                            )
                            .map_err(|e| e.to_string())?;
                            visited += 1;
                            if kind.is_dir() && !existing.contains(&path_text) {
                                stack.push(path);
                            }
                        }
                    }
                    if watched.len() < 8192 {
                        watched.insert(directory.clone());
                    }
                }
                for removed in existing.difference(&seen) {
                    let nested = format!("{removed}/");
                    db.execute(
                        "DELETE FROM files WHERE path=? OR substr(path,1,?)=?",
                        params![removed, nested.len(), nested],
                    )
                    .map_err(|e| e.to_string())?;
                    watched.retain(|path| {
                        path.to_string_lossy() != removed.as_str()
                            && !path.to_string_lossy().starts_with(&nested)
                    });
                }
            }
            db.query_row("SELECT count(*) FROM files", [], |row| {
                row.get::<_, usize>(0)
            })
            .map_err(|e| e.to_string())
        })();
        match result {
            Ok(count) => {
                let mut status = self.status.lock().unwrap();
                status.count = count;
                if limited {
                    status.limited = true;
                }
            }
            Err(_) => self.status.lock().unwrap().unreadable += 1,
        }
        self.set_watches(watched);
        self.scanning.store(false, Ordering::SeqCst);
        (self.emit)(self.status());
    }
    pub fn search(&self, query: &str) -> Result<Value> {
        let query = query.chars().take(256).collect::<String>();
        let terms = shell_words::split(&query).map_err(|e| e.to_string())?;
        let kind = terms
            .iter()
            .find_map(|term| term.strip_prefix("kind:"))
            .unwrap_or("")
            .to_lowercase();
        let extension = terms
            .iter()
            .find_map(|term| term.strip_prefix("ext:"))
            .unwrap_or("")
            .trim_start_matches('.')
            .to_lowercase();
        let folder = terms
            .iter()
            .find_map(|term| term.strip_prefix("in:"))
            .unwrap_or("");
        let tags = terms
            .iter()
            .filter_map(|term| term.strip_prefix("tag:"))
            .map(str::to_lowercase)
            .collect::<Vec<_>>();
        if tags.iter().any(|tag| {
            !["red", "orange", "yellow", "green", "blue", "purple", "grey"].contains(&tag.as_str())
        }) {
            return Err("Unknown colour tag".into());
        }
        let mut words = normalize(
            &terms
                .iter()
                .filter(|term| {
                    !["kind:", "ext:", "in:", "since:", "before:", "tag:"]
                        .iter()
                        .any(|prefix| term.starts_with(prefix))
                })
                .map(String::as_str)
                .collect::<Vec<_>>()
                .join(" "),
        )
        .split_whitespace()
        .take(8)
        .map(str::to_owned)
        .collect::<Vec<_>>();
        let mut bounds = Vec::new();
        for key in ["since", "before"] {
            let prefix = format!("{key}:");
            if let Some(value) = terms.iter().find_map(|term| term.strip_prefix(&prefix)) {
                if !value.is_empty() {
                    let date = NaiveDate::parse_from_str(value, "%Y-%m-%d")
                        .map_err(|_| format!("{key} requires YYYY-MM-DD"))?;
                    let midnight = date.and_hms_opt(0, 0, 0).ok_or("Invalid date")?;
                    let local = Local
                        .from_local_datetime(&midnight)
                        .earliest()
                        .ok_or("Invalid local date")?;
                    bounds.push((key, local.timestamp() as f64));
                }
            }
        }
        if (!kind.is_empty()
            || !extension.is_empty()
            || !folder.is_empty()
            || !bounds.is_empty()
            || !tags.is_empty())
            && words.is_empty()
        {
            words.push(String::new());
        }
        if words.is_empty() {
            return Ok(json!({"results":[],"more":false}));
        }
        let mut sql = format!(
            "SELECT path,name,folder,size,mtime FROM files WHERE {}",
            vec!["norm LIKE ? ESCAPE '\\'"; words.len()].join(" AND ")
        );
        let mut params = words
            .iter()
            .map(|word| SqlValue::Text(format!("%{}%", escape_like(word))))
            .collect::<Vec<_>>();
        if kind == "folder" {
            sql.push_str(" AND folder=1");
        } else if kind == "file" {
            sql.push_str(" AND folder=0");
        }
        if !extension.is_empty() {
            sql.push_str(" AND lower(name) LIKE ? ESCAPE '\\'");
            params.push(SqlValue::Text(format!("%.{}", escape_like(&extension))));
        }
        if !folder.is_empty() {
            sql.push_str(" AND substr(norm,1,length(norm)-length(basename)) LIKE ? ESCAPE '\\'");
            params.push(SqlValue::Text(format!(
                "%{}%",
                escape_like(&normalize(folder))
            )));
        }
        for tag in &tags {
            sql.push_str(" AND instr(tags,?)>0");
            params.push(SqlValue::Text(format!("|{tag}|")));
        }
        for (key, value) in bounds {
            sql.push_str(if key == "since" {
                " AND mtime>=?"
            } else {
                " AND mtime<?"
            });
            params.push(SqlValue::Real(value));
        }
        sql.push_str(" ORDER BY CASE WHEN basename=? THEN 0 WHEN substr(basename,1,?)=? THEN 1 ELSE 2 END, folder DESC, length(name), basename, path LIMIT 121");
        let words = words.join(" ").trim().to_owned();
        params.extend([
            SqlValue::Text(words.clone()),
            SqlValue::Integer(words.len() as i64),
            SqlValue::Text(words),
        ]);
        let db = self.connect()?;
        let mut statement = db.prepare(&sql).map_err(|e| e.to_string())?;
        let rows = statement
            .query_map(params_from_iter(params), |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, f64>(4)?,
                ))
            })
            .map_err(|e| e.to_string())?;
        let mut results = Vec::new();
        for row in rows {
            let (path, name, folder, size, mtime) = row.map_err(|e| e.to_string())?;
            let selected = match self.allowed(&path) {
                Ok(value) => value,
                Err(_) => continue,
            };
            let token = match self.token(&selected) {
                Ok(value) => value,
                Err(_) => continue,
            };
            let mime = if folder != 0 {
                "inode/directory"
            } else {
                mime_guess::from_path(&name)
                    .first_raw()
                    .unwrap_or("application/octet-stream")
            };
            let parent = Path::new(&path).parent().unwrap_or(Path::new("/"));
            results.push(json!({"kind":"file","id":path,"path":path,"token":token,"title":name,"subtitle":parent,"folder":folder!=0,"size":size,"mtime":mtime,"mime":mime}));
        }
        let more = results.len() > 80;
        results.truncate(80);
        Ok(json!({"results":results,"more":more}))
    }
    pub fn close(&self) {
        self.closed.store(true, Ordering::SeqCst);
    }
    pub fn allowed(&self, path: &str) -> Result<PathBuf> {
        let path = Path::new(path);
        if !path.is_absolute()
            || fs::symlink_metadata(path).is_ok_and(|metadata| metadata.file_type().is_symlink())
        {
            return Err("This file is no longer available".into());
        }
        let resolved = path
            .canonicalize()
            .map_err(|_| "This file is no longer available")?;
        let root = self
            .roots
            .iter()
            .find(|root| resolved.starts_with(root))
            .ok_or("This file is outside the search folders")?;
        if resolved
            .strip_prefix(root)
            .unwrap_or(Path::new(""))
            .components()
            .any(|part| skipped(&part.as_os_str().to_string_lossy()))
        {
            return Err("Hidden files are excluded from search".into());
        }
        Ok(resolved)
    }
    pub fn token(&self, path: &Path) -> Result<String> {
        let info =
            fs::symlink_metadata(path).map_err(|_| "The selected file is no longer available")?;
        preview_token(&self.source_generation, path, &info)
    }
    pub fn use_source_generation(&mut self, generation: String) {
        self.source_generation = generation;
    }
    pub fn selected(&self, path: &str, token: &str) -> Result<PathBuf> {
        let selected = self.allowed(path)?;
        if token.is_empty() || self.token(&selected)? != token {
            return Err("The selected file changed. Search again before opening it.".into());
        }
        Ok(selected)
    }
    pub fn preview(
        &self,
        session: &PreviewSession,
        request_id: u64,
        path: &str,
        token: &str,
    ) -> Result<Value> {
        let path = self.selected(path, token)?;
        let mut result = session
            .request(&path, 1, &Value::Null, Some(token), Some(request_id))
            .map_err(|e| e.to_string())?;
        if mime_guess::from_path(&path)
            .first_raw()
            .is_some_and(|mime| mime.starts_with("audio/") || mime.starts_with("video/"))
        {
            if let Some(map) = result.as_object_mut() {
                map.remove("uri");
            }
        }
        Ok(result)
    }
    pub fn open(&self, path: &str, token: &str, parent: bool) -> Result<Value> {
        let mut path = self.selected(path, token)?;
        if parent {
            path = path.parent().unwrap_or(&path).to_path_buf();
        }
        let mut args = vec!["8".to_owned()];
        if env::var("LUMEN_LAUNCH_SCOPES").as_deref() == Ok("1") {
            args.extend(["lumen-launch-app".into(), "--".into()]);
        }
        args.extend(["xdg-open".into(), uri(&path)]);
        let status = Command::new("timeout")
            .args(args)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map_err(|e| e.to_string())?;
        if !status.success() {
            return Err("Application could not be opened".into());
        }
        Ok(json!({"opened":true}))
    }
}
fn escape_like(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scoped_search_filters_ranking_and_dirty_changes() {
        // This test checks the index and query contract without a packaged tag service.
        unsafe {
            std::env::set_var("LUMEN_FILE_TAGS_BIN", "cat");
        }
        assert_eq!(normalize("Straße É"), "strasse e");
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("files");
        fs::create_dir_all(root.join("Documents")).unwrap();
        fs::create_dir(root.join("Other")).unwrap();
        fs::create_dir(root.join("node_modules")).unwrap();
        for name in [
            "Documents/report.txt",
            "Documents/report old.txt",
            "Other/report.txt",
            "node_modules/hidden.txt",
        ] {
            fs::write(root.join(name), b"sample").unwrap();
        }
        for (name, day) in [
            ("Documents/report.txt", 10),
            ("Documents/report old.txt", 1),
            ("Other/report.txt", 10),
        ] {
            let file = fs::File::options()
                .write(true)
                .open(root.join(name))
                .unwrap();
            let stamp = chrono::NaiveDate::from_ymd_opt(2026, 1, day)
                .unwrap()
                .and_hms_opt(12, 0, 0)
                .unwrap()
                .and_local_timezone(Local)
                .earliest()
                .unwrap()
                .timestamp() as u64;
            file.set_times(
                fs::FileTimes::new().set_modified(UNIX_EPOCH + Duration::from_secs(stamp)),
            )
            .unwrap();
        }
        let (changes, registered) = std::sync::mpsc::channel();
        let index = Index::new(
            vec![root.clone()],
            temp.path().join("index/index.sqlite"),
            changes,
            Arc::new(|_| {}),
        )
        .unwrap();
        assert!(index.begin_scan(true));
        index.scan();
        let registered = (0..3)
            .map(|_| registered.recv_timeout(Duration::from_secs(1)).unwrap())
            .collect::<HashSet<_>>();
        assert!(registered.contains(&root));
        let results = index.search("report in:Documents ext:txt").unwrap();
        assert_eq!(results["results"].as_array().unwrap().len(), 2);
        let first = &results["results"][0];
        let path = first["path"].as_str().unwrap();
        let token = first["token"].as_str().unwrap();
        assert!(index.selected(path, token).is_ok());
        assert!(index.selected(path, "stale").is_err());
        assert!(index.search("tag:unknown").is_err());
        let dated = index
            .search("report in:Documents since:2026-01-02 before:2026-02-01 ext:txt")
            .unwrap();
        assert_eq!(dated["results"].as_array().unwrap().len(), 1);
        assert_eq!(dated["results"][0]["title"], "report.txt");
        assert_eq!(
            index.search("report.txt ext:txt").unwrap()["results"][0]["title"],
            "report.txt"
        );
        assert_eq!(
            index.search("ext:txt").unwrap()["results"]
                .as_array()
                .unwrap()
                .len(),
            3
        );
        assert!(index.search("since:2026-02-31").is_err());
        assert!(
            index
                .allowed(root.join("node_modules/hidden.txt").to_str().unwrap())
                .is_err()
        );
        fs::write(root.join("new.txt"), b"new").unwrap();
        index.refresh_dirty(HashSet::from([root.clone()]));
        assert_eq!(
            index.search("new.txt").unwrap()["results"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        fs::rename(root.join("new.txt"), root.join("renamed.txt")).unwrap();
        index.refresh_dirty(HashSet::from([root.clone()]));
        assert_eq!(
            index.search("renamed.txt").unwrap()["results"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        assert!(
            index.search("new.txt").unwrap()["results"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        index.close();
    }
}
