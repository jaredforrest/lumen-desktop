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
    app: String,
}
fn options() -> Result<Options> {
    let mut writer = None;
    let mut output = None;
    let mut inside = false;
    let mut app = None;
    let mut args = env::args_os().skip(1);
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("--provider-cli") | Some("--providerCLI") => {
                writer = Some(PathBuf::from(args.next().ok_or("Missing provider")?))
            }
            Some("--app") => {
                app = Some(
                    args.next()
                        .ok_or("Missing app")?
                        .to_str()
                        .ok_or("App must be UTF-8")?
                        .to_owned(),
                )
            }
            Some("--output") => output = Some(PathBuf::from(args.next().ok_or("Missing output")?)),
            Some("--inside") => inside = true,
            _ => return Err(error("Unknown history fixture option")),
        }
    }
    Ok(Options {
        writer: writer.ok_or("--provider-cli required")?,
        output: output.ok_or("--output required")?,
        inside,
        app: app
            .filter(|app| matches!(app.as_str(), "sheet" | "slides"))
            .ok_or("--app sheet|slides required")?,
    })
}
struct Provider {
    tool: &'static str,
    child: Child,
    serial: u64,
    index: usize,
    facts: Vec<Value>,
    closed: bool,
}
impl Provider {
    fn start(writer: &Path, out: &Path, index: usize, app: &str) -> Result<Self> {
        let child = Command::new(writer)
            .arg("mcp")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(File::create(out.join(format!("provider-{index}.log")))?)
            .process_group(0)
            .spawn()?;
        let mut result = Self {
            tool: if app == "sheet" {
                "execute_command"
            } else {
                "run_command"
            },
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
            json!({"name":self.tool,"arguments":{"command":name,"params":data}}),
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
fn capture(p: &mut Provider, label: &str) -> Result<Value> {
    p.cmd("file.history", json!({"action":"capture","label":label}))
}
fn owner(p: &mut Provider) -> Result<Value> {
    p.cmd("file.history", json!({"action":"list"}))
}
fn request(owner: &Value, id: &Value) -> Value {
    json!({"action":"restore","id":id,"document":owner["document"],"revision":owner["revision"]})
}
fn edit(p: &mut Provider, app: &str, text: &str) -> Result<()> {
    if app == "sheet" {
        p.cmd("cell.set", json!({"cell":"A1","input":text}))?;
    } else {
        p.cmd("slide.notes", json!({"index":0,"text":text}))?;
    }
    Ok(())
}
fn inspect(p: &mut Provider) -> Result<Value> {
    p.cmd("document.inspect", json!({}))
}
// A disposable explicit capture reads the complete provider snapshot representation. This
// preserves embedded payloads that an outline or exported compatibility format would omit.
// It changes history only; identity, revision, dirty state and source bytes must remain fixed.
fn projection(p: &mut Provider, archive: &Path) -> Result<Value> {
    let before = inspect(p)?;
    let rows = capture(p, "Fixture semantic projection")?;
    let id = &rows["versions"][0]["id"];
    let value: Value = serde_json::from_slice(&fs::read(archive)?)?;
    let document = value["versions"]
        .as_array()
        .ok_or("Archive versions missing")?
        .iter()
        .find(|v| &v["id"] == id)
        .ok_or("Captured projection missing")?["document"]
        .clone();
    ensure(
        inspect(p)? == before,
        "Projection capture changed working facts",
    )?;
    Ok(document)
}
fn lifecycle(
    o: &Options,
    source: &Path,
    archive: &Path,
    facts: &mut Vec<Value>,
) -> Result<Vec<&'static str>> {
    let mut checks = Vec::new();
    let mut first = Provider::start(&o.writer, &o.output, 0, &o.app)?;
    first.cmd("file.new", json!({}))?;
    edit(&mut first, &o.app, ORIGINAL)?;
    if o.app == "sheet" {
        first.cmd("cell.set", json!({"cell":"B1","input":"=1+2"}))?;
    } else {
        first.cmd(
            "file.properties",
            json!({"comments":"Retained structured presentation metadata"}),
        )?;
    }
    first.cmd("file.saveAs", json!({"path":source}))?;
    let source_hash = digest(source)?;
    let original = projection(&mut first, archive)?;
    let original_id =
        capture(&mut first, "Original captured version")?["versions"][0]["id"].clone();
    ensure(
        inspect(&mut first)?["dirty"] == false,
        "Capture changed clean state",
    )?;
    edit(&mut first, &o.app, "Newer unsaved marker")?;
    let newer = projection(&mut first, archive)?;
    let newer_id = capture(&mut first, "Unsaved snapshot")?["versions"][0]["id"].clone();
    ensure(
        newer != original
            && newer_id != original_id
            && inspect(&mut first)?["dirty"] == true
            && digest(source)? == source_hash,
        "Capture changed dirty/source facts",
    )?;
    first.close()?;
    facts.append(&mut first.facts);
    checks.push("Full semantic captures preserve working/source facts and distinct identities");
    let mut second = Provider::start(&o.writer, &o.output, 1, &o.app)?;
    second.cmd("file.open", json!({"path":source}))?;
    let rows = owner(&mut second)?;
    ensure(
        rows["versions"].as_array().is_some_and(|rows| {
            rows.iter().any(|r| r["id"] == original_id) && rows.iter().any(|r| r["id"] == newer_id)
        }),
        "Persistent capture identities changed",
    )?;
    ensure(
        second.cmd("file.history", json!({"action":"preview","id":newer_id}))?["text"]
            .as_str()
            .is_some_and(|s| s.contains("Newer unsaved marker")),
        "Reopened preview missing",
    )?;
    checks.push("Independent provider process reopens retained identities and preview");
    edit(&mut second, &o.app, "Newer unsaved working state")?;
    let before = projection(&mut second, archive)?;
    let mut restore = request(&owner(&mut second)?, &original_id);
    second.command_budget("file.history", restore.clone(), true, META)?;
    ensure(
        projection(&mut second, archive)? == before && digest(source)? == source_hash,
        "Refused dirty restore changed content/source",
    )?;
    checks.push("Dirty restore requires explicit consent and preserves full working projection");
    restore["allowDiscard"] = json!(true);
    second.cmd("file.history", restore)?;
    let rows = owner(&mut second)?;
    ensure(
        rows["versions"][0]["label"] == "Before restore",
        "Missing Before restore snapshot",
    )?;
    ensure(
        second.cmd(
            "file.history",
            json!({"action":"preview","id":rows["versions"][0]["id"]}),
        )?["text"]
            .as_str()
            .is_some_and(|s| s.contains("Newer unsaved working state")),
        "Before restore lost replaced edits",
    )?;
    ensure(
        projection(&mut second, archive)? == original && inspect(&mut second)?["dirty"] == true,
        "Restored complete working projection differs",
    )?;
    second.cmd("edit.undo", json!({}))?;
    ensure(
        projection(&mut second, archive)? == before,
        "Undo lost complete replaced working projection",
    )?;
    checks.push("Restore publishes Before restore before mutation and Undo retains the full model");
    let mut invalid = request(&owner(&mut second)?, &json!("not-a-version"));
    invalid["allowDiscard"] = json!(true);
    second.command_budget("file.history", invalid, true, META)?;
    ensure(
        projection(&mut second, archive)? == before,
        "Invalid identity changed working projection",
    )?;
    checks.push("Invalid version identity preserves working projection");
    let mut stale = request(&owner(&mut second)?, &original_id);
    stale["allowDiscard"] = json!(true);
    edit(
        &mut second,
        &o.app,
        "Changed after captured restore request",
    )?;
    let current = projection(&mut second, archive)?;
    second.command_budget("file.history", stale, true, META)?;
    ensure(
        projection(&mut second, archive)? == current,
        "Stale revision changed working projection",
    )?;
    checks.push("Captured revision rejects later edits without replacing their full projection");
    let lock = File::open(archive.parent().ok_or("Archive parent")?.join("lock"))?;
    ensure(
        unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0,
        "Fixture lock unavailable",
    )?;
    second.refusal(json!({"action":"list"}), "another history operation")?;
    drop(lock);
    checks.push("Concurrent archive owner causes a bounded busy refusal");
    let saved = fs::read(archive)?;
    fs::remove_file(archive)?;
    let name = std::ffi::CString::new(archive.as_os_str().as_encoded_bytes())?;
    ensure(
        unsafe { libc::mkfifo(name.as_ptr(), 0o600) } == 0,
        "Fixture FIFO unavailable",
    )?;
    let started = Instant::now();
    second.command_budget("file.history", json!({"action":"list"}), true, META)?;
    ensure(
        started.elapsed() < Duration::from_secs(2),
        "Special archive blocked",
    )?;
    fs::remove_file(archive)?;
    fs::write(archive, &saved)?;
    checks.push("Special archive entry refuses without blocking");
    fs::set_permissions(archive, fs::Permissions::from_mode(0o644))?;
    second.refusal(json!({"action":"list"}), "private")?;
    ensure(
        fs::read(archive)? == saved,
        "Private permissions refusal changed archive",
    )?;
    fs::set_permissions(archive, fs::Permissions::from_mode(0o600))?;
    checks.push("Overly readable archive refuses without silent repair");
    second.cmd(
        "file.history",
        json!({"action":"capture","label":"Protected complete history","historyPassword":PASSWORD}),
    )?;
    let encrypted = fs::read(archive)?;
    ensure(
        encrypted.starts_with(b"LUMENHST1")
            && !encrypted
                .windows(ORIGINAL.len())
                .any(|w| w == ORIGINAL.as_bytes())
            && !encrypted
                .windows(26)
                .any(|w| w == b"Protected complete history"),
        "Whole archive protection missing",
    )?;
    second.close()?;
    facts.append(&mut second.facts);
    checks.push(
        "Explicit archive password encrypts prior content and labels without encrypting source",
    );
    let mut third = Provider::start(&o.writer, &o.output, 2, &o.app)?;
    third.cmd("file.open", json!({"path":source}))?;
    third.command_budget("file.history", json!({"action":"list"}), true, META)?;
    third.command_budget(
        "file.history",
        json!({"action":"list","historyPassword":"Wrong synthetic password"}),
        true,
        META,
    )?;
    let rows = third.cmd(
        "file.history",
        json!({"action":"list","historyPassword":PASSWORD}),
    )?;
    ensure(
        rows["protected"] == true
            && rows["versions"]
                .as_array()
                .is_some_and(|rows| rows.iter().any(|r| r["id"] == original_id)),
        "Protected archive reopen missing",
    )?;
    third.cmd(
        "file.history",
        json!({"action":"capture","label":"Protection retained","historyPassword":PASSWORD}),
    )?;
    ensure(
        fs::read(archive)?.starts_with(b"LUMENHST1"),
        "Later capture removed protection",
    )?;
    third.close()?;
    facts.append(&mut third.facts);
    checks.push("Missing and wrong passwords refuse; correct password reopens and later capture retains protection");
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
        "Source or private permissions changed",
    )?;
    let mut entries = fs::read_dir(archive.parent().ok_or("Archive parent")?)?
        .map(|e| e.map(|e| e.file_name()))
        .collect::<std::io::Result<Vec<_>>>()?;
    entries.sort();
    ensure(
        entries == ["archive", "lock"],
        "Abandoned archive staging files",
    )?;
    checks.push("All independent sessions preserve source bytes, private permissions and no abandoned staging");
    Ok(checks)
}
fn main() -> Result<()> {
    let mut o = options()?;
    if !o.inside {
        o.writer = o.writer.canonicalize()?;
        ensure(
            o.writer.starts_with("/nix/store") && o.writer.is_file(),
            "Pinned installed provider required",
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
            &source.join("tests/desktop/office-workflows-rust/src/bin/provider-durable-history.rs"),
            &[o.writer.clone(), exe.clone()],
        )
        .map_err(|e| error(e.to_string()))?;
        let exe = if exe.starts_with(&source) {
            Path::new("/source").join(exe.strip_prefix(&source)?)
        } else {
            exe
        };
        let status = Command::new("lumen-application-isolation")
            .args(["run", "--source"])
            .arg(&source)
            .arg("--output")
            .arg(&o.output)
            .arg("--")
            .arg(exe)
            .arg("--inside")
            .arg("--app")
            .arg(&o.app)
            .arg("--provider-cli")
            .arg(&o.writer)
            .arg("--output")
            .arg("/artifacts")
            .status();
        frozen
            .finish(
                &o.output,
                status.as_ref().ok().and_then(|s| s.code()).unwrap_or(1),
            )
            .map_err(|e| error(e.to_string()))?;
        return ensure(
            status?.success(),
            "Isolated provider history fixture failed",
        );
    }
    ensure(
        unsafe { libc::getuid() } != 0
            && env::var("HOME")? == "/home/fixture"
            && !Path::new("/.old-root").exists(),
        "Private fixture identity required",
    )?;
    let name = if o.app == "sheet" {
        "document.xlsx"
    } else {
        "document.deckcraft"
    };
    let source = o.output.join(name);
    let archive = o.output.join(format!(".{name}.lumen-history/archive"));
    let mut facts = Vec::new();
    let checks = lifecycle(&o, &source, &archive, &mut facts)?;
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
    fs::write(
        o.output.join("result.json"),
        format!(
            "{}\n",
            serde_json::to_string_pretty(
                &json!({"passed":true,"app":o.app,"checks":checks,"transactions":facts,"sourceSha256":digest(&source)?,"scope":"Installed disposable stdio provider lifecycle and full semantic archive projections, including only corpus features created here. Embedded binary representation is covered separately by provider unit tests. No native UI or reference visual parity claim."})
            )?
        ),
    )?;
    println!("{}", json!({"passed":true,"checks":checks.len()}));
    Ok(())
}
