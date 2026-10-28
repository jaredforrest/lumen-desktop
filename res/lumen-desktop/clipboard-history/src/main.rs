use lumen_clipboard_history::{History, MAX_BYTES, Result, process_identity};
use serde_json::{Value, json};
use std::io::{Read, Write};
use std::os::fd::AsRawFd;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

static STOP: AtomicBool = AtomicBool::new(false);
extern "C" fn stop(_: libc::c_int) { STOP.store(true, Ordering::SeqCst); }

fn service(action: &str, required: bool) -> Result<()> {
    let systemctl = std::env::var_os("LUMEN_CLIPBOARD_SYSTEMCTL").unwrap_or_else(|| "systemctl".into());
    let status = Command::new("timeout").arg("8").arg(systemctl).args(["--user", action, "lumen-clipboard-history.service"])
        .stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).status().map_err(|error| error.to_string())?;
    if required && !status.success() { return Err("Clipboard integration is unavailable".into()); }
    Ok(())
}

fn terminate(child: &mut Child) {
    if child.try_wait().ok().flatten().is_some() { return; }
    unsafe { libc::kill(child.id() as i32, libc::SIGTERM); }
    let until = Instant::now() + Duration::from_secs(3);
    while Instant::now() < until {
        if child.try_wait().ok().flatten().is_some() { return; }
        std::thread::sleep(Duration::from_millis(30));
    }
    let _ = child.kill();
    let _ = child.wait();
}

fn watch(store: &History) -> Result<Option<Value>> {
    if !store.enabled() { store.prune()?; return Ok(None); }
    STOP.store(false, Ordering::SeqCst);
    unsafe { libc::signal(libc::SIGTERM, stop as *const () as libc::sighandler_t); }
    let executable = std::env::var_os("LUMEN_CLIPBOARD_OBSERVER").unwrap_or_else(|| "lumen-clipboard-observer".into());
    let mut child = Command::new(executable).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn().map_err(|error| error.to_string())?;
    let owner = process_identity(std::process::id()).ok_or("Clipboard process identity unavailable")?;
    let observer = process_identity(child.id()).ok_or("Clipboard process identity unavailable")?;
    let epoch = store.barrier()?["epoch"].as_str().ok_or("Invalid clear barrier")?.to_owned();
    let result = (|| -> Result<()> {
        store.publish_observer(&owner, &observer, false, &epoch)?;
        let mut output = child.stdout.take().ok_or("Clipboard observer unavailable")?;
        let fd = output.as_raw_fd();
        let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
        if flags < 0 || unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } != 0 { return Err("Clipboard observer unavailable".into()); }
        let mut received = Vec::new();
        let mut ready = false;
        let deadline = Instant::now() + Duration::from_secs(3);
        while !STOP.load(Ordering::SeqCst) && child.try_wait().map_err(|error| error.to_string())?.is_none() && store.enabled() {
            store.prune()?;
            if !ready && Instant::now() > deadline { return Err("Clipboard observer unavailable".into()); }
            let mut poll = libc::pollfd { fd, events: libc::POLLIN, revents: 0 };
            let available = unsafe { libc::poll(&mut poll, 1, 500) };
            if available < 0 { if std::io::Error::last_os_error().kind() == std::io::ErrorKind::Interrupted { continue; } return Err("Clipboard observer unavailable".into()); }
            if available == 0 { continue; }
            let mut chunk = [0_u8; 65_536];
            let count = match output.read(&mut chunk) {
                Ok(0) => break,
                Ok(count) => count,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock || error.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(_) => return Err("Clipboard observer unavailable".into()),
            };
            received.extend_from_slice(&chunk[..count]);
            if received.len() > 2 * (MAX_BYTES + 8) { return Err("Invalid clipboard transport".into()); }
            while received.len() >= 5 {
                let kind = received[0];
                let size = u32::from_be_bytes(received[1..5].try_into().map_err(|_| "Invalid clipboard transport")?) as usize;
                if size > MAX_BYTES + 8 || !matches!(kind, 1 | 2) || kind == 1 && (ready || size != 0) || kind == 2 && (!ready || size < 8) { return Err("Invalid clipboard transport".into()); }
                if received.len() < 5 + size { break; }
                let payload = received[5..5+size].to_vec();
                received.drain(..5+size);
                if kind == 1 {
                    ready = true;
                    store.publish_observer(&owner, &observer, true, &epoch)?;
                    println!("{}", json!({"watching":true}));
                } else {
                    let stamp = u64::from_be_bytes(payload[..8].try_into().map_err(|_| "Invalid clipboard transport")?) as f64 / 1_000_000_000.0;
                    let Ok(text) = std::str::from_utf8(&payload[8..]) else { continue; };
                    println!("{}", store.ingest(Some(text), false, Some(stamp), Some(&epoch))?);
                }
            }
        }
        if store.enabled() && !STOP.load(Ordering::SeqCst) { return Err("Clipboard observer stopped".into()); }
        Ok(())
    })();
    terminate(&mut child);
    let _ = store.retire_observer(&owner);
    if !store.enabled() { let _ = store.prune(); }
    result.map(|()| None)
}

