//! Real private Wayland clipboard consent, history, paste, and failure recovery.
use lumen_native_input_fixtures::{
    Result, Session, copy_tree, ensure, fail, number, pause, round, s, wait,
};
use serde_json::{Value, json};
use std::env;
use std::fs::{self, OpenOptions};
use std::os::unix::fs::symlink;
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::{Command, Stdio};

struct Options {
    launcher: PathBuf,
    sway: PathBuf,
    producer: PathBuf,
    observer: PathBuf,
    pointer: PathBuf,
    output: PathBuf,
    source: PathBuf,
    effects: bool,
}
impl Options {
    fn parse() -> Result<Self> {
        let mut launcher = None;
        let mut sway = None;
        let mut producer = None;
        let mut observer = None;
        let mut pointer = None;
        let mut output = None;
        let mut source = env::current_dir()?.join("res/lumen-desktop");
        let mut effects = false;
        let mut args = env::args_os().skip(1);
        while let Some(arg) = args.next() {
            match arg.to_str() {
                Some("--launcher") => {
                    launcher = Some(PathBuf::from(args.next().ok_or("Missing launcher")?))
                }
                Some("--sway") => sway = Some(PathBuf::from(args.next().ok_or("Missing Sway")?)),
                Some("--producer") => {
                    producer = Some(PathBuf::from(args.next().ok_or("Missing producer")?))
                }
                Some("--observer") => {
                    observer = Some(PathBuf::from(args.next().ok_or("Missing observer")?))
                }
                Some("--pointer-tool") => {
                    pointer = Some(PathBuf::from(args.next().ok_or("Missing pointer")?))
                }
                Some("--output") => {
                    output = Some(PathBuf::from(args.next().ok_or("Missing output")?))
                }
                Some("--source") => source = PathBuf::from(args.next().ok_or("Missing source")?),
                Some("--effects") => effects = true,
                _ => {
                    return Err(fail(format!(
                        "Unknown clipboard option: {}",
                        arg.to_string_lossy()
                    )));
                }
            }
        }
        let output = output.ok_or("--output required")?;
        Ok(Self {
            launcher: launcher.ok_or("--launcher required")?,
            sway: sway.ok_or("--sway required")?,
            producer: producer.ok_or("--producer required")?,
            observer: observer.ok_or("--observer required")?,
            pointer: pointer.ok_or("--pointer-tool required")?,
            output: if output.is_absolute() {
                output
            } else {
                env::current_dir()?.join(output)
            },
            source,
            effects,
        })
    }
}
fn which(name: &str, path: &str) -> Result<PathBuf> {
    for dir in env::split_paths(path) {
        let p = dir.join(name);
        if p.is_file() {
            return Ok(p);
        }
    }
    Err(fail(format!("Missing {name} in launcher PATH")))
}
fn proc_zombie(pid: i32) -> bool {
    let p = PathBuf::from(format!("/proc/{pid}/stat"));
    fs::read_to_string(p).map_or(true, |s| {
        s.split_once(") ")
            .is_some_and(|(_, tail)| tail.starts_with('Z'))
    })
}
fn systemctl_shim() -> Result<()> {
    let args = env::args().skip(1).collect::<Vec<_>>();
    ensure(
        args.len() == 3
            && args[0] == "--user"
            && ["start", "stop"].contains(&args[1].as_str())
            && args[2] == "lumen-clipboard-history.service",
        "Unexpected private systemctl request",
    )?;
    let out = PathBuf::from(env::var("CLIPBOARD_NATIVE_OUTPUT")?);
    let pidfile = out.join("watcher.pid");
    let failure = out.join("control-failure");
    if failure.exists() && fs::read_to_string(&failure)? == args[1] {
        return Err(fail("Injected private systemctl failure"));
    }
    if args[1] == "start" {
        if pidfile.exists() {
            let old: i32 = fs::read_to_string(&pidfile)?.trim().parse()?;
            if !proc_zombie(old) {
                return Ok(());
            }
        }
        let mut command = Command::new(env::var("LUMEN_CLIPBOARD_COMMAND")?);
        command.arg("watch");
        if out.join("empty-provider").exists() {
            command.env(
                "WAYLAND_DISPLAY",
                fs::read_to_string(out.join("empty-provider"))?,
            );
        }
        let log = OpenOptions::new()
            .append(true)
            .create(true)
            .open(out.join("watcher.log"))?;
        let child = command
            .stdin(Stdio::null())
            .stdout(Stdio::from(log.try_clone()?))
            .stderr(Stdio::from(log))
            .process_group(0)
            .spawn()?;
        fs::write(&pidfile, child.id().to_string())?;
    } else if pidfile.exists() {
        let pid: i32 = fs::read_to_string(&pidfile)?.trim().parse()?;
        unsafe {
            libc::killpg(pid, libc::SIGTERM);
        }
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        while !proc_zombie(pid) {
            ensure(
                std::time::Instant::now() < deadline,
                "Private watcher did not stop",
            )?;
            pause(20)
        }
        fs::remove_file(&pidfile)?;
    }
    Ok(())
}
struct Fixture {
    f: Session,
    opt: Options,
    qml: PathBuf,
    cli: PathBuf,
    ctl: PathBuf,
    pointer: usize,
    producer_number: usize,
}
impl Fixture {
    fn new(opt: Options) -> Result<Self> {
        let mut f = Session::new(
            &opt.output,
            &opt.launcher,
            &opt.sway,
            "lumen-clipboard-native-",
        )?;
        f.set_many(&[
            ("WLR_BACKENDS", "headless"),
            ("WLR_RENDERER", if opt.effects { "gles2" } else { "pixman" }),
            (
                "WLR_RENDERER_FORCE_SOFTWARE",
                if opt.effects { "1" } else { "0" },
            ),
            ("WLR_RENDERER_ALLOW_SOFTWARE", "1"),
            ("LIBGL_ALWAYS_SOFTWARE", "1"),
            ("WLR_LIBINPUT_NO_DEVICES", "1"),
            ("QT_QPA_PLATFORM", "wayland"),
            ("QT_QPA_PLATFORMTHEME", "generic"),
            ("QT_QUICK_BACKEND", "software"),
            ("QT_ACCESSIBILITY", "0"),
            ("QT_LINUX_ACCESSIBILITY_ALWAYS_ON", "0"),
            ("NO_AT_BRIDGE", "1"),
            ("GTK_A11Y", "none"),
            ("QT_NO_XDG_DESKTOP_PORTAL", "1"),
            ("LUMEN_FILE_SERVICE", "0"),
            ("LUMEN_LAUNCH_SCOPES", "0"),
            ("GLOBAL_MENU_ENABLED", "0"),
        ]);
        let documents = f.root.path().join("Documents");
        f.set(
            "LUMEN_SEARCH_ROOTS",
            serde_json::to_string(&vec![s(&documents)])?,
        );
        let installed = which(
            "lumen-clipboard-history",
            f.env.get("PATH").ok_or("Launcher PATH missing")?,
        )?;
        let binary = f.root.path().join("bin");
        fs::create_dir(&binary)?;
        let cli = binary.join("lumen-clipboard-history");
        symlink(installed, &cli)?;
        let ctl = binary.join("systemctl");
        symlink(env::current_exe()?, &ctl)?;
        f.set(
            "PATH",
            format!(
                "{}:{}",
                binary.display(),
                f.env.get("PATH").cloned().unwrap_or_default()
            ),
        );
        f.set("CLIPBOARD_NATIVE_OUTPUT", s(&f.out));
        f.set(
            "LUMEN_CLIPBOARD_TEST_CLOCK_OFFSET",
            s(&f.out.join("clock-offset")),
        );
        f.set("LUMEN_CLIPBOARD_RETENTION_SECONDS", "60");
        f.set("LUMEN_CLIPBOARD_COMMAND", s(&cli));
        f.set("LUMEN_CLIPBOARD_OBSERVER", s(&opt.observer));
        f.set("LUMEN_CLIPBOARD_SYSTEMCTL", s(&ctl));
        f.start_bus()?;
        let conf = f.out.join("sway.conf");
        fs::write(
            &conf,
            "output HEADLESS-1 resolution 1200x900\nxwayland disable\nseat seat0 fallback true\nfocus_follows_mouse no\ndefault_border none\nfor_window [app_id=\"org.quickshell\"] floating enable, move position 0 0\n",
        )?;
        f.launch(
            &s(&opt.sway),
            &["--unsupported-gpu", "-c", &s(&conf)],
            "sway.log",
            false,
        )?;
        f.wait_sway(1)?;
        f.launch("wtype", &["-s", "600000"], "keyboard.log", false)?;
        let pointer = f.launch(
            &s(&opt.pointer),
            &["pointer", "move", "0", "0"],
            "pointer.log",
            true,
        )?;
        let assets = f.out.join("assets");
        copy_tree(&opt.source, &assets)?;
        let palettes: Value =
            serde_json::from_slice(&fs::read(assets.join("appearance/palette.json"))?)?;
        let appearance = f.root.path().join("state/lumen-desktop/appearance.json");
        fs::create_dir_all(appearance.parent().unwrap())?;
        fs::write(
            &appearance,
            serde_json::to_vec(
                &json!({"font":palettes["font"],"palette":palettes["light"],"preferences":{"mode":"light","reduceMotion":true}}),
            )?,
        )?;
        let qml = f.out.join("shell.qml");
        fs::write(&qml, include_str!("../../clipboard.qml"))?;
        f.launch("quickshell", &["-p", &s(&qml)], "shell.log", false)?;
        let this = Self {
            f,
            opt,
            qml,
            cli,
            ctl,
            pointer,
            producer_number: 0,
        };
        wait("clipboard launcher", 12, || {
            let state = this.state()?;
            Ok((state["available"] == true && state["busy"] == false).then_some(()))
        })?;
        Ok(this)
    }
    fn ipc(&self, method: &str, args: &[&str]) -> Result<String> {
        let mut cmd = vec![
            "-p",
            self.qml.to_str().ok_or("Non UTF-8 QML")?,
            "ipc",
            "call",
            "fixture",
            method,
        ];
        cmd.extend_from_slice(args);
        self.f.call("quickshell", &cmd)
    }
    fn state(&self) -> Result<Value> {
        serde_json::from_str(&self.ipc("status", &[])?).map_err(Into::into)
    }
    fn history(&self) -> Result<Value> {
        serde_json::from_str(&self.f.call(&s(&self.cli), &["list"])?).map_err(Into::into)
    }
    fn focus(&self, title: &str) -> Result<()> {
        self.f.ipc(&format!("[title=\"^{title}$\"] focus"))
    }
    fn pointer(&mut self, text: &str) -> Result<()> {
        let pos = wait(&format!("visible {text} button"), 12, || {
            let v: Value = serde_json::from_str(&self.ipc("button", &[text])?)?;
            Ok((v["visible"] == true && v["enabled"] == true).then_some(v))
        })?;
        self.f.ipc(&format!(
            "seat seat0 cursor set {} {}",
            round(number(&pos, "x")?),
            round(number(&pos, "y")?)
        ))?;
        fs::write(
            self.f.out.join("last-button.json"),
            serde_json::to_vec(&pos)?,
        )?;
        self.f.write(self.pointer, "m 0.5 0.5\n")?;
        pause(120);
        self.f.write(self.pointer, "p\n")?;
        pause(120);
        self.f.write(self.pointer, "r\n")
    }
    fn producer(&mut self, text: &str, marker: Option<&str>, delay: u64) -> Result<PathBuf> {
        let name = format!("producer-{}.log", self.producer_number);
        self.producer_number += 1;
        let mut args = vec![text.to_owned()];
        if marker.is_some() || delay > 0 {
            args.push(marker.unwrap_or("-").to_owned());
            args.push(delay.to_string());
        }
        self.f.launch(
            &s(&self.opt.producer),
            &args.iter().map(String::as_str).collect::<Vec<_>>(),
            &name,
            false,
        )?;
        let path = self.f.out.join(name);
        wait("clipboard producer", 12, || {
            Ok((fs::read_to_string(&path)?.contains("ready")).then_some(()))
        })?;
        Ok(path)
    }
    fn refresh(&self, query: &str) -> Result<()> {
        self.ipc("query", &[query])?;
        pause(180);
        wait("clipboard refresh", 12, || {
            let v = self.state()?;
            Ok((v["busy"] == false && v["refreshPending"] == false).then_some(()))
        })
    }
    fn stop(&self) -> Result<()> {
        self.f.call(
            &s(&self.ctl),
            &["--user", "stop", "lumen-clipboard-history.service"],
        )?;
        pause(200);
        Ok(())
    }
    fn start(&self) -> Result<()> {
        self.f.call(
            &s(&self.ctl),
            &["--user", "start", "lumen-clipboard-history.service"],
        )?;
        Ok(())
    }
    fn paste(&self) -> Result<String> {
        self.f.call("wl-paste", &["--no-newline"])
    }
}
fn main() {
    let result = (|| -> Result<()> {
        if env::args_os()
            .next()
            .and_then(|v| PathBuf::from(v).file_name().map(|n| n.to_owned()))
            .is_some_and(|n| n == "systemctl")
        {
            return systemctl_shim();
        }
        run(Options::parse()?)
    })();
    if let Err(e) = result {
        eprintln!("clipboard native: {e}");
        std::process::exit(1)
    }
}
fn run(opt: Options) -> Result<()> {
    let mut fixture = Fixture::new(opt)?;
    let result = test(&mut fixture);
    let _ = fixture
        .f
        .capture(&fixture.f.out.join("last-screen.png"), &[]);
    if let Ok(v) = fixture.history() {
        let _ = fs::write(
            fixture.f.out.join("last-history.json"),
            serde_json::to_vec(&v).unwrap_or_default(),
        );
    }
    if let Ok(v) = fixture.state() {
        let _ = fs::write(
            fixture.f.out.join("last-state.json"),
            serde_json::to_vec(&v).unwrap_or_default(),
        );
    }
    if let Ok(v) = fixture.f.tree() {
        let _ = fs::write(
            fixture.f.out.join("last-tree.json"),
            serde_json::to_vec(&v).unwrap_or_default(),
        );
    }
    let _ = fixture.stop();
    result
}
fn has(c: &Fixture, text: &str) -> Result<bool> {
    Ok(c.history()?["entries"]
        .as_array()
        .is_some_and(|rows| rows.iter().any(|row| row["text"] == text)))
}
fn empty(c: &Fixture) -> Result<bool> {
    Ok(c.history()?["entries"]
        .as_array()
        .is_some_and(Vec::is_empty))
}
fn clear(c: &mut Fixture) -> Result<()> {
    c.pointer("Clear History")?;
    wait("empty history", 12, || {
        Ok((c.state()?["busy"] == false && empty(c)?).then_some(()))
    })
}
fn watcher_pid(c: &Fixture) -> Result<i32> {
    Ok(fs::read_to_string(c.f.out.join("watcher.pid"))?
        .trim()
        .parse()?)
}
fn test(c: &mut Fixture) -> Result<()> {
    ensure(
        c.state()?["enabled"] == false && empty(c)?,
        "Fresh home retained clipboard history",
    )?;
    c.producer("Synthetic before consent", None, 0)?;
    wait("ordinary clipboard pre-consent", 12, || {
        Ok((c.paste()? == "Synthetic before consent").then_some(()))
    })?;
    c.focus("Clipboard Launcher Fixture")?;
    c.ipc("open", &[])?;
    pause(150);
    c.f.call("wtype", &["-k", "Escape"])?;
    wait("consent decline", 12, || {
        Ok((c.state()?["visible"] == false).then_some(()))
    })?;
    ensure(
        c.history()?["enabled"] == false && empty(c)? && c.paste()? == "Synthetic before consent",
        "Declining consent changed ordinary clipboard",
    )?;
    c.ipc("open", &[])?;
    wait("launcher reopen", 12, || {
        Ok((c.state()?["visible"] == true).then_some(()))
    })?;
    c.focus("Clipboard Launcher Fixture")?;
    c.pointer("Applications")?;
    wait("Applications provider", 12, || {
        Ok((c.state()?["provider"] == "Applications").then_some(()))
    })?;
    c.pointer("Clipboard")?;
    wait("Clipboard provider", 12, || {
        Ok((c.state()?["provider"] == "Clipboard").then_some(()))
    })?;
    c.pointer("Enable Clipboard History")?;
    wait("history enabled", 12, || {
        let v = c.state()?;
        Ok((v["enabled"] == true && v["busy"] == false).then_some(()))
    })?;
    wait("watcher ready", 12, || {
        Ok((fs::read_to_string(c.f.out.join("watcher.log"))?.contains("watching")).then_some(()))
    })?;
    ensure(empty(c)?, "Initial pre-consent offer was imported")?;
    c.producer("Synthetic alpha", None, 0)?;
    wait("alpha history", 12, || {
        Ok(has(c, "Synthetic alpha")?.then_some(()))
    })?;
    c.producer("Synthetic beta", None, 0)?;
    wait("beta history", 12, || {
        Ok(has(c, "Synthetic beta")?.then_some(()))
    })?;
    c.refresh("")?;
    let state = c.state()?;
    let alpha = state["results"]
        .as_array()
        .and_then(|rows| rows.iter().find(|v| v["title"] == "Synthetic alpha"))
        .ok_or("Alpha launcher result missing")?;
    let alpha_id = alpha["id"].as_str().ok_or("Alpha id missing")?.to_owned();
    c.ipc("select", &[&alpha_id])?;
    c.f.call("wtype", &["-k", "Return"])?;
    wait("history selection", 12, || {
        Ok(c.state()?["message"]
            .as_str()
            .unwrap_or("")
            .contains("Paste")
            .then_some(()))
    })?;
    ensure(
        c.paste()? == "Synthetic alpha" && c.state()?["document"] == "",
        "History selection inserted into document or copied wrong text",
    )?;
    c.ipc("documentFocus", &[])?;
    pause(200);
    c.focus("Clipboard Document Observer")?;
    c.f.call("wtype", &["-M", "ctrl", "v", "-m", "ctrl"])?;
    wait("explicit document paste", 12, || {
        Ok((c.state()?["document"] == "Synthetic alpha").then_some(()))
    })?;
    c.ipc("open", &[])?;
    wait("launcher return", 12, || {
        Ok((c.state()?["visible"] == true).then_some(()))
    })?;
    c.refresh("")?;
    clear(c)?;
    c.stop()?;
    c.start()?;
    pause(500);
    c.refresh("")?;
    ensure(
        empty(c)?,
        "Clear and watcher restart replayed retired offer",
    )?;
    c.producer("Synthetic alpha", None, 0)?;
    wait("same-byte new offer", 12, || {
        Ok(has(c, "Synthetic alpha")?.then_some(()))
    })?;
    clear(c)?;
    c.stop()?;
    c.f.call("wl-copy", &["--clear"])?;
    c.start()?;
    pause(200);
    c.producer("Synthetic first after empty", None, 0)?;
    wait("first after empty", 12, || {
        Ok(has(c, "Synthetic first after empty")?.then_some(()))
    })?;
    c.ipc("capturedCopy", &[&format!("clipboard:{alpha_id}:")])?;
    pause(150);
    ensure(
        c.paste()? == "Synthetic first after empty",
        "Retired identity copied replacement row",
    )?;
    clear(c)?;
    c.producer("@html-first", None, 0)?;
    wait("plaintext preferred", 12, || {
        Ok(has(c, "Synthetic plain preferred")?.then_some(()))
    })?;
    ensure(
        !c.history()?["entries"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v["text"].as_str().unwrap_or("").contains("<b>")),
        "HTML displaced plaintext",
    )?;
    clear(c)?;
    c.producer("@html-only", None, 0)?;
    pause(250);
    ensure(empty(c)?, "Rich-only offer treated as text")?;
    for marker in ["x-kde-passwordManagerHint", "application/x-keepassxc"] {
        c.producer(&format!("Synthetic excluded {marker}"), Some(marker), 0)?;
        pause(200);
        let replacement = format!("Synthetic replacement {marker}");
        c.producer(&replacement, None, 0)?;
        wait("sensitive marker replacement", 12, || {
            Ok(has(c, &replacement)?.then_some(()))
        })?;
        c.refresh("")?;
        ensure(
            !c.history()?["entries"].as_array().unwrap().iter().any(|v| {
                v["text"]
                    .as_str()
                    .unwrap_or("")
                    .contains("Synthetic excluded")
            }),
            "Sensitive MIME offer retained",
        )?;
    }
    let pending_before_clear = c.producer("Synthetic pending before clear", None, 1000)?;
    wait("pending transfer", 12, || {
        Ok(fs::read_to_string(&pending_before_clear)?
            .contains("send text/")
            .then_some(()))
    })?;
    clear(c)?;
    pause(1100);
    ensure(empty(c)?, "In-flight pre-clear offer repopulated history")?;
    let delayed = c.producer("Synthetic delayed retired", None, 500)?;
    wait("delayed transfer", 12, || {
        Ok(fs::read_to_string(&delayed)?
            .contains("send text/")
            .then_some(()))
    })?;
    c.producer("Synthetic replacement after pending", None, 0)?;
    wait("replacement after pending", 12, || {
        Ok(has(c, "Synthetic replacement after pending")?.then_some(()))
    })?;
    pause(550);
    ensure(
        !has(c, "Synthetic delayed retired")?,
        "Retired delayed offer was retained",
    )?;
    c.producer("@oversize", None, 0)?;
    pause(250);
    ensure(
        !c.history()?["entries"].as_array().unwrap().iter().any(|v| {
            let text = v["text"].as_str().unwrap_or("");
            text.len() > 4096 || text.starts_with("XXX")
        }),
        "Oversize offer retained",
    )?;
    let slow = c.producer("Synthetic timed out", None, 3500)?;
    wait("slow transfer", 12, || {
        Ok(fs::read_to_string(&slow)?
            .contains("send text/")
            .then_some(()))
    })?;
    pause(3200);
    ensure(!has(c, "Synthetic timed out")?, "Timed-out offer retained")?;
    c.producer("Synthetic allowed after markers", None, 0)?;
    wait("allowed after markers", 12, || {
        Ok(has(c, "Synthetic allowed after markers")?.then_some(()))
    })?;
    clipboard_failure_recovery(c)?;
    let log = fs::read_to_string(c.f.out.join("shell.log"))?;
    for bad in [
        "ReferenceError:",
        "TypeError:",
        "Failed to load configuration",
    ] {
        ensure(
            !log.contains(bad),
            format!("Shell log contains {bad}: {log}"),
        )?;
    }
    let checks = [
        "New-home consent decline preserves ordinary clipboard without history",
        "Actual launcher enable starts private real Wayland watcher",
        "Selected captured history ID copies exact real clipboard without insertion",
        "Explicit native document paste inserts selected text",
        "Clear and watcher restart leave history empty",
        "Initial nil and same-byte future selections preserve event identity",
        "Retired history ID cannot copy replacement row",
        "Plaintext preferred despite HTML-first order; rich-only offers excluded",
        "Both supported sensitive MIME markers excluded from history despite replacement",
        "Pending pre-clear/replaced/oversize/timed-out transfers excluded",
        "Disable clears history and stops collection while ordinary clipboard works",
        "Clear stop failure revokes consent and reaps actual idle native observer",
        "Clear start failure leaves storage empty and ordinary clipboard intact",
        "Unsupported real Wayland provider cannot complete initial enablement",
        "Transient provider failure retains consent and explicitly reports unavailable collection",
        "Actual paused supervisor rejects queued data after simulated BOOTTIME expiry",
    ];
    fs::write(
        c.f.out.join("result.json"),
        serde_json::to_vec_pretty(&json!({"passed":true,"checks":checks}))?,
    )?;
    println!("{}", c.f.out.join("result.json").display());
    Ok(())
}
fn proc_stat(pid: i32) -> Result<String> {
    Ok(fs::read_to_string(format!("/proc/{pid}/stat"))?
        .rsplit_once(") ")
        .ok_or("Malformed proc stat")?
        .1
        .to_owned())
}
fn proc_start(pid: i32) -> Result<String> {
    Ok(proc_stat(pid)?
        .split_whitespace()
        .nth(19)
        .ok_or("Process start identity missing")?
        .to_owned())
}
fn clipboard_failure_recovery(c: &mut Fixture) -> Result<()> {
    c.pointer("Disable History")?;
    wait("disabled history", 12, || {
        let v = c.state()?;
        Ok((v["busy"] == false && v["enabled"] == false).then_some(()))
    })?;
    ensure(empty(c)?, "Disable retained history")?;
    c.producer("Synthetic after disable", None, 0)?;
    pause(300);
    ensure(
        empty(c)? && c.paste()? == "Synthetic after disable",
        "Disable interrupted ordinary clipboard",
    )?;
    c.pointer("Enable Clipboard History")?;
    wait("reenabled history", 12, || {
        let v = c.state()?;
        Ok((v["enabled"] == true && v["busy"] == false).then_some(()))
    })?;
    pause(150);
    c.producer("Synthetic before stop failure", None, 0)?;
    wait("pre-stop-failure offer", 12, || {
        Ok(has(c, "Synthetic before stop failure")?.then_some(()))
    })?;
    c.refresh("")?;
    let watcher = watcher_pid(c)?;
    let children = fs::read_to_string(format!("/proc/{watcher}/task/{watcher}/children"))?;
    let children = children.split_whitespace().collect::<Vec<_>>();
    ensure(
        children.len() == 1,
        format!("Watcher observer children: {children:?}"),
    )?;
    let observer_pid: i32 = children[0].parse()?;
    ensure(
        fs::canonicalize(format!("/proc/{observer_pid}/exe"))?
            == fs::canonicalize(&c.opt.observer)?,
        "Watcher child is not native observer",
    )?;
    let failure = c.f.out.join("control-failure");
    fs::write(&failure, "stop")?;
    clear(c)?;
    wait("observer reaped after stop failure", 12, || {
        Ok((!PathBuf::from(format!("/proc/{observer_pid}")).exists()).then_some(()))
    })?;
    ensure(
        c.state()?["enabled"] == false
            && empty(c)?
            && c.paste()? == "Synthetic before stop failure",
        "Stop failure did not revoke consent or preserve ordinary clipboard",
    )?;
    fs::remove_file(&failure)?;
    c.pointer("Enable Clipboard History")?;
    wait("reenabled after stop failure", 12, || {
        let v = c.state()?;
        Ok((v["enabled"] == true && v["busy"] == false).then_some(()))
    })?;
    pause(150);
    c.producer("Synthetic before start failure", None, 0)?;
    wait("pre-start-failure offer", 12, || {
        Ok(has(c, "Synthetic before start failure")?.then_some(()))
    })?;
    c.refresh("")?;
    fs::write(&failure, "start")?;
    clear(c)?;
    ensure(
        c.state()?["enabled"] == false
            && empty(c)?
            && c.paste()? == "Synthetic before start failure",
        "Start failure left history or changed clipboard",
    )?;
    fs::remove_file(&failure)?;
    let empty_provider = c
        .opt
        .producer
        .parent()
        .ok_or("Producer has no parent")?
        .join("lumen-test-clipboard-empty-provider");
    c.f.launch(&s(&empty_provider), &[], "empty-provider.log", false)?;
    let empty_log = c.f.out.join("empty-provider.log");
    let unavailable = wait("empty provider display", 12, || {
        Ok(fs::read_to_string(&empty_log)
            .ok()
            .filter(|s| !s.trim().is_empty()))
    })?;
    let marker = c.f.out.join("empty-provider");
    fs::write(&marker, unavailable.trim())?;
    c.pointer("Enable Clipboard History")?;
    wait("unsupported provider rejected", 12, || {
        let v = c.state()?;
        Ok((v["busy"] == false
            && v["enabled"] == false
            && v["error"].as_str().is_some_and(|s| !s.is_empty()))
        .then_some(()))
    })?;
    ensure(
        c.history()?["enabled"] == false && c.history()?["collecting"] == false,
        "Unsupported provider claimed collection",
    )?;
    fs::remove_file(&marker)?;
    c.pointer("Enable Clipboard History")?;
    wait("supported provider enabled", 12, || {
        let v = c.state()?;
        Ok((v["busy"] == false && v["enabled"] == true && v["collecting"] == true).then_some(()))
    })?;
    c.stop()?;
    fs::write(&marker, unavailable.trim())?;
    c.start()?;
    pause(200);
    c.refresh("")?;
    let state = c.state()?;
    ensure(
        state["enabled"] == true
            && state["collecting"] == false
            && state["error"]
                .as_str()
                .unwrap_or("")
                .to_lowercase()
                .contains("unavailable")
            && c.history()?["enabled"] == true
            && c.history()?["collecting"] == false,
        "Transient provider failure discarded consent or claimed collection",
    )?;
    fs::remove_file(&marker)?;
    c.stop()?;
    c.start()?;
    wait("watcher collecting after transient failure", 12, || {
        Ok((c.history()?["collecting"] == true).then_some(()))
    })?;
    let watcher = watcher_pid(c)?;
    let identity = proc_start(watcher)?;
    let old_log = fs::read_to_string(c.f.out.join("watcher.log"))?.len();
    unsafe {
        libc::kill(watcher, libc::SIGSTOP);
    }
    let paused = (|| -> Result<()> {
        let queued = c.producer("Synthetic queued past retention", None, 0)?;
        wait("queued transfer", 12, || {
            Ok(fs::read_to_string(&queued)?
                .contains("send text/")
                .then_some(()))
        })?;
        pause(100);
        fs::write(c.f.out.join("clock-offset"), "61")?;
        Ok(())
    })();
    if PathBuf::from(format!("/proc/{watcher}/stat")).exists() {
        ensure(
            proc_start(watcher)? == identity,
            "Watcher identity changed while paused",
        )?;
        unsafe {
            libc::kill(watcher, libc::SIGCONT);
        }
    }
    paused?;
    wait("expired queue rejection", 12, || {
        let log = fs::read_to_string(c.f.out.join("watcher.log"))?;
        Ok(log
            .get(old_log..)
            .unwrap_or("")
            .lines()
            .filter(|line| line.starts_with('{'))
            .filter_map(|line| serde_json::from_str::<Value>(line).ok())
            .any(|v| v["recorded"] == false)
            .then_some(()))
    })?;
    ensure(empty(c)?, "Paused supervisor refreshed expired queued data")?;
    c.pointer("Disable History")?;
    wait("final disable", 12, || {
        let v = c.state()?;
        Ok((v["busy"] == false && v["enabled"] == false).then_some(()))
    })?;
    let folder = c.f.root.path().join("state/lumen-desktop/clipboard");
    ensure(
        fs::read_dir(&folder)?.next().is_none(),
        "Clipboard data persisted after revocation",
    )?;
    Ok(())
}
