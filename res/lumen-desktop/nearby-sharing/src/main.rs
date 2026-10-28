//! Bounded provider status and explicit open/send commands.
use serde_json::{Value, json};
use std::{env, fs, io, os::{fd::AsRawFd, unix::{fs::PermissionsExt, process::CommandExt}},
    path::{Path, PathBuf}, process::{Command, ExitCode, Stdio}, time::{Duration, Instant}};

const LIMIT: usize = 16_384;

fn normalize(value: Value) -> Result<Value, String> {
    let available = value.get("available").and_then(Value::as_bool).ok_or("Invalid provider status")?;
    let active = value.get("active").and_then(Value::as_bool).ok_or("Invalid provider status")?;
    let send = value.get("canSend").map(Value::as_bool).unwrap_or(Some(false))
        .ok_or("Invalid provider send capability")?;
    let detail = value.get("detail").map(Value::as_str).unwrap_or(Some(""))
        .ok_or("Invalid provider detail")?;
    if detail.chars().count() > 160 || detail.chars().any(char::is_control) {
        return Err("Invalid provider detail".into());
    }
    if (active || send) && !available { return Err("Unavailable provider cannot report active or send".into()); }
    Ok(json!({"available":available,"active":active,"canSend":send,"detail":detail}))
}

fn configured() -> Result<Option<PathBuf>, String> {
    let Some(value) = env::var_os("LUMEN_NEARBY_SHARING_PROVIDER") else { return Ok(None); };
    if value.is_empty() { return Ok(None); }
    let path = PathBuf::from(value);
    if !path.is_absolute() { return Err("Immutable configured provider required".into()); }
    let resolved = fs::canonicalize(path).map_err(|_| "Immutable configured provider required")?;
    let meta = resolved.metadata().map_err(|_| "Immutable configured provider required")?;
    if !resolved.starts_with("/nix/store") || !meta.is_file() || meta.permissions().mode() & 0o111 == 0 {
        return Err("Immutable configured provider required".into());
    }
    Ok(Some(resolved))
}

fn status(provider: &Path, timeout: Duration) -> Result<Value, String> {
    let deadline = Instant::now() + timeout;
    let mut command = Command::new(provider);
    command.arg("status").stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    unsafe { command.pre_exec(|| if libc::setsid() == -1 { Err(io::Error::last_os_error()) } else { Ok(()) }); }
    let mut child = command.spawn().map_err(|_| "Provider status failed")?;
    let pid = child.id() as i32;
    let result = (|| {
        let stdout = child.stdout.as_ref().ok_or("Provider status failed")?.as_raw_fd();
        let stderr = child.stderr.as_ref().ok_or("Provider status failed")?.as_raw_fd();
        let mut fds = [libc::pollfd { fd: stdout, events: libc::POLLIN, revents: 0 },
                       libc::pollfd { fd: stderr, events: libc::POLLIN, revents: 0 }];
        let mut output = Vec::new();
        let mut total = 0;
        let mut diagnostic = false;
        while fds.iter().any(|f| f.fd >= 0) {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() { return Err("Provider status deadline".into()); }
            let millis = remaining.as_millis().clamp(1, i32::MAX as u128) as i32;
            let ready = unsafe { libc::poll(fds.as_mut_ptr(), fds.len() as libc::nfds_t, millis) };
            if ready == 0 { return Err("Provider status deadline".into()); }
            if ready < 0 {
                if io::Error::last_os_error().kind() == io::ErrorKind::Interrupted { continue; }
                return Err("Provider status failed".into());
            }
            for (index, fd) in fds.iter_mut().enumerate() {
                if fd.fd < 0 || fd.revents == 0 { continue; }
                let mut buffer = [0u8; 4096];
                let count = unsafe { libc::read(fd.fd, buffer.as_mut_ptr().cast(), buffer.len().min(LIMIT + 1 - total)) };
                if count < 0 { return Err("Provider status failed".into()); }
                if count == 0 { fd.fd = -1; continue; }
                total += count as usize;
                if total > LIMIT { return Err("Provider status exceeds byte budget".into()); }
                if index == 0 { output.extend_from_slice(&buffer[..count as usize]); } else { diagnostic = true; }
            }
        }
        let mut info: libc::siginfo_t = unsafe { std::mem::zeroed() };
        loop {
            if Instant::now() >= deadline { return Err("Provider status deadline".into()); }
            let rc = unsafe { libc::waitid(libc::P_PID, pid as libc::id_t, &mut info, libc::WEXITED | libc::WNOHANG | libc::WNOWAIT) };
            if rc != 0 { return Err("Provider status failed".into()); }
            if unsafe { info.si_pid() } != 0 { break; }
            std::thread::sleep(Duration::from_millis(5).min(deadline.saturating_duration_since(Instant::now())));
        }
        if info.si_code != libc::CLD_EXITED || unsafe { info.si_status() } != 0 { return Err("Provider status failed".into()); }
        if diagnostic { return Err("Provider emitted a diagnostic".into()); }
        let value = serde_json::from_slice(&output).map_err(|_| "Invalid provider status")?;
        let value = normalize(value)?;
        if Instant::now() > deadline { return Err("Provider status deadline".into()); }
        Ok(value)
    })();
    // The leader remains unreaped until the complete owned process group is retired.
    unsafe { libc::kill(-pid, libc::SIGKILL); }
    let _ = child.wait();
    result
}

