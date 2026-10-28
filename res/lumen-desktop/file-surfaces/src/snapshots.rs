//! Immutable preview copies with explicit UI leases and worker pins.
use std::collections::{HashMap, HashSet};
use std::fs::{self, Metadata, OpenOptions};
use std::io::{Read, Write};
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::{Condvar, Mutex};
use std::time::Duration;

const MAX_SOURCE: u64 = 64 * 1024 * 1024;
const MAX_WORKING_SET: u64 = 128 * 1024 * 1024;

fn failure(reason: &'static str, message: &'static str) -> crate::captured::Unavailable {
    crate::captured::Unavailable { reason, message }
}

struct Lease {
    tokens: HashSet<String>,
    pins: usize,
    retired: bool,
}

struct State {
    directory: Option<PrivateDirectory>,
    copies: HashMap<String, PathBuf>,
    sizes: HashMap<String, u64>,
    references: HashMap<String, HashSet<String>>,
    leases: HashMap<String, Lease>,
    total: u64,
}

struct PrivateDirectory(PathBuf);

fn process_start(pid: u32) -> std::io::Result<Option<u64>> {
    let stat = match fs::read_to_string(format!("/proc/{pid}/stat")) {
        Ok(stat) => stat,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    let (_, tail) = stat
        .rsplit_once(") ")
        .ok_or_else(|| std::io::Error::other("Invalid process identity"))?;
    let start = tail
        .split_whitespace()
        .nth(19)
        .and_then(|value| value.parse().ok())
        .ok_or_else(|| std::io::Error::other("Invalid process identity"))?;
    Ok(Some(start))
}

fn stale_owner(name: &str) -> Option<(u32, u64)> {
    let mut parts = name.split('-');
    let pid = parts.next()?.parse().ok()?;
    let start = parts.next()?.parse().ok()?;
    let suffix = parts.next()?;
    if parts.next().is_some()
        || !(suffix.len() == 8 || suffix.len() == 16)
        || !suffix
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
    {
        return None;
    }
    Some((pid, start))
}

impl PrivateDirectory {
    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for PrivateDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

pub struct Snapshots<I, V>
where
    I: Fn(&Path, &Metadata) -> Result<String, String> + Send + Sync,
    V: Fn(&Path, &str) -> Result<(), String> + Send + Sync,
{
    identity: I,
    validate: V,
    state: Mutex<State>,
    changed: Condvar,
    provider_lease: String,
    runtime: Option<PathBuf>,
}

fn token() -> Result<String, crate::captured::Unavailable> {
    let mut bytes = [0_u8; 24];
    getrandom::fill(&mut bytes)
        .map_err(|_| failure("provider_failure", "Private preview storage is unavailable"))?;
    Ok(hex::encode(bytes))
}

fn private_directory(
    runtime: Option<&Path>,
) -> Result<PrivateDirectory, crate::captured::Unavailable> {
    let unavailable = || {
        failure(
            "provider_failure",
            "A private session runtime directory is required",
        )
    };
    let runtime = runtime.ok_or_else(unavailable)?;
    let info = fs::symlink_metadata(runtime).map_err(|_| unavailable())?;
    if !info.is_dir() || info.uid() != unsafe { libc::geteuid() } || info.mode() & 0o077 != 0 {
        return Err(unavailable());
    }
    let base = runtime.join(format!("lumen-preview-snapshots-{}", info.uid()));
    if !base.exists() {
        match fs::DirBuilder::new().mode(0o700).create(&base) {
            Ok(()) => (),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => (),
            Err(_) => {
                return Err(failure(
                    "provider_failure",
                    "Private preview storage is unavailable",
                ));
            }
        }
    }
    let base_info = fs::symlink_metadata(&base)
        .map_err(|_| failure("provider_failure", "Private preview storage is unavailable"))?;
    if !base_info.is_dir() || base_info.uid() != info.uid() || base_info.mode() & 0o077 != 0 {
        return Err(failure(
            "provider_failure",
            "Private preview storage is unavailable",
        ));
    }
    for old in fs::read_dir(&base)
        .map_err(|_| failure("provider_failure", "Private preview storage is unavailable"))?
        .flatten()
    {
        let name = old.file_name();
        let Some((pid, old_start)) = name.to_str().and_then(stale_owner) else {
            continue;
        };
        let Ok(info) = old.path().symlink_metadata() else {
            continue;
        };
        if !info.is_dir()
            || info.file_type().is_symlink()
            || info.uid() != unsafe { libc::geteuid() }
        {
            continue;
        }
        if let Ok(actual) = process_start(pid) {
            if actual != Some(old_start) {
                let _ = fs::remove_dir_all(old.path());
            }
        }
    }
    let start = process_start(std::process::id())
        .map_err(|_| failure("provider_failure", "Preview owner identity is unavailable"))?
        .ok_or_else(|| failure("provider_failure", "Preview owner identity is unavailable"))?;
    for _ in 0..16 {
        let key = token()?;
        let directory = base.join(format!("{}-{start}-{}", std::process::id(), &key[..16]));
        match fs::DirBuilder::new().mode(0o700).create(&directory) {
            Ok(()) => return Ok(PrivateDirectory(directory)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(_) => break,
        }
    }
    Err(failure(
        "provider_failure",
        "Private preview storage is unavailable",
    ))
}

impl<I, V> Snapshots<I, V>
where
    I: Fn(&Path, &Metadata) -> Result<String, String> + Send + Sync,
    V: Fn(&Path, &str) -> Result<(), String> + Send + Sync,
{
    pub fn new(identity: I, validate: V) -> Result<Self, crate::captured::Unavailable> {
        Self::new_with_runtime(
            identity,
            validate,
            std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from),
        )
    }

    pub fn new_with_runtime(
        identity: I,
        validate: V,
        runtime: Option<PathBuf>,
    ) -> Result<Self, crate::captured::Unavailable> {
        let provider_lease = token()?;
        let mut leases = HashMap::new();
        leases.insert(
            provider_lease.clone(),
            Lease {
                tokens: HashSet::new(),
                pins: 0,
                retired: false,
            },
        );
        Ok(Self {
            identity,
            validate,
            state: Mutex::new(State {
                directory: None,
                copies: HashMap::new(),
                sizes: HashMap::new(),
                references: HashMap::new(),
                leases,
                total: 0,
            }),
            changed: Condvar::new(),
            provider_lease,
            runtime,
        })
    }

    pub fn provider_lease(&self) -> &str {
        &self.provider_lease
    }

    pub fn begin(&self) -> Result<String, crate::captured::Unavailable> {
        let key = token()?;
        self.state.lock().unwrap().leases.insert(
            key.clone(),
            Lease {
                tokens: HashSet::new(),
                pins: 0,
                retired: false,
            },
        );
        Ok(key)
    }

    fn live(state: &State, lease: &str) -> Result<(), crate::captured::Unavailable> {
        if state.leases.get(lease).is_some_and(|entry| !entry.retired) {
            Ok(())
        } else {
            Err(failure("source_changed", "The preview request was retired"))
        }
    }

    fn attach(state: &mut State, lease: &str, key: &str) {
        state
            .leases
            .get_mut(lease)
            .unwrap()
            .tokens
            .insert(key.to_owned());
        state
            .references
            .entry(key.to_owned())
            .or_default()
            .insert(lease.to_owned());
    }

    fn detach(state: &mut State, lease: &str, key: &str) {
        if let Some(entry) = state.leases.get_mut(lease) {
            entry.tokens.remove(key);
        }
        if let Some(references) = state.references.get_mut(key) {
            references.remove(lease);
            if references.is_empty() {
                if let Some(path) = state.copies.remove(key) {
                    let _ = fs::remove_file(path);
                }
                state.total = state
                    .total
                    .saturating_sub(state.sizes.remove(key).unwrap_or(0));
                state.references.remove(key);
            }
        }
    }

    fn remove_lease(state: &mut State, lease: &str) {
        if let Some(entry) = state.leases.get(lease) {
            let keys: Vec<String> = entry.tokens.iter().cloned().collect();
            for key in keys {
                Self::detach(state, lease, &key);
            }
        }
        state.leases.remove(lease);
    }

    pub fn pin(&self, lease: &str) -> Result<Pin<'_, I, V>, crate::captured::Unavailable> {
        let mut state = self.state.lock().unwrap();
        Self::live(&state, lease)?;
        state.leases.get_mut(lease).unwrap().pins += 1;
        Ok(Pin {
            owner: self,
            lease: lease.to_owned(),
        })
    }

    pub fn retain(&self, lease: &str, key: &str) -> Result<(), crate::captured::Unavailable> {
        let mut state = self.state.lock().unwrap();
        Self::live(&state, lease)?;
        if !state.copies.contains_key(key) {
            return Err(failure(
                "source_changed",
                "The preview resource was retired",
            ));
        }
        Self::attach(&mut state, lease, key);
        Ok(())
    }

    pub fn keep(&self, lease: &str, paths: &[PathBuf]) -> Result<(), crate::captured::Unavailable> {
        let mut state = self.state.lock().unwrap();
        Self::live(&state, lease)?;
        let keys: Vec<String> = state.leases[lease].tokens.iter().cloned().collect();
        for key in keys {
            if !state
                .copies
                .get(&key)
                .is_some_and(|path| paths.contains(path))
            {
                Self::detach(&mut state, lease, &key);
            }
        }
        Ok(())
    }

    pub fn release(&self, lease: &str) -> bool {
        let mut state = self.state.lock().unwrap();
        let Some(entry) = state.leases.get_mut(lease) else {
            return true;
        };
        entry.retired = true;
        if entry.pins == 0 {
            Self::remove_lease(&mut state, lease);
        }
        self.changed.notify_all();
        !state.leases.contains_key(lease)
    }

    pub fn wait_released(&self, lease: &str, timeout: Duration) -> bool {
        let state = self.state.lock().unwrap();
        self.changed
            .wait_timeout_while(state, timeout, |state| state.leases.contains_key(lease))
            .unwrap()
            .0
            .leases
            .get(lease)
            .is_none()
    }

    pub fn capture(
        &self,
        path: &Path,
        key: &str,
        lease: Option<&str>,
    ) -> Result<PathBuf, crate::captured::Unavailable> {
        let lease = lease.unwrap_or(&self.provider_lease);
        let mut state = self.state.lock().unwrap();
        Self::live(&state, lease)?;
        if let Some(copy) = state.copies.get(key).cloned() {
            Self::attach(&mut state, lease, key);
            return Ok(copy);
        }
        let changed = || failure("source_changed", "The selected preview changed");
        let path_handle = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_PATH | libc::O_NOFOLLOW)
            .open(path)
            .map_err(|_| changed())?;
        let info = path_handle.metadata().map_err(|_| changed())?;
        if !info.is_file() {
            return Err(failure(
                "unsupported",
                "This resource has no supported preview",
            ));
        }
        if info.len() > MAX_SOURCE {
            return Err(failure(
                "policy_limit",
                "This preview supports files up to 64 MiB",
            ));
        }
        if (self.identity)(path, &info).map_err(|_| changed())? != key {
            return Err(changed());
        }
        use std::os::fd::AsRawFd;
        let reopened = PathBuf::from(format!("/proc/self/fd/{}", path_handle.as_raw_fd()));
        let mut source = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NONBLOCK)
            .open(reopened)
            .map_err(|_| changed())?;
        if (self.identity)(path, &source.metadata().map_err(|_| changed())?)
            .map_err(|_| changed())?
            != key
        {
            return Err(changed());
        }
        if state.total.saturating_add(info.len()) > MAX_WORKING_SET {
            return Err(failure(
                "policy_limit",
                "The private preview working set is full",
            ));
        }
        if state.directory.is_none() {
            state.directory = Some(private_directory(self.runtime.as_deref())?);
        }
        let dir = state.directory.as_ref().unwrap().path();
        let suffix = path
            .extension()
            .and_then(|value| value.to_str())
            .map(|value| format!(".{value}"))
            .unwrap_or_default();
        let mut temporary = tempfile::Builder::new()
            .suffix(&suffix)
            .tempfile_in(dir)
            .map_err(|_| failure("provider_failure", "Private preview storage is unavailable"))?;
        if temporary
            .as_file()
            .metadata()
            .map_err(|_| changed())?
            .mode()
            & 0o077
            != 0
        {
            return Err(failure(
                "provider_failure",
                "Private preview storage is unavailable",
            ));
        }
        let size = std::io::copy(
            &mut std::io::Read::by_ref(&mut source).take(MAX_SOURCE + 1),
            &mut temporary,
        )
        .map_err(|_| changed())?;
        if size > MAX_SOURCE
            || size != info.len()
            || (self.identity)(path, &source.metadata().map_err(|_| changed())?)
                .map_err(|_| changed())?
                != key
        {
            return Err(changed());
        }
        (self.validate)(path, key).map_err(|_| changed())?;
        temporary.flush().map_err(|_| changed())?;
        fs::set_permissions(temporary.path(), fs::Permissions::from_mode(0o400))
            .map_err(|_| changed())?;
        let copy = temporary.into_temp_path().keep().map_err(|_| changed())?;
        state.total += size;
        state.sizes.insert(key.to_owned(), size);
        state.copies.insert(key.to_owned(), copy.clone());
        Self::attach(&mut state, lease, key);
        Ok(copy)
    }

    pub fn close(&self) -> Result<(), crate::captured::Unavailable> {
        let mut state = self.state.lock().unwrap();
        if state.leases.values().any(|value| value.pins > 0) {
            return Err(failure(
                "provider_failure",
                "Preview workers are still using resources",
            ));
        }
        state.copies.clear();
        state.sizes.clear();
        state.references.clear();
        state.leases.clear();
        state.total = 0;
        state.directory.take();
        Ok(())
    }
}

