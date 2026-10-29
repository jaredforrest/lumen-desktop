//! Anonymous loopback DAV and private GVfs acceptance for Network Locations.
use serde_json::{Value, json};
use std::env;
use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

struct Args {
    provider: Option<PathBuf>,
    client: PathBuf,
    inside: Option<String>,
}

fn arguments() -> Result<Args> {
    let mut provider = None;
    let mut client = None;
    let mut inside = None;
    let mut values = env::args_os().skip(1);
    while let Some(option) = values.next() {
        let value = values.next().ok_or("Every option needs a value")?;
        match option.to_str() {
            Some("--provider") => provider = Some(PathBuf::from(value)),
            Some("--client") => client = Some(PathBuf::from(value)),
            Some("--inside") => inside = Some(value.into_string().map_err(|_| "Invalid URI")?),
            _ => return Err(format!("Unknown option: {}", option.to_string_lossy()).into()),
        }
    }
    Ok(Args { provider, client: client.ok_or("--client is required")?, inside })
}

fn dav_body(depth: &str) -> Vec<u8> {
    let mut xml = String::from("<?xml version=\"1.0\"?><d:multistatus xmlns:d=\"DAV:\">");
    let count = if depth == "0" { 0 } else { 120 };
    for index in 0..=count {
        let (name, directory) = if index == 0 {
            ("/".to_owned(), true)
        } else {
            (format!("/fixture-{:03}.txt", index - 1), false)
        };
        xml.push_str("<d:response><d:href>");
        xml.push_str(&name);
        xml.push_str("</d:href><d:propstat><d:prop><d:resourcetype>");
        if directory { xml.push_str("<d:collection/>"); }
        xml.push_str("</d:resourcetype><d:getcontentlength>18</d:getcontentlength><d:getlastmodified>Mon, 19 Oct 2026 10:00:00 GMT</d:getlastmodified></d:prop><d:status>HTTP/1.1 200 OK</d:status></d:propstat></d:response>");
    }
    xml.push_str("</d:multistatus>");
    xml.into_bytes()
}

fn handle(mut stream: TcpStream) -> Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(5)))?;
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut request = String::new();
    if reader.read_line(&mut request)? == 0 { return Ok(()); }
    let mut words = request.split_whitespace();
    let method = words.next().ok_or("Missing HTTP method")?.to_owned();
    let path = words.next().ok_or("Missing HTTP path")?.to_owned();
    let mut depth = String::new();
    let mut content_length = 0usize;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line)? == 0 || line == "\r\n" || line == "\n" { break; }
        if let Some((key, value)) = line.split_once(':') {
            if key.eq_ignore_ascii_case("Depth") { depth = value.trim().to_owned(); }
            if key.eq_ignore_ascii_case("Content-Length") { content_length = value.trim().parse()?; }
        }
    }
    if content_length > 1024 * 1024 { return Err("DAV fixture body exceeded 1 MiB".into()); }
    let mut body = vec![0; content_length];
    reader.read_exact(&mut body)?;
    if path.starts_with("/slow") { thread::sleep(Duration::from_secs(1)); }
    let (status, response, extras) = match method.as_str() {
        "OPTIONS" => (200, Vec::new(), "DAV: 1,2\r\nAllow: OPTIONS,HEAD,GET,PROPFIND\r\n"),
        "HEAD" => (200, Vec::new(), ""),
        "GET" => (200, b"disposable fixture".to_vec(), ""),
        "PROPFIND" => (207, dav_body(&depth), "Content-Type: application/xml\r\n"),
        _ => (405, Vec::new(), ""),
    };
    let phrase = match status { 200 => "OK", 207 => "Multi-Status", _ => "Method Not Allowed" };
    write!(stream, "HTTP/1.1 {status} {phrase}\r\nServer: LumenFixture\r\nConnection: close\r\nContent-Length: {}\r\n{extras}\r\n", response.len())?;
    if method != "HEAD" { stream.write_all(&response)?; }
    Ok(())
}

struct DavServer {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
    port: u16,
}

