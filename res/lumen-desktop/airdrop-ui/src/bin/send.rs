use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::os::fd::AsRawFd;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

fn peers(output: &str) -> Vec<(String, String)> {
    output.lines().filter_map(|line| {
        let (address, rest) = line.split_once(char::is_whitespace)?;
        if address.len() != 17 || !address.split(':').all(|part| part.len() == 2 && part.chars().all(|ch| ch.is_ascii_hexdigit())) { return None; }
        let (_, name) = rest.split_once("]:8770")?;
        let name = name.trim();
        let name = if name.is_empty() || name == "(no response)" || name == "(anonymous)" { "Nearby device" } else { name };
        Some((address.to_ascii_lowercase(), name.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(80).collect()))
    }).collect()
}

fn message(kind: &str, body: &str) {
    let _ = Command::new("timeout").args(["30", "zenity", &format!("--{kind}"), "--title=AirDrop", &format!("--text={body}")])
        .stdout(Stdio::null()).stderr(Stdio::null()).status();
}

fn progress(text: &str) -> Result<Child, String> {
    Command::new("zenity").args(["--progress", "--pulsate", "--auto-close", "--title=AirDrop", &format!("--text={text}"), "--percentage=5"])
        .stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::null()).spawn().map_err(|error| error.to_string())
}

fn streaming_child(command: &mut Command) -> Result<Child, String> {
    // A dedicated process group lets cancellation stop the transfer and descendants.
    unsafe { command.pre_exec(|| { if libc::setsid() == -1 { return Err(std::io::Error::last_os_error()); } Ok(()) }); }
    command.stdout(Stdio::piped()).stderr(Stdio::null()).spawn().map_err(|error| error.to_string())
}

fn lines(child: &mut Child) -> Receiver<String> {
    let (send, receive) = mpsc::channel();
    if let Some(output) = child.stdout.take() {
        std::thread::spawn(move || {
            for line in BufReader::new(output).lines().map_while(Result::ok) {
                if send.send(line).is_err() { break; }
            }
        });
    }
    receive
}

fn stop_group(child: &mut Child) {
    unsafe { libc::kill(-(child.id() as i32), libc::SIGTERM); }
    let until = Instant::now() + Duration::from_secs(3);
    while Instant::now() < until {
        if child.try_wait().ok().flatten().is_some() { return; }
        std::thread::sleep(Duration::from_millis(50));
    }
    unsafe { libc::kill(-(child.id() as i32), libc::SIGKILL); }
    let _ = child.wait();
}

