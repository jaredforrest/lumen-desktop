use super::*;
use std::os::unix::net::UnixStream;
use std::time::Duration;

struct Directory(PathBuf);
impl Drop for Directory {
    fn drop(&mut self) { let _ = fs::remove_dir_all(&self.0); }
}
struct Forked { pid: Option<libc::pid_t>, control: UnixStream }
impl Forked {
    fn running(&mut self) -> std::io::Result<bool> {
        let Some(pid) = self.pid else { return Ok(false); };
        let result = unsafe { libc::waitpid(pid, std::ptr::null_mut(), libc::WNOHANG) };
        if result == 0 { return Ok(true); }
        if result == pid {
            self.pid = None;
            return Ok(false);
        }
        let error = std::io::Error::last_os_error();
        if error.raw_os_error() == Some(libc::ECHILD) {
            self.pid = None;
            return Ok(false);
        }
        Err(error)
    }
    fn cleanup(&mut self) -> bool {
        // A readiness assertion may itself reap the child. Never signal a PID
        // after that ownership has ended, including an ECHILD observation.
        if !matches!(self.running(), Ok(true)) { return false; }
        let pid = self.pid.take().unwrap();
        // The child has an independent three-second bound as well. Always reap,
        // including when an assertion unwinds before the normal release.
        unsafe { libc::kill(pid, libc::SIGKILL); }
        loop {
            let result = unsafe { libc::waitpid(pid, std::ptr::null_mut(), 0) };
            if result >= 0 || std::io::Error::last_os_error().kind() != std::io::ErrorKind::Interrupted { break; }
        }
        true
    }
}
impl Drop for Forked { fn drop(&mut self) { self.cleanup(); } }
fn fork_holder(store: &mut ObservationStore, drop_child_lease: bool) -> Forked {
    let (mut parent, child) = UnixStream::pair().unwrap();
    parent.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
    let parent_fd = parent.as_raw_fd();
    let child_fd = child.as_raw_fd();
    let lease = &raw mut store.lock;
    let pid = unsafe { libc::fork() };
    assert!(pid >= 0);
    if pid == 0 {
        // After a multithreaded fork use only async-signal-safe syscalls. This
        // exact lease destructor performs getpid/flock/close, with no heap
        // fields or allocation. Do not drop the surrounding store in the child.
        unsafe {
            libc::close(parent_fd);
            if drop_child_lease { std::ptr::drop_in_place(lease); }
            let ready = [1_u8];
            if libc::write(child_fd, ready.as_ptr().cast(), 1) != 1 { libc::_exit(2); }
            let mut poll = libc::pollfd { fd: child_fd, events: libc::POLLIN, revents: 0 };
            libc::poll(&mut poll, 1, 3000);
            libc::_exit(0);
        }
    }
    drop(child);
    let holder = Forked { pid: Some(pid), control: parent.try_clone().unwrap() };
    let mut ready = [0];
    parent.read_exact(&mut ready).unwrap();
    assert_eq!(ready, [1]);
    holder
}
#[test]
fn actual_fork_inheritance_does_not_extend_or_release_the_creating_owners_lease() {
    let root = Directory(std::env::temp_dir().join(format!("lumen-lease-fork-{}", std::process::id())));
    fs::DirBuilder::new().mode(0o700).create(&root.0).unwrap();
    let epoch = "0123456789abcdef01234567";
    let boot = "01234567-89ab-cdef-0123-456789abcdef";
    for child_drops in [false, true] {
        let mut store = ObservationStore::new(&root.0, epoch, boot).unwrap();
        assert_ne!(unsafe { libc::fcntl(store.lock_fd(), libc::F_GETFD) } & libc::FD_CLOEXEC, 0);
        let mut holder = fork_holder(&mut store, child_drops);
        assert!(holder.running().unwrap());
        assert!(ObservationStore::new(&root.0, epoch, boot).is_err(), "an actual live owner must remain exclusive, including after child lease drop");
        drop(store);
        let replacement = ObservationStore::new(&root.0, epoch, boot)
            .expect("owner drop must release even while a pre-exec child retains its descriptor");
        assert!(holder.running().unwrap());
        assert!(ObservationStore::new(&root.0, epoch, boot).is_err());
        holder.control.write_all(&[1]).unwrap();
        drop(holder);
        drop(replacement);
    }
}

#[test]
fn fork_cleanup_does_not_signal_after_reaping_or_echild() {
    let root = Directory(std::env::temp_dir().join(format!("lumen-lease-reaped-{}", std::process::id())));
    fs::DirBuilder::new().mode(0o700).create(&root.0).unwrap();
    let mut store = ObservationStore::new(&root.0, "0123456789abcdef01234567", "01234567-89ab-cdef-0123-456789abcdef").unwrap();
    for external_reap in [false, true] {
        let mut holder = fork_holder(&mut store, false);
        holder.control.write_all(&[1]).unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        if external_reap {
            let pid = holder.pid.unwrap();
            loop {
                let result = unsafe { libc::waitpid(pid, std::ptr::null_mut(), libc::WNOHANG) };
                if result == pid { break; }
                assert_eq!(result, 0);
                assert!(std::time::Instant::now() < deadline);
                std::thread::sleep(Duration::from_millis(1));
            }
        } else {
            while holder.running().unwrap() {
                assert!(std::time::Instant::now() < deadline);
                std::thread::sleep(Duration::from_millis(1));
            }
        }
        assert!(!holder.running().unwrap());
        assert!(holder.pid.is_none());
        assert!(!holder.cleanup(), "an already reaped child must never be signalled");
    }
}
