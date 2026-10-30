//! Actual Speech Dispatcher transport acceptance on a private Unix socket.
use regex::Regex;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::env;
use std::error::Error;
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::{FileTypeExt, MetadataExt, PermissionsExt, symlink};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

type Result<T> = std::result::Result<T, Box<dyn Error>>;

fn fail(message: impl Into<String>) -> Box<dyn Error> {
    message.into().into()
}

fn arguments() -> Result<(PathBuf, PathBuf, PathBuf)> {
    let mut dispatcher = None;
    let mut source = None;
    let mut output = None;
    let mut args = env::args_os().skip(1);
    while let Some(key) = args.next() {
        let value = PathBuf::from(args.next().ok_or("Missing option value")?);
        match key.to_str() {
            Some("--dispatcher") => dispatcher = Some(value),
            Some("--dispatcher-source") => source = Some(value),
            Some("--output") => output = Some(value),
            _ => return Err(fail("Unknown speech fixture option")),
        }
    }
    Ok((
        dispatcher.ok_or("Missing dispatcher")?,
        source.ok_or("Missing dispatcher source")?,
        output.ok_or("Missing output")?,
    ))
}

fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn source_proof(archive: &Path) -> Result<Value> {
    let listing = Command::new(env!("LUMEN_TAR"))
        .args(["-tf"])
        .arg(archive)
        .output()?;
    if !listing.status.success() {
        return Err(fail("Cannot list pinned speech source archive"));
    }
    let listing = String::from_utf8(listing.stdout)?;
    let member = listing
        .lines()
        .find(|line| line.ends_with("/src/modules/generic.c"))
        .ok_or("Missing generic module source")?;
    let extraction = Command::new(env!("LUMEN_TAR"))
        .args(["-xOf"])
        .arg(archive)
        .arg(member)
        .output()?;
    if !extraction.status.success() || extraction.stdout.len() > 1024 * 1024 {
        return Err(fail("Unexpected generic module source"));
    }
    let body = extraction.stdout;
    let sha = digest(&body);
    if sha != "db6ee64b64ff1a057f452a3567af7d4ad4b4538bdacb7fcb86e9e6f7e36b1c7c"
        || !body
            .windows(b"message = g_string_append(message, \"'\\\\''\");".len())
            .any(|part| part == b"message = g_string_append(message, \"'\\\\''\");")
    {
        return Err(fail(
            "Review changed speech module quoting before using this observer",
        ));
    }
    Ok(json!({"archive": archive, "genericSourceSha256": sha, "quotedDataArgument": true}))
}

fn shell_proof(dispatcher: &Path) -> Result<Value> {
    let proof: Value = serde_json::from_slice(&fs::read(
        dispatcher.join("share/lumen-fixtures/speech-shell.json"),
    )?)?;
    let shell = Path::new(proof["shell"].as_str().ok_or("Missing shell proof")?);
    let binary = dispatcher.join("libexec/speech-dispatcher-modules/sd_generic");
    let content = fs::read(&binary)?;
    if !shell.starts_with("/nix/store")
        || shell.file_name().is_none_or(|name| name != "bash")
        || !shell.is_file()
        || !content
            .windows(shell.as_os_str().len())
            .any(|part| part == shell.as_os_str().as_encoded_bytes())
    {
        return Err(fail("Generic fixture does not contain its pinned shell"));
    }
    let mut object = proof.as_object().ok_or("Invalid shell proof")?.clone();
    object.insert("genericBinarySha256".into(), json!(digest(&content)));
    Ok(Value::Object(object))
}

struct RuntimeLease {
    identity: Value,
    armed: bool,
}
impl RuntimeLease {
    fn adopt(runtime: tempfile::TempDir) -> Result<Self> {
        let info = fs::symlink_metadata(runtime.path())?;
        let identity = json!({"path": runtime.path(), "device": info.dev(), "inode": info.ino(), "uid": info.uid()});
        let _ = runtime.keep();
        Ok(Self {
            identity,
            armed: true,
        })
    }
    fn release(mut self) {
        self.armed = false;
    }
}
impl Drop for RuntimeLease {
    fn drop(&mut self) {
        if self.armed {
            if let Err(error) = retire_runtime(&self.identity) {
                eprintln!("speech runtime retained: {error}");
            }
        }
    }
}