fn lookup_peers() -> Result<Option<Vec<(String, String)>>, String> {
    let mut progress = progress("Finding nearby devices…")?;
    let mut lookup = streaming_child(Command::new("iphone-airdrop").args(["peers", "--stream"]))?;
    let output = lines(&mut lookup);
    let mut found = Vec::new();
    let mut first_seen = None;
    let deadline = Instant::now() + Duration::from_secs(34);
    let mut cancelled = false;
    while Instant::now() < deadline {
        if progress.try_wait().map_err(|error| error.to_string())?.is_some() { cancelled = true; break; }
        match output.recv_timeout(Duration::from_millis(200)) {
            Ok(line) => {
                for item in peers(&line) {
                    if !found.contains(&item) { found.push(item); first_seen.get_or_insert_with(Instant::now); }
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
            Err(mpsc::RecvTimeoutError::Timeout) => {}
        }
        if first_seen.is_some_and(|instant: Instant| instant.elapsed() >= Duration::from_millis(1500)) { break; }
    }
    if lookup.try_wait().map_err(|error| error.to_string())?.is_none() { stop_group(&mut lookup); }
    if progress.try_wait().map_err(|error| error.to_string())?.is_none() { let _ = progress.kill(); }
    let _ = progress.wait();
    Ok(if cancelled { None } else { Some(found) })
}

fn select_peer() -> Result<Option<String>, String> {
    let Some(found) = lookup_peers()? else { return Ok(None); };
    if found.is_empty() {
        message("warning", "No AirDrop device is nearby. Check the iPhone's AirDrop setting and try again.");
        return Ok(None);
    }
    let rows: Vec<String> = found.iter().map(|(address, name)| format!("{name}  ·  {address}")).collect();
    let mut picker = Command::new("rofi").args(["-no-config", "-dmenu", "-no-custom", "-p", "AirDrop to"])
        .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn().map_err(|error| error.to_string())?;
    if let Some(mut input) = picker.stdin.take() { let _ = writeln!(input, "{}", rows.join("\n")); }
    let selection = picker.wait_with_output().map_err(|error| error.to_string())?;
    if !selection.status.success() { return Ok(None); }
    let selected = String::from_utf8_lossy(&selection.stdout).trim().to_owned();
    Ok(rows.iter().position(|row| row == &selected).map(|index| found[index].0.clone()))
}

fn send_command(paths: &[PathBuf], address: &str) -> Command {
    let mut command = Command::new("iphone-airdrop");
    if paths.len() == 1 { command.arg("send").arg(&paths[0]).arg(address); }
    else { command.arg("send-files").arg(address).args(paths); }
    command
}

fn send(paths: &[PathBuf], address: &str) -> Result<i32, String> {
    let mut transfer = streaming_child(&mut send_command(paths, address))?;
    let mut progress = progress("Sending…")?;
    let output = lines(&mut transfer);
    let mut tail: Vec<String> = Vec::new();
    let mut cancelled = false;
    loop {
        if progress.try_wait().map_err(|error| error.to_string())?.is_some() && transfer.try_wait().map_err(|error| error.to_string())?.is_none() {
            cancelled = true; stop_group(&mut transfer); break;
        }
        match output.recv_timeout(Duration::from_millis(200)) {
            Ok(line) => {
                tail.push(line.clone()); if tail.len() > 12 { tail.remove(0); }
                let stage = if line.contains("DISCOVER") { Some("30\n# Finding device\n") } else if line.contains("ASK") { Some("60\n# Waiting for acceptance\n") } else if line.contains("UPLOAD") { Some("90\n# Uploading\n") } else { None };
                if let Some(stage) = stage { if progress.stdin.as_mut().is_some_and(|input| input.write_all(stage.as_bytes()).is_err()) { cancelled = true; stop_group(&mut transfer); break; } }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
            Err(mpsc::RecvTimeoutError::Timeout) => {}
        }
    }
    let result = transfer.wait().map_err(|error| error.to_string())?;
    if let Some(mut input) = progress.stdin.take() { let _ = input.write_all(if !cancelled && result.success() { b"100\n" } else { b"# Stopped\n" }); }
    if progress.try_wait().map_err(|error| error.to_string())?.is_none() { let _ = progress.kill(); }
    let _ = progress.wait();
    if cancelled { return Ok(130); }
    if !result.success() {
        let detail = tail.iter().rev().find(|line| line.to_ascii_lowercase().contains("declined")).map(String::as_str).unwrap_or("Transfer failed. Check AirDrop and try again.");
        message("error", detail);
    }
    Ok(result.code().unwrap_or(1))
}

fn lock(runtime: &Path) -> Result<Option<File>, String> {
    let file = OpenOptions::new().create(true).read(true).write(true).open(runtime.join("lumen-airdrop-send.lock")).map_err(|error| error.to_string())?;
    if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
        if std::io::Error::last_os_error().kind() == std::io::ErrorKind::WouldBlock { return Ok(None); }
        return Err(std::io::Error::last_os_error().to_string());
    }
    Ok(Some(file))
}

fn run(paths: &[PathBuf]) -> Result<i32, String> {
    if paths.is_empty() || !paths.iter().all(|path| path.is_file()) {
        message("error", "Select one or more files in Files before sending by AirDrop.");
        return Ok(2);
    }
    let runtime = std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(format!("/run/user/{}", unsafe { libc::getuid() })));
    let Some(_lock) = lock(&runtime)? else { return Ok(0); };
    match select_peer()? { Some(address) => send(paths, &address), None => Ok(1) }
}

fn main() {
    let paths = std::env::args_os().skip(1).map(PathBuf::from).collect::<Vec<_>>();
    let code = match run(&paths) { Ok(code) => code, Err(error) => { message("error", &format!("AirDrop could not start: {error}")); 1 } };
    std::process::exit(code);
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_only_named_peer_rows() {
        assert_eq!(peers("AirDrop is available\nc6:6b:14:cd:8a:63  -55 dBm  [fe80::1%awdl0]:8770  iPhone\n"), vec![("c6:6b:14:cd:8a:63".into(), "iPhone".into())]);
        assert!(peers("invalid  [fe80::1]:8770 iPhone").is_empty());
    }
    #[test]
    fn batch_uses_one_transfer() {
        let command = send_command(&["/tmp/a".into(), "/tmp/b".into()], "aa:bb:cc:dd:ee:ff");
        assert_eq!(command.get_args().map(|arg| arg.to_string_lossy().into_owned()).collect::<Vec<_>>(), vec!["send-files", "aa:bb:cc:dd:ee:ff", "/tmp/a", "/tmp/b"]);
    }
}
