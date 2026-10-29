use serde::{Deserialize, Serialize};
use serde_json::{Value, json, value::RawValue};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Read, Write},
    os::unix::{ffi::OsStringExt, fs::MetadataExt, io::AsRawFd, process::CommandExt},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};
type Result<T> = std::result::Result<T, String>;
const LIMIT: usize = 65536;
static INTERRUPTED: AtomicBool = AtomicBool::new(false);
extern "C" fn interrupt(_: libc::c_int) {
    INTERRUPTED.store(true, Ordering::SeqCst);
}
fn valid_text(s: &str, max: usize) -> bool {
    !s.is_empty() && s.chars().count() <= max && !s.chars().any(|c| (c as u32) < 32)
}
fn hash(s: &[u8]) -> String {
    hex::encode(Sha256::digest(s))
}
fn immutable(path: &Path, executable: bool) -> Result<PathBuf> {
    let p = fs::canonicalize(path).map_err(|_| "Configured sharing provider is unavailable")?;
    if !p.starts_with("/nix/store")
        || !p.is_file()
        || (executable && p.metadata().map_err(|_| "Unavailable provider")?.mode() & 0o111 == 0)
    {
        return Err("An immutable configured share provider is required".into());
    }
    Ok(p)
}
#[derive(Deserialize, Serialize, Clone)]
#[serde(deny_unknown_fields)]
struct Provider {
    id: String,
    label: String,
    executable: PathBuf,
    mode: String,
}
fn valid_id(s: &str) -> bool {
    s.len() <= 48
        && s.as_bytes().first().is_some_and(u8::is_ascii_lowercase)
        && s.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}
