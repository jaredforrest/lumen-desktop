//! Exclusive descriptor-anchored publication for Linux document providers.
#[cfg(target_os = "linux")]
mod native {
    use std::fs::{self, OpenOptions};
    use std::io::{self, Write};
    use std::os::fd::AsRawFd;
    use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    fn identity(info: &fs::Metadata) -> (u64, u64) { (info.dev(), info.ino()) }
    fn owns(path: &Path, expected: (u64, u64)) -> bool {
        fs::symlink_metadata(path).is_ok_and(|info| info.is_file() && identity(&info) == expected)
    }
    pub(super) fn publish(path: &Path, bytes: &[u8]) -> io::Result<()> {
        publish_with(path, bytes, |_| Ok(()))
    }
    fn publish_with(path: &Path, bytes: &[u8], before_commit: impl FnOnce(&Path) -> io::Result<()>) -> io::Result<()> {
        let parent = path.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(Path::new("."));
        let name = path.file_name().ok_or_else(|| io::Error::other("A document filename is required"))?;
        // Linux UAPI O_DIRECTORY on the supported x86_64 target rejects FIFOs before opening.
        let directory = OpenOptions::new().read(true).custom_flags(0o200000).open(parent)?;
        let parent_info = directory.metadata()?;
        if !parent_info.is_dir() { return Err(io::Error::other("The output parent is not a directory")); }
        let anchored = PathBuf::from(format!("/proc/self/fd/{}", directory.as_raw_fd()));
        let target = anchored.join(name);
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
        }
        #[test]
        fn fifo_parent_is_rejected_without_blocking() {
            use std::process::{Command, Stdio};
            use std::time::{Duration, Instant};
            let root=Fixture::new();let fifo=root.path("fifo");assert!(Command::new("mkfifo").arg(&fifo).status().unwrap().success());
            let name=format!("{}::fifo_parent_probe",module_path!().split_once("::").unwrap().1);
            let mut child=Command::new(std::env::current_exe().unwrap()).args(["--exact",&name]).env("LUMEN_ATOMIC_FIFO_PROBE",&fifo).stdout(Stdio::null()).spawn().unwrap();
            let deadline=Instant::now()+Duration::from_secs(2);
            loop {
                if let Some(status)=child.try_wait().unwrap(){assert!(status.success());break;}
                if Instant::now()>=deadline { child.kill().unwrap();child.wait().unwrap();assert!(Instant::now()<deadline,"Opening a non-directory output parent blocked"); }
                std::thread::sleep(Duration::from_millis(10));
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
    }
}

pub(crate) fn write(path: &std::path::Path, bytes: &[u8]) -> Result<(), String> {
    #[cfg(target_os = "linux")]
    { native::publish(path, bytes).map_err(|error| format!("{}: {error}", path.display())) }
    #[cfg(not(target_os = "linux"))]
    { let _=bytes; Err(format!("{}: saving files is not supported on this platform",path.display())) }
}
