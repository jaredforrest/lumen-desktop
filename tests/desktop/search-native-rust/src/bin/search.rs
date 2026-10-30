//! Native launcher search lifecycle: production index with a Rust delivery gate.
use lumen_search_native_fixture::{
    Raster, Result, Session, copy_tree, ensure, fail, pause, round, s, sha_bytes, sha_file, wait,
};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    env, fs,
    io::{self, BufRead, BufReader, Write},
    os::unix::fs::{PermissionsExt, symlink},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
};

fn append(path: &Path, value: &Value) -> Result<()> {
    let mut f = fs::OpenOptions::new()
        .append(true)
        .create(true)
        .open(path)?;
    writeln!(f, "{value}")?;
    Ok(())
}
fn stat_start(pid: u32) -> Result<String> {
    Ok(fs::read_to_string(format!("/proc/{pid}/stat"))?
        .rsplit_once(") ")
        .ok_or("proc stat")?
        .1
        .split_whitespace()
        .nth(19)
        .ok_or("proc start")?
        .to_owned())
}
fn output() -> Result<PathBuf> {
    Ok(PathBuf::from(env::var("SEARCH_AUDIT_OUTPUT")?))
}
fn sink() -> Result<()> {
    let out = output()?;
    let args = env::args().skip(1).collect::<Vec<_>>();
    append(&out.join("opened.jsonl"), &json!(args))?;
    let marker = out.join("external.pid");
    if !marker.exists() {
        let child = Command::new("sleep")
            .arg("120")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;
        fs::write(marker, child.id().to_string())?;
        std::mem::forget(child)
    }
    Ok(())
}
static STOP: AtomicBool = AtomicBool::new(false);
extern "C" fn stop(_signal: i32) {
    STOP.store(true, Ordering::SeqCst);
}
fn gate() -> Result<()> {
    let provider = PathBuf::from(env::args_os().nth(1).ok_or("Provider path required")?);
    let out = output()?;
    let mut child = Command::new(provider)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;
    let pid = child.id();
    append(
        &out.join("providers.jsonl"),
        &json!({"pid":pid,"start":stat_start(pid)?}),
    )?;
    fs::write(out.join("provider.pid"), pid.to_string())?;
    fs::write(out.join("transport.pid"), std::process::id().to_string())?;
    unsafe {
        libc::signal(libc::SIGTERM, stop as *const () as libc::sighandler_t);
    }
    let requests: Arc<Mutex<HashMap<i64, Value>>> = Arc::new(Mutex::new(HashMap::new()));
    let lock = Arc::new(Mutex::new(()));
    let input = child.stdin.take().ok_or("Provider stdin")?;
    let stdout = child.stdout.take().ok_or("Provider stdout")?;
    let request_map = requests.clone();
    let log_lock = lock.clone();
    let request_out = out.clone();
    thread::spawn(move || {
        let mut provider_input = input;
        for line in io::stdin().lock().lines() {
            let Ok(line) = line else { break };
            let Ok(data) = serde_json::from_str::<Value>(&line) else {
                break;
            };
            if let Some(id) = data["id"].as_i64() {
                request_map.lock().unwrap().insert(id, data.clone());
            }
            let _guard = log_lock.lock().unwrap();
            let _ = append(
                &request_out.join("transport.jsonl"),
                &json!({"kind":"request","data":data}),
            );
            if writeln!(provider_input, "{line}").is_err() {
                break;
            }
            let _ = provider_input.flush();
        }
    });
    let monitor_out = out.clone();
    thread::spawn(move || {
        loop {
            if STOP.load(Ordering::SeqCst) && !monitor_out.join("defer-stop").exists() {
                unsafe { libc::kill(pid as i32, libc::SIGTERM) };
                std::process::exit(0)
            }
            pause(20)
        }
    });
    for line in BufReader::new(stdout).lines() {
        let line = line?;
        let data: Value = serde_json::from_str(&line)?;
        let req = requests
            .lock()
            .unwrap()
            .get(&data["id"].as_i64().unwrap_or(-1))
            .cloned()
            .unwrap_or(Value::Null);
        let action = req["action"].as_str().unwrap_or("");
        let query = req["query"]
            .as_str()
            .or_else(|| {
                req["path"]
                    .as_str()
                    .and_then(|p| Path::new(p).file_name().and_then(|n| n.to_str()))
            })
            .unwrap_or("");
        let hold = out.join(format!("hold-{action}-{query}"));
        {
            let _guard = lock.lock().unwrap();
            append(
                &out.join("transport.jsonl"),
                &json!({"kind":"response","data":data}),
            )?;
        }
        if hold.exists() {
            let delayed_out = out.clone();
            let delayed_lock = lock.clone();
            thread::spawn(move || {
                while hold.exists() {
                    pause(20)
                }
                let _guard = delayed_lock.lock().unwrap();
                println!("{data}");
                let _ = io::stdout().flush();
                let _ = append(
                    &delayed_out.join("transport.jsonl"),
                    &json!({"kind":"released","data":data}),
                );
                if STOP.load(Ordering::SeqCst) {
                    unsafe { libc::kill(pid as i32, libc::SIGTERM) };
                    std::process::exit(0)
                }
            });
        } else {
            let _guard = lock.lock().unwrap();
            println!("{data}");
            io::stdout().flush()?;
        }
    }
    let _ = child.kill();
    let _ = child.wait();
    Ok(())
}
struct Opt {
    launcher: PathBuf,
    sway: PathBuf,
    pointer: PathBuf,
    output: PathBuf,
    effects: bool,
}
impl Opt {
    fn parse() -> Result<Self> {
        let mut launcher = None;
        let mut sway = None;
        let mut pointer = None;
        let mut output = None;
        let mut effects = false;
        let mut a = env::args_os().skip(1);
        while let Some(v) = a.next() {
            match v.to_str() {
                Some("--launcher") => launcher = Some(PathBuf::from(a.next().ok_or("launcher")?)),
                Some("--sway") => sway = Some(PathBuf::from(a.next().ok_or("sway")?)),
                Some("--pointer-tool") => pointer = Some(PathBuf::from(a.next().ok_or("pointer")?)),
                Some("--output") => output = Some(PathBuf::from(a.next().ok_or("output")?)),
                Some("--effects") => effects = true,
                _ => {
                    return Err(fail(format!(
                        "Unknown search option: {}",
                        v.to_string_lossy()
                    )));
                }
            }
        }
        Ok(Self {
            launcher: launcher.ok_or("launcher required")?,
            sway: sway.ok_or("sway required")?,
            pointer: pointer.ok_or("pointer required")?,
            output: output.ok_or("output required")?,
            effects,
        })
    }
}
fn files(root: &Path) -> Result<Vec<Value>> {
    if !root.exists() {
        return Ok(Vec::new());
    }
    Ok(fs::read_to_string(root)?
        .lines()
        .map(serde_json::from_str)
        .collect::<std::result::Result<Vec<_>, _>>()?)
}
fn tree_node(node: &Value, pid: u32) -> Option<Value> {
    if node["pid"].as_u64() == Some(pid as u64)
        && node["name"] == "Search Lifecycle Fixture"
        && node["app_id"] == "org.quickshell"
        && node["rect"]["width"].as_i64().unwrap_or(0) > 0
        && node["rect"]["height"].as_i64().unwrap_or(0) > 0
    {
        return Some(node.clone());
    }
    for key in ["nodes", "floating_nodes"] {
        for child in node[key].as_array().into_iter().flatten() {
            if let Some(n) = tree_node(child, pid) {
                return Some(n);
            }
        }
    }
    None
}
fn png(path: &Path, rgb: [u8; 3]) -> Result<()> {
    Raster {
        width: 4,
        height: 4,
        pixels: rgb.repeat(16),
    }
    .save(path)
}
fn uri(path: &Path) -> String {
    format!(
        "file://{}",
        s(path)
            .replace('%', "%25")
            .replace(' ', "%20")
            .replace(';', "%3B")
            .replace('é', "%C3%A9")
    )
}
struct Test {
    f: Session,
    o: Opt,
    docs: PathBuf,
    qml: PathBuf,
    pointer: usize,
    window: i64,
    checks: Vec<&'static str>,
}
impl Drop for Test {
    fn drop(&mut self) {
        if let Ok(rows) = files(&self.f.out.join("providers.jsonl")) {
            for row in rows {
                if let (Some(pid), Some(start)) = (row["pid"].as_u64(), row["start"].as_str()) {
                    if stat_start(pid as u32).is_ok_and(|current| current == start) {
                        unsafe { libc::kill(pid as i32, libc::SIGKILL) };
                    }
                }
            }
        }
    }
}
impl Test {
    fn ipc(&self, method: &str, args: &[&str]) -> Result<String> {
        let mut v = vec![
            "-p",
            self.qml.to_str().ok_or("QML path")?,
            "ipc",
            "call",
            "fixture",
            method,
        ];
        v.extend(args);
        self.f.call("quickshell", &v)
    }
    fn state(&self) -> Result<Value> {
        Ok(serde_json::from_str(&self.ipc("status", &[])?)?)
    }
    fn rows(&self) -> Result<Vec<Value>> {
        Ok(self.state()?["results"]
            .as_array()
            .ok_or("Results missing")?
            .clone())
    }
    fn key(&self, args: &[&str]) -> Result<()> {
        self.f.call("wtype", args)?;
        Ok(())
    }
    fn query(&self, text: &str) -> Result<()> {
        self.key(&["-M", "ctrl", "a", "-m", "ctrl"])?;
        self.key(&[text])
    }
    fn settled(&self, text: &str) -> Result<Value> {
        wait(&format!("results {text}"), 12, || {
            let st = self.state()?;
            Ok((st["query"] == text
                && st["searching"] == false
                && st["results"].as_array().is_some_and(|r| !r.is_empty()))
            .then_some(st))
        })
    }
    fn logs(&self) -> Result<Vec<Value>> {
        files(&self.f.out.join("transport.jsonl"))
    }
    fn opened(&self) -> Result<Vec<Value>> {
        files(&self.f.out.join("opened.jsonl"))
    }
    fn response(&self, query: &str) -> Result<bool> {
        Ok(self
            .logs()?
            .iter()
            .any(|r| r["kind"] == "request" && r["data"]["query"] == query))
    }
    fn pointer_down(&mut self, text: &str) -> Result<()> {
        let b = wait(&format!("button {text}"), 12, || {
            let v: Value = serde_json::from_str(&self.ipc("button", &[text])?)?;
            Ok(v["enabled"].as_bool().filter(|e| *e).map(|_| v))
        })?;
        let x = round(b["x"].as_f64().ok_or("button x")?);
        let y = round(b["y"].as_f64().ok_or("button y")?);
        self.f.call(
            "swaymsg",
            &["-r", &format!("seat seat0 cursor set {x} {y}")],
        )?;
        self.f.write(self.pointer, "m 0.5 0.5\n")?;
        pause(120);
        self.f.write(self.pointer, "p\n")?;
        pause(120);
        Ok(())
    }
    fn pointer_up(&mut self) -> Result<()> {
        self.f.write(self.pointer, "r\n")
    }
    fn pointer(&mut self, text: &str) -> Result<()> {
        self.pointer_down(text)?;
        self.pointer_up()
    }
    fn reopen(&self) -> Result<()> {
        self.ipc("reopen", &[])?;
        pause(150);
        Ok(())
    }
    fn provider_pid(&self) -> Result<i32> {
        Ok(fs::read_to_string(self.f.out.join("provider.pid"))?
            .trim()
            .parse()?)
    }
    fn marker(&self, name: &str) -> Result<()> {
        fs::write(self.f.out.join(name), "")?;
        Ok(())
    }
    fn unmarker(&self, name: &str) -> Result<()> {
        fs::remove_file(self.f.out.join(name))?;
        Ok(())
    }
    fn selected(&self) -> Result<Value> {
        let st = self.state()?;
        let at = st["selected"].as_i64().ok_or("selection index")?;
        ensure(at >= 0, "No selected row")?;
        Ok(st["results"][at as usize].clone())
    }
    fn check(&mut self, description: &'static str) {
        self.checks.push(description)
    }
}
fn setup(o: Opt) -> Result<Test> {
    let mut f = Session::new(&o.output, &o.launcher, &o.sway, "lumen-search-native-")?;
    f.set_many(&[
        ("QT_QPA_PLATFORM", "wayland"),
        ("QT_QPA_PLATFORMTHEME", "generic"),
        ("QT_QUICK_BACKEND", "software"),
        ("QT_ACCESSIBILITY", "0"),
        ("NO_AT_BRIDGE", "1"),
        ("GTK_A11Y", "none"),
        ("QT_NO_XDG_DESKTOP_PORTAL", "1"),
        ("WLR_BACKENDS", "headless"),
        ("WLR_RENDERER", if o.effects { "gles2" } else { "pixman" }),
        (
            "WLR_RENDERER_FORCE_SOFTWARE",
            if o.effects { "1" } else { "0" },
        ),
        ("WLR_RENDERER_ALLOW_SOFTWARE", "1"),
        ("LIBGL_ALWAYS_SOFTWARE", "1"),
        ("WLR_LIBINPUT_NO_DEVICES", "1"),
        ("GLOBAL_MENU_ENABLED", "0"),
        ("LUMEN_FILE_SERVICE", "0"),
        ("LUMEN_LAUNCH_SCOPES", "0"),
    ]);
    let docs = f.root.path().join("Documents");
    fs::create_dir(&docs)?;
    f.set(
        "LUMEN_SEARCH_ROOTS",
        serde_json::to_string(&vec![s(&docs)])?,
    );
    for name in [
        "alpha one.txt",
        "beta two.txt",
        "retire three.txt",
        "neighbor four.txt",
        "quoted ; café.txt",
    ] {
        fs::write(docs.join(name), format!("Preview {name}"))?
    }
    png(&docs.join("picture.png"), [255, 0, 0])?;
    let assets = f.out.join("assets");
    let installed = PathBuf::from(
        f.env
            .get("LUMEN_DESKTOP_ASSETS")
            .ok_or("Desktop assets missing")?,
    );
    copy_tree(&installed, &assets)?;
    let source = assets.join("shell");
    let bin = f.root.path().join("bin");
    fs::create_dir(&bin)?;
    let exe = env::current_exe()?;
    symlink(&exe, bin.join("xdg-open"))?;
    let gate = f.out.join("search-gate");
    symlink(&exe, &gate)?;
    f.set(
        "PATH",
        format!(
            "{}:{}",
            bin.display(),
            f.env.get("PATH").cloned().unwrap_or_default()
        ),
    );
    f.set("SEARCH_AUDIT_OUTPUT", s(&f.out));
    let qml_state = source.join("SearchState.qml");
    let original = fs::read_to_string(&qml_state)?;
    let needle = "command:[root.helper]";
    ensure(
        original.matches(needle).count() == 1,
        "SearchState provider command changed",
    )?;
    let replacement = format!(
        "command:[{},root.helper]",
        serde_json::to_string(&s(&gate))?
    );
    let modified = original.replace(needle, &replacement);
    fs::set_permissions(&qml_state, fs::Permissions::from_mode(0o600))?;
    fs::write(&qml_state, &modified)?;
    let provider = PathBuf::from(
        f.env
            .get("LUMEN_SEARCH_BIN")
            .ok_or("Search provider missing")?,
    );
    fs::write(
        f.out.join("manifest.json"),
        serde_json::to_vec_pretty(
            &json!({"launcher":s(&o.launcher),"originalProvider":s(&provider),"providerSha256":sha_file(&provider)?,"originalStateSha256":sha_bytes(original.as_bytes()),"modifiedStateSha256":sha_bytes(modified.as_bytes()),"transformation":"Process command wraps exact installed helper with Rust response-delivery gate","roundtrip":modified.replace(&replacement,needle)==original}),
        )?,
    )?;
    let palettes: Value =
        serde_json::from_slice(&fs::read(assets.join("appearance/palette.json"))?)?;
    let appearance = f.root.path().join("state/lumen-desktop/appearance.json");
    fs::create_dir_all(appearance.parent().ok_or("appearance parent")?)?;
    fs::write(
        &appearance,
        serde_json::to_vec(
            &json!({"font":palettes["font"],"palette":palettes["light"],"preferences":{"mode":"light","reduceMotion":true}}),
        )?,
    )?;
    let qml = f.out.join("shell.qml");
    fs::write(&qml, include_str!("../../shell.qml"))?;
    f.start_bus()?;
    let config = f.out.join("sway.conf");
    fs::write(
        &config,
        "output HEADLESS-1 resolution 1200x900\nxwayland disable\nseat seat0 fallback true\nfocus_follows_mouse no\ndefault_border none\nfor_window [app_id=\"org.quickshell\"] floating enable, move position 0 0\n",
    )?;
    f.launch(
        &s(&o.sway),
        &["--unsupported-gpu", "-c", &s(&config)],
        "sway.log",
        false,
    )?;
    f.wait_sway(1)?;
    f.launch("wtype", &["-s", "600000"], "keyboard.log", false)?;
    let pointer = f.launch(
        &s(&o.pointer),
        &["pointer", "move", "0", "0"],
        "pointer.log",
        true,
    )?;
    let shell = f.launch("quickshell", &["-p", &s(&qml)], "shell.log", false)?;
    let shell_pid = f.child_pid(shell);
    let mut t = Test {
        f,
        o,
        docs,
        qml,
        pointer,
        window: 0,
        checks: Vec::new(),
    };
    wait("provider ready", 12, || {
        Ok(t.state()?["available"].as_bool().filter(|v| *v))
    })?;
    let node = wait("mapped launcher window", 12, || {
        Ok(tree_node(&t.f.tree()?, shell_pid))
    })?;
    t.window = node["id"].as_i64().ok_or("window id")?;
    t.f.call("swaymsg", &["-r", &format!("[con_id={}] focus", t.window)])?;
    wait("captured launcher focus", 12, || {
        Ok(tree_node(&t.f.tree()?, shell_pid).filter(|n| n["focused"] == true))
    })?;
    Ok(t)
}
fn run_checks(t: &mut Test) -> Result<()> {
    t.pointer("Files")?;
    wait("Files category", 12, || {
        Ok((t.state()?["provider"] == "Files").then_some(()))
    })?;
    ensure(t.rows()?.is_empty(), "File category should begin empty")?;
    t.check("empty file category");
    t.marker("hold-search-alpha")?;
    t.query("alpha")?;
    wait("delayed first query", 12, || {
        Ok(t.response("alpha")?.then_some(()))
    })?;
    t.query("beta")?;
    t.settled("beta")?;
    ensure(
        t.rows()?
            .iter()
            .all(|r| r["title"].as_str().is_some_and(|s| s.contains("beta"))),
        "Beta rows changed",
    )?;
    t.unmarker("hold-search-alpha")?;
    pause(300);
    ensure(
        t.rows()?
            .iter()
            .all(|r| r["title"].as_str().is_some_and(|s| s.contains("beta"))),
        "Late alpha results replaced beta",
    )?;
    t.check("late old query cannot replace current results");
    t.key(&["-M", "ctrl", "-k", "space", "-m", "ctrl"])?;
    wait("preview", 12, || {
        Ok((t.state()?["preview"]["type"] == "text").then_some(()))
    })?;
    ensure(
        t.state()?["preview"]["text"] == "Preview beta two.txt" && t.opened()?.is_empty(),
        "Preview changed selection or opened file",
    )?;
    t.check("preview selected identity without opening");
    t.marker("hold-preview-alpha one.txt")?;
    t.query("alpha")?;
    t.settled("alpha")?;
    let alpha = s(&t.docs.join("alpha one.txt"));
    wait("delayed preview", 12, || {
        Ok(t.logs()?
            .iter()
            .any(|r| {
                r["kind"] == "request"
                    && r["data"]["action"] == "preview"
                    && r["data"]["path"] == alpha
            })
            .then_some(()))
    })?;
    t.query("beta")?;
    t.settled("beta")?;
    wait("new preview", 12, || {
        Ok((t.state()?["preview"]["text"] == "Preview beta two.txt").then_some(()))
    })?;
    t.unmarker("hold-preview-alpha one.txt")?;
    pause(250);
    ensure(
        t.state()?["preview"]["text"] == "Preview beta two.txt",
        "Late preview displaced beta",
    )?;
    t.check("late preview cannot replace newly selected resource");
    t.key(&["-k", "Return"])?;
    wait("selected open", 12, || {
        Ok((t.opened()?.len() == 1).then_some(()))
    })?;
    ensure(
        t.opened()? == vec![json!([uri(&t.docs.join("beta two.txt"))])],
        "Structured beta launch mismatch",
    )?;
    t.check("exact selected structured launch");
    t.reopen()?;
    t.pointer("Files")?;
    t.query("quoted")?;
    t.settled("quoted")?;
    t.key(&["-k", "Return"])?;
    wait("quoted open", 12, || {
        Ok((t.opened()?.len() == 2).then_some(()))
    })?;
    ensure(
        t.opened()?.last() == Some(&json!([uri(&t.docs.join("quoted ; café.txt"))])),
        "Quoted path changed",
    )?;
    t.check("non ASCII metacharacter path preserved");
    t.reopen()?;
    t.pointer("Files")?;
    t.query("retire")?;
    t.settled("retire")?;
    let retired = t.docs.join("retire three.txt");
    fs::rename(&retired, t.docs.join("old-retired"))?;
    fs::write(&retired, "Replacement must not open")?;
    t.key(&["-k", "Return"])?;
    wait("replacement rejected", 12, || {
        Ok((t.state()?["opening"] == false).then_some(()))
    })?;
    ensure(
        t.opened()?.len() == 2 && t.state()?["visible"] == true,
        "Replacement opened or launcher dismissed",
    )?;
    t.check("same path replacement rejected before external dispatch");
    t.query("neighbor")?;
    t.settled("neighbor")?;
    let neighbor_token = t.rows()?[0]["token"].clone();
    t.marker("hold-search-neighbor ")?;
    t.query("neighbor ")?;
    wait("pending neighbor query", 12, || {
        Ok(t.response("neighbor ")?.then_some(()))
    })?;
    unsafe { libc::kill(t.provider_pid()?, libc::SIGTERM) };
    wait("provider exit", 12, || {
        Ok((t.state()?["available"] == false).then_some(()))
    })?;
    ensure(
        t.rows()?.is_empty() && t.state()?["error"].as_str().is_some_and(|s| !s.is_empty()),
        "Provider loss retained actions",
    )?;
    t.key(&["-k", "Return"])?;
    ensure(t.opened()?.len() == 2, "Provider loss opened cached action")?;
    t.check("provider loss clears cached action and prevents launch");
    t.unmarker("hold-search-neighbor ")?;
    wait("provider reconnect", 12, || {
        let st = t.state()?;
        Ok((st["available"] == true
            && st["searching"] == false
            && st["results"].as_array().is_some_and(|a| !a.is_empty()))
        .then_some(()))
    })?;
    ensure(
        t.rows()?[0]["path"] == s(&t.docs.join("neighbor four.txt"))
            && t.rows()?[0]["token"] != neighbor_token,
        "Provider reconnect not scoped",
    )?;
    t.check("provider restart recovers scoped results");
    t.query("ext:txt")?;
    t.settled("ext:txt")?;
    t.key(&["-k", "Down"])?;
    let selected = t.selected()?["path"]
        .as_str()
        .ok_or("selected path")?
        .to_owned();
    fs::remove_file(&selected)?;
    wait("removed selected result", 12, || {
        Ok(t.rows()?
            .iter()
            .all(|r| r["path"] != selected)
            .then_some(()))
    })?;
    ensure(
        t.state()?["selected"] == -1,
        "Removed row retargeted selection",
    )?;
    t.key(&["-k", "Return"])?;
    pause(150);
    ensure(t.opened()?.len() == 2, "Removed row opened adjacent result")?;
    t.check("removed selected row never retargets adjacent result");
    let before = t.logs()?.len();
    t.marker("hold-search-alpha")?;
    t.query("alpha")?;
    wait("category pending query", 12, || {
        Ok(t.logs()?
            .iter()
            .skip(before)
            .any(|r| r["kind"] == "request" && r["data"]["query"] == "alpha")
            .then_some(()))
    })?;
    t.pointer("Applications")?;
    wait("changed category", 12, || {
        Ok((t.state()?["provider"] == "Applications").then_some(()))
    })?;
    t.unmarker("hold-search-alpha")?;
    pause(200);
    ensure(
        t.rows()?.is_empty(),
        "Late file rows leaked into Applications",
    )?;
    t.check("late file results do not leak into another category");
    t.pointer("Applications")?;
    wait("Applications category", 12, || {
        Ok((t.state()?["provider"] == "Applications").then_some(()))
    })?;
    ensure(t.rows()?.is_empty(), "Unexpected application rows")?;
    t.pointer("Actions")?;
    wait("Actions category", 12, || {
        Ok((t.state()?["provider"] == "Actions").then_some(()))
    })?;
    t.query("Control")?;
    wait("action category", 12, || {
        Ok(t.rows()?
            .iter()
            .any(|r| r["kind"] == "action")
            .then_some(()))
    })?;
    ensure(
        t.rows()?.iter().all(|r| r["kind"] == "action"),
        "Category provenance changed",
    )?;
    t.check("category transitions preserve provenance");
    t.pointer("Files")?;
    t.query("neighbor")?;
    t.settled("neighbor")?;
    let pid = t.provider_pid()?;
    unsafe { libc::kill(pid, libc::SIGSTOP) };
    let before = t.logs()?.len();
    t.query("beta")?;
    wait("hung query dispatched", 12, || {
        Ok(t.logs()?
            .iter()
            .skip(before)
            .any(|r| r["kind"] == "request" && r["data"]["query"] == "beta")
            .then_some(()))
    })?;
    wait("bounded hung provider deadline", 12, || {
        let st = t.state()?;
        Ok((st["available"] == false
            && st["error"]
                .as_str()
                .is_some_and(|s| s.contains("unavailable") || s.contains("timed out")))
        .then_some(()))
    })?;
    ensure(
        t.rows()?.is_empty() && t.state()?["searching"] == false,
        "Hung provider retained rows",
    )?;
    unsafe { libc::kill(pid, libc::SIGKILL) };
    wait("hung provider recovery", 12, || {
        let st = t.state()?;
        Ok((st["available"] == true && st["searching"] == false).then_some(()))
    })?;
    ensure(t.opened()?.len() == 2, "Hung provider retried action")?;
    t.check("SIGSTOP provider retires cached actions within bounded deadline and recovers without open retry");
    let out = t.f.out.clone();
    t.pointer("Files")?;
    t.query("quoted")?;
    t.settled("quoted")?;
    t.marker("hold-open-quoted ; café.txt")?;
    t.marker("defer-stop")?;
    t.key(&["-k", "Return"])?;
    wait("pending open acknowledgement", 12, || {
        Ok((t.opened()?.len() == 3 && t.state()?["opening"] == true).then_some(()))
    })?;
    t.key(&["-k", "Escape"])?;
    wait("dismiss pending action", 12, || {
        Ok((t.state()?["visible"] == false).then_some(()))
    })?;
    t.reopen()?;
    t.pointer("Files")?;
    t.query("neighbor")?;
    t.unmarker("hold-open-quoted ; café.txt")?;
    wait("retired action reply", 12, || {
        Ok(t.logs()?
            .iter()
            .any(|r| r["kind"] == "released" && r["data"]["action"] == "open")
            .then_some(()))
    })?;
    t.unmarker("defer-stop")?;
    t.settled("neighbor")?;
    ensure(
        t.state()?["visible"] == true && t.opened()?.len() == 3 && t.state()?["opening"] == false,
        "Retired reply altered new invocation",
    )?;
    t.check("retired open acknowledgement cannot dismiss new invocation or retry action");
    let external: usize = fs::read_to_string(out.join("external.pid"))?
        .trim()
        .parse()?;
    let stat = fs::read_to_string(format!("/proc/{external}/stat"))?;
    ensure(
        !stat
            .rsplit_once(") ")
            .ok_or("external stat")?
            .1
            .starts_with('Z'),
        "External launch observer died",
    )?;
    t.check("external launch observer survives exact provider retirement");
    t.pointer("Files")?;
    t.marker("hold-preview-picture.png")?;
    t.query("picture")?;
    t.settled("picture")?;
    if t.state()?["previewOpen"] == false {
        t.key(&["-M", "ctrl", "-k", "space", "-m", "ctrl"])?
    }
    wait("retained image response", 12, || {
        Ok(t.logs()?
            .iter()
            .any(|r| {
                r["kind"] == "response"
                    && r["data"]["action"] == "preview"
                    && r["data"]["name"] == "picture.png"
            })
            .then_some(()))
    })?;
    let held = t.state()?["previewId"].as_i64().ok_or("preview id")?;
    ensure(held > 0, "No held preview")?;
    let before = t.logs()?.len();
    t.key(&["-M", "ctrl", "-k", "r", "-m", "ctrl"])?;
    let refresh = wait("identical selection refresh request", 12, || {
        Ok(t.logs()?
            .iter()
            .skip(before)
            .find(|r| {
                r["kind"] == "request"
                    && r["data"]["action"] == "search"
                    && r["data"]["query"] == "picture"
            })
            .and_then(|r| r["data"]["id"].as_i64()))
    })?;
    wait("identical selection refresh response", 12, || {
        Ok(t.logs()?
            .iter()
            .skip(before)
            .any(|r| r["kind"] == "response" && r["data"]["id"] == refresh)
            .then_some(()))
    })?;
    wait("identical selection refresh applied", 12, || {
        Ok((t.state()?["searching"] == false).then_some(()))
    })?;
    ensure(
        t.state()?["previewId"] == held,
        "Same selection retired preview",
    )?;
    ensure(
        !t.logs()?.iter().skip(before).any(|r| {
            r["kind"] == "request"
                && r["data"]["action"] == "retire-preview"
                && r["data"]["requests"]
                    .as_array()
                    .is_some_and(|a| a.contains(&json!(held)))
        }),
        "Same selection sent retire-preview",
    )?;
    fs::write(
        out.join("same-selection-refresh.json"),
        serde_json::to_vec(
            &json!({"previewId":held,"selection":t.selected()?,"windowId":t.window}),
        )?,
    )?;
    t.marker("hold-search-picture")?;
    let replacement = t.docs.join("replacement.png");
    png(&replacement, [0, 0, 255])?;
    fs::rename(&replacement, t.docs.join("picture.png"))?;
    t.unmarker("hold-preview-picture.png")?;
    let image = wait("actual image loaded", 12, || {
        let v: Value = serde_json::from_str(&t.ipc("image", &[])?)?;
        Ok((v["status"] == 1).then_some(v))
    })?;
    let source = image["source"].as_str().ok_or("image source")?;
    let snapshot = PathBuf::from(source.strip_prefix("file://").unwrap_or(source));
    let red = fs::read(&snapshot)?;
    let red_source = out.join("expected-red.png");
    png(&red_source, [255, 0, 0])?;
    let blue_source = out.join("expected-blue.png");
    png(&blue_source, [0, 0, 255])?;
    ensure(
        red == fs::read(&red_source)?
            && fs::read(t.docs.join("picture.png"))? == fs::read(&blue_source)?,
        "Retained image bytes changed",
    )?;
    let x = round(image["x"].as_f64().ok_or("image x")?);
    let y = round(image["y"].as_f64().ok_or("image y")?);
    let pixel = wait("retained pixels presented", 12, || {
        t.f.capture(&out.join("retained-image.png"), &["-o", "HEADLESS-1"])?;
        let v = t.f.call(
            "magick",
            &[
                &s(&out.join("retained-image.png")),
                "-format",
                &format!("%[pixel:p{{{x},{y}}}]"),
                "info:",
            ],
        )?;
        Ok(["srgb(255,0,0)", "srgba(255,0,0,1)"]
            .contains(&v.as_str())
            .then_some(v))
    })?;
    fs::write(
        out.join("retained-image-proof.json"),
        serde_json::to_vec(
            &json!({"image":image,"pixel":pixel,"retainedSha256":sha_file(&snapshot)?,"replacementSha256":sha_file(&t.docs.join("picture.png"))?}),
        )?,
    )?;
    t.check("delayed image response loads retained original pixels after source replacement");
    t.unmarker("hold-search-picture")?;
    t.query("ext:txt")?;
    t.settled("ext:txt")?;
    let captured = t.selected()?["path"]
        .as_str()
        .ok_or("captured path")?
        .to_owned();
    t.pointer_down("Open")?;
    let pressed = wait("physical preview button press and focus", 12, || {
        let button: Value = serde_json::from_str(&t.ipc("button", &["Open"])?)?;
        Ok((button["pressed"] == true && button["activeFocus"] == true).then_some(button))
    })?;
    t.key(&["-k", "Down"])?;
    ensure(
        t.selected()?["path"] == captured && t.opened()?.len() == 3,
        "Preview-focused Down changed the selected resource or dispatched during held press",
    )?;
    fs::remove_file(&captured)?;
    let removed = wait("held preview selected resource removal", 12, || {
        let state = t.state()?;
        Ok((state["selected"] == -1
            && state["results"].as_array().is_some_and(|rows| rows.iter().all(|row| row["path"] != captured)))
            .then_some(state))
    })?;
    t.pointer_up()?;
    t.reopen()?;
    t.query("neighbor")?;
    t.settled("neighbor")?;
    ensure(
        t.opened()?.len() == 3,
        "Held preview release opened a removed or replacement resource",
    )?;
    let fresh = t.selected()?["path"].as_str().ok_or("fresh selected path")?.to_owned();
    fs::write(
        out.join("held-preview-refusal.json"),
        serde_json::to_vec_pretty(&json!({"capturedPath":captured,"physicalButton":pressed,"afterRemoval":removed,"freshPath":fresh,"openedAfterRelease":t.opened()?}))?,
    )?;
    t.pointer("Open")?;
    wait("captured preview button", 12, || {
        Ok((t.opened()?.len() == 4).then_some(()))
    })?;
    ensure(
        t.opened()?.last() == Some(&json!([uri(Path::new(&fresh))])),
        "Fresh preview press opened wrong resource",
    )?;
    t.check(
        "physical preview focus keeps Down in preview; held Open refuses removed selection and fresh press opens exact current resource",
    );
    t.reopen()?;
    wait("fresh quota provider", 12, || {
        Ok(t.state()?["available"].as_bool().filter(|v| *v))
    })?;
    t.pointer("Files")?;
    for i in 1..=3 {
        fs::File::create(t.docs.join(format!("quota{i}.bin")))?.set_len(64 * 1024 * 1024)?
    }
    let mut retired = Vec::<PathBuf>::new();
    let mut working = Vec::<Value>::new();
    for i in 1..=3 {
        let q = format!("quota{i}");
        t.query(&q)?;
        t.settled(&q)?;
        if t.state()?["previewOpen"] == false {
            t.key(&["-M", "ctrl", "-k", "space", "-m", "ctrl"])?
        }
        wait(&format!("accepted bounded working set {i}"), 12, || {
            Ok((t.state()?["preview"]["reason"] == "unsupported").then_some(()))
        })?;
        let pid = t.provider_pid()?;
        let base =
            t.f.root
                .path()
                .join(format!("lumen-preview-snapshots-{}", unsafe {
                    libc::getuid()
                }));
        let mut live = Vec::<PathBuf>::new();
        if base.exists() {
            for entry in fs::read_dir(&base)? {
                let p = entry?.path();
                if p.file_name()
                    .is_some_and(|n| n.to_string_lossy().starts_with(&format!("{pid}-")))
                {
                    for child in fs::read_dir(p)? {
                        let file = child?.path();
                        if file.is_file() {
                            live.push(file)
                        }
                    }
                }
            }
        }
        ensure(
            live.len() == 1 && fs::metadata(&live[0])?.len() == 64 * 1024 * 1024,
            "Snapshot working set exceeded 64 MiB",
        )?;
        ensure(
            retired.iter().all(|p| !p.exists()),
            "Retired snapshot still exists",
        )?;
        retired.extend(live.iter().cloned());
        working.push(json!({"selection":i,"bytes":64*1024*1024,"paths":live.iter().map(|p|s(p)).collect::<Vec<_>>() }));
    }
    fs::write(
        out.join("working-set.json"),
        serde_json::to_vec_pretty(&working)?,
    )?;
    ensure(
        t.state()?["visible"] == true,
        "Quota check dismissed launcher",
    )?;
    t.check("192 MiB of successive real input copies reuse a bounded64 MiB working set after exact retirement");
    let previous = t.provider_pid()?;
    t.key(&["-k", "Escape"])?;
    wait("quota dismissal", 12, || {
        Ok((t.state()?["visible"] == false).then_some(()))
    })?;
    t.reopen()?;
    wait("new quota provider", 12, || {
        Ok(
            (t.state()?["available"] == true && t.provider_pid().is_ok_and(|p| p != previous))
                .then_some(()),
        )
    })?;
    t.pointer("Files")?;
    t.query("quota3")?;
    t.settled("quota3")?;
    t.key(&["-M", "ctrl", "-k", "space", "-m", "ctrl"])?;
    wait("quota reset after cached reopen", 12, || {
        Ok((t.state()?["preview"]["reason"] == "unsupported").then_some(()))
    })?;
    ensure(t.opened()?.len() == 4, "Quota reopen replayed launch")?;
    t.check("actual dismissal reopens cached index with fresh snapshot quota and no action replay");
    ensure(t.checks.len() == 19, "Search check count changed")?;
    let result = json!({"passed":true,"checks":t.checks,"launcher":s(&t.o.launcher),"sway":s(&t.o.sway),"effects":t.o.effects,"hostClipboardUntouched":true,"externalOpenBoundary":"Validated selected identity at dispatch; external handler may reopen the path later."});
    fs::write(out.join("result.json"), serde_json::to_vec_pretty(&result)?)?;
    println!("{result}");
    Ok(())
}
fn main() {
    let result = (|| -> Result<()> {
        let name = env::args_os()
            .next()
            .and_then(|v| PathBuf::from(v).file_name().map(|s| s.to_owned()));
        if name.as_ref().is_some_and(|v| v == "xdg-open") {
            return sink();
        }
        if name.is_some_and(|v| v == "search-gate") {
            return gate();
        }
        let o = Opt::parse()?;
        let mut t = setup(o)?;
        run_checks(&mut t)?;
        Ok(())
    })();
    if let Err(e) = result {
        eprintln!("Search native fixture: {e}");
        std::process::exit(1)
    }
}