fn configured(manifest: Option<PathBuf>) -> Result<Vec<Provider>> {
    let Some(path) = manifest else {
        return Ok(vec![]);
    };
    let p = immutable(&path, false)?;
    let data = read_bounded(&p)?;
    let mut values: Vec<Provider> =
        serde_json::from_slice(&data).map_err(|_| "Invalid share provider configuration")?;
    if values.len() > 4 {
        return Err("At most four share providers are supported".into());
    }
    let mut ids = std::collections::HashSet::new();
    for v in &mut values {
        if !valid_id(&v.id)
            || !ids.insert(v.id.clone())
            || !valid_text(&v.label, 80)
            || !matches!(v.mode.as_str(), "protocol" | "nearby")
        {
            return Err("Invalid share provider configuration".into());
        }
        v.executable = immutable(&v.executable, true)?;
    }
    Ok(values)
}
fn read_bounded(path: &Path) -> Result<Vec<u8>> {
    let mut data = vec![];
    fs::File::open(path)
        .map_err(|_| "Cannot read sharing configuration")?
        .take((LIMIT + 1) as u64)
        .read_to_end(&mut data)
        .map_err(|_| "Cannot read sharing configuration")?;
    if data.len() > LIMIT {
        Err("Sharing configuration exceeds size limit".into())
    } else {
        Ok(data)
    }
}
// Hold the session leader unreaped until the entire owned group is retired.
// Subreaper ownership is scoped to this synchronous single-operation executable.
fn exchange(
    command: &Path,
    action: &str,
    request: Option<&[u8]>,
    timeout: Duration,
) -> Result<Vec<u8>> {
    if request.is_some_and(|x| x.len() > LIMIT) {
        return Err("Share request exceeds size limit".into());
    }
    let mut previous: libc::c_int = 0;
    if unsafe { libc::prctl(libc::PR_GET_CHILD_SUBREAPER, &mut previous, 0, 0, 0) } != 0
        || unsafe { libc::prctl(libc::PR_SET_CHILD_SUBREAPER, 1, 0, 0, 0) } != 0
    {
        return Err("Cannot own provider descendants".into());
    }
    let mut cmd = Command::new(command);
    cmd.arg(action)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    unsafe {
        cmd.pre_exec(|| {
            if libc::setsid() < 0 {
                Err(std::io::Error::last_os_error())
            } else {
                Ok(())
            }
        });
    }
    let spawned = cmd.spawn();
    let mut child = match spawned {
        Ok(c) => c,
        Err(_) => {
            unsafe {
                libc::prctl(libc::PR_SET_CHILD_SUBREAPER, previous, 0, 0, 0);
            }
            return Err("Share provider could not start".into());
        }
    };
    let pid = child.id() as i32;
    let result = (|| {
        let mut input = child.stdin.take();
        let mut output = child.stdout.take().ok_or("Missing output")?;
        let mut diagnostic = child.stderr.take().ok_or("Missing diagnostic")?;
        for fd in [
            input.as_ref().unwrap().as_raw_fd(),
            output.as_raw_fd(),
            diagnostic.as_raw_fd(),
        ] {
            if unsafe { libc::fcntl(fd, libc::F_SETFL, libc::O_NONBLOCK) } < 0 {
                return Err("Cannot bound provider pipes".into());
            }
        }
        let bytes = request.unwrap_or(&[]);
        let mut written = 0;
        if bytes.is_empty() {
            input.take();
        }
        let deadline = Instant::now() + timeout;
        let mut out = vec![];
        let mut out_open = true;
        let mut err_open = true;
        while input.is_some() || out_open || err_open {
            if INTERRUPTED.load(Ordering::SeqCst) {
                return Err("Sharing interrupted; provider outcome is unconfirmed".into());
            }
            if Instant::now() >= deadline {
                return Err("Share provider did not respond in time".into());
            }
            let mut fds = [
                libc::pollfd {
                    fd: input.as_ref().map_or(-1, AsRawFd::as_raw_fd),
                    events: libc::POLLOUT,
                    revents: 0,
                },
                libc::pollfd {
                    fd: if out_open { output.as_raw_fd() } else { -1 },
                    events: libc::POLLIN,
                    revents: 0,
                },
                libc::pollfd {
                    fd: if err_open { diagnostic.as_raw_fd() } else { -1 },
                    events: libc::POLLIN,
                    revents: 0,
                },
            ];
            unsafe {
                libc::poll(fds.as_mut_ptr(), 3, 10);
            }
            if fds[0].revents != 0 {
                match input.as_mut().unwrap().write(&bytes[written..]) {
                    Ok(n) => {
                        written += n;
                        if written == bytes.len() {
                            input.take();
                        }
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
                    Err(_) => return Err("Share provider refused request".into()),
                }
            }
            for (i, stream) in [
                (1, &mut output as &mut dyn Read),
                (2, &mut diagnostic as &mut dyn Read),
            ] {
                if fds[i].revents == 0 {
                    continue;
                }
                let mut buf = [0; 4096];
                match stream.read(&mut buf) {
                    Ok(0) => {
                        if i == 1 {
                            out_open = false
                        } else {
                            err_open = false
                        }
                    }
                    Ok(n) => {
                        if i == 2 {
                            return Err(
                                "Provider reported a diagnostic; open its application".into()
                            );
                        }
                        out.extend_from_slice(&buf[..n]);
                        if out.len() > LIMIT {
                            return Err("Provider response exceeds size limit".into());
                        }
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
                    Err(_) => return Err("Cannot read provider response".into()),
                }
            }
        }
        loop {
            if INTERRUPTED.load(Ordering::SeqCst) {
                return Err("Sharing interrupted; provider outcome is unconfirmed".into());
            }
            if Instant::now() >= deadline {
                return Err("Provider did not finish in time".into());
            }
            let mut info: libc::siginfo_t = unsafe { std::mem::zeroed() };
            let r = unsafe {
                libc::waitid(
                    libc::P_PID,
                    pid as u32,
                    &mut info,
                    libc::WEXITED | libc::WNOHANG | libc::WNOWAIT,
                )
            };
            if r < 0 {
                return Err("Cannot observe provider completion".into());
            }
            if unsafe { info.si_pid() } != 0 {
                if info.si_code != libc::CLD_EXITED || unsafe { info.si_status() } != 0 {
                    return Err("Provider failed; open its application".into());
                }
                return Ok(out);
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    })();
    let cleanup = (|| {
        let mut block: libc::sigset_t = unsafe { std::mem::zeroed() };
        let mut mask = unsafe { std::mem::zeroed() };
        unsafe {
            libc::sigemptyset(&mut block);
            libc::sigaddset(&mut block, libc::SIGTERM);
            libc::sigaddset(&mut block, libc::SIGINT);
            libc::pthread_sigmask(libc::SIG_BLOCK, &block, &mut mask);
            libc::kill(-pid, libc::SIGKILL);
        }
        let leader = child.wait();
        let deadline = Instant::now() + Duration::from_secs(1);
        let mut result = leader
            .map(|_| ())
            .map_err(|_| "Could not reap provider".to_string());
        loop {
            let r = unsafe { libc::waitpid(-pid, std::ptr::null_mut(), libc::WNOHANG) };
            if r < 0 {
                if std::io::Error::last_os_error().raw_os_error() != Some(libc::ECHILD) {
                    result = Err("Could not reap provider descendants".into())
                }
                break;
            }
            if r == 0 {
                if Instant::now() >= deadline {
                    result = Err("Provider descendants did not finish cleanup".into());
                    break;
                }
                std::thread::sleep(Duration::from_millis(5));
            }
        }
        unsafe {
            libc::prctl(libc::PR_SET_CHILD_SUBREAPER, previous, 0, 0, 0);
            libc::pthread_sigmask(libc::SIG_SETMASK, &mask, std::ptr::null_mut());
        }
        result
    })();
    cleanup?;
    result
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Advertisement {
    schema: u8,
    generation: String,
    label: String,
    state: String,
    detail: String,
    actions: Vec<String>,
}
#[derive(Serialize, Deserialize, Clone)]
struct Destination {
    id: String,
    label: String,
    state: String,
    detail: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    generation: Option<String>,
    fingerprint: String,
}
fn describe(p: &Provider) -> Result<Destination> {
    let raw = exchange(
        &p.executable,
        if p.mode == "nearby" {
            "status"
        } else {
            "share-describe"
        },
        None,
        Duration::from_millis(750),
    )?;
    let (state, label, detail, generation) = if p.mode == "nearby" {
        #[derive(Deserialize)]
        struct Status {
            available: bool,
            #[serde(rename = "canSend")]
            can_send: bool,
            detail: Option<String>,
        }
        let s: Status =
            serde_json::from_slice(&raw).map_err(|_| "Invalid provider advertisement")?;
        let ready = s.available && s.can_send;
        (
            if ready { "ready" } else { "unavailable" }.to_string(),
            p.label.clone(),
            s.detail.filter(|s| !s.is_empty()).unwrap_or(
                if ready {
                    "Ready to choose a recipient"
                } else {
                    "Open provider application or refresh destinations"
                }
                .into(),
            ),
            format!(
                "configured-{}",
                hash(p.executable.as_os_str().as_encoded_bytes())
            ),
        )
    } else {
        let s: Advertisement =
            serde_json::from_slice(&raw).map_err(|_| "Invalid provider advertisement")?;
        if s.schema != 1 || s.actions != ["files"] {
            return Err("Invalid provider advertisement".into());
        }
        (s.state, s.label, s.detail, s.generation)
    };
    if !matches!(state.as_str(), "ready" | "offline" | "unavailable")
        || !valid_text(&label, 80)
        || !valid_text(&detail, 160)
        || !valid_text(&generation, 128)
    {
        return Err("Invalid provider advertisement".into());
    }
    let mut row = Destination {
        id: p.id.clone(),
        label,
        state,
        detail,
        generation: Some(generation),
        fingerprint: String::new(),
    };
    row.fingerprint = hash(&serde_json::to_vec(&(p, &row)).unwrap());
    Ok(row)
}
fn listing(providers: &[Provider]) -> Value {
    json!({"schema":1,"destinations":providers.iter().map(|p|describe(p).unwrap_or_else(|_|Destination{id:p.id.clone(),label:p.label.clone(),state:"unavailable".into(),detail:"Provider unavailable. Open its application, then refresh destinations.".into(),generation:None,fingerprint:String::new()})).collect::<Vec<_>>()})
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct File {
    path: Box<RawValue>,
    device: String,
    inode: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    id: String,
    fingerprint: String,
    files: Vec<File>,
}
// JSON filenames use surrogate escapes for undecodable Unix bytes. Preserve their
// literal JSON and filesystem identity; never coerce through a lossy UTF-8 string.
fn path_bytes(raw: &RawValue) -> Result<Vec<u8>> {
    let s = raw.get();
    if !s.starts_with('"') || !s.ends_with('"') {
        return Err("Invalid captured file path".into());
    }
    let mut chars = s[1..s.len() - 1].chars();
    let mut bytes = vec![];
    while let Some(c) = chars.next() {
        if c != '\\' {
            let mut b = [0; 4];
            bytes.extend_from_slice(c.encode_utf8(&mut b).as_bytes());
            continue;
        }
        let c = chars.next().ok_or("Invalid filename escape")?;
        match c {
            '"' | '\\' | '/' => bytes.push(c as u8),
            'b' => bytes.push(8),
            'f' => bytes.push(12),
            'n' => bytes.push(10),
            'r' => bytes.push(13),
            't' => bytes.push(9),
            'u' => {
                let mut digits = String::new();
                for _ in 0..4 {
                    digits.push(chars.next().ok_or("Invalid filename escape")?)
                }
                let first =
                    u16::from_str_radix(&digits, 16).map_err(|_| "Invalid filename escape")?;
                if (0xdc80..=0xdcff).contains(&first) {
                    bytes.push((first - 0xdc00) as u8)
                } else {
                    let value = if (0xd800..=0xdbff).contains(&first) {
                        if chars.next() != Some('\\') || chars.next() != Some('u') {
                            return Err("Invalid filename surrogate".into());
                        }
                        digits.clear();
                        for _ in 0..4 {
                            digits.push(chars.next().ok_or("Invalid filename surrogate")?)
                        }
                        let second = u16::from_str_radix(&digits, 16)
                            .map_err(|_| "Invalid filename surrogate")?;
                        if !(0xdc00..=0xdfff).contains(&second) {
                            return Err("Invalid filename surrogate".into());
                        }
                        0x10000 + (((first as u32 - 0xd800) << 10) | (second as u32 - 0xdc00))
                    } else {
                        first as u32
                    };
                    let ch = char::from_u32(value).ok_or("Invalid filename surrogate")?;
                    let mut b = [0; 4];
                    bytes.extend_from_slice(ch.encode_utf8(&mut b).as_bytes());
                }
            }
            _ => return Err("Invalid filename escape".into()),
        }
    }
    Ok(bytes)
}
fn selection(files: &[File]) -> Result<Vec<PathBuf>> {
    if !(1..=128).contains(&files.len()) {
        return Err("Select between one and 128 regular files".into());
    }
    let mut paths = vec![];
    for row in files {
        let bytes = path_bytes(&row.path)?;
        if bytes.first() != Some(&b'/') || bytes.contains(&0) || bytes.len() > 4096 {
            return Err("Invalid captured file path".into());
        }
        let path = PathBuf::from(std::ffi::OsString::from_vec(bytes));
        if paths.contains(&path) {
            return Err("Duplicate captured file path".into());
        }
        for s in [&row.device, &row.inode] {
            if s.parse::<u64>().map(|n| n.to_string()).as_ref() != Ok(s) {
                return Err("Invalid captured file identity".into());
            }
        }
        let info = fs::symlink_metadata(&path)
            .map_err(|_| "Selected files changed. Select them and open Share again.")?;
        if !info.is_file()
            || info.dev().to_string() != row.device
            || info.ino().to_string() != row.inode
        {
            return Err("Selected files changed. Select them and open Share again.".into());
        }
        paths.push(path);
    }
    Ok(paths)
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Outcome {
    schema: u8,
    state: String,
    detail: String,
}
fn invoke(providers: &[Provider], request: &Request) -> Result<Value> {
    let p = providers
        .iter()
        .find(|p| p.id == request.id)
        .ok_or("Destination changed. Refresh destinations and choose again.")?;
    let live = describe(p)?;
    if live.state != "ready" {
        return Ok(
            json!({"state":if live.state=="offline"{"offline"}else{"error"},"detail":"Destination unavailable. Open its application, then refresh destinations."}),
        );
    }
    if live.fingerprint != request.fingerprint {
        return Err("Destination changed. Refresh destinations and choose again.".into());
    }
    let paths = selection(&request.files)?;
    if p.mode == "nearby" {
        let mut command = vec![p.executable.to_string_lossy().into_owned(), "send".into()];
        for path in paths {
            command.push(path.to_str().ok_or("This destination cannot preserve this filename. Choose another destination or rename a copy.")?.into())
        }
        return Ok(
            json!({"state":"handoff","command":command,"detail":"Complete sharing in the provider application. Delivery is unconfirmed."}),
        );
    }
    #[derive(Serialize)]
    struct Payload<'a> {
        schema: u8,
        files: &'a [File],
    }
    let data = serde_json::to_vec(&Payload {
        schema: 1,
        files: &request.files,
    })
    .map_err(|_| "Invalid sharing request")?;
    let raw = exchange(
        &p.executable,
        "share-files",
        Some(&data),
        Duration::from_secs(120),
    )
    .map_err(|_| "Provider outcome is unconfirmed. Check its application before trying again.")?;
    let o:Outcome=serde_json::from_slice(&raw).map_err(|_|"Provider returned invalid outcome. Delivery is unconfirmed; check its application before retrying.")?;
    if o.schema != 1
        || !matches!(
            o.state.as_str(),
            "completed" | "cancelled" | "offline" | "error" | "opened"
        )
        || !valid_text(&o.detail, 160)
    {
        return Err("Provider returned invalid outcome. Delivery is unconfirmed; check its application before retrying.".into());
    }
    Ok(json!({"state":o.state,"detail":o.detail}))
}
fn run() -> Result<Value> {
    let providers = configured(
        std::env::var_os("LUMEN_SHARE_DESTINATIONS")
            .filter(|s| !s.is_empty())
            .map(PathBuf::from),
    )?;
    let args: Vec<_> = std::env::args().skip(1).collect();
    match args.as_slice() {
        [a] if a == "list" => {
            let result = listing(&providers);
            if INTERRUPTED.load(Ordering::SeqCst) {
                Err("Sharing interrupted; provider outcome is unconfirmed".into())
            } else {
                Ok(result)
            }
        }
        [a, data] if a == "invoke" && data.len() <= LIMIT => invoke(
            &providers,
            &serde_json::from_str(data).map_err(|_| "Invalid sharing request")?,
        ),
        _ => Err("Expected list or one bounded explicit sharing request".into()),
    }
}
fn main() {
    unsafe {
        libc::signal(libc::SIGTERM, interrupt as *const () as usize);
        libc::signal(libc::SIGINT, interrupt as *const () as usize);
    }
    let result = run().unwrap_or_else(|detail| json!({"state":"error","detail":detail}));
    println!("{}", result);
}