pub struct Pin<'a, I, V>
where
    I: Fn(&Path, &Metadata) -> Result<String, String> + Send + Sync,
    V: Fn(&Path, &str) -> Result<(), String> + Send + Sync,
{
    owner: &'a Snapshots<I, V>,
    lease: String,
}

impl<I, V> Drop for Pin<'_, I, V>
where
    I: Fn(&Path, &Metadata) -> Result<String, String> + Send + Sync,
    V: Fn(&Path, &str) -> Result<(), String> + Send + Sync,
{
    fn drop(&mut self) {
        let mut state = self.owner.state.lock().unwrap();
        if let Some(entry) = state.leases.get_mut(&self.lease) {
            entry.pins -= 1;
            if entry.pins == 0 && entry.retired {
                Snapshots::<I, V>::remove_lease(&mut state, &self.lease);
            }
        }
        self.owner.changed.notify_all();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::MetadataExt;

    fn identity(_: &Path, info: &Metadata) -> Result<String, String> {
        Ok(format!(
            "{}:{}:{}:{}",
            info.dev(),
            info.ino(),
            info.len(),
            info.ctime_nsec()
        ))
    }
    fn validate(path: &Path, expected: &str) -> Result<(), String> {
        if identity(
            path,
            &fs::symlink_metadata(path).map_err(|error| error.to_string())?,
        )? == expected
        {
            Ok(())
        } else {
            Err("changed".into())
        }
    }

    #[test]
    fn retained_copy_survives_release_until_last_ui_lease() {
        let runtime = tempfile::tempdir().unwrap();
        fs::set_permissions(runtime.path(), fs::Permissions::from_mode(0o700)).unwrap();
        let source = runtime.path().join("source.txt");
        fs::write(&source, "private fixture").unwrap();
        let key = identity(&source, &fs::metadata(&source).unwrap()).unwrap();
        let snapshots =
            Snapshots::new_with_runtime(identity, validate, Some(runtime.path().to_owned()))
                .unwrap();
        let one = snapshots.begin().unwrap();
        let two = snapshots.begin().unwrap();
        let copy = snapshots.capture(&source, &key, Some(&one)).unwrap();
        snapshots.retain(&two, &key).unwrap();
        assert_eq!(fs::read_to_string(&copy).unwrap(), "private fixture");
        assert!(snapshots.release(&one));
        assert!(copy.exists());
        assert!(snapshots.release(&two));
        assert!(!copy.exists());
    }

    #[test]
    fn pin_drains_after_retirement() {
        let snapshots = Snapshots::new(identity, validate).unwrap();
        let lease = snapshots.begin().unwrap();
        let guard = snapshots.pin(&lease).unwrap();
        assert!(!snapshots.release(&lease));
        assert!(!snapshots.wait_released(&lease, Duration::from_millis(1)));
        drop(guard);
        assert!(snapshots.wait_released(&lease, Duration::from_millis(1)));
    }

    #[test]
    fn stale_snapshot_owner_is_pruned_without_touching_live_or_unrelated_directories() {
        let runtime = tempfile::tempdir().unwrap();
        fs::set_permissions(runtime.path(), fs::Permissions::from_mode(0o700)).unwrap();
        let base = runtime
            .path()
            .join(format!("lumen-preview-snapshots-{}", unsafe {
                libc::geteuid()
            }));
        fs::create_dir(&base).unwrap();
        fs::set_permissions(&base, fs::Permissions::from_mode(0o700)).unwrap();
        let pid = std::process::id();
        let start = process_start(pid).unwrap().unwrap();
        let live = base.join(format!("{pid}-{start}-live0001"));
        let stale = base.join(format!("{pid}-{}-dead0001", start + 1));
        let unrelated = base.join("unrelated");
        for path in [&live, &stale, &unrelated] {
            fs::create_dir(path).unwrap();
        }
        fs::write(live.join("leased"), "live").unwrap();
        let source = runtime.path().join("source.txt");
        fs::write(&source, "source").unwrap();
        let key = identity(&source, &fs::metadata(&source).unwrap()).unwrap();
        let snapshots =
            Snapshots::new_with_runtime(identity, validate, Some(runtime.path().to_owned()))
                .unwrap();
        let retained = snapshots.capture(&source, &key, None).unwrap();
        assert!(retained.exists());
        assert!(live.exists());
        assert!(!stale.exists());
        assert!(unrelated.exists());
        assert_eq!(fs::read_to_string(live.join("leased")).unwrap(), "live");
        snapshots.close().unwrap();
    }
}
