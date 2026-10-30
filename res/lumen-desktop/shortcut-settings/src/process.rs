//! Bounded synchronous child groups, following the sharing transport ownership model.
use std::{
    fs,
    io::Read,
    os::{
        fd::AsRawFd,
        unix::{fs::OpenOptionsExt, process::CommandExt},
    },
    path::Path,
    process::{Command, Stdio},
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};
type Result<T> = std::result::Result<T, String>;
static INTERRUPTED: AtomicBool = AtomicBool::new(false);
extern "C" fn interrupted(_: i32) {
    INTERRUPTED.store(true, Ordering::SeqCst);
}
pub fn install_interrupts() {
    unsafe {
        libc::signal(libc::SIGINT, interrupted as libc::sighandler_t);
        libc::signal(libc::SIGTERM, interrupted as libc::sighandler_t);
        libc::signal(libc::SIGHUP, interrupted as libc::sighandler_t);
    }
}
pub struct Reply {
    pub stdout: Vec<u8>,
    pub success: bool,
}
pub fn run(program: &str, args: &[&str], budget: Duration, diagnostic: &Path) -> Result<Reply> {
    let mut command = Command::new(program);
    command.args(args);
    execute(command, budget, diagnostic)
}
pub fn execute(mut command: Command, budget: Duration, diagnostic: &Path) -> Result<Reply> {
    let mut previous = 0;
    if unsafe { libc::prctl(libc::PR_GET_CHILD_SUBREAPER, &mut previous, 0, 0, 0) } != 0
        || unsafe { libc::prctl(libc::PR_SET_CHILD_SUBREAPER, 1, 0, 0, 0) } != 0
    {
        return Err("Cannot own settings provider descendants".into());
    }
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    unsafe {
        command.pre_exec(|| {
            if libc::setsid() < 0 {
                Err(std::io::Error::last_os_error())
            } else {
                Ok(())
            }
        });
    }
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => {
            unsafe {
                libc::prctl(libc::PR_SET_CHILD_SUBREAPER, previous, 0, 0, 0);
            }
            return Err(format!("Settings provider could not start: {error}"));
        }
    };
    let pid = child.id() as i32;
    let mut stderr = Vec::new();
    let result = (|| -> Result<Reply> {
        let mut output = child.stdout.take().ok_or("Missing provider output")?;
        let mut errors = child.stderr.take().ok_or("Missing provider diagnostics")?;
        for fd in [output.as_raw_fd(), errors.as_raw_fd()] {
            let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
            if flags < 0 || unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0
            {
                return Err("Could not bound provider pipes".into());
            }
        }
        let deadline = Instant::now() + budget;
        let mut stdout = Vec::new();
        let mut open = [true, true];
        loop {
            if INTERRUPTED.load(Ordering::SeqCst) {
                return Err("Settings operation interrupted; outcome is unconfirmed".into());
            }
            if Instant::now() >= deadline {
                return Err("Settings provider timed out; outcome is unconfirmed".into());
            }
            let mut polls = [
                libc::pollfd {
                    fd: if open[0] { output.as_raw_fd() } else { -1 },
                    events: libc::POLLIN,
                    revents: 0,
                },
                libc::pollfd {
                    fd: if open[1] { errors.as_raw_fd() } else { -1 },
                    events: libc::POLLIN,
                    revents: 0,
                },
            ];
            unsafe {
                libc::poll(polls.as_mut_ptr(), 2, 10);
            }
            for (index, pipe) in [
                (0, &mut output as &mut dyn Read),
                (1, &mut errors as &mut dyn Read),
            ] {
                if polls[index].revents == 0 {
                    continue;
                }
                let mut buffer = [0u8; 4096];
                match pipe.read(&mut buffer) {
                    Ok(0) => open[index] = false,
                    Ok(n) => {
                        let target = if index == 0 { &mut stdout } else { &mut stderr };
                        if target.len() + n > if index == 0 { 524288 } else { 65536 } {
                            return Err("Settings provider exceeded its output limit; outcome is unconfirmed".into());
                        }
                        target.extend_from_slice(&buffer[..n]);
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
                    Err(_) => {
                        return Err(
                            "Settings provider output failed; outcome is unconfirmed".into()
                        );
                    }
                }
            }
            if !open[0] && !open[1] {
                let mut info: libc::siginfo_t = unsafe { std::mem::zeroed() };
                if unsafe {
                    libc::waitid(
                        libc::P_PID,
                        pid as u32,
                        &mut info,
                        libc::WEXITED | libc::WNOHANG | libc::WNOWAIT,
                    )
                } < 0
                {
                    return Err("Could not observe settings provider completion".into());
                }
                if unsafe { info.si_pid() } != 0 {
                    return Ok(Reply {
                        stdout,
                        success: info.si_code == libc::CLD_EXITED
                            && unsafe { info.si_status() } == 0,
                    });
                }
            }
        }
    })();
    // Keep the leader unreaped until the group is retired, preventing PID reuse.
    unsafe {
        libc::kill(-pid, libc::SIGKILL);
    }
    let reaped = child
        .wait()
        .map_err(|_| "Could not reap settings provider".to_owned());
    let deadline = Instant::now() + Duration::from_secs(1);
    let mut clean = reaped.map(|_| ());
    loop {
        let result = unsafe { libc::waitpid(-pid, std::ptr::null_mut(), libc::WNOHANG) };
        if result < 0 {
            if std::io::Error::last_os_error().raw_os_error() != Some(libc::ECHILD) {
                clean = Err("Could not reap settings descendants".into());
            }
            break;
        }
        if result == 0 {
            if Instant::now() >= deadline {
                clean = Err("Settings descendants did not finish cleanup".into());
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }
    unsafe {
        libc::prctl(libc::PR_SET_CHILD_SUBREAPER, previous, 0, 0, 0);
    }
    if !stderr.is_empty() {
        let temp = diagnostic.with_extension(format!("{}.next", std::process::id()));
        if let Ok(mut file) = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .mode(0o600)
            .open(&temp)
        {
            use std::io::Write;
            let _ = file.write_all(&stderr);
            let _ = file.sync_all();
            let _ = fs::rename(&temp, diagnostic);
        }
        let _ = fs::remove_file(temp);
    }
    clean?;
    result
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn provider_child() {
        let Ok(role) = std::env::var("SHORTCUT_PROCESS_TEST_ROLE") else {
            return;
        };
        let file = std::env::var("SHORTCUT_PROCESS_TEST_PIDS").unwrap();
        if role == "tree" {
            let child = unsafe { libc::fork() };
            assert!(child >= 0);
            if child == 0 {
                loop {
                    std::thread::sleep(Duration::from_secs(1));
                }
            }
            fs::write(file, format!("{} {child}", std::process::id())).unwrap();
        }
        if role == "flood" || role == "stderr" {
            use std::io::Write;
            let bytes = [b'x'; 4096];
            loop {
                if role == "stderr" {
                    std::io::stderr().write_all(&bytes).unwrap();
                } else {
                    std::io::stdout().write_all(&bytes).unwrap();
                }
            }
        }
        loop {
            std::thread::sleep(Duration::from_secs(1));
        }
    }
    #[test]
    fn bounded_process_cleanup() {
        let root = std::env::temp_dir().join(format!("shortcut-process-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        for role in ["hang", "flood", "stderr", "tree"] {
            let file = root.join(format!("{role}.pids"));
            let mut cmd = Command::new(std::env::current_exe().unwrap());
            cmd.args(["--exact", "process::tests::provider_child", "--nocapture"])
                .env("SHORTCUT_PROCESS_TEST_ROLE", role)
                .env("SHORTCUT_PROCESS_TEST_PIDS", &file);
            let before = Instant::now();
            let result = execute(
                cmd,
                Duration::from_millis(200),
                &root.join("diagnostic.log"),
            );
            assert!(result.is_err());
            assert!(before.elapsed() < Duration::from_secs(3));
            if role == "tree" {
                for pid in fs::read_to_string(file).unwrap().split_whitespace() {
                    let pid = pid.parse::<i32>().unwrap();
                    assert_eq!(unsafe { libc::kill(pid, 0) }, -1);
                    assert_eq!(
                        std::io::Error::last_os_error().raw_os_error(),
                        Some(libc::ESRCH)
                    );
                }
            }
        }
        fs::remove_dir_all(root).unwrap();
    }
}
