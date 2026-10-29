//! Explicit bounded per-document local history, independent of the source file.
use std::fs::{self, File, OpenOptions};
use std::io::Read;
use std::os::fd::AsRawFd;
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use crate::{CmdError, CmdResult, Session};
const MAX_BYTES: u64 = 32 * 1024 * 1024;
const MAX_VERSIONS: usize = 20;
static NEXT: AtomicU64 = AtomicU64::new(0);
#[derive(Serialize, Deserialize)]
struct Version { id: String, label: String, date: String, document: wordcraft_doc::Document }
#[derive(Serialize, Deserialize)]
struct Archive { schema: u32, document: String, versions: Vec<Version> }
struct Store { directory_path: PathBuf, directory: File, _lock: File, output: crate::CapturedOutput, archive: Archive, encrypted: bool }
fn failed(error: impl std::fmt::Display) -> CmdError { CmdError::Failed(format!("Document history: {error}")) }
fn lock_failure(error: std::fs::TryLockError) -> CmdError {
    match error {
        std::fs::TryLockError::WouldBlock => failed("another history operation is active; retry after it finishes"),
        std::fs::TryLockError::Error(error) => failed(format!("history locking is unavailable: {error}")),
    }
}
fn user_owned(info: &fs::Metadata) -> Result<(), CmdError> {
    let uid = fs::metadata("/proc/self").map_err(failed)?.uid();
    if info.uid() != uid || info.mode() & 0o077 != 0 { return Err(failed("history must be owned by this user and private")); }
    Ok(())
}
fn canonical_document(session: &Session) -> Result<PathBuf, CmdError> {
    let path = session.path.as_ref().ok_or_else(|| failed("save the document before using persistent history"))?;
    let info = fs::symlink_metadata(path).map_err(failed)?;
    if !info.is_file() { return Err(failed("history requires a regular saved document, not an alias or special file")); }
    let canonical=fs::canonicalize(path).map_err(failed)?;
    if canonical.to_str().is_none(){return Err(failed("history requires a UTF-8 saved document path"));}
    Ok(canonical)
}
fn password<'a>(session: &'a Session, request: &'a Value) -> Option<&'a str> {
    request.get("historyPassword").and_then(Value::as_str).filter(|value| !value.is_empty())
        .or_else(|| session.password.as_ref().map(crate::Password::as_str))
}
impl Store {
    fn open(session: &Session, request: &Value, create: bool) -> Result<Option<Self>, CmdError> {
        let document = canonical_document(session)?;
        let mut name = std::ffi::OsString::from(".");
        name.push(document.file_name().ok_or_else(|| failed("document filename is unavailable"))?);
        name.push(".lumen-history");
        let directory_path = document.with_file_name(name);
        match fs::symlink_metadata(&directory_path) {
            Ok(info) => { if !info.is_dir() { return Err(failed("history path is not a private directory")); } user_owned(&info)?; }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound && !create => return Ok(None),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => fs::DirBuilder::new().mode(0o700).create(&directory_path).map_err(failed)?,
            Err(error) => return Err(failed(error)),
        }
        let directory = OpenOptions::new().read(true).custom_flags(0o200000 | 0o400000).open(&directory_path).map_err(failed)?;
        user_owned(&directory.metadata().map_err(failed)?)?;
        let anchored = PathBuf::from(format!("/proc/self/fd/{}", directory.as_raw_fd()));
        let lock = OpenOptions::new().read(true).write(true).create(true).truncate(false).mode(0o600).custom_flags(0o400000 | 0o4000).open(anchored.join("lock")).map_err(failed)?;
        let info = lock.metadata().map_err(failed)?;user_owned(&info)?;
        if !info.is_file() { return Err(failed("history lock is not a regular file")); }
        lock.try_lock().map_err(lock_failure)?;
        let path = anchored.join("archive");
        let output=crate::capture_output(&path).map_err(failed)?;
        let (archive, encrypted) = match OpenOptions::new().read(true).custom_flags(0o400000 | 0o4000).open(&path) {
            Ok(mut file) => {
                let info = file.metadata().map_err(failed)?;user_owned(&info)?;
                if !info.is_file() || info.len() > MAX_BYTES { return Err(failed("history archive is unsupported or exceeds 32 MiB")); }
                let mut bytes = Vec::new();file.by_ref().take(MAX_BYTES+1).read_to_end(&mut bytes).map_err(failed)?;
                if bytes.len() as u64 > MAX_BYTES { return Err(failed("history archive grew beyond 32 MiB")); }
                let encrypted = wordcraft_docx::is_encrypted(&bytes);
                let plain = if encrypted { zeroize::Zeroizing::new(wordcraft_docx::decrypt(&bytes, password(session, request)).map_err(|_| failed("history needs its capture-time password, or its protected archive is damaged"))?) } else { zeroize::Zeroizing::new(bytes) };
                let archive: Archive = serde_json::from_slice(&plain).map_err(|_| failed("history archive is damaged or unsupported"))?;
                (archive, encrypted)
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => (Archive { schema:1, document:document.to_string_lossy().into_owned(), versions:Vec::new() }, false),
            Err(error) => return Err(failed(error)),
        };
        if archive.schema != 1 || archive.document != document.to_string_lossy() || archive.versions.len() > MAX_VERSIONS { return Err(failed("history belongs to a different document or unsupported schema")); }
        for (index, version) in archive.versions.iter().enumerate() {
            if version.id.is_empty() || version.id.len()>100 || !version.id.bytes().all(|ch| ch.is_ascii_hexdigit() || ch==b'-') || archive.versions[..index].iter().any(|other| other.id==version.id) { return Err(failed("history version identities are invalid")); }
        }
        Ok(Some(Self { directory_path, directory, _lock:lock, output, archive, encrypted }))
    }
    fn rows(&self) -> Value { Value::Array(self.archive.versions.iter().rev().map(|version| json!({"id":version.id,"label":version.label,"date":version.date,"words":version.document.word_count()})).collect()) }
    fn append(&mut self, session: &Session, label: &str) -> Result<(), CmdError> {
        let time = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_err(failed)?;
        let id = format!("{:x}-{:x}-{:x}",time.as_nanos(),std::process::id(),NEXT.fetch_add(1,Ordering::Relaxed));
        self.archive.versions.push(Version { id,label:label.chars().take(80).collect(),date:crate::cmd::now_iso(),document:session.doc.clone() });
        while self.archive.versions.len()>MAX_VERSIONS { self.archive.versions.remove(0); }
        Ok(())
    }
    fn publish(&mut self, session: &Session, request: &Value) -> Result<(), CmdError> {
        let named=fs::symlink_metadata(&self.directory_path).map_err(failed)?;
        let owned=self.directory.metadata().map_err(failed)?;
        user_owned(&named)?;
        if !named.is_dir() || (named.dev(),named.ino())!=(owned.dev(),owned.ino()) {return Err(failed("the captured history directory changed"));}
        let key = session.password.as_ref().map(crate::Password::as_str).or_else(|| self.encrypted.then(||password(session,request)).flatten());
        loop {
            let plain = zeroize::Zeroizing::new(serde_json::to_vec(&self.archive).map_err(failed)?);
            let bytes = if let Some(key)=key { wordcraft_docx::encrypt(&plain,key).map_err(failed)? } else { plain.to_vec() };
            if bytes.len() as u64 <= MAX_BYTES {
                crate::write_captured(&self.output,&bytes).map_err(failed)?;
                self.encrypted=key.is_some();self.directory.sync_all().map_err(failed)?;return Ok(());
            }
            if self.archive.versions.len()<=1 { return Err(failed("one snapshot exceeds the 32 MiB archive limit; no history was replaced")); }
            self.archive.versions.remove(0);
        }
    }
}
pub(crate) fn command(session: &mut Session, request: &Value) -> CmdResult {
    if request.get("action").is_none(){session.ui_requests.push(json!({"open":"history"}));}
    let action=request.get("action").and_then(Value::as_str).unwrap_or("list");
    if !matches!(action,"list"|"capture"|"preview"|"restore") { return Err(failed("unknown history action")); }
    if action=="restore" {
        if request.get("document").and_then(Value::as_u64)!=Some(session.document_id()) || request.get("revision").and_then(Value::as_u64)!=Some(session.rev()) { return Err(failed("the captured document changed; reopen history")); }
        if session.dirty && request.get("allowDiscard").and_then(Value::as_bool)!=Some(true) { return Err(failed("confirm replacement of unsaved changes before restoring")); }
    }
    let Some(mut store)=Store::open(session,request,action=="capture")? else { return if matches!(action,"list") { Ok(json!({"versions":[],"protected":false,"limit":MAX_VERSIONS,"document":session.document_id(),"revision":session.rev()})) } else { Err(failed("this saved document has no persistent history")) }; };
    if action=="preview" || action=="restore" {
        let id=request.get("id").and_then(Value::as_str).ok_or_else(||failed("select a version identity"))?;
        let version=store.archive.versions.iter().find(|version|version.id==id).ok_or_else(||failed("the selected version no longer exists"))?;
        if action=="preview" { return Ok(json!({"id":version.id,"label":version.label,"text":version.document.plain_text(wordcraft_doc::StoryRef::Body).chars().take(2000).collect::<String>()})); }
        let mut document=version.document.clone();document.ensure_nonempty();
        store.append(session,"Before restore")?;store.publish(session,request)?;
        session.checkpoint("Restore persistent version");session.doc=document;session.touch();session.clamp_selection();
        return Ok(json!({"restored":id,"dirty":true,"sourceFileUnchanged":true}));
    }
    if action=="capture" { let label=request.get("label").and_then(Value::as_str).unwrap_or("Saved version");store.append(session,label)?;store.publish(session,request)?; }
    Ok(json!({"versions":store.rows(),"protected":store.encrypted,"limit":MAX_VERSIONS,"document":session.document_id(),"revision":session.rev()}))
}

#[cfg(test)]
mod persistent_history_tests {
    use super::*;
    use wordcraft_doc::{Document, StoryRef};
    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self { let root=std::env::temp_dir().join(format!("history-{}-{}",std::process::id(),NEXT.fetch_add(1,Ordering::Relaxed)));fs::create_dir(&root).unwrap();Self(root) }
        fn session(&self)->Session { let path=self.0.join("document.docx");let mut session=Session::new(Document::from_text("Original document"));session.run("file.save",&json!({"path":path})).unwrap();session }
        fn directory(&self)->PathBuf { self.0.join(".document.docx.lumen-history") }
    }
    impl Drop for Fixture {fn drop(&mut self){fs::remove_dir_all(&self.0).unwrap();}}
    fn capture(session:&mut Session,label:&str)->String {let value=command(session,&json!({"action":"capture","label":label})).unwrap();value["versions"][0]["id"].as_str().unwrap().to_string()}
    fn restore_request(session:&Session,id:&str)->Value {json!({"action":"restore","id":id,"document":session.document_id(),"revision":session.rev(),"allowDiscard":true})}
    #[test]
    fn persistent_history_lock_failure_distinguishes_contention_and_io(){
        let busy=lock_failure(std::fs::TryLockError::WouldBlock).to_string();assert!(busy.contains("another history operation"));
        for kind in [std::io::ErrorKind::Unsupported,std::io::ErrorKind::PermissionDenied] {let error=lock_failure(std::fs::TryLockError::Error(std::io::Error::from(kind))).to_string();assert!(error.contains("locking is unavailable"));assert!(!error.contains("another history operation"));}
    }
    #[test]
    fn persistent_history_reopens_and_restores_without_overwriting_source_or_losing_undo(){
        let root=Fixture::new();let mut first=root.session();let path=first.path.clone().unwrap();let original=fs::read(&path).unwrap();let id=capture(&mut first,"Original captured");
        drop(first);let mut second=Session::new(Document::new());second.run("file.open",&json!({"path":path})).unwrap();assert_eq!(command(&mut second,&json!({})).unwrap()["versions"][0]["id"],id);
        second.run("text.insert",&json!({"text":"Unsaved edits "})).unwrap();let before=second.doc.plain_text(StoryRef::Body);let request=restore_request(&second,&id);command(&mut second,&request).unwrap();assert_eq!(second.doc.plain_text(StoryRef::Body),"Original document");assert!(second.dirty);assert_eq!(fs::read(&path).unwrap(),original);
        let rows=command(&mut second,&json!({})).unwrap();assert_eq!(rows["versions"][0]["label"],"Before restore");assert_eq!(rows["versions"].as_array().unwrap().len(),2);second.run("edit.undo",&json!({})).unwrap();assert_eq!(second.doc.plain_text(StoryRef::Body),before);
    }
    #[test]
    fn persistent_history_invalid_and_stale_restore_preserve_work(){
        let root=Fixture::new();let mut session=root.session();let id=capture(&mut session,"Original");let stale=restore_request(&session,&id);session.run("text.insert",&json!({"text":"Changed "})).unwrap();let before=serde_json::to_value(&session.doc).unwrap();assert!(command(&mut session,&stale).is_err());
        let invalid=restore_request(&session,"unknown");assert!(command(&mut session,&invalid).is_err());let mut no_consent=restore_request(&session,&id);no_consent["allowDiscard"]=json!(false);assert!(command(&mut session,&no_consent).is_err());assert_eq!(serde_json::to_value(&session.doc).unwrap(),before);
    }
    #[test]
    fn persistent_history_changed_archive_is_not_overwritten(){
        let root=Fixture::new();let mut session=root.session();capture(&mut session,"First");let mut store=Store::open(&session,&json!({}),false).unwrap().unwrap();fs::write(root.directory().join("archive"),b"Newer independently written archive").unwrap();store.append(&session,"Second").unwrap();assert!(store.publish(&session,&json!({})).is_err());assert_eq!(fs::read(root.directory().join("archive")).unwrap(),b"Newer independently written archive");
    }
    #[test]
    fn persistent_history_retention_stable_ids_and_capture_preserves_dirty_metadata(){
        let root=Fixture::new();let mut session=root.session();session.run("text.insert",&json!({"text":"Unsaved "})).unwrap();let before=json!({"doc":session.doc,"path":session.path,"dirty":session.dirty,"rev":session.rev()});let first=capture(&mut session,"First");
        for index in 0..23 {capture(&mut session,&format!("Version {index}"));}
        let value=command(&mut session,&json!({})).unwrap();assert_eq!(value["versions"].as_array().unwrap().len(),MAX_VERSIONS);assert!(!value["versions"].as_array().unwrap().iter().any(|row|row["id"]==first));assert_eq!(before,json!({"doc":session.doc,"path":session.path,"dirty":session.dirty,"rev":session.rev()}));
        assert_eq!(fs::metadata(root.directory()).unwrap().mode()&0o777,0o700);assert_eq!(fs::metadata(root.directory().join("archive")).unwrap().mode()&0o777,0o600);
    }
    #[test]
    fn persistent_history_directory_retarget_does_not_write_either_replacement(){
        use std::os::unix::fs::PermissionsExt;
        let root=Fixture::new();let mut session=root.session();capture(&mut session,"First");let mut store=Store::open(&session,&json!({}),false).unwrap().unwrap();let original=fs::read(root.directory().join("archive")).unwrap();let moved=root.0.join("captured-moved");fs::rename(root.directory(),&moved).unwrap();fs::create_dir(root.directory()).unwrap();fs::set_permissions(root.directory(),fs::Permissions::from_mode(0o700)).unwrap();store.append(&session,"Second").unwrap();assert!(store.publish(&session,&json!({})).is_err());assert_eq!(fs::read(moved.join("archive")).unwrap(),original);assert_eq!(fs::read_dir(root.directory()).unwrap().count(),0);
    }
    #[test]
    fn persistent_history_protection_never_silently_becomes_plaintext(){
        let root=Fixture::new();let mut session=root.session();session.run("file.encrypt",&json!({"password":"Synthetic history password"})).unwrap();capture(&mut session,"Private label");let bytes=fs::read(root.directory().join("archive")).unwrap();assert!(wordcraft_docx::is_encrypted(&bytes));assert!(!bytes.windows(13).any(|window|window==b"Private label"));session.password=None;
        assert!(command(&mut session,&json!({})).is_err());let request=json!({"action":"capture","label":"Another protected version","historyPassword":"Synthetic history password"});let value=command(&mut session,&request).unwrap();assert_eq!(value["protected"],true);assert!(wordcraft_docx::is_encrypted(&fs::read(root.directory().join("archive")).unwrap()));
    }
    #[test]
    fn persistent_history_unsupported_owner_schema_alias_and_busy_archive_refuse(){
        use std::os::unix::fs::{symlink,PermissionsExt};
        let root=Fixture::new();let mut session=root.session();assert_eq!(command(&mut session,&json!({})).unwrap()["versions"],json!([]));assert!(!root.directory().exists());capture(&mut session,"First");
        let store=Store::open(&session,&json!({}),false).unwrap().unwrap();assert!(command(&mut session,&json!({})).is_err());drop(store);
        fs::set_permissions(root.directory(),fs::Permissions::from_mode(0o755)).unwrap();assert!(command(&mut session,&json!({})).is_err());fs::set_permissions(root.directory(),fs::Permissions::from_mode(0o700)).unwrap();
        let path=root.directory().join("archive");let original=fs::read(&path).unwrap();let mut archive:Value=serde_json::from_slice(&original).unwrap();archive["document"]=json!("Other captured document");fs::write(&path,serde_json::to_vec(&archive).unwrap()).unwrap();assert!(command(&mut session,&json!({})).is_err());fs::write(&path,&original).unwrap();
        let mut archive:Value=serde_json::from_slice(&original).unwrap();archive["versions"][0]["id"]=json!("");fs::write(&path,serde_json::to_vec(&archive).unwrap()).unwrap();assert!(command(&mut session,&json!({})).is_err());
        fs::remove_file(&path).unwrap();symlink(session.path.as_ref().unwrap(),&path).unwrap();assert!(command(&mut session,&json!({})).is_err());
    }
}
