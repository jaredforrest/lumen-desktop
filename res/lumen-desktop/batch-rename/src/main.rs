use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};
use std::ffi::{CString, OsStr};
use std::io::{self, BufRead, Read, Write};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::os::unix::ffi::OsStrExt;
use std::path::{Component, Path, PathBuf};
use std::time::{Duration, Instant};

type Result<T> = std::result::Result<T, String>;

#[derive(Clone, Copy, PartialEq, Eq)]
struct Facts {
    device: u64,
    inode: u64,
    mode: u32,
    size: i64,
    mtime_ns: i128,
    ctime_ns: i128,
}
impl Facts {
    fn of(info: libc::stat) -> Self {
        Self {
            device: info.st_dev,
            inode: info.st_ino,
            mode: info.st_mode,
            size: info.st_size,
            mtime_ns: (info.st_mtime as i128) * 1_000_000_000 + info.st_mtime_nsec as i128,
            ctime_ns: (info.st_ctime as i128) * 1_000_000_000 + info.st_ctime_nsec as i128,
        }
    }
    fn stable_after_rename(self, other: Self) -> bool {
        self.device == other.device
            && self.inode == other.inode
            && self.mode == other.mode
            && self.size == other.size
            && self.mtime_ns == other.mtime_ns
    }
}

#[derive(Clone)]
struct Row {
    path: String,
    before: String,
    after: String,
    status: &'static str,
    error: String,
    facts: Option<Facts>,
}
impl Row {
    fn public(&self) -> Value {
        json!({"path":self.path,"before":self.before,"after":self.after,
            "status":self.status,"error":self.error})
    }
}
struct Plan {
    created: Instant,
    parent: PathBuf,
    parent_identity: (u64, u64),
    rows: Vec<Row>,
    executable: bool,
}
#[derive(Default)]
struct Plans {
    values: HashMap<String, Plan>,
}

fn c_name(value: &OsStr) -> Result<CString> {
    CString::new(value.as_bytes()).map_err(|_| "A captured local selection is required".into())
}
fn io_error(error: io::Error) -> String {
    error.to_string()
}
fn directory(path: &Path) -> Result<OwnedFd> {
    let name = c_name(path.as_os_str())?;
    let fd = unsafe {
        libc::open(
            name.as_ptr(),
            libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
        )
    };
    if fd < 0 {
        return Err(io_error(io::Error::last_os_error()));
    }
    Ok(unsafe { OwnedFd::from_raw_fd(fd) })
}
fn fd_stat(fd: &OwnedFd) -> Result<Facts> {
    let mut value = std::mem::MaybeUninit::<libc::stat>::uninit();
    if unsafe { libc::fstat(fd.as_raw_fd(), value.as_mut_ptr()) } != 0 {
        return Err(io_error(io::Error::last_os_error()));
    }
    Ok(Facts::of(unsafe { value.assume_init() }))
}
fn at_stat(fd: &OwnedFd, name: &str) -> Result<Option<Facts>> {
    let name = c_name(OsStr::new(name))?;
    let mut value = std::mem::MaybeUninit::<libc::stat>::uninit();
    if unsafe {
        libc::fstatat(
            fd.as_raw_fd(),
            name.as_ptr(),
            value.as_mut_ptr(),
            libc::AT_SYMLINK_NOFOLLOW,
        )
    } == 0
    {
        return Ok(Some(Facts::of(unsafe { value.assume_init() })));
    }
    let error = io::Error::last_os_error();
    if error.kind() == io::ErrorKind::NotFound {
        Ok(None)
    } else {
        Err(io_error(error))
    }
}
fn rename_noreplace(fd: &OwnedFd, before: &str, after: &str) -> Result<()> {
    let before = c_name(OsStr::new(before))?;
    let after = c_name(OsStr::new(after))?;
    let value = unsafe {
        libc::renameat2(
            fd.as_raw_fd(),
            before.as_ptr(),
            fd.as_raw_fd(),
            after.as_ptr(),
            libc::RENAME_NOREPLACE,
        )
    };
    if value == 0 {
        Ok(())
    } else {
        Err(io_error(io::Error::last_os_error()))
    }
}
fn random_token() -> Result<String> {
    let mut bytes = [0_u8; 24];
    std::fs::File::open("/dev/urandom")
        .and_then(|mut file| file.read_exact(&mut bytes))
        .map_err(io_error)?;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}