fn copy(text: &str) -> Result<()> {
    let mut child = Command::new("timeout").args(["5", "wl-copy", "--type", "text/plain;charset=utf-8"])
        .stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::null()).spawn().map_err(|error| error.to_string())?;
    if let Some(mut input) = child.stdin.take() { input.write_all(text.as_bytes()).map_err(|error| error.to_string())?; }
    if !child.wait().map_err(|error| error.to_string())?.success() { return Err("Clipboard copy failed".into()); }
    Ok(())
}

fn ingest(store: &History) -> Result<Value> {
    if !store.enabled() { return Ok(json!({"recorded":false})); }
    let mut bytes = Vec::new();
    std::io::stdin().take((MAX_BYTES + 1) as u64).read_to_end(&mut bytes).map_err(|error| error.to_string())?;
    let types = Command::new("timeout").args(["3", "wl-paste", "--list-types"]).stdin(Stdio::null()).output().map_err(|error| error.to_string())?;
    let sensitive = !types.status.success() || matches!(std::env::var("CLIPBOARD_STATE").ok().as_deref(),Some("sensitive" | "clear" | "nil"))
        || [b"x-kde-passwordManagerHint".as_slice(),b"application/x-keepassxc".as_slice()].iter().any(|marker| types.stdout.windows(marker.len()).any(|window| window == *marker));
    let text = std::str::from_utf8(&bytes).ok();
    store.ingest(text, sensitive, None, None)
}

fn action(store: &History, args: &[String]) -> Result<Option<Value>> {
    match args.first().map(String::as_str).ok_or("Clipboard action required")? {
        "status" => store.status().map(Some),
        "list" => {
            let query = args.windows(2).find(|pair| pair[0] == "--query").map(|pair| pair[1].as_str()).unwrap_or("");
            store.list(query).map(Some)
        }
        "enable" => {
            let new = !store.enabled();
            store.set_enabled(true)?;
            let result = (|| -> Result<()> {
                if new { service("stop", true)?; }
                service("start", true)?;
                if store.enabled() && !store.await_observer() { return Err("Clipboard observer unavailable".into()); }
                Ok(())
            })();
            if result.is_err() { store.set_enabled(false)?; return Err("Clipboard integration is unavailable; history remains disabled".into()); }
            store.status().map(Some)
        }
        "disable" => { let result = store.set_enabled(false)?; let _ = service("stop", false); Ok(Some(result)) }
        "clear" => {
            let enabled = store.enabled();
            let result = (|| -> Result<Value> {
                if enabled { service("stop", true)?; }
                store.clear()?;
                if enabled && store.enabled() { service("start", true)?; }
                if enabled && store.enabled() { store.await_observer(); }
                store.status()
            })();
            match result { Ok(value) => Ok(Some(value)), Err(_) => { store.set_enabled(false)?; store.clear()?; Err("Clipboard collection unavailable; history cleared and disabled".into()) } }
        }
        "copy" => store.copy(args.get(1).map(String::as_str).ok_or("Clipboard item required")?, copy).map(Some),
        "ingest" => ingest(store).map(Some),
        "watch" => watch(store),
        _ => Err("Unsupported clipboard action".into()),
    }
}

fn main() {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let result = History::from_env().and_then(|store| action(&store, &args));
    match result {
        Ok(Some(value)) => println!("{value}"),
        Ok(None) => (),
        Err(_) => {
            let mut state = History::from_env().and_then(|store| store.status()).unwrap_or_else(|_| json!({"entries":[]}));
            state["error"] = json!("Clipboard operation unavailable; retry or disable history");
            println!("{state}");
            std::process::exit(1);
        }
    }
}