impl DavServer {
    fn start() -> Result<Self> {
        let listener = TcpListener::bind(("127.0.0.1", 0))?;
        listener.set_nonblocking(true)?;
        let port = listener.local_addr()?.port();
        let stop = Arc::new(AtomicBool::new(false));
        let stopping = Arc::clone(&stop);
        let thread = thread::spawn(move || {
            while !stopping.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((stream, _)) => { thread::spawn(move || { let _ = handle(stream); }); }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => thread::sleep(Duration::from_millis(10)),
                    Err(_) => break,
                }
            }
        });
        Ok(Self { stop, thread: Some(thread), port })
    }
}

impl Drop for DavServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() { let _ = thread.join(); }
    }
}

fn receive(receiver: &mpsc::Receiver<std::result::Result<Value, String>>, events: &mut Vec<Value>, wanted: impl Fn(&Value) -> bool) -> Result<Value> {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() { return Err(format!("Network client timed out: {events:?}").into()); }
        let event = receiver.recv_timeout(remaining).map_err(|_| format!("Network client timed out or closed: {events:?}"))??;
        let found = wanted(&event);
        events.push(event.clone());
        if found { return Ok(event); }
    }
}

fn send(child: &mut Child, receiver: &mpsc::Receiver<std::result::Result<Value, String>>, events: &mut Vec<Value>, request: Value) -> Result<Value> {
    let input = child.stdin.as_mut().ok_or("Network client stdin closed")?;
    writeln!(input, "{request}")?;
    input.flush()?;
    let id = request["id"].clone();
    receive(receiver, events, |event| event["id"] == id)
}

fn inside(uri: &str, client: &Path) -> Result<()> {
    let mut child = Command::new(client).arg("--anonymous-fixture")
        .stdin(Stdio::piped()).stdout(Stdio::piped()).spawn()?;
    let stdout = child.stdout.take().ok_or("Missing network client stdout")?;
    let (sender, receiver) = mpsc::channel::<std::result::Result<Value, String>>();
    let reader = thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            let event = line.map_err(|error| error.to_string())
                .and_then(|line| serde_json::from_str(&line).map_err(|error| error.to_string()));
            if sender.send(event).is_err() { break; }
        }
    });
    let result = (|| -> Result<()> {
        let mut events = Vec::new();
        let ready = receive(&receiver, &mut events, |event| event["event"] == "ready")?;
        if !ready["protocols"].as_array().is_some_and(|protocols| protocols.iter().any(|item| item == "dav")) {
            return Err(format!("Network client did not advertise DAV: {ready}").into());
        }
        let secret_uri = format!("{uri}?token=hidden-secret");
        let rejected = send(&mut child, &receiver, &mut events, json!({"id":10,"action":"connect","uri":secret_uri}))?;
        if !rejected["error"].as_str().is_some_and(|error| !error.is_empty())
            || events.iter().any(|event| event.to_string().contains("hidden-secret")) {
            return Err(format!("Credential URI was accepted or leaked: {rejected}").into());
        }
        let rejected = send(&mut child, &receiver, &mut events, json!({"id":11,"action":"connect","uri":uri,"password":"hidden-secret"}))?;
        if !rejected["error"].as_str().is_some_and(|error| !error.is_empty())
            || events.iter().any(|event| event.to_string().contains("hidden-secret")) {
            return Err(format!("Credential field was accepted or leaked: {rejected}").into());
        }
        let connected = send(&mut child, &receiver, &mut events, json!({"id":1,"action":"connect","uri":uri}))?;
        if connected["connected"] != true { return Err(format!("DAV connect failed: {connected}").into()); }
        let connection = connected["connectionId"].clone();
        let browsed = send(&mut child, &receiver, &mut events, json!({"id":2,"action":"browse","connectionId":connection}))?;
        let rows = browsed["rows"].as_array().ok_or("DAV browse did not return rows")?;
        if !browsed["error"].is_null() || rows.len() != 100 || browsed["limited"] != true
            || !rows.iter().all(|row| row["name"].as_str().is_some_and(|name| name.starts_with("fixture-"))) {
            return Err(format!("DAV browse was not bounded: {browsed}").into());
        }
        let outside = send(&mut child, &receiver, &mut events, json!({"id":12,"action":"browse","connectionId":connection,"uri":"dav://127.0.0.1:1/outside"}))?;
        if !outside["error"].as_str().is_some_and(|error| !error.is_empty()) || outside.get("rows").is_some() {
            return Err(format!("Out-of-scope browse was accepted: {outside}").into());
        }
        let input = child.stdin.as_mut().ok_or("Network client stdin closed")?;
        writeln!(input, "{}", json!({"id":4,"action":"browse","connectionId":connection,"uri":format!("{uri}slow")}))?;
        writeln!(input, "{}", json!({"id":5,"action":"cancel","target":4}))?;
        input.flush()?;
        let cancelled = receive(&receiver, &mut events, |event| event["id"] == 4)?;
        if cancelled["cancelled"] != true || cancelled.get("rows").is_some() {
            return Err(format!("Cancelled browse produced rows: {cancelled}").into());
        }
        let acknowledged = receive(&receiver, &mut events, |event| event["id"] == 5)?;
        if acknowledged["cancelled"] != 4 { return Err(format!("Cancellation was not acknowledged: {acknowledged}").into()); }
        thread::sleep(Duration::from_millis(1200));
        let disconnected = send(&mut child, &receiver, &mut events, json!({"id":3,"action":"disconnect","connectionId":connection}))?;
        if disconnected["disconnected"] != true || events.iter().filter(|event| event["id"] == 4).count() != 1 {
            return Err(format!("DAV disconnect or late cancellation failed: {disconnected}").into());
        }
        println!("{}", json!({"realDavConnectBrowseDisconnect":true,"credentialCollection":"none; anonymous loopback fixture","rows":rows.len()}));
        Ok(())
    })();
    drop(child.stdin.take());
    let deadline = Instant::now() + Duration::from_secs(5);
    while child.try_wait()?.is_none() && Instant::now() < deadline { thread::sleep(Duration::from_millis(20)); }
    if child.try_wait()?.is_none() { let _ = child.kill(); }
    let _ = child.wait();
    let _ = reader.join();
    result
}