fn normalize_path(value: &str) -> Result<PathBuf> {
    if !value.starts_with('/') || value.contains('\0') {
        return Err("A captured local selection is required".into());
    }
    let mut path = PathBuf::from("/");
    for component in Path::new(value).components() {
        match component {
            Component::RootDir | Component::CurDir => (),
            Component::ParentDir => {
                path.pop();
            }
            Component::Normal(name) => path.push(name),
            Component::Prefix(_) => return Err("A captured local selection is required".into()),
        }
    }
    if path == Path::new("/") {
        return Err("The filesystem root cannot be renamed".into());
    }
    Ok(path)
}
fn identity(value: &Value) -> Result<(u64, u64)> {
    let data = value.as_object().ok_or("Invalid file identity")?;
    if data.len() != 2 || !data.contains_key("device") || !data.contains_key("inode") {
        return Err("Invalid file identity".into());
    }
    fn component(value: &Value) -> Result<u64> {
        if let Some(text) = value.as_str() {
            if text == "0"
                || (!text.starts_with('0')
                    && text.len() <= 20
                    && text.bytes().all(|byte| byte.is_ascii_digit()))
            {
                return text
                    .parse()
                    .map_err(|_| "Invalid or imprecise file identity".into());
            }
        }
        if let Some(integer) = value
            .as_u64()
            .filter(|number| *number <= 9_007_199_254_740_991)
        {
            return Ok(integer);
        }
        Err("Invalid or imprecise file identity".into())
    }
    Ok((component(&value["device"])?, component(&value["inode"])?))
}
fn validate_name(value: &str, maximum: usize) -> Result<()> {
    if value.is_empty()
        || matches!(value, "." | "..")
        || value.contains('/')
        || value.chars().any(|ch| ch < ' ' || ch == '\u{7f}')
    {
        return Err("Names must not be empty or contain slashes or control characters".into());
    }
    if value.len() > maximum {
        return Err("The proposed name is too long for this folder".into());
    }
    Ok(())
}
fn transform(name: &str, rule: &Value) -> Result<String> {
    let data = rule.as_object().ok_or("Invalid rename transformation")?;
    if data.len() != 3
        || !["mode", "first", "second"]
            .iter()
            .all(|key| data.contains_key(*key))
    {
        return Err("Invalid rename transformation".into());
    }
    let mode = rule["mode"]
        .as_str()
        .ok_or("Invalid rename transformation")?;
    let first = rule["first"]
        .as_str()
        .filter(|value| value.chars().count() <= 1024)
        .ok_or("Invalid rename transformation")?;
    let second = rule["second"]
        .as_str()
        .filter(|value| value.chars().count() <= 1024)
        .ok_or("Invalid rename transformation")?;
    match mode {
        "replace" if first.is_empty() => Err("Enter the text to replace".into()),
        "replace" => Ok(name.replace(first, second)),
        "prefix" | "suffix" if !second.is_empty() => {
            Err("Unexpected rename transformation text".into())
        }
        "prefix" => Ok(format!("{first}{name}")),
        "suffix" => Ok(format!("{name}{first}")),
        _ => Err("Invalid rename transformation".into()),
    }
}
impl Plans {
    fn prune(&mut self) {
        self.values
            .retain(|_, plan| plan.created.elapsed() < Duration::from_secs(300));
    }
    fn preview(&mut self, request: &Value) -> Result<Value> {
        let policy = if request.get("policy").is_none() {
            "stop"
        } else {
            request["policy"]
                .as_str()
                .ok_or("Choose stop or skip for conflicts")?
        };
        if !matches!(policy, "stop" | "skip") {
            return Err("Choose stop or skip for conflicts".into());
        }
        let items = request["items"]
            .as_array()
            .filter(|items| (2..=5000).contains(&items.len()))
            .ok_or("Select between 2 and 5,000 items to rename")?;
        let mut paths = Vec::with_capacity(items.len());
        let mut expected = Vec::with_capacity(items.len());
        for item in items {
            let object = item
                .as_object()
                .ok_or("A captured local selection is required")?;
            if object.len() != 2 || !object.contains_key("path") || !object.contains_key("identity")
            {
                return Err("A captured local selection is required".into());
            }
            paths.push(normalize_path(
                item["path"]
                    .as_str()
                    .ok_or("A captured local selection is required")?,
            )?);
            expected.push(identity(&item["identity"])?);
        }
        let parent = paths[0].parent().unwrap().to_path_buf();
        if paths
            .iter()
            .any(|path| path.parent() != Some(parent.as_path()))
            || paths.iter().collect::<HashSet<_>>().len() != paths.len()
        {
            return Err("Select distinct items in one folder".into());
        }
        transform("probe", &request["rule"])?;
        let fd = directory(&parent)?;
        let parent_facts = fd_stat(&fd)?;
        let maximum = unsafe { libc::fpathconf(fd.as_raw_fd(), libc::_PC_NAME_MAX) };
        let maximum = if maximum < 0 { 255 } else { maximum as usize };
        let mut rows = Vec::with_capacity(paths.len());
        let mut targets = HashMap::<String, usize>::new();
        for (path, identity) in paths.into_iter().zip(expected) {
            let before = path
                .file_name()
                .unwrap()
                .to_str()
                .ok_or("A captured local selection is required")?
                .to_owned();
            let after = transform(&before, &request["rule"])?;
            let mut row = Row {
                path: path.to_string_lossy().into_owned(),
                before: before.clone(),
                after: after.clone(),
                status: "ready",
                error: String::new(),
                facts: None,
            };
            let check = (|| -> Result<()> {
                validate_name(&after, maximum)?;
                let facts =
                    at_stat(&fd, &before)?.ok_or("The selected item changed. Select it again")?;
                if (facts.device, facts.inode) != identity {
                    return Err("The selected item changed. Select it again".into());
                }
                row.facts = Some(facts);
                if after == before {
                    row.status = "unchanged";
                } else if at_stat(&fd, &after)?.is_some() {
                    row.status = "conflict";
                    row.error = "An item with this name already exists".into();
                }
                Ok(())
            })();
            if let Err(error) = check {
                row.status = "invalid";
                row.error = error;
            }
            *targets.entry(after).or_default() += 1;
            rows.push(row);
        }
        for row in &mut rows {
            if targets[&row.after] > 1 && !matches!(row.status, "invalid" | "unchanged") {
                row.status = "conflict";
                row.error = "Several selected items would have the same name".into();
            }
        }
        let count = rows.iter().filter(|row| row.status == "ready").count();
        let executable = count > 0
            && !rows.iter().any(|row| row.status == "invalid")
            && (policy == "skip" || !rows.iter().any(|row| row.status == "conflict"));
        self.prune();
        if self.values.len() >= 64 {
            return Err("Too many rename previews are open. Close one and try again".into());
        }
        let token = random_token()?;
        let result = json!({"token":token,"rows":rows.iter().map(Row::public).collect::<Vec<_>>(),
            "executable":executable,"count":count,"policy":policy});
        self.values.insert(
            token,
            Plan {
                created: Instant::now(),
                parent,
                parent_identity: (parent_facts.device, parent_facts.inode),
                rows,
                executable,
            },
        );
        Ok(result)
    }
    fn cancel(&mut self, token: &Value) -> Value {
        if let Some(token) = token.as_str() {
            self.values.remove(token);
        }
        json!({"canceled":true})
    }
    fn execute(&mut self, request: &Value) -> Result<Value> {
        if request["confirmed"] != Value::Bool(true) {
            return Err("Confirm the preview before renaming".into());
        }
        self.prune();
        let token = request["token"]
            .as_str()
            .ok_or("This rename preview expired or was already used. Preview again")?;
        let plan = self
            .values
            .remove(token)
            .ok_or("This rename preview expired or was already used. Preview again")?;
        if !plan.executable {
            return Err("Resolve the preview conflicts before renaming".into());
        }
        let mut rows = plan.rows;
        let fd = directory(&plan.parent)?;
        let parent = fd_stat(&fd)?;
        if (parent.device, parent.inode) != plan.parent_identity {
            return Err("The selected folder changed. Preview again".into());
        }
        for row in &rows {
            if at_stat(&fd, &row.before)? != row.facts {
                return Err("A selected item changed. Nothing was renamed. Preview again".into());
            }
            if row.status == "ready" && at_stat(&fd, &row.after)?.is_some() {
                return Err("A destination appeared. Nothing was renamed. Preview again".into());
            }
        }
        let mut stopped = false;
        for index in 0..rows.len() {
            if rows[index].status == "conflict" {
                rows[index].status = "skipped";
                continue;
            }
            if rows[index].status == "unchanged" {
                continue;
            }
            if stopped {
                rows[index].status = "not_run";
                rows[index].error = "Stopped after the preceding error".into();
                continue;
            }
            let previous = rows[index].facts.unwrap();
            let result = (|| -> Result<Facts> {
                if at_stat(&fd, &rows[index].before)? != Some(previous) {
                    return Err("The selected item changed before its rename".into());
                }
                rename_noreplace(&fd, &rows[index].before, &rows[index].after)?;
                rows[index].status = "renamed";
                at_stat(&fd, &rows[index].after)?
                    .ok_or("The renamed item changed concurrently".into())
            })();
            match result {
                Ok(observed) => {
                    if !previous.stable_after_rename(observed) {
                        stopped = true;
                        rows[index].error =
                            "The renamed item changed concurrently; remaining changes stopped"
                                .into();
                    } else {
                        for pending in &mut rows {
                            if pending.status == "ready" && pending.facts == Some(previous) {
                                pending.facts = Some(observed);
                            }
                        }
                    }
                }
                Err(error) => {
                    if rows[index].status != "renamed" {
                        rows[index].status = "failed";
                    }
                    rows[index].error = error;
                    stopped = true;
                }
            }
        }
        let public = rows.iter().map(Row::public).collect::<Vec<_>>();
        if unsafe { libc::fsync(fd.as_raw_fd()) } != 0 {
            return Ok(json!({"ok":false,"rows":public,"error":format!(
                "Names changed but the folder durability acknowledgement failed: {}", io::Error::last_os_error())}));
        }
        Ok(json!({"ok":!stopped,"rows":public}))
    }
    fn dispatch(&mut self, request: &Value) -> Result<Value> {
        match request["action"].as_str() {
            Some("rename-preview") => self.preview(request),
            Some("rename-confirm") => self.execute(request),
            Some("rename-cancel") => Ok(self.cancel(&request["token"])),
            _ => Err("Unsupported rename request".into()),
        }
    }
}
fn main() {
    unsafe {
        libc::umask(0o077);
    }
    let stdin = io::stdin();
    let mut plans = Plans::default();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        let result = serde_json::from_str::<Value>(&line)
            .map_err(|_| "Invalid rename request".to_owned())
            .and_then(|request| {
                let id = request.get("id").cloned().unwrap_or(Value::Null);
                let action = request.get("action").cloned().unwrap_or(Value::Null);
                plans
                    .dispatch(&request)
                    .map(|mut value| {
                        value["id"] = id.clone();
                        value["action"] = action.clone();
                        value
                    })
                    .map_err(|error| json!({"id":id,"action":action,"error":error}).to_string())
            });
        let value = match result {
            Ok(value) => value,
            Err(error) => serde_json::from_str(&error).unwrap_or_else(|_| json!({"error":error})),
        };
        println!("{value}");
        let _ = io::stdout().flush();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::MetadataExt;

    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            let path = std::env::temp_dir()
                .join(format!("lumen-batch-rename-{}", random_token().unwrap()));
            std::fs::create_dir(&path).unwrap();
            Self(path)
        }
        fn item(&self, name: &str) -> Value {
            let path = self.0.join(name);
            if !path.exists() && path.symlink_metadata().is_err() {
                std::fs::write(&path, name).unwrap();
            }
            let metadata = path.symlink_metadata().unwrap();
            json!({"path":path,"identity":{"device":metadata.dev().to_string(),"inode":metadata.ino().to_string()}})
        }
        fn request(&self, names: &[&str], mode: &str, first: &str, policy: &str) -> Value {
            json!({"action":"rename-preview","items":names.iter().map(|name|self.item(name)).collect::<Vec<_>>(),
                "rule":{"mode":mode,"first":first,"second":""},"policy":policy})
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    fn confirm(plans: &mut Plans, preview: &Value) -> Result<Value> {
        plans.execute(&json!({"token":preview["token"],"confirmed":true}))
    }
    #[test]
    fn preview_is_readonly_and_confirmation_is_single_use() {
        let fixture = Fixture::new();
        let mut plans = Plans::default();
        let preview = plans
            .preview(&fixture.request(&["café.txt", "二.txt"], "prefix", "new-", "stop"))
            .unwrap();
        assert_eq!(preview["count"], 2);
        assert!(fixture.0.join("café.txt").exists());
        let result = confirm(&mut plans, &preview).unwrap();
        assert_eq!(result["ok"], true);
        assert!(fixture.0.join("new-café.txt").exists());
        assert!(fixture.0.join("new-二.txt").exists());
        assert!(confirm(&mut plans, &preview).is_err());
    }
    #[test]
    fn conflicts_skip_without_replacing_and_destination_races_abort_all() {
        let fixture = Fixture::new();
        let mut plans = Plans::default();
        fixture.item("one");
        fixture.item("two");
        std::fs::write(fixture.0.join("new-one"), "bystander").unwrap();
        let stop = plans
            .preview(&fixture.request(&["one", "two"], "prefix", "new-", "stop"))
            .unwrap();
        assert_eq!(stop["executable"], false);
        assert!(confirm(&mut plans, &stop).is_err());
        let skip = plans
            .preview(&fixture.request(&["one", "two"], "prefix", "new-", "skip"))
            .unwrap();
        assert_eq!(
            confirm(&mut plans, &skip).unwrap()["rows"][0]["status"],
            "skipped"
        );
        assert_eq!(
            std::fs::read_to_string(fixture.0.join("new-one")).unwrap(),
            "bystander"
        );
        std::fs::write(fixture.0.join("three"), "three").unwrap();
        let preview = plans
            .preview(&fixture.request(&["one", "three"], "prefix", "fresh-", "stop"))
            .unwrap();
        std::fs::write(fixture.0.join("fresh-three"), "late").unwrap();
        assert!(
            confirm(&mut plans, &preview)
                .unwrap_err()
                .contains("destination appeared")
        );
        assert!(fixture.0.join("one").exists());
        assert!(!fixture.0.join("fresh-one").exists());
    }
    #[test]
    fn changed_selection_and_parent_replacement_abort_without_changes() {
        let fixture = Fixture::new();
        let mut plans = Plans::default();
        let preview = plans
            .preview(&fixture.request(&["one", "two"], "prefix", "new-", "stop"))
            .unwrap();
        std::fs::write(fixture.0.join("two"), "changed").unwrap();
        assert!(
            confirm(&mut plans, &preview)
                .unwrap_err()
                .contains("selected item changed")
        );
        assert!(fixture.0.join("one").exists());
        let preview = plans
            .preview(&fixture.request(&["one", "two"], "prefix", "new-", "stop"))
            .unwrap();
        let old = fixture.0.with_extension("old");
        std::fs::rename(&fixture.0, &old).unwrap();
        std::fs::create_dir(&fixture.0).unwrap();
        assert!(
            confirm(&mut plans, &preview)
                .unwrap_err()
                .contains("selected folder changed")
        );
        assert!(old.join("one").exists());
        std::fs::remove_dir_all(old).unwrap();
    }
    #[test]
    fn hardlinks_symlinks_and_invalid_names_preserve_objects() {
        let fixture = Fixture::new();
        fixture.item("one");
        std::fs::hard_link(fixture.0.join("one"), fixture.0.join("two")).unwrap();
        let mut plans = Plans::default();
        let preview = plans
            .preview(&fixture.request(&["one", "two"], "prefix", "new-", "stop"))
            .unwrap();
        assert_eq!(confirm(&mut plans, &preview).unwrap()["ok"], true);
        assert_eq!(
            fixture.0.join("new-one").symlink_metadata().unwrap().ino(),
            fixture.0.join("new-two").symlink_metadata().unwrap().ino()
        );
        std::os::unix::fs::symlink("missing", fixture.0.join("link")).unwrap();
        std::fs::create_dir(fixture.0.join("folder")).unwrap();
        let preview = plans
            .preview(&fixture.request(&["link", "folder"], "prefix", "new-", "stop"))
            .unwrap();
        assert_eq!(confirm(&mut plans, &preview).unwrap()["ok"], true);
        assert!(
            fixture
                .0
                .join("new-link")
                .symlink_metadata()
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert!(fixture.0.join("new-folder").is_dir());
        let preview = plans
            .preview(&fixture.request(&["new-one", "new-two"], "prefix", "../", "stop"))
            .unwrap();
        assert_eq!(preview["executable"], false);
    }
    #[test]
    fn duplicate_targets_and_bad_identity_do_not_create_authority() {
        let fixture = Fixture::new();
        let mut plans = Plans::default();
        assert_eq!(
            plans
                .preview(&fixture.request(&["a", "aa"], "replace", "a", "stop"))
                .unwrap()["executable"],
            false
        );
        let mut request = fixture.request(&["a", "aa"], "prefix", "new-", "stop");
        request["items"][1]["identity"]["inode"] = json!("18446744073709551616");
        assert!(plans.preview(&request).is_err());
        request["items"][1]["identity"]["inode"] = json!("0");
        let preview = plans.preview(&request).unwrap();
        assert_eq!(preview["executable"], false);
    }
    #[test]
    fn expiry_cancel_case_change_and_noop() {
        let fixture = Fixture::new();
        let mut plans = Plans::default();
        let preview = plans
            .preview(&fixture.request(&["lower-one", "lower-two"], "replace", "lower", "stop"))
            .unwrap();
        assert_eq!(preview["executable"], true);
        let token = preview["token"].as_str().unwrap();
        plans.values.get_mut(token).unwrap().created = Instant::now() - Duration::from_secs(301);
        assert!(confirm(&mut plans, &preview).is_err());
        let preview = plans
            .preview(&fixture.request(&["lower-one", "lower-two"], "prefix", "new-", "stop"))
            .unwrap();
        plans.cancel(&preview["token"]);
        assert!(confirm(&mut plans, &preview).is_err());
        let mut request = fixture.request(&["lower-one", "lower-two"], "replace", "lower", "stop");
        request["rule"]["second"] = json!("LOWER");
        let preview = plans.preview(&request).unwrap();
        assert_eq!(confirm(&mut plans, &preview).unwrap()["ok"], true);
        assert!(fixture.0.join("LOWER-one").exists());
        let mut request = fixture.request(&["LOWER-one", "LOWER-two"], "replace", "absent", "stop");
        request["rule"]["second"] = json!("same");
        let preview = plans.preview(&request).unwrap();
        assert_eq!(preview["executable"], false);
        assert_eq!(preview["rows"][0]["status"], "unchanged");
    }
}
