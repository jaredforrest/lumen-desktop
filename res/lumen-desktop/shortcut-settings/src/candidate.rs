//! A transaction owns only the new regular file it created in its private directory.
use std::{
    fs::{self, File, OpenOptions},
    os::unix::fs::{MetadataExt, OpenOptionsExt},
    path::{Path, PathBuf},
};
pub struct Candidate {
    pub file: File,
    path: PathBuf,
    device: u64,
    inode: u64,
}
impl Candidate {
    pub fn create(path: PathBuf) -> Result<Self, String> {
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
            .open(&path)
            .map_err(|_| "Could not own shortcut candidate")?;
        let metadata = file
            .metadata()
            .map_err(|_| "Shortcut candidate identity unavailable")?;
        Ok(Self {
            file,
            path,
            device: metadata.dev(),
            inode: metadata.ino(),
        })
    }
    pub fn path(&self) -> &Path {
        &self.path
    }
}
impl Drop for Candidate {
    fn drop(&mut self) {
        if fs::symlink_metadata(&self.path)
            .is_ok_and(|m| m.is_file() && m.dev() == self.device && m.ino() == self.inode)
        {
            let _ = fs::remove_file(&self.path);
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    #[test]
    fn early_partial_write_and_preflight_errors_remove_owned_candidates() {
        let root = std::env::temp_dir().join(format!("lumen-candidate-{}", std::process::id()));
        fs::create_dir(&root).unwrap();
        let path = root.join("candidate.json");
        for error in ["injected partial-write error", "preflight refused"] {
            let result = (|| -> Result<(), String> {
                let mut candidate = Candidate::create(path.clone())?;
                candidate.file.write_all(b"partial").unwrap();
                assert!(candidate.path().is_file());
                Err(error.into())
            })();
            assert_eq!(result.unwrap_err(), error);
            assert!(!path.exists());
            let retry = Candidate::create(path.clone()).unwrap();
            drop(retry);
            assert!(!path.exists());
        }
        fs::remove_dir(root).unwrap();
    }
    #[test]
    fn cleanup_preserves_preexisting_or_replaced_paths() {
        let root =
            std::env::temp_dir().join(format!("lumen-candidate-replaced-{}", std::process::id()));
        fs::create_dir(&root).unwrap();
        let path = root.join("candidate.json");
        fs::write(&path, b"preexisting").unwrap();
        assert!(Candidate::create(path.clone()).is_err());
        assert_eq!(fs::read(&path).unwrap(), b"preexisting");
        fs::remove_file(&path).unwrap();
        let candidate = Candidate::create(path.clone()).unwrap();
        fs::rename(&path, root.join("moved-owned.json")).unwrap();
        fs::write(&path, b"replacement").unwrap();
        drop(candidate);
        assert_eq!(fs::read(&path).unwrap(), b"replacement");
        fs::remove_dir_all(root).unwrap();
    }
}
