//! Short, privately owned runtime directory independent of artifact path length.
use std::{ffi::CStr, fs, io, os::unix::fs::MetadataExt, path::{Path, PathBuf}};

pub struct PrivateRuntime {
    path: PathBuf,
    device: u64,
    inode: u64,
}
impl PrivateRuntime {
    pub fn new() -> io::Result<Self> {
        let mut template = b"/tmp/lqr-XXXXXX\0".to_vec();
        let pointer = unsafe { libc::mkdtemp(template.as_mut_ptr().cast()) };
        if pointer.is_null() { return Err(io::Error::last_os_error()); }
        let path = PathBuf::from(unsafe { CStr::from_ptr(pointer) }.to_str().map_err(io::Error::other)?);
        let metadata = fs::symlink_metadata(&path)?;
        let runtime = Self { path, device: metadata.dev(), inode: metadata.ino() };
        if !metadata.is_dir() || metadata.uid() != unsafe { libc::geteuid() }
            || metadata.mode() & 0o777 != 0o700 || !Self::ipc_budget(runtime.path()) {
            return Err(io::Error::other("Private runtime ownership, permissions or IPC budget invalid"));
        }
        Ok(runtime)
    }
    pub fn path(&self) -> &Path { &self.path }
    pub fn verify_log(&self, log: &str) -> io::Result<usize> {
        use std::os::unix::ffi::OsStrExt;
        if log.contains("Failed to start IPC") { return Err(io::Error::other("Quickshell IPC failed")); }
        let saved = log.lines().find(|line| line.contains("Saving logs to"))
            .and_then(|line| line.split('"').nth(1)).ok_or_else(|| io::Error::other("Missing actual runtime log path"))?;
        let socket = Path::new(saved).with_file_name("ipc.sock");
        let bytes = socket.as_os_str().as_bytes().len();
        if !socket.starts_with(&self.path) || bytes > 106 { return Err(io::Error::other("Actual IPC path exceeds owned runtime byte budget")); }
        Ok(bytes)
    }
    // Qt 6.11 Linux reserves two sun_path bytes, even for filesystem sockets.
    // Reserve 32 encoded bytes for the instance ID (current provider uses ten).
    fn ipc_budget(path: &Path) -> bool {
        use std::os::unix::ffi::OsStrExt;
        path.as_os_str().as_bytes().len() + b"/quickshell/by-id/".len() + 32 + b"/ipc.sock".len() <= 106
    }
}
impl Drop for PrivateRuntime {
    fn drop(&mut self) {
        // Never recursively remove a replacement path or a symlink.
        if let Ok(metadata) = fs::symlink_metadata(&self.path) {
            if metadata.is_dir() && metadata.uid() == unsafe { libc::geteuid() }
                && metadata.dev() == self.device && metadata.ino() == self.inode {
                let _ = fs::remove_dir_all(&self.path);
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn runtime_is_short_private_unique_and_owned_cleanup() {
        let first = PrivateRuntime::new().unwrap();
        let second = PrivateRuntime::new().unwrap();
        assert_ne!(first.path(), second.path());
        let path = first.path().to_owned();
        let metadata = fs::symlink_metadata(&path).unwrap();
        assert_eq!(metadata.mode() & 0o777, 0o700);
        assert_eq!(metadata.uid(), unsafe { libc::geteuid() });
        assert!(PrivateRuntime::ipc_budget(&path));
        fs::write(path.join("owned"), b"fixture").unwrap();
        drop(first);
        assert!(!path.exists());
        assert!(second.path().exists());
    }
    #[test]
    fn encoded_ipc_budget_rejects_long_and_multibyte_roots() {
        let suffix = b"/quickshell/by-id/".len() + 32 + b"/ipc.sock".len();
        assert!(PrivateRuntime::ipc_budget(Path::new(&"x".repeat(106-suffix))));
        assert!(!PrivateRuntime::ipc_budget(Path::new(&"x".repeat(107-suffix))));
        assert!(!PrivateRuntime::ipc_budget(Path::new(&"é".repeat(106-suffix))));
    }
}
