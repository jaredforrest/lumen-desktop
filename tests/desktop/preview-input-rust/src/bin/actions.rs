//! Private native Quick Look and Files action dispatch through physical controls.
use lumen_preview_input_fixtures::{
    Raster, Result, Session, copy_tree, ensure, fail, pause, round, s, sha_bytes, sha_file, wait,
};
use serde_json::{Value, json};
use std::{
    env, fs,
    io::{self, BufRead, Write},
    os::unix::fs::{PermissionsExt, symlink},
    path::{Path, PathBuf},
};

fn append(path: &Path, value: &Value) -> Result<()> {
    let mut f = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    writeln!(f, "{value}")?;
    Ok(())
}
fn records(path: &Path) -> Result<Vec<Value>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    Ok(fs::read_to_string(path)?
        .lines()
        .map(serde_json::from_str)
        .collect::<std::result::Result<_, _>>()?)
}
fn image(path: &Path, rgb: [u8; 3]) -> Result<()> {
    Raster {
        width: 32,
        height: 32,
        pixels: rgb.repeat(32 * 32),
    }
    .save(path)
}
fn uri(path: &Path) -> String {
    format!("file://{}", s(path).replace('%', "%25").replace(' ', "%20"))
}
fn observer() -> Result<()> {
    let out = PathBuf::from(env::var("PREVIEW_ACTION_AUDIT")?);
    let name = env::args_os()
        .next()
        .and_then(|v| PathBuf::from(v).file_name().map(|n| n.to_owned()));
    if name.as_ref().is_some_and(|v| v == "xdg-open") {
        append(
            &out.join("opened.jsonl"),
            &json!(env::args().skip(1).collect::<Vec<_>>()),
        )?;
        return Ok(());
    }
    let mut line = String::new();
    io::stdin().lock().read_line(&mut line)?;
    let data: Value = serde_json::from_str(&line)?;
    let source = PathBuf::from(data["path"].as_str().ok_or("context source")?);
    let bytes = fs::read(&source)?;
    let action = data["action"].as_str().ok_or("context action")?;
    append(
        &out.join("content.jsonl"),
        &json!({"action":action,"path":s(&source),"sha256":sha_file(&source)?,"pid":std::process::id()}),
    )?;
    while out.join("hold-content").exists() {
        pause(1000)
    }
    if action == "ocr" {
        println!(
            "{}",
            json!({"text":"Private recognized text","message":"Private observer"})
        )
    } else {
        let target = out.join(format!("subject-{}.png", std::process::id()));
        fs::write(&target, bytes)?;
        println!(
            "{}",
            json!({"subject":s(&target),"message":"Private extraction observer"})
        )
    }
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
                        "Unknown actions argument: {}",
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
fn node(tree: &Value, pid: u32, title: &str) -> Option<Value> {
    if tree["pid"].as_u64() == Some(pid as u64)
        && tree["name"] == title
        && tree["app_id"] == "org.quickshell"
    {
        return Some(tree.clone());
    }
    for key in ["nodes", "floating_nodes"] {
        for child in tree[key].as_array().into_iter().flatten() {
            if let Some(n) = node(child, pid, title) {
                return Some(n);
            }
        }
    }
    None
}
fn descendants(pid: u32, out: &mut Vec<u32>) -> Result<()> {
    let p = format!("/proc/{pid}/task/{pid}/children");
    for part in fs::read_to_string(p)?.split_whitespace() {
        let child: u32 = part.parse()?;
        out.push(child);
        descendants(child, out).ok();
    }
    Ok(())
}
struct Test {
    f: Session,
    o: Opt,
    one: PathBuf,
    two: PathBuf,
    docs: PathBuf,
    qml: PathBuf,
    pointer: usize,
    shell: u32,
    checks: Vec<&'static str>,
}
impl Test {
    fn ipc(&self, method: &str, args: &[&str]) -> Result<String> {
        let mut cmd = vec![
            "-p",
            self.qml.to_str().ok_or("QML path")?,
            "ipc",
            "call",
            "fixture",
            method,
        ];
        cmd.extend(args);
        self.f.call("quickshell", &cmd)
    }
    fn state(&self) -> Result<Value> {
        Ok(serde_json::from_str(&self.ipc("status", &[])?)?)
    }
    fn records(&self, name: &str) -> Result<Vec<Value>> {
        records(&self.f.out.join(name))
    }
    fn button(&self, surface: &str, text: &str) -> Result<Value> {
        Ok(serde_json::from_str(
            &self.ipc("button", &[surface, text])?,
        )?)
    }
    fn point(&mut self, surface: &str, text: &str) -> Result<Value> {
        let b = wait(&format!("button {text}"), 12, || {
            let v = self.button(surface, text)?;
            Ok(v["title"].is_string().then_some(v))
        })?;
        let title = b["title"].as_str().ok_or("button owner title")?.to_owned();
        let owned = wait("mapped exact control owner", 12, || {
            Ok(node(&self.f.tree()?, self.shell, &title))
        })?;
        let id = owned["id"].as_i64().ok_or("control owner id")?;
        self.f
            .call("swaymsg", &["-r", &format!("[con_id={id}] focus")])?;
        let (n, b) = wait("focused configured exact control owner", 12, || {
            let n = node(&self.f.tree()?, self.shell, &title);
            let b = self.button(surface, text)?;
            Ok(n.filter(|v| {
                v["id"] == id
                    && v["focused"] == true
                    && b["active"] == true
                    && v["window_rect"]["width"] == b["windowWidth"]
                    && v["window_rect"]["height"] == b["windowHeight"]
            })
            .map(|v| (v, b)))
        })?;
        fs::write(
            self.f.out.join("point.json"),
            serde_json::to_vec_pretty(&json!({"position":b,"node":n}))?,
        )?;
        let x = round(
            n["rect"]["x"].as_f64().unwrap_or(0.)
                + n["window_rect"]["x"].as_f64().unwrap_or(0.)
                + b["x"].as_f64().ok_or("button x")?,
        );
        let y = round(
            n["rect"]["y"].as_f64().unwrap_or(0.)
                + n["window_rect"]["y"].as_f64().unwrap_or(0.)
                + b["y"].as_f64().ok_or("button y")?,
        );
        self.f.call(
            "swaymsg",
            &["-r", &format!("seat seat0 cursor set {x} {y}")],
        )?;
        self.f.write(self.pointer, "m 0.5 0.5\n")?;
        wait("hovered exact control", 12, || {
            Ok(self.button(surface, text)?["hovered"]
                .as_bool()
                .filter(|v| *v))
        })?;
        Ok(n)
    }
    fn press(&mut self, surface: &str, text: &str) -> Result<()> {
        self.point(surface, text)?;
        self.f.write(self.pointer, "p\n")?;
        wait(&format!("actual pressed {text}"), 12, || {
            Ok(self.button(surface, text)?["down"].as_bool().filter(|v| *v))
        })?;
        Ok(())
    }
    fn release(&mut self) -> Result<()> {
        self.f.write(self.pointer, "r\n")
    }
    fn click(&mut self, surface: &str, text: &str) -> Result<()> {
        self.press(surface, text)?;
        self.release()
    }
    fn menu(&mut self, text: &str) -> Result<()> {
        self.click("quick", "•••")?;
        self.click("quick", text)
    }
    fn show(&self, path: &Path) -> Result<Value> {
        self.ipc("startPreview", &[&s(path)])?;
        wait("captured preview", 12, || {
            let st = self.state()?;
            Ok((st["result"]["sourcePath"] == s(path)
                && st["result"]["actionCapability"].as_str().is_some())
            .then_some(st))
        })
    }
    fn key(&self, args: &[&str]) -> Result<()> {
        self.f.call("wtype", args)?;
        Ok(())
    }
    fn check(&mut self, name: &'static str) {
        self.checks.push(name)
    }
}
fn setup(o: Opt) -> Result<Test> {
    let mut f = Session::new(&o.output, &o.launcher, &o.sway, "lumen-preview-actions-")?;
    f.set_many(&[
        ("QT_QPA_PLATFORM", "wayland"),
        ("QT_QPA_PLATFORMTHEME", "generic"),
        ("QT_QUICK_BACKEND", "software"),
        ("QT_ACCESSIBILITY", "0"),
        ("NO_AT_BRIDGE", "1"),
        ("GTK_A11Y", "none"),
        ("WLR_BACKENDS", "headless"),
        ("WLR_RENDERER", if o.effects { "gles2" } else { "pixman" }),
        ("WLR_RENDERER_ALLOW_SOFTWARE", "1"),
        (
            "WLR_RENDERER_FORCE_SOFTWARE",
            if o.effects { "1" } else { "0" },
        ),
        ("LIBGL_ALWAYS_SOFTWARE", "1"),
        ("WLR_LIBINPUT_NO_DEVICES", "1"),
        ("LUMEN_LAUNCH_SCOPES", "0"),
        ("GLOBAL_MENU_ENABLED", "0"),
        ("LUMEN_FILE_SERVICE", "0"),
    ]);
    let out = f.out.clone();
    f.set("PREVIEW_ACTION_AUDIT", s(&out));
    let bin = f.root.path().join("bin");
    fs::create_dir(&bin)?;
    let exe = env::current_exe()?;
    let open = bin.join("xdg-open");
    symlink(&exe, &open)?;
    let context = bin.join("private-context-observer");
    symlink(&exe, &context)?;
    f.set(
        "PATH",
        format!(
            "{}:{}",
            bin.display(),
            f.env.get("PATH").cloned().unwrap_or_default()
        ),
    );
    let docs = f.root.path().join("Documents");
    fs::create_dir(&docs)?;
    let one = docs.join("one.png");
    let two = docs.join("two.png");
    image(&one, [255, 0, 0])?;
    image(&two, [0, 0, 255])?;
    let appdir = f.root.path().join("data/applications");
    fs::create_dir_all(&appdir)?;
    fs::write(
        appdir.join("lumen-private-observer.desktop"),
        format!(
            "[Desktop Entry]\nType=Application\nName=Private Preview Observer\nExec={} %U\nMimeType=image/png;application/octet-stream;\n",
            open.display()
        ),
    )?;
    fs::write(
        appdir.join("mimeinfo.cache"),
        "[MIME Cache]\nimage/png=lumen-private-observer.desktop;\napplication/octet-stream=lumen-private-observer.desktop;\n",
    )?;
    let assets = out.join("assets");
    let installed = PathBuf::from(
        f.env
            .get("LUMEN_DESKTOP_ASSETS")
            .ok_or("Installed assets missing")?,
    );
    copy_tree(&installed, &assets)?;
    let original = f
        .env
        .get("LUMEN_CONTEXT_TOOLS")
        .cloned()
        .ok_or("Context helper missing")?;
    f.set("LUMEN_CONTEXT_TOOLS", s(&context));
    let quick = assets.join("shell/QuickLookWindow.qml");
    let original_qml = fs::read_to_string(&quick)?;
    let marker = "    id:root\n";
    ensure(
        original_qml.matches(marker).count() == 1,
        "QuickLook root marker changed",
    )?;
    let added = "    id:root\n    readonly property var fixtureMenu:previewActions\n";
    let modified = original_qml.replacen(marker, added, 1);
    ensure(
        modified.replacen(added, marker, 1) == original_qml,
        "QuickLook probe roundtrip",
    )?;
    fs::set_permissions(&quick, fs::Permissions::from_mode(0o600))?;
    fs::write(&quick, &modified)?;
    fs::write(
        out.join("instrumentation.json"),
        serde_json::to_vec_pretty(
            &json!({"originalHelper":original,"observer":s(&context),"scope":"Fixed OCR and extraction subprocess replaced by a private byte-reading Rust observer; UI, broker, source authority and child supervisor unchanged.","quickOriginalSha256":sha_bytes(original_qml.as_bytes()),"quickModifiedSha256":sha_bytes(modified.as_bytes()),"quickScope":"Only read-only actual Menu object exposure for native popup item geometry/status"}),
        )?,
    )?;
    let qml = out.join("shell.qml");
    fs::write(
        &qml,
        include_str!("../../actions.qml")
            .replace("__DOCS_JSON__", &serde_json::to_string(&s(&docs))?),
    )?;
    f.start_bus()?;
    let config = out.join("sway.conf");
    fs::write(
        &config,
        "output HEADLESS-1 resolution 1280x900\nxwayland disable\nseat seat0 fallback true\nfocus_follows_mouse no\ndefault_border none\nfor_window [app_id=\"org.quickshell\"] floating enable, move position 20 20\n",
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
    let t = Test {
        f,
        o,
        one,
        two,
        docs,
        qml,
        pointer,
        shell: shell_pid,
        checks: Vec::new(),
    };
    wait("Files ready", 12, || {
        Ok(t.state()?["ready"].as_bool().filter(|v| *v))
    })?;
    Ok(t)
}
struct Paused(i32);
impl Drop for Paused {
    fn drop(&mut self) {
        unsafe { libc::kill(self.0, libc::SIGCONT) };
    }
}
fn test(t: &mut Test) -> Result<()> {
    let one = t.one.clone();
    let two = t.two.clone();
    let docs = t.docs.clone();
    t.show(&one)?;
    t.click("quick", "Open")?;
    wait("captured Open", 12, || {
        Ok((t.records("opened.jsonl")?.len() == 1).then_some(()))
    })?;
    ensure(
        t.records("opened.jsonl")?[0] == json!([uri(&one)]),
        "Quick Look Open URI changed",
    )?;
    t.check("Actual Quick Look Open dispatches exact captured source through private observer");
    let before = t.state()?["result"].clone();
    let replacement = docs.join("replacement.png");
    image(&replacement, [0, 255, 0])?;
    fs::rename(&replacement, &one)?;
    t.click("quick", "Open")?;
    wait("replacement refused", 12, || {
        Ok((t.state()?["message"] == "The selected file changed").then_some(()))
    })?;
    ensure(
        t.records("opened.jsonl")?.len() == 1
            && t.state()?["result"]["actionCapability"] == before["actionCapability"],
        "Replacement dispatched or altered preview",
    )?;
    t.check(
        "Original replacement produces zero dispatch and preserves successful immutable preview",
    );
    t.show(&one)?;
    t.press("quick", "Open")?;
    t.show(&two)?;
    t.release()?;
    pause(150);
    ensure(
        t.records("opened.jsonl")?.len() == 1,
        "Held Open retargeted",
    )?;
    t.check("Held pointer Open cannot retarget a newly selected preview");
    t.menu("Open With…")?;
    t.click("quick", "Private Preview Observer")?;
    wait("Open With observer", 12, || {
        Ok((t.records("opened.jsonl")?.len() == 2).then_some(()))
    })?;
    let opened = t.records("opened.jsonl")?;
    ensure(
        opened
            .last()
            .and_then(|v| v[0].as_str())
            .is_some_and(|v| v == s(&two) || v == uri(&two)),
        "Open With target changed",
    )?;
    t.check("Actual Quick Look Open With chooses registered private application");
    t.menu("Open With…")?;
    wait("chooser visible", 12, || {
        Ok(t.button("quick", "Private Preview Observer")?["title"]
            .as_str()
            .map(str::to_owned))
    })?;
    image(&replacement, [255, 255, 0])?;
    fs::rename(&replacement, &two)?;
    t.click("quick", "Private Preview Observer")?;
    wait("stale chooser refused", 12, || {
        Ok((t.state()?["message"] == "The selected file changed").then_some(()))
    })?;
    ensure(
        t.records("opened.jsonl")?.len() == 2,
        "Stale chooser dispatched",
    )?;
    t.check("Stale application chooser cannot dispatch replacement resource");
    t.show(&two)?;
    let hash = sha_file(&two)?;
    t.menu("Copy Text…")?;
    wait("private OCR input", 12, || {
        Ok(t.records("content.jsonl")?.last().cloned())
    })?;
    let observed = t
        .records("content.jsonl")?
        .last()
        .ok_or("OCR observation")?
        .clone();
    ensure(
        observed["sha256"] == hash
            && observed["path"] != s(&two)
            && observed["path"]
                .as_str()
                .is_some_and(|v| v.contains("lumen-preview-snapshots-")),
        "OCR did not read immutable snapshot",
    )?;
    t.key(&["-k", "Escape"])?;
    t.check("Actual OCR menu supplies immutable retained bytes to fixed private helper");
    fs::write(t.f.out.join("hold-content"), "")?;
    let preserved = t.state()?["result"].clone();
    t.menu("Copy Text…")?;
    wait("held content helper", 12, || {
        Ok((t.records("content.jsonl")?.len() == 2).then_some(()))
    })?;
    let helper = t
        .records("content.jsonl")?
        .last()
        .and_then(|r| r["pid"].as_i64())
        .ok_or("helper pid")?;
    t.menu("Cancel Text Recognition")?;
    wait("canceled helper reaped", 12, || {
        Ok((t.state()?["message"] == "Content action canceled"
            && t.state()?["actionBusy"] == false)
            .then_some(()))
    })?;
    ensure(
        !PathBuf::from(format!("/proc/{helper}")).exists()
            && t.state()?["result"]["actionCapability"] == preserved["actionCapability"]
            && Path::new(preserved["resourcePath"].as_str().ok_or("resource path")?).exists(),
        "Canceled helper altered retained preview",
    )?;
    fs::remove_file(t.f.out.join("hold-content"))?;
    t.check("Actual Cancel Text Recognition reaps scoped helper while retaining visible immutable preview");
    t.menu("Lift Subject…")?;
    wait("private extraction input", 12, || {
        Ok((t.records("content.jsonl")?.len() == 3).then_some(()))
    })?;
    ensure(
        t.records("content.jsonl")?
            .last()
            .is_some_and(|r| r["sha256"] == hash),
        "Extraction read wrong bytes",
    )?;
    t.key(&["-k", "Escape"])?;
    t.check("Actual extraction menu supplies same captured immutable source");
    t.ipc("closePreview", &[])?;
    t.ipc("choose", &[&s(&one)])?;
    wait("Files selection", 12, || {
        Ok((t.state()?["current"] == s(&one)).then_some(()))
    })?;
    t.ipc("applications", &[])?;
    wait("Files captured chooser", 12, || {
        Ok(t.state()?["applications"]
            .as_object()
            .filter(|a| !a.is_empty())
            .cloned())
    })?;
    let mut pids = Vec::new();
    descendants(t.shell, &mut pids)?;
    let helper_name = PathBuf::from(
        t.f.env
            .get("LUMEN_FILES_BIN")
            .ok_or("Files helper missing")?,
    )
    .file_name()
    .ok_or("Files helper name")?
    .to_string_lossy()
    .into_owned();
    let pid = pids
        .into_iter()
        .find(|v| {
            fs::read(format!("/proc/{v}/cmdline")).is_ok_and(|b| {
                b.windows(helper_name.len())
                    .any(|w| w == helper_name.as_bytes())
            })
        })
        .ok_or("Owned Files provider PID")?;
    unsafe { libc::kill(pid as i32, libc::SIGSTOP) };
    let paused = Paused(pid as i32);
    image(&replacement, [128, 0, 128])?;
    fs::rename(&replacement, &one)?;
    t.click("files", "Private Preview Observer")?;
    drop(paused);
    wait("Files stale action refused", 12, || {
        Ok(t.state()?["fileErrors"]
            .as_array()
            .filter(|a| {
                a.iter().any(|e| {
                    e.as_str()
                        .is_some_and(|s| s.contains("The selected file changed"))
                })
            })
            .cloned())
    })?;
    ensure(
        t.records("opened.jsonl")?.len() == 2,
        "Stale Files chooser dispatched",
    )?;
    t.check("Files chooser rejects replacement after actual queued selection while owned provider paused");
    t.ipc("refresh", &[])?;
    wait("refreshed Files", 12, || {
        Ok(t.state()?["ready"].as_bool().filter(|v| *v))
    })?;
    t.ipc("choose", &[&s(&two)])?;
    t.ipc("applications", &[])?;
    wait("fresh Files chooser", 12, || {
        Ok(t.state()?["applications"]
            .as_object()
            .filter(|a| !a.is_empty())
            .cloned())
    })?;
    let chooser = t.state()?["applications"].clone();
    t.ipc("refresh", &[])?;
    wait("unchanged-source background refresh", 12, || {
        Ok(t.state()?["ready"].as_bool().filter(|v| *v))
    })?;
    ensure(
        t.state()?["applications"] == chooser,
        "Background refresh changed chooser",
    )?;
    t.click("files", "Private Preview Observer")?;
    wait("Files Open With observer", 12, || {
        Ok((t.records("opened.jsonl")?.len() == 3).then_some(()))
    })?;
    let opened = t.records("opened.jsonl")?;
    ensure(
        opened
            .last()
            .and_then(|v| v[0].as_str())
            .is_some_and(|v| v == s(&two) || v == uri(&two)),
        "Fresh Files observer target changed",
    )?;
    t.check("Fresh Files chooser dispatches exact registered private observer after stale refusal");
    ensure(t.checks.len() == 10, "Preview actions check count changed")?;
    let result = json!({"passed":true,"checks":t.checks,"launcher":s(&t.o.launcher),"sway":s(&t.o.sway),"effects":t.o.effects,"externalOpenBoundary":"Predispatch captured identity only; an external handler may later reopen its original path."});
    fs::write(
        t.f.out.join("result.json"),
        serde_json::to_vec_pretty(&result)?,
    )?;
    println!("{result}");
    Ok(())
}
fn main() {
    let result = (|| -> Result<()> {
        let name = env::args_os()
            .next()
            .and_then(|v| PathBuf::from(v).file_name().map(|s| s.to_owned()));
        if name.is_some_and(|v| v == "xdg-open" || v == "private-context-observer") {
            return observer();
        }
        let mut t = setup(Opt::parse()?)?;
        test(&mut t)
    })();
    if let Err(e) = result {
        eprintln!("Preview actions fixture: {e}");
        std::process::exit(1)
    }
}
