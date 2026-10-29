//! Exclusive descriptor-anchored publication for Linux document providers.
#[cfg(target_os = "linux")]
mod native {
    use std::fs::{self, File, OpenOptions};
    use std::io::{self, Write};
    use std::os::fd::AsRawFd;
    use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    fn identity(info: &fs::Metadata) -> (u64, u64) { (info.dev(), info.ino()) }
    #[derive(PartialEq)]
    struct EntryFacts { identity: (u64, u64), size: u64, modified: (i64, i64), changed: (i64, i64), mode: u32, owner: (u32, u32) }
    impl EntryFacts {
        fn from(info: &fs::Metadata) -> Self { Self { identity: identity(info), size: info.len(), modified: (info.mtime(), info.mtime_nsec()), changed: (info.ctime(), info.ctime_nsec()), mode: info.mode(), owner: (info.uid(), info.gid()) } }
    }
    struct CapturedEntry { handle: File, facts: EntryFacts }
    pub(super) struct Captured { directory: File, entry: Option<CapturedEntry> }
    fn entry(path: &Path) -> io::Result<Option<EntryFacts>> {
        match fs::symlink_metadata(path) { Ok(info) => Ok(Some(EntryFacts::from(&info))), Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None), Err(error) => Err(error) }
    }
    fn open_parent(path: &Path) -> io::Result<File> { OpenOptions::new().read(true).custom_flags(0o200000).open(path) }
    pub(super) fn capture(path: &Path) -> io::Result<Captured> {
        let parent = path.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(Path::new("."));
        let name = path.file_name().ok_or_else(|| io::Error::other("A document filename is required"))?;
        let directory = open_parent(parent)?;
        let target = PathBuf::from(format!("/proc/self/fd/{}", directory.as_raw_fd())).join(name);
        // O_PATH|O_NOFOLLOW retains an entry identity without read-opening a FIFO or device.
        let entry = match OpenOptions::new().read(true).custom_flags(0o10000000 | 0o400000).open(&target) {
            Ok(handle) => {
                let info = handle.metadata()?;
                if !info.is_file() && !info.file_type().is_symlink() { return Err(io::Error::other("A confirmed output must be a regular file or a file link")); }
                Some(CapturedEntry { facts: EntryFacts::from(&info), handle })
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => None,
            Err(error) => return Err(error),
        };
        if identity(&fs::metadata(parent)?) != identity(&directory.metadata()?) { return Err(io::Error::other("The output directory changed")); }
        Ok(Captured { directory, entry })
    }
    impl Captured {
        fn validate(&self, directory: &File, target: &Path) -> io::Result<()> {
            if identity(&directory.metadata()?) != identity(&self.directory.metadata()?) { return Err(io::Error::other("The confirmed output directory changed")); }
            let expected = self.entry.as_ref().map(|value| &value.facts);
            if let Some(value) = &self.entry {
                if EntryFacts::from(&value.handle.metadata()?) != value.facts { return Err(io::Error::other("The confirmed output file changed")); }
            }
            if entry(target)?.as_ref() != expected { return Err(io::Error::other("The confirmed output file changed")); }
            Ok(())
        }
    }
    fn owns(path: &Path, expected: (u64, u64)) -> bool {
        fs::symlink_metadata(path).is_ok_and(|info| info.is_file() && identity(&info) == expected)
    }
    pub(super) fn publish(path: &Path, bytes: &[u8]) -> io::Result<()> {
        publish_with(path, bytes, |_| Ok(()))
    }
    pub(super) fn publish_captured(path: &Path, bytes: &[u8], captured: &Captured) -> io::Result<()> {
        publish_checked(path, bytes, Some(captured), |_| Ok(()))
    }
    fn publish_with(path: &Path, bytes: &[u8], before_commit: impl FnOnce(&Path) -> io::Result<()>) -> io::Result<()> {
        publish_checked(path, bytes, None, before_commit)
    }
    fn publish_checked(path: &Path, bytes: &[u8], captured: Option<&Captured>, before_commit: impl FnOnce(&Path) -> io::Result<()>) -> io::Result<()> {
        let parent = path.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(Path::new("."));
        let name = path.file_name().ok_or_else(|| io::Error::other("A document filename is required"))?;
        // Linux UAPI O_DIRECTORY on the supported x86_64 target rejects FIFOs before opening.
        let directory = open_parent(parent)?;
        let parent_info = directory.metadata()?;
        if !parent_info.is_dir() { return Err(io::Error::other("The output parent is not a directory")); }
        let anchored = PathBuf::from(format!("/proc/self/fd/{}", directory.as_raw_fd()));
        let target = anchored.join(name);
        if let Some(captured) = captured { captured.validate(&directory, &target)?; }
        let permissions = fs::symlink_metadata(&target).ok().filter(|info| info.is_file()).map(|info| info.permissions());
        let stamp = SystemTime::now().duration_since(UNIX_EPOCH).map_err(io::Error::other)?.as_nanos();
        let mut created = None;
        for _ in 0..32 {
            let temporary = anchored.join(format!(".lumen-save-{}-{stamp}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
            match OpenOptions::new().write(true).create_new(true).mode(0o600).open(&temporary) {
                Ok(file) => { created = Some((temporary, file)); break; }
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error),
            }
        }
        let (temporary, mut file) = created.ok_or_else(|| io::Error::other("Unable to reserve an exclusive output file"))?;
        let owned = identity(&file.metadata()?);
        let result = (|| {
            file.write_all(bytes)?;
            if let Some(permissions) = permissions { file.set_permissions(permissions)?; }
            file.sync_all()?;
            before_commit(&temporary)?;
            if !owns(&temporary, owned) { return Err(io::Error::other("The reserved output file changed")); }
            if identity(&fs::metadata(parent)?) != identity(&parent_info) { return Err(io::Error::other("The output directory changed")); }
            if let Some(captured) = captured { captured.validate(&directory, &target)?; }
            fs::rename(&temporary, &target)
        })();
        if result.is_err() && owns(&temporary, owned) { let _ = fs::remove_file(&temporary); }
        result
    }
    #[cfg(test)]
    mod tests {
        use super::*;
        use std::os::unix::fs::{symlink, FileTypeExt, PermissionsExt};
        struct Fixture(PathBuf);
        impl Fixture {
            fn new() -> Self {
                let path = std::env::temp_dir().join(format!("lumen-output-test-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
                fs::create_dir(&path).unwrap(); Self(path)
            }
            fn path(&self, name: &str) -> PathBuf { self.0.join(name) }
        }
        impl Drop for Fixture { fn drop(&mut self) { fs::remove_dir_all(&self.0).unwrap(); } }
        #[test]
        fn fifo_parent_probe() {
            if let Some(parent)=std::env::var_os("LUMEN_ATOMIC_FIFO_PROBE") {
                assert!(fs::symlink_metadata(&parent).unwrap().file_type().is_fifo());
                assert!(publish(&PathBuf::from(parent).join("copy"),b"copy").is_err());
            }
            if let Some(path)=std::env::var_os("LUMEN_CAPTURE_FIFO_PROBE") {
                assert!(fs::symlink_metadata(&path).unwrap().file_type().is_fifo());
                assert!(super::super::capture_output(&PathBuf::from(path)).is_err());
            }
        }
        #[test]
        fn fifo_parent_is_rejected_without_blocking() {
            use std::process::{Command, Stdio};
            use std::time::{Duration, Instant};
            let root=Fixture::new();let fifo=root.path("fifo");assert!(Command::new("mkfifo").arg(&fifo).status().unwrap().success());
            let name=format!("{}::fifo_parent_probe",module_path!().split_once("::").unwrap().1);
            for variable in ["LUMEN_ATOMIC_FIFO_PROBE","LUMEN_CAPTURE_FIFO_PROBE"] {
                let mut child=Command::new(std::env::current_exe().unwrap()).args(["--exact",&name]).env(variable,&fifo).stdout(Stdio::null()).spawn().unwrap();
                let deadline=Instant::now()+Duration::from_secs(2);
                loop {
                    if let Some(status)=child.try_wait().unwrap(){assert!(status.success());break;}
                    if Instant::now()>=deadline { child.kill().unwrap();child.wait().unwrap();assert!(Instant::now()<deadline,"Opening an unsupported output path blocked"); }
                    std::thread::sleep(Duration::from_millis(10));
                }
            }
        }
        #[test]
        fn old_staging_hardlinks_and_symlinks_remain_untouched() {
            let root=Fixture::new(); let original=root.path("original"); fs::write(&original,b"original").unwrap();
            for (index, suffix) in [".tmp", ".sctmp", ".saving"].iter().enumerate() {
                let target=root.path(&format!("copy-{index}.doc")); let alias=root.path(&format!("copy-{index}.doc{suffix}"));
                fs::hard_link(&original,&alias).unwrap(); publish(&target,b"copy").unwrap();
                assert_eq!(fs::read(&original).unwrap(),b"original"); assert_eq!(fs::read(&alias).unwrap(),b"original"); assert_eq!(fs::read(&target).unwrap(),b"copy");
            }
            let target=root.path("symlink-copy"); let alias=root.path("symlink-copy.tmp");symlink(&original,&alias).unwrap();
            publish(&target,b"copy").unwrap();assert_eq!(fs::read(&original).unwrap(),b"original");assert!(fs::symlink_metadata(alias).unwrap().file_type().is_symlink());
        }
        #[test]
        fn destination_hardlink_replacement_does_not_modify_original() {
            let root=Fixture::new();let original=root.path("original");let target=root.path("copy");fs::write(&original,b"original").unwrap();fs::hard_link(&original,&target).unwrap();
            publish(&target,b"copy").unwrap();assert_eq!(fs::read(&original).unwrap(),b"original");assert_eq!(fs::read(&target).unwrap(),b"copy");
        }
        #[test]
        fn parent_retarget_is_refused_and_owned_staging_is_cleaned() {
            let root=Fixture::new();let first=root.path("first");let second=root.path("second");let alias=root.path("alias");fs::create_dir(&first).unwrap();fs::create_dir(&second).unwrap();symlink(&first,&alias).unwrap();
            let result=publish_with(&alias.join("copy"),b"copy", |_|{fs::remove_file(&alias)?;symlink(&second,&alias)});
            assert!(result.is_err());assert_eq!(fs::read_dir(&first).unwrap().count(),0);assert_eq!(fs::read_dir(&second).unwrap().count(),0);
        }
        #[test]
        fn replaced_staging_is_refused_without_removing_foreign_alias() {
            let root=Fixture::new();let original=root.path("original");let target=root.path("copy");fs::write(&original,b"original").unwrap();let mut foreign=None;
            let result=publish_with(&target,b"copy",|stage|{fs::remove_file(stage)?;fs::hard_link(&original,stage)?;foreign=Some(stage.file_name().unwrap().to_owned());Ok(())});
            assert!(result.is_err());assert!(!target.exists());assert_eq!(fs::read(&original).unwrap(),b"original");assert_eq!(fs::read(root.0.join(foreign.unwrap())).unwrap(),b"original");
        }
        #[test]
        fn publication_failure_cleans_owned_staging_and_preserves_existing_target() {
            let root=Fixture::new();let target=root.path("directory");fs::create_dir(&target).unwrap();assert!(publish(&target,b"copy").is_err());assert!(target.is_dir());assert_eq!(fs::read_dir(&root.0).unwrap().count(),1);
        }
        #[test]
        fn replacement_preserves_existing_file_permissions() {
            let root=Fixture::new();let target=root.path("copy");fs::write(&target,b"old").unwrap();fs::set_permissions(&target,fs::Permissions::from_mode(0o640)).unwrap();super::super::write(&target,b"new").unwrap();assert_eq!(fs::read(&target).unwrap(),b"new");assert_eq!(fs::metadata(target).unwrap().permissions().mode() & 0o777,0o640);
        }
        #[test]
        fn confirmed_parent_retarget_refuses_without_creating_staging() {
            let root=Fixture::new();let first=root.path("first");let second=root.path("second");let alias=root.path("alias");fs::create_dir(&first).unwrap();fs::create_dir(&second).unwrap();symlink(&first,&alias).unwrap();
            let path=alias.join("copy");let captured=super::super::capture_output(&path).unwrap();fs::remove_file(&alias).unwrap();symlink(&second,&alias).unwrap();
            assert!(super::super::write_captured(&captured,b"copy").is_err());assert_eq!(fs::read_dir(first).unwrap().count(),0);assert_eq!(fs::read_dir(second).unwrap().count(),0);
        }
        #[test]
        fn confirmed_replacement_and_in_place_edit_are_not_overwritten() {
            let root=Fixture::new();let path=root.path("copy");
            for replace in [false,true] {
                fs::write(&path,b"old").unwrap();let captured=super::super::capture_output(&path).unwrap();
                if replace { fs::remove_file(&path).unwrap(); }
                fs::write(&path,b"Newer destination contents").unwrap();let facts=EntryFacts::from(&fs::symlink_metadata(&path).unwrap());
                assert!(super::super::write_captured(&captured,b"copy").is_err());assert_eq!(fs::read(&path).unwrap(),b"Newer destination contents");assert!(EntryFacts::from(&fs::symlink_metadata(&path).unwrap())==facts);assert_eq!(fs::read_dir(&root.0).unwrap().count(),1);
            }
        }
        #[test]
        fn confirmed_absent_entry_becoming_present_is_preserved() {
            let root=Fixture::new();let path=root.path("copy");let captured=super::super::capture_output(&path).unwrap();fs::write(&path,b"Another producer").unwrap();let facts=EntryFacts::from(&fs::symlink_metadata(&path).unwrap());
            assert!(super::super::write_captured(&captured,b"copy").is_err());assert_eq!(fs::read(&path).unwrap(),b"Another producer");assert!(EntryFacts::from(&fs::symlink_metadata(&path).unwrap())==facts);assert_eq!(fs::read_dir(&root.0).unwrap().count(),1);
        }
        #[test]
        fn confirmed_change_during_staging_cleans_only_owned_output() {
            let root=Fixture::new();let path=root.path("copy");fs::write(&path,b"old").unwrap();let captured=capture(&path).unwrap();
            assert!(publish_checked(&path,b"copy",Some(&captured),|_|fs::write(&path,b"Concurrent edit")).is_err());assert_eq!(fs::read(&path).unwrap(),b"Concurrent edit");assert_eq!(fs::read_dir(&root.0).unwrap().count(),1);
        }
        #[test]
        fn confirmed_ordinary_publication_preserves_existing_permissions() {
            let root=Fixture::new();let path=root.path("copy");fs::write(&path,b"old").unwrap();fs::set_permissions(&path,fs::Permissions::from_mode(0o640)).unwrap();
            let captured=super::super::capture_output(&path).unwrap();assert_eq!(captured.path(),path);super::super::write_captured(&captured,b"copy").unwrap();assert_eq!(fs::read(&path).unwrap(),b"copy");assert_eq!(fs::metadata(&path).unwrap().permissions().mode()&0o777,0o640);assert_eq!(fs::read_dir(&root.0).unwrap().count(),1);
        }
    }
}

/// A selected output entry held across a provider confirmation. Facts are private.
pub struct CapturedOutput {
    path: std::path::PathBuf,
    #[cfg(target_os = "linux")]
    captured: native::Captured,
}
impl CapturedOutput { pub fn path(&self) -> &std::path::Path { &self.path } }
pub fn capture_output(path: &std::path::Path) -> Result<CapturedOutput, String> {
    #[cfg(target_os = "linux")]
    { native::capture(path).map(|captured| CapturedOutput { path: path.to_owned(), captured }).map_err(|error| format!("{}: {error}", path.display())) }
    #[cfg(not(target_os = "linux"))]
    { Err(format!("{}: confirmed filesystem output is not supported on this platform", path.display())) }
}
pub fn write_captured(output: &CapturedOutput, bytes: &[u8]) -> Result<(), String> {
    #[cfg(target_os = "linux")]
    { native::publish_captured(&output.path, bytes, &output.captured).map_err(|error| format!("{}: {error}", output.path.display())) }
    #[cfg(not(target_os = "linux"))]
    { let _=bytes; Err(format!("{}: confirmed filesystem output is not supported on this platform", output.path.display())) }
}

pub(crate) fn write(path: &std::path::Path, bytes: &[u8]) -> Result<(), String> {
    #[cfg(target_os = "linux")]
    { native::publish(path, bytes).map_err(|error| format!("{}: {error}", path.display())) }
    #[cfg(not(target_os = "linux"))]
    { let _=bytes; Err(format!("{}: saving files is not supported on this platform",path.display())) }
}
