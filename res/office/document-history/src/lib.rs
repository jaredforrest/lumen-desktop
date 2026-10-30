//! Bounded, descriptor-anchored local snapshots with explicit archive protection.
use aes_gcm::{
    Aes256Gcm, Nonce,
    aead::{Aead, KeyInit, Payload},
};
use argon2::{Algorithm, Argon2, Params, Version as KdfVersion};
use rand_core::{OsRng, RngCore};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::fs::{self, File, OpenOptions};
use std::io::Read;
use std::os::fd::AsRawFd;
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use zeroize::Zeroizing;
#[path = "../../atomic_output.rs"]
mod atomic;
const MAX_BYTES: usize = 32 * 1024 * 1024;
const MAX_VERSIONS: usize = 20;
const MAGIC: &[u8] = b"LUMENHST1";
static NEXT: AtomicU64 = AtomicU64::new(0);
/// Transient archive credential: never serialized and redacted in diagnostics.
#[derive(Clone, Default)]
pub struct Password(Zeroizing<String>);
impl Password {
    pub fn new(value: impl Into<String>) -> Self {
        Self(Zeroizing::new(value.into()))
    }
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
    pub fn as_mut_string(&mut self) -> &mut String {
        &mut self.0
    }
}
impl std::fmt::Debug for Password {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("[redacted]")
    }
}
type Result<T> = std::result::Result<T, String>;
fn err(error: impl std::fmt::Display) -> String {
    format!("Document history: {error}")
}
#[derive(Clone, Serialize, Deserialize)]
struct Snapshot {
    id: String,
    label: String,
    date: String,
    summary: String,
    document: Value,
}
#[derive(Clone, Serialize, Deserialize)]
struct Archive {
    schema: u32,
    provider: String,
    document: String,
    versions: Vec<Snapshot>,
}
pub struct Store {
    path: PathBuf,
    directory: File,
    _lock: File,
    output: atomic::CapturedOutput,
    archive: Archive,
    key: Option<Zeroizing<String>>,
    protected: bool,
}
fn private(info: &fs::Metadata) -> Result<()> {
    let uid = fs::metadata("/proc/self").map_err(err)?.uid();
    if info.uid() != uid || info.mode() & 0o077 != 0 {
        return Err(err("history must be owned by this user and private"));
    }
    Ok(())
}
fn derive(password: &str, salt: &[u8]) -> Result<Zeroizing<[u8; 32]>> {
    let params = Params::new(19456, 2, 1, Some(32)).map_err(err)?;
    let mut key = Zeroizing::new([0u8; 32]);
    Argon2::new(Algorithm::Argon2id, KdfVersion::V0x13, params)
        .hash_password_into(password.as_bytes(), salt, key.as_mut())
        .map_err(err)?;
    Ok(key)
}
fn encode(plain: &[u8], password: Option<&str>) -> Result<Vec<u8>> {
    let Some(password) = password else {
        return Ok(plain.to_vec());
    };
    let mut salt = [0u8; 16];
    let mut nonce = [0u8; 12];
    OsRng.try_fill_bytes(&mut salt).map_err(err)?;
    OsRng.try_fill_bytes(&mut nonce).map_err(err)?;
    let key = derive(password, &salt)?;
    let cipher = Aes256Gcm::new_from_slice(key.as_ref()).map_err(err)?;
    let ciphertext = cipher
        .encrypt(
            Nonce::from_slice(&nonce),
            Payload {
                msg: plain,
                aad: MAGIC,
            },
        )
        .map_err(|_| err("archive encryption failed"))?;
    Ok([MAGIC, &salt, &nonce, &ciphertext].concat())
}
fn decode(bytes: Vec<u8>, password: Option<&str>) -> Result<(Zeroizing<Vec<u8>>, bool)> {
    if !bytes.starts_with(MAGIC) {
        return Ok((Zeroizing::new(bytes), false));
    }
    let password = password
        .filter(|key| !key.is_empty())
        .ok_or_else(|| err("protected archive needs its history password"))?;
    let offset = MAGIC.len();
    if bytes.len() < offset + 16 + 12 + 16 {
        return Err(err("protected archive is damaged"));
    }
    let key = derive(password, &bytes[offset..offset + 16])?;
    let cipher = Aes256Gcm::new_from_slice(key.as_ref()).map_err(err)?;
    let plain = cipher
        .decrypt(
            Nonce::from_slice(&bytes[offset + 16..offset + 28]),
            Payload {
                msg: &bytes[offset + 28..],
                aad: MAGIC,
            },
        )
        .map_err(|_| err("history password is incorrect or protected archive is damaged"))?;
    Ok((Zeroizing::new(plain), true))
}
impl Store {
    pub fn open(
        path: &Path,
        provider: &str,
        password: Option<&str>,
        create: bool,
    ) -> Result<Option<Self>> {
        if !fs::symlink_metadata(path).map_err(err)?.is_file() {
            return Err(err(
                "history requires a regular saved document, not an alias or special file",
            ));
        }
        let document = fs::canonicalize(path).map_err(err)?;
        let document_name = document
            .to_str()
            .ok_or_else(|| err("history requires a UTF-8 saved document path"))?;
        let mut name = std::ffi::OsString::from(".");
        name.push(
            document
                .file_name()
                .ok_or_else(|| err("document filename unavailable"))?,
        );
        name.push(".lumen-history");
        let path = document.with_file_name(name);
        match fs::symlink_metadata(&path) {
            Ok(info) => {
                if !info.is_dir() {
                    return Err(err("history path is not a private directory"));
                }
                private(&info)?;
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound && !create => {
                return Ok(None);
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => fs::DirBuilder::new()
                .mode(0o700)
                .create(&path)
                .map_err(err)?,
            Err(error) => return Err(err(error)),
        }
        let directory = OpenOptions::new()
            .read(true)
            .custom_flags(0o200000 | 0o400000)
            .open(&path)
            .map_err(err)?;
        private(&directory.metadata().map_err(err)?)?;
        let anchored = PathBuf::from(format!("/proc/self/fd/{}", directory.as_raw_fd()));
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .custom_flags(0o400000 | 0o4000)
            .open(anchored.join("lock"))
            .map_err(err)?;
        let info = lock.metadata().map_err(err)?;
        private(&info)?;
        if !info.is_file() {
            return Err(err("history lock is not regular"));
        }
        lock.try_lock().map_err(|error| match error {
            std::fs::TryLockError::WouldBlock => {
                err("another history operation is active; retry after it finishes")
            }
            std::fs::TryLockError::Error(error) => {
                err(format!("history locking is unavailable: {error}"))
            }
        })?;
        let output = atomic::capture_output(&anchored.join("archive"))?;
        let (archive, protected) = match OpenOptions::new()
            .read(true)
            .custom_flags(0o400000 | 0o4000)
            .open(output.path())
        {
            Ok(mut file) => {
                let info = file.metadata().map_err(err)?;
                private(&info)?;
                if !info.is_file() || info.len() > MAX_BYTES as u64 {
                    return Err(err("history archive is unsupported or exceeds 32 MiB"));
                }
                let mut bytes = Vec::new();
                file.by_ref()
                    .take(MAX_BYTES as u64 + 1)
                    .read_to_end(&mut bytes)
                    .map_err(err)?;
                if bytes.len() > MAX_BYTES {
                    return Err(err("history archive grew beyond 32 MiB"));
                }
                let (plain, protected) = decode(bytes, password)?;
                let archive: Archive = serde_json::from_slice(&plain)
                    .map_err(|_| err("history archive is damaged or unsupported"))?;
                (archive, protected)
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => (
                Archive {
                    schema: 1,
                    provider: provider.into(),
                    document: document_name.into(),
                    versions: Vec::new(),
                },
                false,
            ),
            Err(error) => return Err(err(error)),
        };
        if archive.schema != 1
            || archive.provider != provider
            || archive.document != document_name
            || archive.versions.len() > MAX_VERSIONS
        {
            return Err(err(
                "history belongs to a different document/provider or unsupported schema",
            ));
        }
        for (index, version) in archive.versions.iter().enumerate() {
            if version.id.is_empty()
                || version.id.len() > 100
                || !version
                    .id
                    .bytes()
                    .all(|ch| ch.is_ascii_hexdigit() || ch == b'-')
                || archive.versions[..index]
                    .iter()
                    .any(|other| other.id == version.id)
            {
                return Err(err("history version identities are invalid"));
            }
        }
        Ok(Some(Self {
            path,
            directory,
            _lock: lock,
            output,
            archive,
            key: password
                .filter(|key| !key.is_empty())
                .map(|key| Zeroizing::new(key.to_string())),
            protected,
        }))
    }
    pub fn protected(&self) -> bool {
        self.protected
    }
    pub fn rows(&self) -> Value {
        Value::Array(
            self.archive
                .versions
                .iter()
                .rev()
                .map(|v| json!({"id":v.id,"label":v.label,"date":v.date,"summary":v.summary}))
                .collect(),
        )
    }
    pub fn version(&self, id: &str) -> Result<Value> {
        self.archive
            .versions
            .iter()
            .find(|version| version.id == id)
            .map(|version| version.document.clone())
            .ok_or_else(|| err("the selected version no longer exists"))
    }
    pub fn capture(
        &mut self,
        document: Value,
        label: &str,
        summary: &str,
        password: Option<&str>,
    ) -> Result<String> {
        let now = std::time::SystemTime::now();
        let nanos = now
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(err)?
            .as_nanos();
        let id = format!(
            "{nanos:x}-{:x}-{:x}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        );
        let date = chrono::DateTime::<chrono::Utc>::from(now)
            .to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
        let mut candidate = self.archive.clone();
        candidate.versions.push(Snapshot {
            id: id.clone(),
            label: label.chars().take(80).collect(),
            date,
            summary: summary.chars().take(160).collect(),
            document,
        });
        while candidate.versions.len() > MAX_VERSIONS {
            candidate.versions.remove(0);
        }
        let key = password
            .filter(|key| !key.is_empty())
            .or_else(|| {
                self.protected
                    .then(|| self.key.as_ref().map(|key| key.as_str()))
                    .flatten()
            })
            .map(|key| Zeroizing::new(key.to_owned()));
        let named = fs::symlink_metadata(&self.path).map_err(err)?;
        let owned = self.directory.metadata().map_err(err)?;
        private(&named)?;
        if !named.is_dir() || (named.dev(), named.ino()) != (owned.dev(), owned.ino()) {
            return Err(err("the captured history directory changed"));
        }
        loop {
            let plain = Zeroizing::new(serde_json::to_vec(&candidate).map_err(err)?);
            let bytes = encode(&plain, key.as_ref().map(|key| key.as_str()))?;
            if bytes.len() <= MAX_BYTES {
                atomic::write_captured(&self.output, &bytes)?;
                self.directory.sync_all().map_err(err)?;
                self.output = atomic::capture_output(self.output.path())?;
                self.archive = candidate;
                self.protected = key.is_some();
                self.key = key;
                return Ok(id);
            }
            if candidate.versions.len() <= 1 {
                return Err(err(
                    "one snapshot exceeds the 32 MiB archive limit; no history was replaced",
                ));
            }
            candidate.versions.remove(0);
        }
    }
}
/// Shared captured-output publication is also available to provider adapters.
pub fn publish_file(path: &Path, bytes: &[u8]) -> Result<()> {
    atomic::write(path, bytes)
}

#[cfg(test)]
mod store_tests {
    use super::*;
    fn fixture() -> (tempfile::TempDir, PathBuf) {
        let root = tempfile::tempdir().unwrap();
        let file = root.path().join("source.document");
        fs::write(&file, b"original source").unwrap();
        (root, file)
    }
    fn open(path: &Path, key: Option<&str>) -> Store {
        Store::open(path, "test", key, true).unwrap().unwrap()
    }
    #[test]
    fn full_document_reopen_and_reused_store_preserve_source() {
        let (_root, path) = fixture();
        let mut store = open(&path, None);
        let doc = json!({"cells":[{"formula":"=A1","media":[0,255,17]}],"notes":"Retained"});
        let id = store
            .capture(doc.clone(), "Original", "one item", None)
            .unwrap();
        store
            .capture(json!({"later":true}), "Later", "two items", None)
            .unwrap();
        drop(store);
        let store = open(&path, None);
        assert_eq!(store.version(&id).unwrap(), doc);
        assert_eq!(store.rows().as_array().unwrap().len(), 2);
        assert_eq!(fs::read(&path).unwrap(), b"original source");
        assert!(store.version("missing").is_err());
    }
    #[test]
    fn whole_archive_encryption_wrong_key_tamper_and_protection_retention() {
        let (_root, path) = fixture();
        let mut store = open(&path, None);
        let id = store
            .capture(
                json!({"secret":"Prior plaintext content"}),
                "Prior label",
                "one",
                None,
            )
            .unwrap();
        store
            .capture(
                json!({"protected":true}),
                "Protected label",
                "two",
                Some("Synthetic archive password"),
            )
            .unwrap();
        let file = store.path.join("archive");
        let bytes = fs::read(&file).unwrap();
        assert!(bytes.starts_with(MAGIC));
        assert!(!bytes.windows(11).any(|part| part == b"Prior label"));
        drop(store);
        assert!(Store::open(&path, "test", None, false).is_err());
        assert!(Store::open(&path, "test", Some("wrong"), false).is_err());
        let mut store = open(&path, Some("Synthetic archive password"));
        assert_eq!(
            store.version(&id).unwrap()["secret"],
            "Prior plaintext content"
        );
        store
            .capture(json!({"later":true}), "Later", "three", None)
            .unwrap();
        assert!(store.protected());
        drop(store);
        let mut bytes = fs::read(&file).unwrap();
        let last = bytes.len() - 1;
        bytes[last] ^= 1;
        fs::write(&file, bytes).unwrap();
        assert!(Store::open(&path, "test", Some("Synthetic archive password"), false).is_err());
    }
    #[test]
    fn schema_provider_ids_alias_permissions_and_busy_archive_refuse() {
        let (root, path) = fixture();
        let mut store = open(&path, None);
        store.capture(json!({}), "Original", "one", None).unwrap();
        assert!(Store::open(&path, "test", None, false).is_err());
        let dir = store.path.clone();
        drop(store);
        assert!(Store::open(&path, "other", None, false).is_err());
        let alias = root.path().join("alias");
        std::os::unix::fs::symlink(&path, &alias).unwrap();
        assert!(Store::open(&alias, "test", None, false).is_err());
        let file = dir.join("archive");
        let saved = fs::read(&file).unwrap();
        let mut invalid: Value = serde_json::from_slice(&saved).unwrap();
        invalid["versions"][0]["id"] = json!("");
        fs::write(&file, serde_json::to_vec(&invalid).unwrap()).unwrap();
        assert!(Store::open(&path, "test", None, false).is_err());
        fs::write(&file, &saved).unwrap();
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&file, fs::Permissions::from_mode(0o644)).unwrap();
        assert!(Store::open(&path, "test", None, false).is_err());
        fs::set_permissions(&file, fs::Permissions::from_mode(0o600)).unwrap();
        assert!(Store::open(&path, "test", None, false).is_ok());
    }
    #[test]
    fn changed_archive_and_retargeted_directory_are_not_overwritten() {
        let (root, path) = fixture();
        let mut store = open(&path, None);
        store.capture(json!({}), "Original", "one", None).unwrap();
        let archive = store.path.join("archive");
        fs::write(&archive, b"foreign changed entry").unwrap();
        assert!(store.capture(json!({}), "Later", "two", None).is_err());
        assert_eq!(fs::read(&archive).unwrap(), b"foreign changed entry");
        drop(store);
        fs::remove_file(&archive).unwrap();
        let mut store = open(&path, None);
        let moved = root.path().join("moved");
        fs::rename(&store.path, &moved).unwrap();
        fs::DirBuilder::new()
            .mode(0o700)
            .create(&store.path)
            .unwrap();
        assert!(store.capture(json!({}), "Later", "one", None).is_err());
        assert!(!store.path.join("archive").exists() && !moved.join("archive").exists());
    }
    #[test]
    fn count_and_byte_eviction_and_oversized_failure_are_transactional() {
        let (_root, path) = fixture();
        let mut store = open(&path, None);
        let first = store.capture(json!({"i":0}), "First", "one", None).unwrap();
        for i in 1..=20 {
            store.capture(json!({"i":i}), "Later", "one", None).unwrap();
        }
        assert_eq!(store.rows().as_array().unwrap().len(), 20);
        assert!(store.version(&first).is_err());
        let large = json!({"text":"x".repeat(17*1024*1024)});
        store
            .capture(large.clone(), "Large one", "one", None)
            .unwrap();
        store.capture(large, "Large two", "one", None).unwrap();
        assert_eq!(store.rows().as_array().unwrap().len(), 1);
        let saved = fs::read(store.path.join("archive")).unwrap();
        let rows = store.rows();
        assert!(
            store
                .capture(
                    json!({"text":"X".repeat(MAX_BYTES)}),
                    "Oversized",
                    "one",
                    None
                )
                .unwrap_err()
                .contains("one snapshot exceeds")
        );
        assert_eq!(store.rows(), rows);
        assert_eq!(fs::read(store.path.join("archive")).unwrap(), saved);
        assert_eq!(fs::read(&path).unwrap(), b"original source");
    }
}
