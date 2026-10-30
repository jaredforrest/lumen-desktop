//! Bounded, recoverable Undo for exact outputs created by Copy.
//! No payload is recursively unlinked, including on cancellation or recovery failure.
use crate::trash::{self, Facts, Identity};
use crate::{JobResult, quote_path, trash_root, unix_date, uuid};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{
    ffi::{CStr, CString, OsStr},
    fs::{self, File},
    io::{Read, Write},
    os::fd::{AsRawFd, IntoRawFd},
    os::unix::{ffi::OsStrExt, fs::DirBuilderExt},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

pub const MAX_NODES: usize = 4096;
pub const MAX_DEPTH: usize = 64;
pub const MAX_BYTES: u64 = 64 * 1024 * 1024;
pub const MAX_MANIFEST_BYTES: usize = 1024 * 1024;
const SECONDS: u64 = 2;
#[derive(Clone, Debug, Eq, PartialEq)]
struct Row {
    path: Vec<String>,
    facts: Facts,
    uid: u32,
    gid: u32,
    digest: Option<[u8; 32]>,
}
#[derive(Clone)]
pub struct Record {
    parent: PathBuf,
    identity: Identity,
    name: String,
    manifest: Vec<Row>,
}
pub enum Outcome {
    Complete(PathBuf),
    Refused(String),
    Preserved {
        path: Option<PathBuf>,
        error: String,
    },
}
struct Budget {
    nodes: usize,
    bytes: u64,
    manifest: usize,
    deadline: Instant,
}
impl Budget {
    fn new() -> Self {
        Self {
            nodes: 0,
            bytes: 0,
            manifest: 2,
            deadline: Instant::now() + Duration::from_secs(SECONDS),
        }
    }
    fn check(&self) -> JobResult<()> {
        if self.nodes > MAX_NODES
            || self.bytes > MAX_BYTES
            || self.manifest > MAX_MANIFEST_BYTES
            || Instant::now() > self.deadline
        {
            return Err("Copy Undo exceeds its 4096-item, 64-level, 64 MiB, 1 MiB manifest or cooperative 2-second inspection limit".into());
        }
        Ok(())
    }
}
fn message(error: impl ToString) -> String {
    error.to_string()
}
fn names(parent: &File, limit: usize, deadline: Instant) -> JobResult<Vec<String>> {
    // read_dir on /proc/self/fd opens another descriptor without O_NOATIME.
    // Preserve the metadata of copied directories while inspecting eligibility.
    let file = trash::open_at(
        parent.as_raw_fd(),
        OsStr::new("."),
        libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_NOATIME,
        0,
    )?;
    let fd = file.into_raw_fd();
    let raw = unsafe { libc::fdopendir(fd) };
    if raw.is_null() {
        unsafe { libc::close(fd) };
        return Err(std::io::Error::last_os_error().to_string());
    }
    struct Directory(*mut libc::DIR);
    impl Drop for Directory {
        fn drop(&mut self) {
            unsafe { libc::closedir(self.0) };
        }
    }
    let directory = Directory(raw);
    let mut result = Vec::new();
    loop {
        unsafe { *libc::__errno_location() = 0 };
        let entry = unsafe { libc::readdir(directory.0) };
        if entry.is_null() {
            if unsafe { *libc::__errno_location() } != 0 {
                return Err(std::io::Error::last_os_error().to_string());
            }
            break;
        }
        let raw = unsafe { CStr::from_ptr((*entry).d_name.as_ptr()) };
        if raw.to_bytes() == b"." || raw.to_bytes() == b".." {
            continue;
        }
        if result.len() >= limit || Instant::now() > deadline {
            return Err(
                "Copy Undo directory inspection exceeds its node or cooperative time budget".into(),
            );
        }
        result.push(
            raw.to_str()
                .map_err(|_| "Copy Undo requires UTF-8 descendant names")?
                .to_owned(),
        );
    }
    result.sort();
    Ok(result)
}
fn inspect(
    parent: &File,
    name: &str,
    mount: u64,
    prefix: &mut Vec<String>,
    rows: &mut Vec<Row>,
    budget: &mut Budget,
) -> JobResult<()> {
    budget.check()?;
    if prefix.len() > MAX_DEPTH {
        return Err("Copy Undo exceeds its 64-level inspection limit".into());
    }
    let (facts, actual_mount) = trash::observed(parent, OsStr::new(name))?;
    let mut ownership: libc::stat = unsafe { std::mem::zeroed() };
    let item = CString::new(name).map_err(message)?;
    if unsafe {
        libc::fstatat(
            parent.as_raw_fd(),
            item.as_ptr(),
            &mut ownership,
            libc::AT_SYMLINK_NOFOLLOW,
        )
    } != 0
    {
        return Err(std::io::Error::last_os_error().to_string());
    }
    if Facts::from_stat(&ownership) != facts {
        return Err("The copied item changed during ownership inspection".into());
    }
    if actual_mount != mount {
        return Err("Copy Undo does not traverse mounted contents or cross volumes".into());
    }
    if !(facts.directory() || facts.regular() || facts.symlink()) {
        return Err("Copy Undo does not remove special files".into());
    }
    budget.nodes += 1;
    if facts.regular() {
        budget.bytes = budget.bytes.saturating_add(facts.size.max(0) as u64);
    }
    budget.check()?;
    let digest: Option<[u8; 32]> = if facts.regular() {
        let mut file = trash::open_at(
            parent.as_raw_fd(),
            OsStr::new(name),
            libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_NOATIME,
            0,
        )?;
        if Facts::from_stat(&trash::stat_fd(&file)?) != facts {
            return Err("The copied file changed during inspection".into());
        }
        let mut hash = Sha256::new();
        let mut bytes = vec![0_u8; 65536];
        let mut read = 0_u64;
        loop {
            budget.check()?;
            let remaining = (facts.size.max(0) as u64).saturating_sub(read);
            let limit = bytes.len().min(remaining.saturating_add(1) as usize);
            let count = file.read(&mut bytes[..limit]).map_err(message)?;
            if count == 0 {
                break;
            }
            read = read.saturating_add(count as u64);
            if read > facts.size.max(0) as u64 {
                return Err("The copied file grew during inspection".into());
            }
            hash.update(&bytes[..count]);
        }
        if read != facts.size.max(0) as u64 || Facts::from_stat(&trash::stat_fd(&file)?) != facts {
            return Err("The copied file changed during inspection".into());
        }
        Some(hash.finalize().into())
    } else if facts.symlink() {
        let name = CString::new(name).map_err(message)?;
        let mut bytes = [0_u8; 4097];
        let length = unsafe {
            libc::readlinkat(
                parent.as_raw_fd(),
                name.as_ptr(),
                bytes.as_mut_ptr().cast(),
                bytes.len(),
            )
        };
        if length < 0 || length as usize == bytes.len() {
            return Err("The copied link cannot be inspected safely".into());
        }
        Some(Sha256::digest(&bytes[..length as usize]).into())
    } else {
        None
    };
    let encoded = serde_json::to_vec(&json!([
        prefix,
        facts.dev,
        facts.ino,
        facts.mode,
        ownership.st_uid,
        ownership.st_gid,
        facts.size,
        facts.mtime_ns.to_string(),
        facts.ctime_ns.to_string(),
        digest.map(|bytes| bytes.iter().map(|b| format!("{b:02x}")).collect::<String>())
    ]))
    .map_err(message)?;
    budget.manifest = budget.manifest.saturating_add(encoded.len() + 1);
    budget.check()?;
    rows.push(Row {
        path: prefix.clone(),
        facts,
        uid: ownership.st_uid,
        gid: ownership.st_gid,
        digest,
    });
    if facts.directory() {
        let child = trash::directory(parent.as_raw_fd(), OsStr::new(name), false)?;
        if Facts::from_stat(&trash::stat_fd(&child)?) != facts {
            return Err("The copied folder changed during inspection".into());
        }
        for name in names(
            &child,
            MAX_NODES.saturating_sub(budget.nodes),
            budget.deadline,
        )? {
            prefix.push(name.clone());
            inspect(&child, &name, mount, prefix, rows, budget)?;
            prefix.pop();
        }
        if Facts::from_stat(&trash::stat_fd(&child)?) != facts {
            return Err("The copied folder changed during inspection".into());
        }
    }
    if trash::observed(parent, OsStr::new(name))?.0 != facts {
        return Err("The copied item changed during inspection".into());
    }
    Ok(())
}
fn manifest(parent: &File, name: &str, mount: u64) -> JobResult<Vec<Row>> {
    let mut rows = Vec::new();
    inspect(
        parent,
        name,
        mount,
        &mut Vec::new(),
        &mut rows,
        &mut Budget::new(),
    )?;
    Ok(rows)
}
fn same_after_rename(expected: &[Row], actual: &[Row]) -> bool {
    expected.len() == actual.len()
        && expected
            .iter()
            .zip(actual)
            .enumerate()
            .all(|(index, (a, b))| {
                let mut a = a.clone();
                if index == 0 {
                    a.facts.ctime_ns = b.facts.ctime_ns;
                }
                a == *b
            })
}
fn anchored_path(parent: &File) -> JobResult<PathBuf> {
    let path = fs::read_link(format!("/proc/self/fd/{}", parent.as_raw_fd())).map_err(message)?;
    if !path.is_absolute() || path.as_os_str().as_bytes().ends_with(b" (deleted)") {
        return Err("The recovery folder no longer has a reachable path".into());
    }
    Ok(path)
}
fn write_private(parent: &File, name: &str, bytes: &[u8]) -> JobResult<Facts> {
    let mut file = trash::open_at(
        parent.as_raw_fd(),
        OsStr::new(name),
        libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL | libc::O_NOFOLLOW,
        0o600,
    )?;
    file.write_all(bytes)
        .and_then(|()| file.sync_all())
        .map_err(message)?;
    Ok(Facts::from_stat(&trash::stat_fd(&file)?))
}
fn check_roots(path: &Path, expected: [Identity; 3]) -> JobResult<()> {
    let root = trash::directory(libc::AT_FDCWD, path.as_os_str(), true)?;
    let files = trash::directory(root.as_raw_fd(), OsStr::new("files"), true)?;
    let info = trash::directory(root.as_raw_fd(), OsStr::new("info"), true)?;
    if [
        trash::identity(&root)?,
        trash::identity(&files)?,
        trash::identity(&info)?,
    ] != expected
    {
        return Err("The personal Trash folders changed".into());
    }
    Ok(())
}
impl Record {
    pub fn capture(destination: &Path) -> JobResult<Self> {
        let parent = fs::canonicalize(
            destination
                .parent()
                .ok_or("Copy destination has no parent")?,
        )
        .map_err(message)?;
        let name = destination
            .file_name()
            .and_then(OsStr::to_str)
            .filter(|name| !name.is_empty() && !name.chars().any(char::is_control))
            .ok_or("Copy Undo requires a supported local name")?
            .to_owned();
        let folder = trash::directory(libc::AT_FDCWD, parent.as_os_str(), false)?;
        let identity = trash::identity(&folder)?;
        let manifest = manifest(&folder, &name, identity.mount)?;
        if trash::identity(&folder)? != identity {
            return Err("Copy destination parent changed".into());
        }
        Ok(Self {
            parent,
            identity,
            name,
            manifest,
        })
    }
    pub fn for_publication(staging: &Path, destination: &Path) -> JobResult<Self> {
        let mut record = Self::capture(staging)?;
        let parent = fs::canonicalize(
            destination
                .parent()
                .ok_or("Copy destination has no parent")?,
        )
        .map_err(message)?;
        let folder = trash::directory(libc::AT_FDCWD, parent.as_os_str(), false)?;
        let identity = trash::identity(&folder)?;
        if identity.mount != record.identity.mount || identity.dev != record.identity.dev {
            return Err("Copy Undo publication crosses a volume".into());
        }
        record.parent = parent;
        record.identity = identity;
        record.name = destination
            .file_name()
            .and_then(OsStr::to_str)
            .filter(|name| !name.is_empty() && !name.chars().any(char::is_control))
            .ok_or("Unsupported Copy Undo name")?
            .to_owned();
        Ok(record)
    }
    pub fn published(mut self) -> JobResult<Self> {
        let parent = trash::directory(libc::AT_FDCWD, self.parent.as_os_str(), false)?;
        if trash::identity(&parent)? != self.identity {
            return Err("Copy destination parent changed during publication".into());
        }
        let actual = manifest(&parent, &self.name, self.identity.mount)?;
        if !same_after_rename(&self.manifest, &actual) {
            return Err("Published copy changed before its Undo record was accepted".into());
        }
        self.manifest = actual;
        Ok(self)
    }
    pub fn label(&self) -> String {
        format!("Undo Copy of “{}”", self.name)
    }
    pub fn undo(
        &self,
        admit: impl Fn() -> JobResult<()>,
        report: impl Fn(&Path, &str) -> JobResult<()>,
    ) -> Outcome {
        match trash_root() {
            Ok(root) => self.undo_inner(root, |_| Ok(()), admit, report),
            Err(error) => Outcome::Refused(error),
        }
    }
    fn undo_inner(
        &self,
        root_path: PathBuf,
        after_claim: impl FnOnce(&Path) -> JobResult<()>,
        admit: impl Fn() -> JobResult<()>,
        report: impl Fn(&Path, &str) -> JobResult<()>,
    ) -> Outcome {
        let result = (|| -> JobResult<Outcome> {
            admit()?;
            let parent = trash::directory(libc::AT_FDCWD, self.parent.as_os_str(), false)?;
            if trash::identity(&parent)? != self.identity
                || manifest(&parent, &self.name, self.identity.mount)? != self.manifest
            {
                return Err("The copied destination or its parent changed; Undo kept it".into());
            }
            fs::DirBuilder::new()
                .recursive(true)
                .mode(0o700)
                .create(&root_path)
                .map_err(message)?;
            let root = trash::directory(libc::AT_FDCWD, root_path.as_os_str(), true)?;
            for name in ["files", "info"] {
                if trash::observed(&root, OsStr::new(name)).is_err() {
                    trash::mkdir_at(&root, name)?;
                }
            }
            let files = trash::directory(root.as_raw_fd(), OsStr::new("files"), true)?;
            let info = trash::directory(root.as_raw_fd(), OsStr::new("info"), true)?;
            let roots = [
                trash::identity(&root)?,
                trash::identity(&files)?,
                trash::identity(&info)?,
            ];
            if roots
                .iter()
                .any(|value| value.mount != self.identity.mount || value.dev != self.identity.dev)
            {
                return Err(
                    "Copy Undo requires personal Trash on the same volume; the copy was kept"
                        .into(),
                );
            }
            let token = uuid()?;
            let stage_name = format!(".lumen-copy-undo-{token}");
            trash::mkdir_at(&root, &stage_name)?;
            let stage = trash::directory(root.as_raw_fd(), OsStr::new(&stage_name), true)?;
            let stage_path = anchored_path(&stage)?;
            let original = self.parent.join(&self.name);
            let record = format!(
                "[Trash Info]\nPath={}\nDeletionDate={}\n",
                quote_path(original.to_str().ok_or("Copy destination is not UTF-8")?),
                unix_date()?
            );
            let record_facts = write_private(&stage, "record.trashinfo", record.as_bytes())?;
            let recovery_facts = write_private(&stage,"RECOVERY.txt",format!("Recoverable Copy Undo staging. No automatic deletion is performed.\nOriginal copied destination: {}\nStaged payload: {}\nPlanned Trash payload: {}\nPlanned Trash record: {}\nIf data exists at either payload path, move it back only when the original destination is free. If it is recognized in Trash, use Put Back.\nThe original copy source was never changed.\n",original.display(),stage_path.join("data").display(),anchored_path(&files)?.join(&token).display(),anchored_path(&info)?.join(format!("{token}.trashinfo")).display()).as_bytes())?;
            stage.sync_all().map_err(message)?;
            root.sync_all().map_err(message)?;
            let mut claimed = false;
            let mut published = false;
            let item = (|| -> JobResult<PathBuf> {
                report(
                    &stage_path.join("RECOVERY.txt"),
                    "Copy Undo started. If interrupted, follow this recovery record; no action is automatically replayed.",
                )?;
                if trash::identity(&parent)? != self.identity
                    || manifest(&parent, &self.name, self.identity.mount)? != self.manifest
                {
                    return Err("The copied destination changed before Undo; it was kept".into());
                }
                check_roots(&root_path, roots)?;
                admit()?;
                trash::rename_exact(&self.name, &parent, "data", &stage)?;
                claimed = true;
                parent.sync_all().map_err(message)?;
                stage.sync_all().map_err(message)?;
                after_claim(&stage_path)?;
                if !same_after_rename(
                    &self.manifest,
                    &manifest(&stage, "data", self.identity.mount)?,
                ) {
                    return Err(
                        "The staged copy changed; its recoverable contents were kept".into(),
                    );
                }
                check_roots(&root_path, roots)?;
                trash::rename_exact("data", &stage, &token, &files)?;
                published = true;
                trash::rename_exact(
                    "record.trashinfo",
                    &stage,
                    &format!("{token}.trashinfo"),
                    &info,
                )?;
                files.sync_all().map_err(message)?;
                info.sync_all().map_err(message)?;
                root.sync_all().map_err(message)?;
                check_roots(&root_path, roots)?;
                let path = anchored_path(&files)?.join(&token);
                report(
                    &path,
                    "Copy undone. The copied item is in Trash; use Put Back to restore it.",
                )?;
                Ok(path)
            })();
            match item {
                Ok(path) => {
                    let _ = trash::remove_at(&stage, "RECOVERY.txt", recovery_facts, false);
                    if let Ok(facts) = trash::observed(&root, OsStr::new(&stage_name)) {
                        let _ = trash::remove_at(&root, &stage_name, facts.0, true);
                    }
                    Ok(Outcome::Complete(path))
                }
                Err(error) if !claimed => {
                    if report(&original,&format!("Copy Undo stopped before claiming an item: {error}. The copied destination was kept.")).is_err() {return Err(format!("{error}. Copy Undo recovery details remain at {}.",anchored_path(&stage).map(|path|path.join("RECOVERY.txt").display().to_string()).unwrap_or_else(|_|"an unverified recovery location".into())));}
                    let _ = trash::remove_at(&stage, "record.trashinfo", record_facts, false);
                    let _ = trash::remove_at(&stage, "RECOVERY.txt", recovery_facts, false);
                    if let Ok(facts) = trash::observed(&root, OsStr::new(&stage_name)) {
                        let _ = trash::remove_at(&root, &stage_name, facts.0, true);
                    }
                    Err(error)
                }
                Err(error) => {
                    // A failed post-claim path never recursively removes data or
                    // overwrites a replacement at the original destination.
                    if !published
                        && trash::rename_exact("data", &stage, &self.name, &parent).is_ok()
                    {
                        let _ = parent.sync_all();
                        let _ = stage.sync_all();
                        let returned = match anchored_path(&parent) {
                            Ok(path) => path.join(&self.name),
                            Err(path_error) => {
                                return Ok(Outcome::Preserved {
                                    path: None,
                                    error: format!(
                                        "{error}. The item was returned to its held original parent, but its reachable path could not be verified: {path_error}. No automatic deletion or retry is performed."
                                    ),
                                });
                            }
                        };
                        if report(&returned,&format!("Copy Undo stopped: {error}. The staged item was returned without overwriting anything.")).is_err() {return Ok(Outcome::Refused(format!("{error}. The staged item was returned to {}; recovery details remain at {}.",returned.display(),anchored_path(&stage).map(|path|path.join("RECOVERY.txt").display().to_string()).unwrap_or_else(|_|"an unverified recovery location".into()))));}
                        let _ = trash::remove_at(&stage, "record.trashinfo", record_facts, false);
                        let _ = trash::remove_at(&stage, "RECOVERY.txt", recovery_facts, false);
                        if let Ok(facts) = trash::observed(&root, OsStr::new(&stage_name)) {
                            let _ = trash::remove_at(&root, &stage_name, facts.0, true);
                        }
                        return Ok(Outcome::Refused(format!(
                            "Copy Undo stopped: {error}. The staged item was returned without replacing anything at {}.",
                            returned.display()
                        )));
                    }
                    let path = if published {
                        anchored_path(&files).map(|path| path.join(&token))
                    } else {
                        anchored_path(&stage).map(|path| path.join("data"))
                    };
                    let (path, location) = match path {
                        Ok(path) => (Some(path), "Use the reported recovery path.".to_string()),
                        Err(reason) => (
                            None,
                            format!(
                                "The held recovery directory remains owned, but its reachable path cannot be verified: {reason}. Inspect personal Trash and retained recovery metadata before taking action."
                            ),
                        ),
                    };
                    Ok(Outcome::Preserved {
                        path,
                        error: format!(
                            "Copy Undo stopped: {error}. Recoverable contents were kept; {location} No automatic retry or deletion was attempted."
                        ),
                    })
                }
            }
        })();
        result.unwrap_or_else(Outcome::Refused)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn setup() -> PathBuf {
        let root = std::env::temp_dir().join(format!("copy-undo-unit-{}", uuid().unwrap()));
        fs::create_dir(&root).unwrap();
        root
    }
    #[test]
    fn private_publication_capture_does_not_adopt_later_edits() {
        let root = setup();
        let stage = root.join("staging");
        let target = root.join("published");
        fs::write(&stage, b"before").unwrap();
        let record = Record::for_publication(&stage, &target).unwrap();
        fs::rename(stage, &target).unwrap();
        fs::write(&target, b"edited").unwrap();
        assert!(record.published().is_err());
        assert_eq!(fs::read(&target).unwrap(), b"edited");
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn rename_comparison_retains_sampled_uid_and_gid() {
        use std::os::unix::fs::MetadataExt;
        let root = setup();
        let destination = root.join("copied");
        fs::write(&destination, b"created").unwrap();
        let record = Record::capture(&destination).unwrap();
        let info = fs::symlink_metadata(&destination).unwrap();
        assert_eq!(
            (record.manifest[0].uid, record.manifest[0].gid),
            (info.uid(), info.gid())
        );
        let mut renamed = record.manifest.clone();
        renamed[0].facts.ctime_ns += 1;
        assert!(same_after_rename(&record.manifest, &renamed));
        renamed[0].uid = renamed[0].uid.wrapping_add(1);
        assert!(!same_after_rename(&record.manifest, &renamed));
        renamed[0].uid = record.manifest[0].uid;
        renamed[0].gid = renamed[0].gid.wrapping_add(1);
        assert!(!same_after_rename(&record.manifest, &renamed));
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn post_claim_change_returns_without_overwrite_or_preserves_recovery() {
        for occupied in [false, true] {
            let root = setup();
            let destination = root.join("copied");
            let trash = root.join("Trash");
            fs::write(&destination, b"created").unwrap();
            let record = Record::capture(&destination).unwrap();
            let outcome = record.undo_inner(
                trash,
                |stage| {
                    fs::write(stage.join("data"), b"changed during claim").map_err(message)?;
                    if occupied {
                        fs::write(&destination, b"new destination occupant").map_err(message)?;
                    }
                    Ok(())
                },
                || Ok(()),
                |_, _| Ok(()),
            );
            match outcome {
                Outcome::Refused(error) if !occupied => {
                    assert!(error.contains("returned"));
                    assert_eq!(fs::read(&destination).unwrap(), b"changed during claim");
                }
                Outcome::Preserved { path, error } if occupied => {
                    assert!(error.contains("recoverable"));
                    assert_eq!(
                        fs::read(path.as_ref().unwrap()).unwrap(),
                        b"changed during claim"
                    );
                    assert_eq!(fs::read(&destination).unwrap(), b"new destination occupant");
                    assert!(
                        path.as_ref()
                            .unwrap()
                            .parent()
                            .unwrap()
                            .join("RECOVERY.txt")
                            .is_file()
                    );
                }
                _ => panic!("Unsafe post-claim outcome"),
            }
            fs::remove_dir_all(root).unwrap();
        }
    }
    #[test]
    fn node_budget_refuses_without_any_recovery_mutation() {
        let root = setup();
        let tree = root.join("tree");
        fs::create_dir(&tree).unwrap();
        for index in 0..MAX_NODES {
            fs::write(tree.join(index.to_string()), b"").unwrap();
        }
        assert!(Record::capture(&tree).is_err());
        assert_eq!(fs::read_dir(&tree).unwrap().count(), MAX_NODES);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn renamed_trash_reports_the_held_payload_location() {
        let root = setup();
        let destination = root.join("copied");
        let trash = root.join("Trash");
        let renamed = root.join("RenamedTrash");
        fs::write(&destination, b"created").unwrap();
        let record = Record::capture(&destination).unwrap();
        let outcome = record.undo_inner(
            trash.clone(),
            |_| {
                fs::rename(&trash, &renamed).map_err(message)?;
                fs::write(&destination, b"new occupant").map_err(message)?;
                Ok(())
            },
            || Ok(()),
            |_, _| Ok(()),
        );
        match outcome {
            Outcome::Preserved {
                path: Some(path), ..
            } => {
                assert!(path.starts_with(&renamed));
                assert_eq!(fs::read(path).unwrap(), b"created");
                assert_eq!(fs::read(destination).unwrap(), b"new occupant");
            }
            _ => panic!("renamed recovery was not disclosed"),
        }
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn reporting_failure_never_loses_claimed_data() {
        for fail_after_claim in [false, true] {
            let root = setup();
            let destination = root.join("copied");
            fs::write(&destination, b"created").unwrap();
            let record = Record::capture(&destination).unwrap();
            let calls = std::cell::Cell::new(0);
            let outcome = record.undo_inner(
                root.join("Trash"),
                |_| Ok(()),
                || Ok(()),
                |_, _| {
                    calls.set(calls.get() + 1);
                    if !fail_after_claim || calls.get() > 1 {
                        Err("journal unavailable".into())
                    } else {
                        Ok(())
                    }
                },
            );
            if fail_after_claim {
                match outcome {
                    Outcome::Preserved {
                        path: Some(path), ..
                    } => assert_eq!(fs::read(path).unwrap(), b"created"),
                    _ => panic!("published data was not retained"),
                }
            } else {
                assert!(matches!(outcome, Outcome::Refused(_)));
                assert_eq!(fs::read(destination).unwrap(), b"created");
            }
            fs::remove_dir_all(root).unwrap();
        }
    }
    #[test]
    fn cancellation_at_entry_or_after_inspection_never_claims_destination() {
        for admission in [1, 2] {
            let root = setup();
            let source = root.join("source");
            let destination = root.join("copied");
            fs::write(&source, b"original").unwrap();
            fs::copy(&source, &destination).unwrap();
            let record = Record::capture(&destination).unwrap();
            let calls = std::cell::Cell::new(0);
            let outcome = record.undo_inner(
                root.join("Trash"),
                |_| panic!("cancelled claim ran"),
                || {
                    calls.set(calls.get() + 1);
                    if calls.get() == admission {
                        Err("Cancelled".into())
                    } else {
                        Ok(())
                    }
                },
                |_, _| Ok(()),
            );
            assert!(matches!(outcome,Outcome::Refused(ref error) if error.contains("Cancelled")));
            assert_eq!(fs::read(&destination).unwrap(), b"original");
            assert_eq!(fs::read(&source).unwrap(), b"original");
            fs::remove_dir_all(root).unwrap();
        }
    }
}
