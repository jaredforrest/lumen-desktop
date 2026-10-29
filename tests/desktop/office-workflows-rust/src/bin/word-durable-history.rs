//! Installed durable history lifecycle in an owned disposable stdio boundary.
use lumen_office_workflows::{Result, ensure, error};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    env,
    fs::{self, File},
    io::{Read, Write},
    os::{
        fd::AsRawFd,
        unix::{fs::PermissionsExt, process::CommandExt},
    },
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};
use wait_timeout::ChildExt;

const META: usize = 8 * 1024 * 1024;
const ORIGINAL: &str = "Disposable complete original document";
const PASSWORD: &str = "Synthetic history acceptance password";
fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn digest(path: &Path) -> Result<String> {
    Ok(hash(&fs::read(path)?))
}
struct Options {
    writer: PathBuf,
    output: PathBuf,
    inside: bool,
    capacity: bool,
}
fn options() -> Result<Options> {
    let mut writer = None;
    let mut output = None;
    let mut inside = false;
    let mut capacity = false;
    let mut args = env::args_os().skip(1);
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("--writer") => writer = Some(PathBuf::from(args.next().ok_or("Missing writer")?)),
            Some("--output") => output = Some(PathBuf::from(args.next().ok_or("Missing output")?)),
            Some("--inside") => inside = true,
            Some("--capacity-probe") => capacity = true,
            _ => return Err(error("Unknown history fixture option")),
        }
    }
    Ok(Options {
        writer: writer.ok_or("--writer required")?,
        output: output.ok_or("--output required")?,
        inside,
        capacity,
    })
}
struct Provider {
    child: Child,
    serial: u64,
    index: usize,
    facts: Vec<Value>,
    closed: bool,
}
impl Provider {
    fn start(writer: &Path, out: &Path, index: usize) -> Result<Self> {
        let child = Command::new(writer)
            .arg("mcp")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(File::create(out.join(format!("provider-{index}.log")))?)
            .process_group(0)
            .spawn()?;
        let mut result = Self {
            child,
            serial: 0,
            index,
            facts: Vec::new(),
            closed: false,
        };
        result.rpc("initialize",json!({"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"disposable-history-acceptance","version":"1"}}),META)?;
        Ok(result)
    }
    fn rpc(&mut self, method: &str, params: Value, budget: usize) -> Result<Value> {
        self.serial += 1;
        let bytes = serde_json::to_vec(
            &json!({"jsonrpc":"2.0","id":self.serial,"method":method,"params":params}),
        )?;
        // Requests can contain the explicit large synthetic capacity document. Poll writes as
        // well as reads, so an unresponsive provider cannot block the fixture on a full pipe.
        let deadline = Instant::now() + Duration::from_secs(10);
        let input = self.child.stdin.as_mut().ok_or("Provider input closed")?;
        let fd = input.as_raw_fd();
        let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
        ensure(
            flags >= 0 && unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } >= 0,
            "Nonblocking provider input unavailable",
        )?;
        let mut frame = bytes;
        frame.push(b'\n');
        let mut offset = 0;
        while offset < frame.len() {
            wait_fd(fd, libc::POLLOUT, deadline)?;
            match input.write(&frame[offset..]) {
                Ok(0) => return Err(error("Provider input closed")),
                Ok(n) => offset += n,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
                Err(e) => return Err(e.into()),
            }
        }
        let output = self.child.stdout.as_mut().ok_or("Provider output closed")?;
        let mut response = Vec::new();
        loop {
            wait_fd(output.as_raw_fd(), libc::POLLIN, deadline)?;
            let mut chunk = [0u8; 65536];
            let size = output.read(&mut chunk)?;
            ensure(size > 0, "Provider exited before response")?;
            ensure(
                response.len() + size <= budget,
                "Provider response exceeds scoped budget",
            )?;
            response.extend_from_slice(&chunk[..size]);
            if response.last() == Some(&b'\n') {
                break;
            }
        }
        let reply: Value =
            serde_json::from_slice(&response).map_err(|_| error("Malformed provider response"))?;
        ensure(
            reply["jsonrpc"] == "2.0" && reply["id"] == self.serial && reply.get("error").is_none(),
            "Unexpected provider response envelope",
        )?;
        Ok(reply["result"].clone())
    }
    fn command_budget(
        &mut self,
        name: &str,
        data: Value,
        expected_error: bool,
        budget: usize,
    ) -> Result<Value> {
        let result = self.rpc(
            "tools/call",
            json!({"name":"execute","arguments":{"command":name,"params":data}}),
            budget,
        )?;
        ensure(
            result["isError"].as_bool().unwrap_or(false) == expected_error,
            format!("Unexpected tool status: {name}"),
        )?;
        let raw = result["content"][0]["text"]
            .as_str()
            .ok_or("Missing provider text")?;
        self.facts.push(json!({"session":self.index,"command":name,"error":expected_error,"responseSha256":hash(raw.as_bytes())}));
        if expected_error {
            Ok(Value::String(raw.to_owned()))
        } else {
            Ok(serde_json::from_str(raw)?)
        }
    }
    fn cmd(&mut self, name: &str, data: Value) -> Result<Value> {
        self.command_budget(name, data, false, META)
    }
    fn refusal(&mut self, data: Value, needle: &str) -> Result<()> {
        let result = self.command_budget("file.history", data, true, META)?;
        ensure(
            result.as_str().is_some_and(|s| s.contains(needle)),
            "Expected explicit history refusal",
        )
    }
    fn close(&mut self) -> Result<()> {
        if self.closed {
            return Ok(());
        }
        self.child.stdin.take();
        let status = match self.child.wait_timeout(Duration::from_secs(3))? {
            Some(status) => status,
            None => {
                unsafe {
                    libc::kill(-(self.child.id() as i32), libc::SIGKILL);
                };
                self.child.wait()?;
                self.closed = true;
                return Err(error("Provider did not close within deadline"));
            }
        };
        self.child.stdout.take();
        self.closed = true;
        ensure(status.success(), "Provider failed during close")
    }
}
impl Drop for Provider {
    fn drop(&mut self) {
        if !self.closed {
            self.child.stdin.take();
            unsafe {
                libc::kill(-(self.child.id() as i32), libc::SIGKILL);
            };
            let _ = self.child.wait();
        }
    }
}
fn wait_fd(fd: i32, events: i16, deadline: Instant) -> Result<()> {
    let remaining = deadline.saturating_duration_since(Instant::now());
    ensure(!remaining.is_zero(), "Bounded provider response deadline")?;
    let mut poll = libc::pollfd {
        fd,
        events,
        revents: 0,
    };
    let result = unsafe {
        libc::poll(
            &mut poll,
            1,
            remaining.as_millis().min(i32::MAX as u128) as i32,
        )
    };
    ensure(result > 0, "Bounded provider pipe deadline")?;
    Ok(())
}
fn text(p: &mut Provider) -> Result<String> {
    Ok(p.cmd("document.text", json!({}))?["text"]
        .as_str()
        .ok_or("Document text missing")?
        .to_owned())
}
fn capture(p: &mut Provider, label: &str) -> Result<Value> {
    p.cmd("file.history", json!({"action":"capture","label":label}))
}
fn owner(p: &mut Provider) -> Result<Value> {
    p.cmd("file.history", json!({"action":"list"}))
}
fn request(owner: &Value, id: &Value) -> Value {
    json!({"action":"restore","id":id,"document":owner["document"],"revision":owner["revision"]})
}
fn lifecycle(
    o: &Options,
    source: &Path,
    archive: &Path,
    facts: &mut Vec<Value>,
) -> Result<Vec<&'static str>> {
    let mut checks = Vec::new();
    let mut first = Provider::start(&o.writer, &o.output, 0)?;
    first.cmd("document.setText", json!({"text":ORIGINAL}))?;
    first.cmd("file.save", json!({"path":source}))?;
    let source_hash = digest(source)?;
    let original_id =
        capture(&mut first, "Original captured version")?["versions"][0]["id"].clone();
    ensure(
        first.cmd("file.info", json!({}))?["dirty"] == false,
        "Capture changed clean state",
    )?;
    first.cmd("caret.docEnd", json!({}))?;
    first.cmd("text.insert", json!({"text":" Newer unsaved marker"}))?;
    let newer_id = capture(&mut first, "Unsaved snapshot")?["versions"][0]["id"].clone();
    ensure(
        newer_id != original_id
            && first.cmd("file.info", json!({}))?["dirty"] == true
            && digest(source)? == source_hash,
        "Capture changed dirty/source facts",
    )?;
    first.close()?;
    facts.append(&mut first.facts);
    checks
        .push("Explicit captures preserve dirty/path/source facts and stable distinct identities");
    let mut second = Provider::start(&o.writer, &o.output, 1)?;
    second.cmd("file.open", json!({"path":source}))?;
    let rows = owner(&mut second)?;
    ensure(
        rows["versions"]
            .as_array()
            .is_some_and(|r| r.len() == 2 && r[0]["id"] == newer_id && r[1]["id"] == original_id),
        "Persistent versions changed",
    )?;
    ensure(
        second.cmd("file.history", json!({"action":"preview","id":newer_id}))?["text"]
            .as_str()
            .is_some_and(|s| s.contains("Newer unsaved marker")),
        "Persistent preview missing",
    )?;
    checks.push("Independent process reopens complete history and preview");
    second.cmd("caret.docEnd", json!({}))?;
    second.cmd(
        "text.insert",
        json!({"text":" Newer unsaved working state"}),
    )?;
    let before = text(&mut second)?;
    let mut restore = request(&owner(&mut second)?, &original_id);
    second.refusal(restore.clone(), "confirm replacement")?;
    ensure(
        text(&mut second)? == before && digest(source)? == source_hash,
        "Refused restore changed content",
    )?;
    checks.push("Dirty restore requires explicit consent");
    restore["allowDiscard"] = json!(true);
    second.cmd("file.history", restore.clone())?;
    ensure(
        text(&mut second)? == ORIGINAL
            && second.cmd("file.info", json!({}))?["dirty"] == true
            && owner(&mut second)?["versions"][0]["label"] == "Before restore",
        "Restore recovery/state incorrect",
    )?;
    second.cmd("edit.undo", json!({}))?;
    ensure(text(&mut second)? == before, "Undo lost replaced edits")?;
    checks.push("Restore captures Before restore and Undo preserves edits");
    restore = request(&owner(&mut second)?, &json!("not-a-version"));
    restore["allowDiscard"] = json!(true);
    second.command_budget("file.history", restore.clone(), true, META)?;
    ensure(text(&mut second)? == before, "Invalid ID changed content")?;
    checks.push("Invalid version identity preserves working content");
    restore["id"] = original_id.clone();
    second.cmd("text.insert", json!({"text":" Changed after capture"}))?;
    let current = text(&mut second)?;
    second.refusal(restore, "captured document changed")?;
    ensure(
        text(&mut second)? == current,
        "Stale restore changed content",
    )?;
    checks.push("Captured revision refuses later edits");
    let lock = File::open(archive.parent().ok_or("Archive parent")?.join("lock"))?;
    ensure(
        unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0,
        "Fixture lock failed",
    )?;
    second.refusal(json!({"action":"list"}), "another history operation")?;
    drop(lock);
    checks.push("Concurrent owner causes bounded busy refusal");
    let saved = fs::read(archive)?;
    fs::remove_file(archive)?;
    let name = std::ffi::CString::new(archive.as_os_str().as_encoded_bytes())?;
    ensure(
        unsafe { libc::mkfifo(name.as_ptr(), 0o600) } == 0,
        "Fixture FIFO creation failed",
    )?;
    let start = Instant::now();
    second.command_budget("file.history", json!({"action":"list"}), true, META)?;
    ensure(
        start.elapsed() < Duration::from_secs(2),
        "Special entry blocked",
    )?;
    fs::remove_file(archive)?;
    fs::write(archive, &saved)?;
    checks.push("Special archive entry refuses without blocking");
    fs::set_permissions(archive, fs::Permissions::from_mode(0o644))?;
    second.refusal(json!({"action":"list"}), "private")?;
    ensure(
        fs::read(archive)? == saved,
        "Private permission refusal changed archive",
    )?;
    fs::set_permissions(archive, fs::Permissions::from_mode(0o600))?;
    checks.push("Overly readable archive refuses without silent repair");
    second.cmd("file.encrypt", json!({"password":PASSWORD}))?;
    capture(&mut second, "Protected complete history")?;
    let encrypted = fs::read(archive)?;
    ensure(
        encrypted.starts_with(&[0xd0, 0xcf, 0x11, 0xe0, 0xa1, 0xb1, 0x1a, 0xe1])
            && !encrypted
                .windows(ORIGINAL.len())
                .any(|w| w == ORIGINAL.as_bytes())
            && !encrypted
                .windows(26)
                .any(|w| w == b"Protected complete history"),
        "Archive not completely protected",
    )?;
    second.close()?;
    facts.append(&mut second.facts);
    checks.push("Protected capture encrypts earlier snapshots and labels");
    let mut third = Provider::start(&o.writer, &o.output, 2)?;
    third.cmd("file.open", json!({"path":source}))?;
    third.command_budget("file.history", json!({"action":"list"}), true, META)?;
    let value = third.cmd(
        "file.history",
        json!({"action":"list","historyPassword":PASSWORD}),
    )?;
    ensure(
        value["protected"] == true
            && value["versions"]
                .as_array()
                .is_some_and(|rows| rows.iter().any(|r| r["id"] == original_id)),
        "Reopened protection access failed",
    )?;
    third.cmd(
        "file.history",
        json!({"action":"capture","label":"Protection retained","historyPassword":PASSWORD}),
    )?;
    ensure(
        fs::read(archive)?.starts_with(&[0xd0, 0xcf, 0x11, 0xe0, 0xa1, 0xb1, 0x1a, 0xe1]),
        "Capture stripped protection",
    )?;
    third.close()?;
    facts.append(&mut third.facts);
    checks.push("Capture-time password reopens history and later capture retains encryption");
    ensure(
        digest(source)? == source_hash
            && archive.metadata()?.permissions().mode() & 0o777 == 0o600
            && archive
                .parent()
                .ok_or("Archive parent")?
                .metadata()?
                .permissions()
                .mode()
                & 0o777
                == 0o700,
        "Source/private permissions changed",
    )?;
    let mut entries = fs::read_dir(archive.parent().ok_or("Archive parent")?)?
        .map(|e| e.map(|e| e.file_name()))
        .collect::<std::io::Result<Vec<_>>>()?;
    entries.sort();
    ensure(
        entries == ["archive", "lock"],
        "Abandoned archive staging files",
    )?;
    checks.push("All sessions retain source bytes, private permissions and no abandoned staging");
    Ok(checks)
}
fn capacity(
    o: &Options,
    source: &Path,
    archive: &Path,
    facts: &mut Vec<Value>,
) -> Result<Vec<&'static str>> {
    let mut p = Provider::start(&o.writer, &o.output, 0)?;
    p.cmd("document.setText", json!({"text":ORIGINAL}))?;
    p.cmd("file.save", json!({"path":source}))?;
    let source_hash = digest(source)?;
    let mut first = Value::Null;
    let mut value = Value::Null;
    for index in 0..2 {
        p.cmd(
            "document.setText",
            json!({"text":index.to_string().repeat(17*1024*1024)}),
        )?;
        value = capture(&mut p, &format!("Capacity capture {index}"))?;
        if index == 0 {
            first = value["versions"][0]["id"].clone();
        }
    }
    ensure(
        value["versions"].as_array().is_some_and(|r| r.len() == 1)
            && value["versions"][0]["id"] != first
            && archive.metadata()?.len() <= 32 * 1024 * 1024
            && digest(source)? == source_hash,
        "Byte-cap eviction failed",
    )?;
    let retained = value["versions"][0]["id"].clone();
    let saved = fs::read(archive)?;
    let large = "X".repeat(32 * 1024 * 1024);
    p.cmd("document.setText", json!({"text":large}))?;
    ensure(
        p.cmd("file.info", json!({}))?["dirty"] == true,
        "Large working state not dirty",
    )?;
    // Only this content query may exceed the unchanged 8 MiB metadata budget.
    let content = |p: &mut Provider| -> Result<String> {
        let v = p.command_budget("document.text", json!({}), false, 34 * 1024 * 1024)?;
        Ok(hash(
            v["text"].as_str().ok_or("Large text missing")?.as_bytes(),
        ))
    };
    let before = content(&mut p)?;
    ensure(
        before == hash(large.as_bytes()),
        "Large content query incomplete",
    )?;
    p.refusal(
        json!({"action":"capture","label":"Oversized single snapshot"}),
        "one snapshot exceeds",
    )?;
    ensure(
        fs::read(archive)? == saved && digest(source)? == source_hash,
        "Oversized capture changed retained bytes",
    )?;
    let mut restore = request(&owner(&mut p)?, &retained);
    restore["allowDiscard"] = json!(true);
    p.refusal(restore, "one snapshot exceeds")?;
    ensure(
        fs::read(archive)? == saved
            && content(&mut p)? == before
            && p.cmd("file.info", json!({}))?["dirty"] == true
            && digest(source)? == source_hash,
        "Failed recovery changed working state",
    )?;
    fs::write(archive, vec![b' '; 32 * 1024 * 1024 + 1])?;
    p.refusal(json!({"action":"list"}), "exceeds 32 MiB")?;
    fs::write(archive, &saved)?;
    ensure(
        archive.metadata()?.permissions().mode() & 0o777 == 0o600 && digest(source)? == source_hash,
        "Oversized archive refusal changed source/permissions",
    )?;
    p.close()?;
    facts.append(&mut p.facts);
    Ok(vec![
        "Byte cap evicts oldest full snapshot before count limit",
        "Oversized single capture preserves retained archive/source",
        "Restore refuses when Before restore cannot protect oversized edits",
        "Oversized disk archive refuses before parsing",
    ])
}
fn main() -> Result<()> {
    let mut o = options()?;
    if !o.inside {
        o.writer = o.writer.canonicalize()?;
        ensure(
            o.writer.starts_with("/nix/store") && o.writer.is_file(),
            "Pinned installed writer required",
        )?;
        if !o.output.is_absolute() {
            o.output = env::current_dir()?.join(&o.output);
        }
        fs::create_dir(&o.output)?;
        fs::set_permissions(&o.output, fs::Permissions::from_mode(0o700))?;
        let source = env::current_dir()?.canonicalize()?;
        let exe = env::current_exe()?;
        let frozen = lumen_application_isolation::provenance::FrozenSource::new(
            &source,
            &source.join("tests/desktop/office-workflows-rust/src/bin/word-durable-history.rs"),
            &[o.writer.clone(), exe.clone()],
        )
        .map_err(|e| error(e.to_string()))?;
        let exe = if exe.starts_with(&source) {
            Path::new("/source").join(exe.strip_prefix(&source)?)
        } else {
            exe
        };
        let mut command = Command::new("lumen-application-isolation");
        command
            .args(["run", "--source"])
            .arg(&source)
            .arg("--output")
            .arg(&o.output)
            .arg("--")
            .arg(exe)
            .arg("--inside")
            .arg("--writer")
            .arg(&o.writer)
            .arg("--output")
            .arg("/artifacts");
        if o.capacity {
            command.arg("--capacity-probe");
        }
        let status = command.status();
        frozen
            .finish(
                &o.output,
                status.as_ref().ok().and_then(|s| s.code()).unwrap_or(1),
            )
            .map_err(|e| error(e.to_string()))?;
        return ensure(status?.success(), "Isolated history fixture failed");
    }
    ensure(
        unsafe { libc::getuid() } != 0
            && env::var("HOME")? == "/home/fixture"
            && !Path::new("/.old-root").exists(),
        "Private fixture identity required",
    )?;
    let source = o.output.join("document.docx");
    let archive = o.output.join(".document.docx.lumen-history/archive");
    let mut facts = Vec::new();
    let checks = if o.capacity {
        capacity(&o, &source, &archive, &mut facts)?
    } else {
        lifecycle(&o, &source, &archive, &mut facts)?
    };
    for entry in fs::read_dir(&o.output)? {
        let path = entry?.path();
        if path
            .file_name()
            .is_some_and(|n| n.to_string_lossy().starts_with("provider-"))
        {
            let log = fs::read_to_string(path)?;
            ensure(
                !["warning:", "ResourceWarning", "Exception ignored"]
                    .iter()
                    .any(|word| log.contains(word)),
                "Provider warning emitted",
            )?;
        }
    }
    let value = json!({"passed":true,"checks":checks,"transactions":facts,"sourceSha256":digest(&source)?,"scope":if o.capacity{"Actual installed history byte capacity"}else{"Actual installed headless provider lifecycle; no native browser/physical restore button or other provider capability claim"}});
    fs::write(
        o.output.join("result.json"),
        format!("{}\n", serde_json::to_string_pretty(&value)?),
    )?;
    println!("{}", json!({"passed":true,"checks":checks.len()}));
    Ok(())
}