// A failed diagnostic write or signal must still reap the exact owned child.
struct OwnedChild(Child);
impl std::ops::Deref for OwnedChild {
    type Target = Child;
    fn deref(&self) -> &Child {
        &self.0
    }
}
impl std::ops::DerefMut for OwnedChild {
    fn deref_mut(&mut self) -> &mut Child {
        &mut self.0
    }
}
impl Drop for OwnedChild {
    fn drop(&mut self) {
        if !matches!(self.0.try_wait(), Ok(Some(_))) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
}

fn configure(
    output: &Path,
    dispatcher: &Path,
) -> Result<(PathBuf, PathBuf, Vec<String>, RuntimeLease)> {
    let proof = shell_proof(dispatcher)?;
    fs::write(
        output.join("execution-shell-proof.json"),
        format!("{}\n", serde_json::to_string_pretty(&proof)?),
    )?;
    let config = output.join("speech-config");
    fs::create_dir(&config)?;
    fs::create_dir(config.join("modules"))?;
    let logs = output.join("speech-logs");
    fs::create_dir(&logs)?;
    let modules = output.join("speech-modules");
    fs::create_dir(&modules)?;
    symlink(
        dispatcher.join("libexec/speech-dispatcher-modules/sd_generic"),
        modules.join("sd_generic"),
    )?;
    // sockaddr_un has a 107-byte pathname limit on Linux. Artifact paths are
    // intentionally unrestricted; keep the live endpoint in an owned short lease.
    let runtime = tempfile::Builder::new()
        .prefix("lumen-speech-")
        .permissions(fs::Permissions::from_mode(0o700))
        .tempdir_in("/tmp")?;
    let socket = runtime.path().join("speech.sock");
    let runtime = RuntimeLease::adopt(runtime)?;
    fs::write(
        output.join("speech-runtime.json"),
        serde_json::to_vec_pretty(&runtime.identity)?,
    )?;
    let protocol = output.join("speech-protocol.log");
    let empty_bin = output.join("speech-empty-bin");
    fs::create_dir(&empty_bin)?;
    fs::write(
        config.join("modules/observer.conf"),
        format!(
            "Debug 1\nGenericExecuteSynth \"{} '$DATA'\"\nAddVoice \"en\" \"MALE1\" \"observer\"\nDefaultVoice \"observer\"\nGenericDefaultCharset \"UTF-8\"\nGenericLanguage \"en\" \"en\" \"UTF-8\"\n",
            env!("LUMEN_TRUE")
        ),
    )?;
    fs::write(
        config.join("speechd.conf"),
        format!(
            "CommunicationMethod \"unix_socket\"\nSocketPath \"{}\"\nLogLevel 5\nLogDir \"{}\"\nCustomLogFile \"protocol\" \"{}\"\nAddModule \"observer\" \"sd_generic\" \"observer.conf\"\nDefaultModule \"observer\"\nDefaultLanguage \"en\"\nDisableAutoSpawn\n",
            socket.display(),
            logs.display(),
            protocol.display()
        ),
    )?;
    let command = vec![
        dispatcher
            .join("bin/speech-dispatcher")
            .display()
            .to_string(),
        "--run-single".into(),
        format!("--config-dir={}", config.display()),
        format!("--module-dir={}", modules.display()),
        format!("--socket-path={}", socket.display()),
        format!("--pid-file={}", output.join("speech.pid").display()),
        "--timeout=0".into(),
    ];
    Ok((socket, protocol, command, runtime))
}

fn retire_runtime(value: &Value) -> Result<()> {
    let path = Path::new(
        value["path"]
            .as_str()
            .ok_or("Missing speech runtime path")?,
    );
    if path.parent() != Some(Path::new("/tmp"))
        || !path
            .file_name()
            .is_some_and(|name| name.as_encoded_bytes().starts_with(b"lumen-speech-"))
    {
        return Err(fail("Invalid speech runtime lease path"));
    }
    let info = match fs::symlink_metadata(path) {
        Ok(info) => info,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
    };
    if !info.is_dir()
        || info.dev() != value["device"].as_u64().ok_or("Missing runtime device")?
        || info.ino() != value["inode"].as_u64().ok_or("Missing runtime inode")?
        || u64::from(info.uid()) != value["uid"].as_u64().ok_or("Missing runtime uid")?
        || info.uid() != unsafe { libc::geteuid() }
        || info.mode() & 0o777 != 0o700
    {
        return Err(fail("Speech runtime lease identity changed"));
    }
    let endpoint = path.join("speech.sock");
    if fs::symlink_metadata(&endpoint).is_ok_and(|metadata| metadata.file_type().is_socket()) {
        fs::remove_file(endpoint)?;
    }
    fs::remove_dir(path)?;
    Ok(())
}

fn retire(root: &Path) -> Result<()> {
    let lease = root.join("speech-runtime.json");
    if lease.exists() {
        retire_runtime(&serde_json::from_slice(&fs::read(&lease)?)?)?;
    }
    let socket = root.join("speech.sock");
    if fs::symlink_metadata(&socket).is_ok_and(|info| info.file_type().is_socket()) {
        fs::remove_file(socket)?;
    }
    let module = root.join("speech-modules/sd_generic");
    if module.is_symlink() {
        fs::remove_file(module)?;
    }
    Ok(())
}

fn utterances(debug: &str) -> Result<Vec<String>> {
    let pattern = Regex::new(r"SPEECH OUTPUT: (.*)")?;
    Ok(pattern
        .captures_iter(debug)
        .map(|row| row[1].to_owned())
        .collect())
}

fn plain_text(message: &str) -> String {
    if !message.trim_start().starts_with("<speak") {
        return message.to_owned();
    }
    let mut reader = quick_xml::Reader::from_str(message);
    let mut plain = String::new();
    loop {
        match reader.read_event() {
            Ok(quick_xml::events::Event::Text(value)) => {
                let decoded = match value.decode() {
                    Ok(text) => text,
                    Err(_) => return message.to_owned(),
                };
                let unescaped = match quick_xml::escape::unescape(&decoded) {
                    Ok(text) => text,
                    Err(_) => return message.to_owned(),
                };
                plain.push_str(&unescaped);
            }
            Ok(quick_xml::events::Event::CData(value)) => match value.decode() {
                Ok(text) => plain.push_str(&text),
                Err(_) => return message.to_owned(),
            },
            Ok(quick_xml::events::Event::GeneralRef(value)) => match value.resolve_char_ref() {
                Ok(Some(character)) => plain.push(character),
                Ok(None) => {
                    let decoded = match value.decode() {
                        Ok(text) => text,
                        Err(_) => return message.to_owned(),
                    };
                    match quick_xml::escape::resolve_predefined_entity(&decoded) {
                        Some(text) => plain.push_str(text),
                        None => return message.to_owned(),
                    }
                }
                Err(_) => return message.to_owned(),
            },
            Ok(quick_xml::events::Event::Eof) => return plain,
            Err(_) => return message.to_owned(),
            _ => {}
        }
    }
}

fn transport_messages(protocol: &str) -> Result<Vec<Value>> {
    let frames = Regex::new(r"(?s)\s(\d+):(DATA|REPLY):\|(.*?)\|(?: \(\d+\))?\n")?;
    let queued = Regex::new(r"225-(\d+)")?;
    let lifecycle = Regex::new(r"(?s)70[123]-(\d+).*?70[123] (BEGIN|END|CANCELED)")?;
    let mut clients = BTreeMap::<String, String>::new();
    let mut collecting = BTreeMap::<String, Vec<String>>::new();
    let mut pending = BTreeMap::<String, Value>::new();
    let mut messages = Vec::<Value>::new();
    let mut by_id = BTreeMap::<u64, usize>::new();
    for frame in frames.captures_iter(protocol) {
        let descriptor = &frame[1];
        let kind = &frame[2];
        let value = frame[3]
            .replace("\r\n", "\n")
            .trim_end_matches('\n')
            .to_owned();
        if kind == "DATA" {
            if let Some(client) = value.strip_prefix("SET self CLIENT_NAME ") {
                clients.insert(descriptor.to_owned(), client.to_owned());
            } else if collecting.contains_key(descriptor) {
                if value == "." {
                    let body = collecting.remove(descriptor).unwrap_or_default().join("\n");
                    pending.insert(descriptor.to_owned(), json!({"client":clients.get(descriptor).cloned().unwrap_or_default(),"text":body,"plainText":plain_text(&body),"offset":frame.get(0).map_or(0, |value| value.start()),"events":[]}));
                } else if let Some(lines) = collecting.get_mut(descriptor) {
                    lines.push(if value.starts_with("..") {
                        value[1..].to_owned()
                    } else {
                        value.clone()
                    });
                }
            } else if value == "SPEAK" {
                collecting.insert(descriptor.to_owned(), Vec::new());
            }
        } else if value.contains("225 OK MESSAGE QUEUED") && pending.contains_key(descriptor) {
            if let Some(found) = queued.captures(&value) {
                let id: u64 = found[1].parse()?;
                let mut message = pending.remove(descriptor).ok_or("Missing queued speech")?;
                message["id"] = json!(id);
                by_id.insert(id, messages.len());
                messages.push(message);
            }
        } else if let Some(event) = lifecycle.captures(&value) {
            let id: u64 = event[1].parse()?;
            if let Some(index) = by_id.get(&id) {
                messages[*index]["events"]
                    .as_array_mut()
                    .ok_or("Invalid speech events")?
                    .push(json!(&event[2]));
            }
        }
    }
    Ok(messages)
}

fn utility_command() -> Result<bool> {
    let mut args = env::args_os().skip(1);
    let Some(action) = args.next() else {
        return Ok(false);
    };
    let Some(action) = action.to_str() else {
        return Err(fail("Invalid speech utility command"));
    };
    if action.starts_with("--") {
        return Ok(false);
    }
    match action {
        "source-proof" => {
            let path = PathBuf::from(args.next().ok_or("Missing source archive")?);
            println!("{}", serde_json::to_string(&source_proof(&path)?)?);
        }
        "configure" => {
            let root = PathBuf::from(args.next().ok_or("Missing output directory")?);
            let dispatcher = PathBuf::from(args.next().ok_or("Missing dispatcher")?);
            let (socket, protocol, command, runtime) = configure(&root, &dispatcher)?;
            println!(
                "{}",
                json!({"address":format!("unix_socket:{}",socket.display()),"socket":socket,"protocol":protocol,"emptyBin":root.join("speech-empty-bin"),"command":command})
            );
            runtime.release();
        }
        "retire" => retire(&PathBuf::from(
            args.next().ok_or("Missing output directory")?,
        ))?,
        "messages" => {
            let path = PathBuf::from(args.next().ok_or("Missing protocol file")?);
            println!(
                "{}",
                serde_json::to_string(&transport_messages(&fs::read_to_string(path)?)?)?
            );
        }
        "utterances" => {
            let path = PathBuf::from(args.next().ok_or("Missing debug file")?);
            println!(
                "{}",
                serde_json::to_string(&utterances(&fs::read_to_string(path)?)?)?
            );
        }
        _ => return Err(fail("Unknown speech utility command")),
    }
    if args.next().is_some() {
        return Err(fail("Unexpected speech utility argument"));
    }
    Ok(true)
}

struct Reply {
    code: u16,
    lines: Vec<String>,
}
struct Ssip {
    input: BufReader<UnixStream>,
    output: UnixStream,
    events: Vec<(u16, u64)>,
}
impl Ssip {
    fn connect(path: &Path) -> Result<Self> {
        let stream = UnixStream::connect(path)?;
        stream.set_read_timeout(Some(Duration::from_secs(5)))?;
        Ok(Self {
            input: BufReader::new(stream.try_clone()?),
            output: stream,
            events: Vec::new(),
        })
    }
    fn receive(&mut self) -> Result<Reply> {
        let mut lines = Vec::new();
        let code = loop {
            let mut line = String::new();
            if self.input.read_line(&mut line)? == 0 {
                return Err(fail("Speech Dispatcher closed the socket"));
            }
            let line = line.trim_end_matches(['\r', '\n']);
            let code: u16 = line.get(..3).ok_or("Malformed SSIP response")?.parse()?;
            let sep = line
                .as_bytes()
                .get(3)
                .copied()
                .ok_or("Malformed SSIP separator")?;
            lines.push(line.get(4..).unwrap_or("").to_owned());
            if sep == b' ' {
                break code;
            }
            if sep != b'-' {
                return Err(fail("Malformed SSIP continuation"));
            }
        };
        Ok(Reply { code, lines })
    }
    fn response(&mut self) -> Result<Reply> {
        loop {
            let reply = self.receive()?;
            if (700..800).contains(&reply.code) {
                let id: u64 = reply
                    .lines
                    .first()
                    .ok_or("Missing SSIP event id")?
                    .parse()?;
                self.events.push((reply.code, id));
            } else {
                return Ok(reply);
            }
        }
    }
    fn send(&mut self, message: &str) -> Result<Reply> {
        self.output.write_all(message.as_bytes())?;
        self.output.write_all(b"\r\n")?;
        let reply = self.response()?;
        if !(200..300).contains(&reply.code) {
            return Err(fail(format!(
                "SSIP command rejected: {} {:?}",
                reply.code, reply.lines
            )));
        }
        Ok(reply)
    }
    fn speak(&mut self, text: &str) -> Result<u64> {
        self.send("SPEAK")?;
        let escaped = text.replace("\r\n.", "\r\n..");
        let escaped = if escaped.starts_with('.') {
            format!(".{escaped}")
        } else {
            escaped
        };
        self.output.write_all(escaped.as_bytes())?;
        self.output.write_all(b"\r\n.\r\n")?;
        let reply = self.response()?;
        if reply.code != 225 {
            return Err(fail(format!("Speech was not queued: {}", reply.code)));
        }
        Ok(reply
            .lines
            .first()
            .ok_or("Missing speech message ID")?
            .parse()?)
    }
    fn wait_event(&mut self, id: u64, code: u16) -> Result<()> {
        let deadline = Instant::now() + Duration::from_secs(5);
        while !self.events.contains(&(code, id)) {
            if Instant::now() >= deadline {
                return Err(fail(format!("Speech event {code} timed out")));
            }
            let reply = self.receive()?;
            if (700..800).contains(&reply.code) {
                let observed: u64 = reply
                    .lines
                    .first()
                    .ok_or("Missing SSIP event id")?
                    .parse()?;
                self.events.push((reply.code, observed));
            } else {
                return Err(fail("Unexpected SSIP response while waiting for event"));
            }
        }
        Ok(())
    }
}

fn run_transport(socket: &Path, protocol: &Path, output: &Path, private: &Path) -> Result<()> {
    let mut client = Ssip::connect(socket)?;
    client.send("SET self CLIENT_NAME unknown:PrivateObserver:default")?;
    for event in ["begin", "end", "cancel"] {
        client.send(&format!("SET self NOTIFICATION {event} on"))?;
    }
    client.send("SET self OUTPUT_MODULE observer")?;
    let marker = private.join("must-not-exist");
    let payload = format!(
        "Transport sentinel ' $(touch {}) `touch {}`\nsecond line",
        marker.display(),
        marker.display()
    );
    let completed = client.speak(&payload)?;
    client.wait_event(completed, 702)?;
    let canceled = client.speak(&"Cancel sentinel ".repeat(1024))?;
    client.wait_event(canceled, 701)?;
    thread::sleep(Duration::from_millis(5));
    client.send("CANCEL self")?;
    client.wait_event(canceled, 703)?;
    if marker.exists() {
        return Err(fail("Speech payload escaped fixed ignored argument"));
    }
    let text = fs::read_to_string(protocol)?;
    if !text.contains("Transport sentinel") {
        return Err(fail("Transport payload missing from protocol log"));
    }
    let result = json!({"passed":true,"events":[["('begin',)","{}"],["('end',)","{}"]],"cancelEvents":[["('begin',)","{}"],["('cancel',)","{}"]],"checks":["real Speech Dispatcher Unix transport","fixed generic no-audio module","actual text accepted","completion callbacks","private cancellation"]});
    fs::write(
        output.join("result.json"),
        serde_json::to_string_pretty(&result)?,
    )?;
    println!("PASS real private speech transport");
    Ok(())
}

fn wait_for_socket(socket: &Path, child: &mut Child) -> Result<()> {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !socket.exists() {
        if let Some(status) = child.try_wait()? {
            return Err(fail(format!(
                "Speech Dispatcher exited before socket readiness: {status}"
            )));
        }
        if Instant::now() >= deadline {
            return Err(fail("Speech Dispatcher socket timed out"));
        }
        thread::sleep(Duration::from_millis(20));
    }
    Ok(())
}

fn main_inner() -> Result<()> {
    if utility_command()? {
        return Ok(());
    }
    let (dispatcher, source, output) = arguments()?;
    let dispatcher = fs::canonicalize(dispatcher)?;
    let source = fs::canonicalize(source)?;
    if !dispatcher.starts_with("/nix/store") || !source.starts_with("/nix/store") {
        return Err(fail("Pinned immutable speech inputs required"));
    }
    fs::create_dir(&output)?;
    let output = fs::canonicalize(output)?;
    fs::write(
        output.join("source-proof.json"),
        serde_json::to_string_pretty(&source_proof(&source)?)?,
    )?;
    let private = tempfile::tempdir()?;
    let (socket, protocol, command, _runtime) = configure(&output, &dispatcher)?;
    let mut environment = env::vars_os().collect::<std::collections::BTreeMap<_, _>>();
    for key in [
        "DISPLAY",
        "WAYLAND_DISPLAY",
        "SWAYSOCK",
        "AT_SPI_BUS_ADDRESS",
    ] {
        environment.remove(&std::ffi::OsString::from(key));
    }
    for (key, value) in [
        ("HOME", private.path().display().to_string()),
        ("XDG_RUNTIME_DIR", private.path().display().to_string()),
        (
            "DBUS_SESSION_BUS_ADDRESS",
            format!("unix:path={}/no-session", private.path().display()),
        ),
        (
            "DBUS_SYSTEM_BUS_ADDRESS",
            format!("unix:path={}/no-system", private.path().display()),
        ),
        (
            "PULSE_SERVER",
            format!("unix:{}/no-audio", private.path().display()),
        ),
        (
            "ALSA_CONFIG_PATH",
            private.path().join("no-alsa").display().to_string(),
        ),
        (
            "SPEECHD_ADDRESS",
            format!("unix_socket:{}", socket.display()),
        ),
        (
            "PATH",
            output.join("speech-empty-bin").display().to_string(),
        ),
    ] {
        environment.insert(key.into(), value.into());
    }
    let log = File::create(output.join("dispatcher.log"))?;
    let mut child = OwnedChild(
        Command::new(&command[0])
            .args(&command[1..])
            .env_clear()
            .envs(environment)
            .stdout(Stdio::from(log.try_clone()?))
            .stderr(Stdio::from(log))
            .spawn()?,
    );
    let started = Instant::now();
    let result = (|| {
        wait_for_socket(&socket, &mut child)?;
        run_transport(&socket, &protocol, &output, private.path())
    })();
    let before_cleanup = child.try_wait()?;
    if child.try_wait()?.is_none() {
        if unsafe { libc::kill(child.id() as i32, libc::SIGTERM) } != 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if child.try_wait()?.is_some() {
                break;
            }
            if Instant::now() >= deadline {
                child.kill()?;
                child.wait()?;
                break;
            }
            thread::sleep(Duration::from_millis(20));
        }
    }
    fs::write(
        output.join("dispatcher-process.json"),
        serde_json::to_vec_pretty(&json!({
            "command": command, "socket": socket, "socketPathBytes": socket.as_os_str().len(),
            "elapsedSeconds": started.elapsed().as_secs_f64(), "pid": child.id(),
            "statusBeforeCleanup": before_cleanup.map(|status| status.to_string()),
            "statusAfterCleanup": child.try_wait()?.map(|status| status.to_string()),
            "result": result.as_ref().err().map(|error| error.to_string()),
            "stdoutAndStderr": output.join("dispatcher.log"), "providerLogs": output.join("speech-logs")
        }))?,
    )?;
    retire(&output)?;
    result
}