fn outer(args: &Args) -> Result<()> {
    let provider = args.provider.as_ref().ok_or("--provider is required for the outer fixture")?;
    let private = tempfile::Builder::new().prefix("lumen-network-provider-").tempdir()?;
    let server = DavServer::start()?;
    let config = private.path().join("bus.conf");
    fs::write(&config, format!("<busconfig><type>session</type><listen>unix:tmpdir={}</listen><auth>EXTERNAL</auth><servicedir>{}/share/dbus-1/services</servicedir><policy context=\"default\"><allow send_destination=\"*\"/><allow receive_sender=\"*\"/><allow own=\"*\"/></policy></busconfig>", private.path().display(), provider.display()))?;
    let root = private.path().to_str().ok_or("Non-UTF8 private path")?;
    let mut child = Command::new("dbus-run-session")
        .arg(format!("--config-file={}", config.display()))
        .arg("--").arg(env::current_exe()?)
        .arg("--inside").arg(format!("dav://127.0.0.1:{}/", server.port))
        .arg("--client").arg(&args.client)
        .env("HOME", root)
        .env("XDG_CONFIG_HOME", root)
        .env("XDG_DATA_HOME", root)
        .env("XDG_CACHE_HOME", root)
        .env("XDG_STATE_HOME", root)
        .env("XDG_RUNTIME_DIR", root)
        .env("GIO_EXTRA_MODULES", provider.join("lib/gio/modules"))
        .env("GVFS_DISABLE_FUSE", "1")
        .env("XDG_DATA_DIRS", provider.join("share"))
        .env("GIO_USE_VFS", "gvfs")
        .process_group(0)
        .spawn()?;
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if let Some(status) = child.try_wait()? {
            return if status.success() { Ok(()) } else { Err(format!("Private DAV fixture exited {status}").into()) };
        }
        if Instant::now() >= deadline {
            unsafe { libc::killpg(child.id() as i32, libc::SIGKILL); }
            let _ = child.wait();
            return Err("Private DAV fixture timed out".into());
        }
        thread::sleep(Duration::from_millis(20));
    }
}

fn main() {
    let result = arguments().and_then(|args| match &args.inside {
        Some(uri) => inside(uri, &args.client),
        None => outer(&args),
    });
    if let Err(error) = result {
        eprintln!("Network provider fixture failed: {error}");
        std::process::exit(1);
    }
}
