//! Map inherited diagnostic pipes to descriptors 3 and 4 before exec.
use std::collections::BTreeSet;
use std::env;
use std::ffi::{CString, OsString};
use std::io;
use std::os::raw::{c_char, c_int};
use std::os::unix::ffi::OsStrExt;

const F_DUPFD_CLOEXEC: c_int = 1030;

unsafe extern "C" {
    fn fcntl(fd: c_int, command: c_int, ...) -> c_int;
    fn dup2(source: c_int, target: c_int) -> c_int;
    fn close(fd: c_int) -> c_int;
    fn execv(path: *const c_char, arguments: *const *const c_char) -> c_int;
}

fn checked_fd(value: Option<OsString>, description: &str) -> Result<c_int, String> {
    let value = value.ok_or_else(|| format!("Missing {description} descriptor"))?;
    let text = value.to_str().ok_or_else(|| format!("Invalid {description} descriptor"))?;
    let fd = text.parse::<c_int>().map_err(|_| format!("Invalid {description} descriptor"))?;
    if fd < 0 {
        return Err(format!("Invalid {description} descriptor"));
    }
    Ok(fd)
}

fn run() -> Result<(), String> {
    let mut arguments = env::args_os().skip(1);
    let incoming = checked_fd(arguments.next(), "input")?;
    let outgoing = checked_fd(arguments.next(), "output")?;
    let command = arguments.collect::<Vec<_>>();
    if command.is_empty() {
        return Err("Missing explicit application command".into());
    }
    let command = command.iter().map(|value| CString::new(value.as_bytes()).map_err(|_| "NUL in application command".to_owned()))
        .collect::<Result<Vec<_>, _>>()?;
    let mut copied = Vec::new();
    for source in [incoming, outgoing] {
        let duplicate = unsafe { fcntl(source, F_DUPFD_CLOEXEC, 10) };
        if duplicate < 0 {
            return Err(format!("Cannot duplicate descriptor: {}", io::Error::last_os_error()));
        }
        copied.push(duplicate);
    }
    for (source, target) in copied.iter().zip([3, 4]) {
        if unsafe { dup2(*source, target) } < 0 {
            return Err(format!("Cannot map diagnostic pipe: {}", io::Error::last_os_error()));
        }
    }
    for fd in BTreeSet::from([incoming, outgoing, copied[0], copied[1]]) {
        if fd != 3 && fd != 4 {
            unsafe { close(fd) };
        }
    }
    let mut pointers = command.iter().map(|value| value.as_ptr()).collect::<Vec<_>>();
    pointers.push(std::ptr::null());
    unsafe { execv(command[0].as_ptr(), pointers.as_ptr()) };
    Err(format!("Cannot start application: {}", io::Error::last_os_error()))
}

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