fn run() -> Result<Option<Value>, String> {
    let args: Vec<_> = env::args_os().skip(1).collect();
    let action = args.first().and_then(|s| s.to_str()).unwrap_or("");
    let sending = action == "send";
    if sending {
        if !(2..=129).contains(&args.len()) || args[1..].iter().any(|p| !Path::new(p).is_absolute()) {
            return Err("Expected status, open, or send with absolute local paths".into());
        }
    } else if args.len() != 1 || !matches!(action, "status" | "open") {
        return Err("Expected status, open, or send with absolute local paths".into());
    }
    let provider = configured()?;
    if action == "status" {
        return Ok(Some(match provider { Some(path) => status(&path, Duration::from_secs(2))?,
            None => json!({"available":false,"active":false,"canSend":false,"detail":"Not configured"}) }));
    }
    let provider = provider.ok_or("No nearby sharing provider configured")?;
    if sending && !status(&provider, Duration::from_secs(2))?.get("canSend").and_then(Value::as_bool).unwrap_or(false) {
        return Err("Provider does not currently offer sending".into());
    }
    let error = Command::new(provider).args(&args).exec();
    Err(error.to_string())
}

fn main() -> ExitCode {
    let action = env::args_os().nth(1);
    match run() {
        Ok(Some(result)) => { println!("{result}"); ExitCode::SUCCESS }
        Ok(None) => ExitCode::SUCCESS,
        Err(_) if action.as_deref() == Some(std::ffi::OsStr::new("status")) => {
            println!("{}", json!({"available":false,"active":false,"canSend":false,"detail":"Unavailable"}));
            ExitCode::SUCCESS
        }
        Err(_) => { eprintln!("Nearby sharing provider is unavailable"); ExitCode::FAILURE }
    }
}

#[cfg(test)]
mod tests {
    use super::normalize;
    use serde_json::json;
    #[test]
    fn provider_schema_is_narrow() {
        assert_eq!(normalize(json!({"available":true,"active":false,"extra":4})).unwrap(),
            json!({"available":true,"active":false,"canSend":false,"detail":""}));
        for value in [json!({}), json!([]), json!({"available":1,"active":false}),
            json!({"available":false,"active":true}), json!({"available":false,"active":false,"canSend":true}),
            json!({"available":true,"active":false,"detail":"bad\nline"})] {
            assert!(normalize(value).is_err());
        }
    }
}
