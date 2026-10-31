//! Short private roots for standalone fixtures and their nested Qt sockets.
use std::fs::{self, DirBuilder, File};
use std::io::{self, Read};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::DirBuilderExt;
use std::path::{Path, PathBuf};

// The pinned instance ID encodes a PID and launch timestamp in base36. Reserve
// 32 bytes, including headroom beyond their current integer representations.
const SOCKET_SUFFIX: &str = "runtime/quickshell/by-id/zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz/ipc.sock";

fn socket_budget(root: &Path) -> io::Result<()> {
    if !root.is_absolute() || root.join(SOCKET_SUFFIX).as_os_str().as_bytes().len() > 106 {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "Private QML runtime exceeds the encoded Qt socket budget"));
    }
    Ok(())
}

pub fn create() -> io::Result<PathBuf> {
    let mut random = File::open("/dev/urandom")?;
    for _ in 0..32 {
        let mut bytes = [0_u8; 8];
        random.read_exact(&mut bytes)?;
        let root = PathBuf::from(format!("/tmp/lq-{:016x}", u64::from_ne_bytes(bytes)));
        socket_budget(&root)?;
        match DirBuilder::new().mode(0o700).create(&root) {
            Ok(()) => return Ok(root),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }
    Err(io::Error::new(io::ErrorKind::AlreadyExists, "Private QML runtime allocation exhausted"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use std::os::unix::net::UnixListener;

    #[test]
    fn private_root_binds_nested_socket_with_budget_and_distinct_ownership() {
        let first = create().unwrap();
        let second = create().unwrap();
        assert_ne!(first, second);
        assert_eq!(fs::metadata(&first).unwrap().permissions().mode() & 0o777, 0o700);
        let socket = first.join(SOCKET_SUFFIX);
        assert!(socket.as_os_str().as_bytes().len() <= 106);
        fs::create_dir_all(socket.parent().unwrap()).unwrap();
        let listener = UnixListener::bind(&socket).unwrap();
        drop(listener);
        fs::remove_dir_all(&first).unwrap();
        assert!(second.is_dir());
        fs::remove_dir_all(second).unwrap();
    }

    #[test]
    fn budget_uses_encoded_bytes_and_rejects_old_long_roots() {
        let suffix_bytes = Path::new(SOCKET_SUFFIX).as_os_str().as_bytes().len();
        let exact = PathBuf::from(format!("/{}", "a".repeat(106 - suffix_bytes - 2)));
        assert_eq!(exact.join(SOCKET_SUFFIX).as_os_str().as_bytes().len(), 106);
        socket_budget(&exact).unwrap();
        assert!(socket_budget(&PathBuf::from(format!("{}a", exact.display()))).is_err());
        assert!(socket_budget(&PathBuf::from(format!("{}é", exact.display()))).is_err());
        assert!(socket_budget(Path::new("relative")).is_err());
        assert!(socket_budget(Path::new("/tmp/lumen-battery-presentation-4194304-18446744073709551615")).is_err());
    }
}