fn main() {
    if let Err(error) = main_inner() {
        eprintln!("speech transport: {error}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn accepted_speech_requires_queue_ack_and_keeps_lifecycle() {
        let protocol = " 24:DATA:|SET self CLIENT_NAME unknown:Orca:default\n| (1)\n 24:DATA:|SPEAK\n| (1)\n 24:DATA:|Sentinel Alpha\n| (1)\n 24:DATA:|.\n| (1)\n 24:REPLY:|225-7\n225 OK MESSAGE QUEUED\n|\n 24:REPLY:|701-7\n701-1\n701 BEGIN\n|\n 24:REPLY:|702-7\n702-1\n702 END\n|\n";
        let rows = transport_messages(protocol).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["client"], "unknown:Orca:default");
        assert_eq!(rows[0]["text"], "Sentinel Alpha");
        assert_eq!(rows[0]["events"], json!(["BEGIN", "END"]));
        assert!(
            transport_messages(&protocol[..protocol.find(" 24:REPLY:").unwrap()])
                .unwrap()
                .is_empty()
        );
    }
    #[test]
    fn spoken_words_survive_ssml_and_debug_extraction() {
        assert_eq!(
            plain_text("<speak>Sentinel <mark name=\"1\"/>Alpha</speak>"),
            "Sentinel Alpha"
        );
        assert_eq!(plain_text("<speak>One &amp; Two</speak>"), "One & Two");
        assert_eq!(
            utterances("x SPEECH OUTPUT: 'Alpha' voice\nx SPEECH OUTPUT: 'Beta' voice").unwrap(),
            ["'Alpha' voice", "'Beta' voice"]
        );
    }
    #[test]
    fn retirement_keeps_logs_and_symlink_target() {
        let root = tempfile::tempdir().unwrap();
        let target = root.path().join("retained");
        fs::write(&target, "retained").unwrap();
        fs::create_dir(root.path().join("speech-modules")).unwrap();
        symlink(&target, root.path().join("speech-modules/sd_generic")).unwrap();
        let socket =
            std::os::unix::net::UnixListener::bind(root.path().join("speech.sock")).unwrap();
        retire(root.path()).unwrap();
        assert!(!root.path().join("speech.sock").exists());
        assert!(!root.path().join("speech-modules/sd_generic").is_symlink());
        assert_eq!(fs::read_to_string(&target).unwrap(), "retained");
        drop(socket);
    }
    #[test]
    fn startup_failure_reports_actual_child_exit() {
        let root = tempfile::tempdir().unwrap();
        let mut child = Command::new(env!("LUMEN_TRUE")).spawn().unwrap();
        let error = wait_for_socket(&root.path().join("absent.sock"), &mut child).unwrap_err();
        assert!(error.to_string().contains("exit status: 0"));
        assert!(child.try_wait().unwrap().is_some());
    }
    #[test]
    fn retirement_requires_exact_private_runtime_identity() {
        let root = tempfile::tempdir().unwrap();
        let runtime = tempfile::Builder::new()
            .prefix("lumen-speech-")
            .permissions(fs::Permissions::from_mode(0o700))
            .tempdir_in("/tmp")
            .unwrap();
        let info = fs::symlink_metadata(runtime.path()).unwrap();
        fs::write(root.path().join("speech-runtime.json"), serde_json::to_vec(&json!({
            "path": runtime.path(), "device": info.dev(), "inode": info.ino() + 1, "uid": info.uid()
        })).unwrap()).unwrap();
        assert!(
            retire(root.path())
                .unwrap_err()
                .to_string()
                .contains("identity changed")
        );
        assert!(runtime.path().is_dir());
        fs::write(
            root.path().join("speech-runtime.json"),
            serde_json::to_vec(&json!({
                "path": runtime.path(), "device": info.dev(), "inode": info.ino(), "uid": info.uid()
            }))
            .unwrap(),
        )
        .unwrap();
        let _socket =
            std::os::unix::net::UnixListener::bind(runtime.path().join("speech.sock")).unwrap();
        fs::write(root.path().join("retained.log"), "diagnostic").unwrap();
        retire(root.path()).unwrap();
        assert!(!runtime.path().exists());
        assert_eq!(
            fs::read_to_string(root.path().join("retained.log")).unwrap(),
            "diagnostic"
        );
    }
    #[test]
    fn lease_drop_retains_actual_replacement_directory() {
        let runtime = tempfile::Builder::new()
            .prefix("lumen-speech-")
            .permissions(fs::Permissions::from_mode(0o700))
            .tempdir_in("/tmp")
            .unwrap();
        let path = runtime.path().to_owned();
        let displaced = path.with_extension("displaced");
        let lease = RuntimeLease::adopt(runtime).unwrap();
        fs::rename(&path, &displaced).unwrap();
        fs::create_dir(&path).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
        fs::write(path.join("retained"), "replacement survives guard refusal").unwrap();
        assert!(retire_runtime(&lease.identity).is_err());
        drop(lease);
        assert_eq!(
            fs::read_to_string(path.join("retained")).unwrap(),
            "replacement survives guard refusal"
        );
        assert!(displaced.is_dir());
        fs::remove_file(path.join("retained")).unwrap();
        fs::remove_dir(&path).unwrap();
        fs::remove_dir(&displaced).unwrap();
    }
    #[test]
    fn early_failure_drops_reap_owned_child_then_retire_runtime() {
        let runtime = tempfile::Builder::new()
            .prefix("lumen-speech-")
            .permissions(fs::Permissions::from_mode(0o700))
            .tempdir_in("/tmp")
            .unwrap();
        let path = runtime.path().to_owned();
        let mut child_id = 0;
        let result: Result<()> = (|| {
            let _lease = RuntimeLease::adopt(runtime)?;
            let child = OwnedChild(
                Command::new(Path::new(env!("LUMEN_TRUE")).with_file_name("sleep"))
                    .arg("60")
                    .spawn()?,
            );
            child_id = child.id();
            Err(fail("synthetic post-spawn failure"))
        })();
        assert!(result.is_err());
        assert!(child_id > 0);
        assert!(!path.exists());
        let mut status = 0;
        assert_eq!(
            unsafe { libc::waitpid(child_id as i32, &mut status, libc::WNOHANG) },
            -1
        );
        assert_eq!(
            std::io::Error::last_os_error().raw_os_error(),
            Some(libc::ECHILD)
        );
    }
}
