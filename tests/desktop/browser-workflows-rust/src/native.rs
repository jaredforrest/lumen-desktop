//! Actual retained-browser workflow inside the private application boundary.
use crate::accessibility_dbus::{self, AccessibilityHost, BusProvider};
use crate::boundary::{Options, Workflow};
use crate::capabilities::Inventory;
use crate::keyboard::PhysicalKeyboard;
use crate::paint::{Image, match_paint};
use crate::portal::{self, PortalHost};
use crate::protocol::PipeTransport;
use crate::server::FixtureServer;
use crate::{Result, fail};
use base64::Engine;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::VecDeque;
use std::collections::{BTreeMap, HashSet};
use std::env;
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Write};
use std::os::fd::AsRawFd;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

fn ensure(value: bool, message: impl Into<String>) -> Result<()> {
    if value { Ok(()) } else { Err(fail(message)) }
}
fn save(path: &Path, value: &Value) -> Result<()> {
    fs::write(path, serde_json::to_vec_pretty(value)?)?;
    Ok(())
}
fn text(path: &Path) -> Result<String> {
    Ok(fs::read_to_string(path)?)
}
fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn as_f64(value: &Value, key: &str) -> Result<f64> {
    value[key]
        .as_f64()
        .ok_or_else(|| fail(format!("missing {key}")))
}
fn as_i64(value: &Value, key: &str) -> Result<i64> {
    value[key]
        .as_i64()
        .ok_or_else(|| fail(format!("missing {key}")))
}
fn wait<T>(
    label: &str,
    timeout: Duration,
    mut probe: impl FnMut() -> Result<Option<T>>,
) -> Result<T> {
    let end = Instant::now() + timeout;
    loop {
        if let Ok(Some(value)) = probe() {
            return Ok(value);
        }
        if Instant::now() >= end {
            return Err(fail(format!("Browser acceptance deadline: {label}")));
        }
        thread::sleep(Duration::from_millis(50));
    }
}
struct Managed(Child);
impl Managed {
    fn stop(&mut self) {
        if self.0.try_wait().ok().flatten().is_none() {
            unsafe {
                libc::killpg(self.0.id() as i32, libc::SIGTERM);
            }
        }
        let end = Instant::now() + Duration::from_secs(5);
        while self.0.try_wait().ok().flatten().is_none() {
            if Instant::now() >= end {
                unsafe {
                    libc::killpg(self.0.id() as i32, libc::SIGKILL);
                }
                break;
            }
            thread::sleep(Duration::from_millis(20));
        }
        let _ = self.0.wait();
    }
}
pub struct Native {
    pub opt: Options,
    pub env: BTreeMap<String, String>,
    pub out: PathBuf,
    pub runtime: PathBuf,
    processes: Vec<Managed>,
    pub server: Option<FixtureServer>,
    pub browser: Option<PipeTransport>,
    pub keyboard: Option<PhysicalKeyboard>,
    pointer: Option<Managed>,
    pub a11y: Option<BusProvider>,
    pub profile: PathBuf,
    pub downloads: PathBuf,
    pub version: Value,
    pub inventory: Inventory,
    pub target: Value,
    pub session: String,
    pub main: Value,
    pub checks: Vec<String>,
    pub command: Vec<String>,
    chooser_mapped: BTreeMap<i64, Instant>,
    portal_facts: Option<Value>,
    keyboard_route_active: bool,
}
impl Drop for Native {
    fn drop(&mut self) {
        if let Some(keyboard) = self.keyboard.as_mut() {
            let _ = keyboard.close();
        }
        if let Some(browser) = self.browser.take() {
            let _ = browser.close();
        }
        if let Some(pointer) = self.pointer.as_mut() {
            pointer.stop();
        }
        for process in self.processes.iter_mut().rev() {
            process.stop();
        }
    }
}
impl Native {
    fn new(opt: Options) -> Result<Self> {
        let out = opt.output.clone();
        let runtime = PathBuf::from(env::var("XDG_RUNTIME_DIR")?);
        let env = env::vars().collect();
        Ok(Self {
            opt,
            env,
            out,
            runtime,
            processes: Vec::new(),
            server: None,
            browser: None,
            keyboard: None,
            pointer: None,
            a11y: None,
            profile: PathBuf::new(),
            downloads: PathBuf::new(),
            version: Value::Null,
            inventory: Inventory::new(Value::Null)?,
            target: Value::Null,
            session: String::new(),
            main: Value::Null,
            checks: Vec::new(),
            command: Vec::new(),
            chooser_mapped: BTreeMap::new(),
            portal_facts: None,
            keyboard_route_active: false,
        })
    }
    fn command(&self, program: impl AsRef<std::ffi::OsStr>) -> Command {
        let mut command = Command::new(program);
        command.env_clear().envs(&self.env);
        command
    }
    fn launch(&mut self, command: &[String], log: &str) -> Result<u32> {
        let file = File::create(self.out.join(log))?;
        let mut proc = self.command(&command[0]);
        proc.args(&command[1..])
            .stdout(Stdio::from(file.try_clone()?))
            .stderr(Stdio::from(file));
        unsafe {
            proc.pre_exec(|| {
                libc::setsid();
                Ok(())
            });
        }
        let child = proc.spawn()?;
        let pid = child.id();
        self.processes.push(Managed(child));
        Ok(pid)
    }
    fn call(&self, command: &[String]) -> Result<String> {
        let output = self.command(&command[0]).args(&command[1..]).output()?;
        ensure(
            output.status.success(),
            format!(
                "{}: {}",
                command[0],
                String::from_utf8_lossy(&output.stderr)
            ),
        )?;
        Ok(String::from_utf8(output.stdout)?.trim().into())
    }
    fn swaymsg(&self, args: &[&str]) -> Result<Value> {
        let bin = self.opt.sway.parent().ok_or("sway parent")?.join("swaymsg");
        let command = std::iter::once(bin.display().to_string())
            .chain(args.iter().map(|v| (*v).to_string()))
            .collect::<Vec<_>>();
        Ok(serde_json::from_str(&self.call(&command)?)?)
    }
    fn capture(&self, name: &str) -> Result<()> {
        self.call(&["grim".into(), self.out.join(name).display().to_string()])?;
        Ok(())
    }
    fn cdp(&mut self, method: &str, params: Value, session: Option<&str>) -> Result<Value> {
        self.browser.as_mut().ok_or("browser")?.request(
            method,
            params,
            session,
            Duration::from_secs(5),
        )
    }
    fn evaluate_session(&mut self, expression: &str, session: &str) -> Result<Value> {
        let value = self.cdp(
            "Runtime.evaluate",
            json!({"expression":expression,"returnByValue":true}),
            Some(session),
        )?;
        Ok(value["result"].get("value").cloned().unwrap_or(Value::Null))
    }
    fn evaluate(&mut self, expression: &str) -> Result<Value> {
        let session = self.session.clone();
        self.evaluate_session(expression, &session)
    }
    fn targets(&mut self) -> Result<Vec<Value>> {
        Ok(
            self.cdp("Target.getTargets", json!({}), None)?["targetInfos"]
                .as_array()
                .ok_or("target list")?
                .clone(),
        )
    }
    fn tree(&self) -> Result<Value> {
        self.swaymsg(&["-r", "-t", "get_tree"])
    }
    fn nodes(node: &Value, result: &mut Vec<Value>) {
        result.push(node.clone());
        for key in ["nodes", "floating_nodes"] {
            if let Some(children) = node[key].as_array() {
                for child in children {
                    Self::nodes(child, result);
                }
            }
        }
    }
    fn windows(&self) -> Result<Vec<Value>> {
        let mut nodes = Vec::new();
        Self::nodes(&self.tree()?, &mut nodes);
        Ok(nodes
            .into_iter()
            .filter(|node| {
                !node["pid"].is_null() && (!node["app_id"].is_null() || !node["window"].is_null())
            })
            .collect())
    }
    fn current_main(&self) -> Result<Value> {
        self.windows()?
            .into_iter()
            .find(|node| node["id"] == self.main["id"] && node["pid"] == self.main["pid"])
            .ok_or_else(|| fail("Main browser window vanished"))
    }
    fn focus_main(&self) -> Result<()> {
        let id = as_i64(&self.main, "id")?;
        let reply = self.swaymsg(&["-r", &format!("[con_id={id}] focus")])?;
        ensure(
            reply
                .as_array()
                .is_some_and(|rows| rows.iter().all(|row| row["success"] == true)),
            "Main focus failed",
        )
    }
    fn key(&mut self, values: &[&str]) -> Result<()> {
        if self.keyboard_route_active {
            writeln!(
                fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(self.out.join("keyboard-save-input.jsonl"))?,
                "{}",
                json!({"keys":values})
            )?;
        }
        self.keyboard.as_mut().ok_or("keyboard")?.key(values)
    }
    fn navigate(&mut self, url: &str) -> Result<()> {
        if self.keyboard_route_active {
            ensure(
                self.current_main()?["focused"] == true,
                "Keyboard-only navigation lost captured main window focus",
            )?;
        } else {
            self.focus_main()?;
        }
        self.key(&["-M", "ctrl", "-k", "l", "-m", "ctrl"])?;
        self.key(&[url])?;
        self.key(&["-k", "Return"])
    }
    fn pointer_click(&mut self, x: f64, y: f64) -> Result<()> {
        ensure(
            !self.keyboard_route_active,
            "Pointer action forbidden during keyboard-only save route",
        )?;
        let reply = self.swaymsg(&[
            "seat",
            "seat0",
            "cursor",
            "set",
            &(x.round() as i64).to_string(),
            &(y.round() as i64).to_string(),
        ])?;
        ensure(
            reply
                .as_array()
                .is_some_and(|rows| rows.iter().all(|row| row["success"] == true)),
            "Pointer positioning failed",
        )?;
        for operation in ["m 0.5 0.5", "p", "r"] {
            writeln!(
                self.pointer
                    .as_mut()
                    .ok_or("pointer")?
                    .0
                    .stdin
                    .as_mut()
                    .ok_or("pointer stdin")?,
                "{operation}"
            )?;
            self.pointer
                .as_mut()
                .unwrap()
                .0
                .stdin
                .as_mut()
                .unwrap()
                .flush()?;
            thread::sleep(Duration::from_millis(120));
        }
        Ok(())
    }
    fn click_expression(
        &mut self,
        expression: &str,
        session: Option<&str>,
        window: Option<&Value>,
    ) -> Result<()> {
        let script = format!(
            "(()=>{{const element={expression};if(!element)return null;const r=element.getBoundingClientRect();return {{x:r.x+r.width/2,y:r.y+r.height/2,width:r.width,height:r.height,innerWidth,innerHeight}};}})()"
        );
        let geometry = if let Some(session) = session {
            self.evaluate_session(&script, session)?
        } else {
            self.evaluate(&script)?
        };
        ensure(
            as_f64(&geometry, "width")? > 0.0 && as_f64(&geometry, "height")? > 0.0,
            "Missing clickable element",
        )?;
        let native = if let Some(window) = window {
            self.windows()?
                .into_iter()
                .find(|node| node["id"] == window["id"] && node["pid"] == window["pid"])
                .ok_or("secondary window")?
        } else {
            self.current_main()?
        };
        let client = &native["window_rect"];
        let rect = &native["rect"];
        let scale = as_f64(client, "width")? / as_f64(&geometry, "innerWidth")?;
        let x = as_f64(rect, "x")? + as_f64(client, "x")? + as_f64(&geometry, "x")? * scale;
        let y = as_f64(rect, "y")? + as_f64(client, "y")? + as_f64(client, "height")?
            - as_f64(&geometry, "innerHeight")? * scale
            + as_f64(&geometry, "y")? * scale;
        self.pointer_click(x, y)
    }
    fn click_page(&mut self, selector: &str) -> Result<()> {
        self.click_expression(
            &format!(
                "document.querySelector({})",
                serde_json::to_string(selector)?
            ),
            None,
            None,
        )
    }
    fn wait_eval(&mut self, expression: &str, expected: Value, label: &str) -> Result<()> {
        wait(label, Duration::from_secs(15), || {
            Ok((self.evaluate(expression)? == expected).then_some(()))
        })
    }
    fn wait_window(&self, predicate: impl Fn(&Value) -> bool, label: &str) -> Result<Value> {
        wait(label, Duration::from_secs(15), || {
            Ok(self.windows()?.into_iter().find(|node| predicate(node)))
        })
    }
    fn capture_presented(
        &mut self,
        expression: &str,
        name: &str,
        session: Option<&str>,
        window: Option<&Value>,
    ) -> Result<bool> {
        let script = format!(
            "(()=>{{const element={expression};if(!element)return null;let r=element.getBoundingClientRect();if(!element.childElementCount){{const range=document.createRange();range.selectNodeContents(element);r=range.getBoundingClientRect();}}return {{x:r.x,y:r.y,width:r.width,height:r.height,text:element.innerText,innerWidth,innerHeight,dpr:devicePixelRatio}};}})()"
        );
        let active = session.unwrap_or(&self.session).to_string();
        let info = self.evaluate_session(&script, &active)?;
        if info.is_null() || as_f64(&info, "width")? <= 0.0 || as_f64(&info, "height")? <= 0.0 {
            return Ok(false);
        }
        let native = if let Some(window) = window {
            self.windows()?
                .into_iter()
                .find(|node| node["id"] == window["id"] && node["pid"] == window["pid"])
                .ok_or("captured window")?
        } else {
            self.current_main()?
        };
        let client = &native["window_rect"];
        let scale = as_f64(client, "width")? / as_f64(&info, "innerWidth")?;
        let stem = name.strip_suffix(".png").unwrap_or(name);
        if (scale - as_f64(&info, "dpr")?).abs() >= 0.000001 {
            save(
                &self.out.join(format!("{stem}-paint.json")),
                &json!({"painted":false,"reason":"Native configure and application viewport have not committed together","content":info,"native":native}),
            )?;
            return Ok(false);
        }
        let screenshot = self.cdp(
            "Page.captureScreenshot",
            json!({"format":"png","fromSurface":true,"captureBeyondViewport":false}),
            Some(&active),
        )?;
        let encoded = screenshot["data"].as_str().ok_or("screenshot data")?;
        let bytes = base64::engine::general_purpose::STANDARD.decode(encoded)?;
        let viewport = Image::from_png(&bytes)?;
        if viewport.width != (as_f64(&info, "innerWidth")? * scale).round() as usize
            || viewport.height != (as_f64(&info, "innerHeight")? * scale).round() as usize
        {
            return Ok(false);
        }
        let left = (as_f64(&info, "x")? * scale).round() as usize;
        let top = (as_f64(&info, "y")? * scale).round() as usize;
        let width = (as_f64(&info, "width")? * scale).round() as usize;
        let height = (as_f64(&info, "height")? * scale).round() as usize;
        let reference = viewport.crop(left, top, width, height)?;
        reference.save_png(&self.out.join(format!("{stem}-reference.png")))?;
        self.capture(name)?;
        let screen = Image::load_png(&self.out.join(name))?;
        let rect = &native["rect"];
        let origin = (
            as_f64(rect, "x")? + as_f64(client, "x")? + as_f64(&info, "x")? * scale,
            as_f64(rect, "y")? + as_f64(client, "y")? + as_f64(client, "height")?
                - as_f64(&info, "innerHeight")? * scale
                + as_f64(&info, "y")? * scale,
        );
        let result = match_paint(&reference, &screen, origin);
        save(
            &self.out.join(format!("{stem}-paint.json")),
            &json!({"result":result,"content":info,"native":native}),
        )?;
        Ok(result["painted"] == true)
    }
    fn wait_capture(
        &mut self,
        expression: &str,
        name: &str,
        session: Option<&str>,
        window: Option<&Value>,
    ) -> Result<()> {
        wait(
            &format!("actual presented application content: {name}"),
            Duration::from_secs(15),
            || {
                Ok(self
                    .capture_presented(expression, name, session, window)?
                    .then_some(()))
            },
        )
    }
    fn start_bus(&mut self) -> Result<()> {
        self.env.insert(
            "DBUS_SYSTEM_BUS_ADDRESS".into(),
            format!("unix:path={}/no-system-bus", self.runtime.display()),
        );
        let config = self.out.join("bus.conf");
        fs::write(
            &config,
            format!(
                "<busconfig><type>session</type><listen>unix:tmpdir={}</listen><policy context=\"default\"><allow own=\"*\"/><allow send_destination=\"*\"/><allow receive_sender=\"*\"/></policy></busconfig>",
                self.runtime.display()
            ),
        )?;
        let log = File::create(self.out.join("bus.log"))?;
        let mut process = self.command("dbus-daemon");
        process
            .args([
                "--nofork",
                "--print-address=1",
                &format!("--config-file={}", config.display()),
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::from(log));
        unsafe {
            process.pre_exec(|| {
                libc::setsid();
                Ok(())
            });
        }
        let mut child = process.spawn()?;
        let stdout = child.stdout.take().ok_or("bus stdout")?;
        let mut poll = libc::pollfd {
            fd: stdout.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        };
        ensure(
            unsafe { libc::poll(&mut poll, 1, 5000) } > 0,
            "Private bus failed to start",
        )?;
        let mut reader = BufReader::new(stdout);
        let mut address = String::new();
        reader.read_line(&mut address)?;
        ensure(
            address.starts_with("unix:"),
            "Private bus did not publish an address",
        )?;
        self.env
            .insert("DBUS_SESSION_BUS_ADDRESS".into(), address.trim().into());
        self.processes.push(Managed(child));
        Ok(())
    }
    fn start_sway(&mut self) -> Result<()> {
        let config = self.runtime.join("sway.conf");
        fs::write(
            &config,
            "include /source/res/lumen-desktop/sway.conf\noutput HEADLESS-1 resolution 1400x1000\nxwayland disable\nseat seat0 fallback true\nfocus_follows_mouse no\n",
        )?;
        self.env.insert("WLR_BACKENDS".into(), "headless".into());
        self.env.insert(
            "WLR_RENDERER".into(),
            if self.opt.effects { "gles2" } else { "pixman" }.into(),
        );
        self.env.insert(
            "WLR_RENDERER_FORCE_SOFTWARE".into(),
            if self.opt.effects { "1" } else { "0" }.into(),
        );
        for (key, value) in [
            ("WLR_RENDERER_ALLOW_SOFTWARE", "1"),
            ("LIBGL_ALWAYS_SOFTWARE", "1"),
            ("WLR_LIBINPUT_NO_DEVICES", "1"),
            ("QT_QPA_PLATFORM", "wayland"),
            ("QT_QPA_PLATFORMTHEME", "generic"),
            ("LANG", "en_GB.UTF-8"),
            ("GSETTINGS_BACKEND", "memory"),
        ] {
            self.env.insert(key.into(), value.into());
        }
        for key in ["DISPLAY", "WAYLAND_DISPLAY", "SWAYSOCK"] {
            self.env.remove(key);
        }
        self.launch(
            &[
                self.opt.sway.display().to_string(),
                "--unsupported-gpu".into(),
                "-d".into(),
                "-c".into(),
                config.display().to_string(),
            ],
            "sway.log",
        )?;
        let runtime = self.runtime.clone();
        let socket = wait("compositor IPC", Duration::from_secs(15), || {
            Ok(fs::read_dir(&runtime)?
                .filter_map(|entry| entry.ok())
                .map(|entry| entry.path())
                .find(|path| {
                    path.file_name().is_some_and(|name| {
                        name.to_string_lossy().starts_with("sway-ipc")
                            && name.to_string_lossy().ends_with(".sock")
                    })
                }))
        })?;
        let wayland = wait("private display", Duration::from_secs(15), || {
            Ok(fs::read_dir(&runtime)?
                .filter_map(|entry| entry.ok())
                .map(|entry| entry.file_name().to_string_lossy().to_string())
                .find(|name| name.starts_with("wayland-") && !name.ends_with(".lock")))
        })?;
        self.env
            .insert("SWAYSOCK".into(), socket.display().to_string());
        self.env.insert("WAYLAND_DISPLAY".into(), wayland);
        Ok(())
    }
    fn start_browser(&mut self) -> Result<()> {
        let home = PathBuf::from(self.env.get("HOME").ok_or("HOME")?);
        self.profile = home.join("browser-profile");
        self.downloads = home.join("Downloads");
        fs::create_dir(&self.downloads)?;
        fs::write(
            Path::new(self.env.get("XDG_CONFIG_HOME").ok_or("XDG_CONFIG_HOME")?)
                .join("user-dirs.dirs"),
            format!("XDG_DOWNLOAD_DIR=\"{}\"\n", self.downloads.display()),
        )?;
        self.server = Some(FixtureServer::start()?);
        if let Some(manifest) = self.opt.portal_tools.clone() {
            let mut provider_env = self.env.clone();
            let desktop = provider_env.get("XDG_CURRENT_DESKTOP").cloned();
            let mut facts =
                crate::presentation::start_preserving_client(&mut provider_env, |environment| {
                    portal::start(environment, &manifest, self)
                })?;
            facts["clientDesktop"] = json!(desktop);
            self.env = provider_env;
            self.verify_portal(&mut facts, &manifest)?;
            save(&self.out.join("portal-providers.json"), &facts)?;
            self.portal_facts = Some(facts);
        }
        let origin = self.server.as_ref().unwrap().origin().to_string();
        let invoke = self.call(&[
            "lumen-browser-invocation".into(),
            self.opt.browser.display().to_string(),
            self.profile.display().to_string(),
            format!("{origin}/page"),
            "normal".into(),
            if matches!(self.opt.workflow, Workflow::Permissions | Workflow::Library) {
                "true"
            } else {
                "false"
            }
            .into(),
        ])?;
        self.command = invoke
            .split('\0')
            .filter(|v| !v.is_empty())
            .map(str::to_string)
            .collect();
        ensure(!self.command.is_empty(), "Browser invocation empty")?;
        let mut browser_env = self.env.clone();
        if self.opt.trace_wayland {
            browser_env.insert("WAYLAND_DEBUG".into(), "client".into());
        }
        self.browser = Some(PipeTransport::start(
            &self.command,
            &browser_env,
            &self.out.join("browser.log"),
        )?);
        self.version = self.browser.as_mut().unwrap().request(
            "Browser.getVersion",
            json!({}),
            None,
            Duration::from_secs(10),
        )?;
        self.inventory = Inventory::new(self.version.clone())?;
        let url = format!("{origin}/page");
        self.target = wait("synthetic page target", Duration::from_secs(15), || {
            Ok(self
                .targets()?
                .into_iter()
                .find(|target| target["url"] == url))
        })?;
        self.session = self.cdp(
            "Target.attachToTarget",
            json!({"targetId":self.target["targetId"],"flatten":true}),
            None,
        )?["sessionId"]
            .as_str()
            .ok_or("session id")?
            .into();
        self.wait_eval(
            "document.title",
            json!("Fixture browser workflow"),
            "painted document title",
        )?;
        self.main = self.wait_window(
            |node| {
                node["name"]
                    .as_str()
                    .is_some_and(|name| name.contains("Fixture browser workflow"))
            },
            "actual browser native window",
        )?;
        save(&self.out.join("main-tree.json"), &self.tree()?)?;
        ensure(
            self.main["lumen_relationships"]["parent"].is_null(),
            "Main browser parent wrong",
        )?;
        ensure(
            self.main["border"] == "normal"
                && self.main["deco_rect"]["height"].as_i64().unwrap_or(0) > 0,
            "Main browser frame wrong",
        )?;
        let preferences: Value =
            serde_json::from_str(&text(&self.profile.join("Default/Preferences"))?)?;
        ensure(
            preferences["browser"]["custom_chrome_frame"] == false,
            "Production native frame preference missing",
        )?;
        self.checks.push("Actual pinned retained browser opened private synthetic page with production native-frame preference".into());
        self.keyboard = Some(PhysicalKeyboard::start(
            &self.opt.keyboard_tool,
            &self.env,
            &self.out.join("keyboard.log"),
        )?);
        let mut pointer = self.command(&self.opt.pointer_tool);
        pointer
            .args(["pointer", "move", "0", "0"])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        unsafe {
            pointer.pre_exec(|| {
                libc::setsid();
                Ok(())
            });
        }
        self.pointer = Some(Managed(pointer.spawn()?));
        thread::sleep(Duration::from_millis(200));
        Ok(())
    }
    fn verify_portal(&mut self, facts: &mut Value, manifest: &Path) -> Result<()> {
        let address = self
            .env
            .get("DBUS_SESSION_BUS_ADDRESS")
            .ok_or("portal bus")?
            .clone();
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        let builder = zbus::connection::Builder::address(address.as_str())?;
        let connection = runtime.block_on(async {
            tokio::time::timeout(Duration::from_millis(500), builder.build()).await
        })??;
        let mut owners = serde_json::Map::new();
        for name in [
            "org.freedesktop.portal.Desktop",
            "org.freedesktop.impl.portal.desktop.gtk",
        ] {
            let owner: String = runtime.block_on(async {
                let proxy = zbus::Proxy::new(
                    &connection,
                    "org.freedesktop.DBus",
                    "/org/freedesktop/DBus",
                    "org.freedesktop.DBus",
                )
                .await?;
                proxy.call("GetNameOwner", &(name,)).await
            })?;
            let pid: u32 = runtime.block_on(async {
                let proxy = zbus::Proxy::new(
                    &connection,
                    "org.freedesktop.DBus",
                    "/org/freedesktop/DBus",
                    "org.freedesktop.DBus",
                )
                .await?;
                proxy
                    .call("GetConnectionUnixProcessID", &(owner.as_str(),))
                    .await
            })?;
            owners.insert(name.into(), json!({"owner":owner,"pid":pid}));
            let maps = fs::read(format!("/proc/{pid}/maps"))?;
            ensure(
                maps.len() < 4 * 1024 * 1024,
                "Portal provider maps exceed evidence budget",
            )?;
            fs::write(self.out.join(format!("portal-{pid}-maps.txt")), maps)?;
        }
        ensure(
            owners["org.freedesktop.impl.portal.desktop.gtk"]["pid"] == facts["backendPid"],
            "Portal backend owner changed",
        )?;
        let property: zbus::zvariant::OwnedValue = runtime.block_on(async {
            let proxy = zbus::Proxy::new(
                &connection,
                "org.freedesktop.portal.Desktop",
                "/org/freedesktop/portal/desktop",
                "org.freedesktop.DBus.Properties",
            )
            .await?;
            proxy
                .call("Get", &("org.freedesktop.portal.FileChooser", "version"))
                .await
        })?;
        let version = u32::try_from(property)?;
        ensure(version >= 3, "Portal FileChooser interface too old")?;
        facts["owners"] = json!(owners);
        facts["fileChooserVersion"] = json!(version);
        let tools: Value = serde_json::from_slice(&fs::read(manifest)?)?;
        let monitor = Path::new(tools["bus"].as_str().ok_or("portal bus tool")?)
            .parent()
            .ok_or("portal bus directory")?
            .join("dbus-monitor");
        ensure(
            monitor.is_file() && monitor.starts_with("/nix/store"),
            "Immutable portal monitor missing",
        )?;
        self.launch(
            &[
                monitor.display().to_string(),
                "--address".into(),
                address,
                "type='method_call',interface='org.freedesktop.portal.FileChooser'".into(),
            ],
            "portal-requests.log",
        )?;
        Ok(())
    }
    fn setup_environment(&mut self) -> Result<()> {
        let fontconfig = self.runtime.join("fonts.conf");
        fs::write(
            &fontconfig,
            format!(
                "<?xml version=\"1.0\"?><!DOCTYPE fontconfig SYSTEM \"urn:fontconfig:fonts.dtd\"><fontconfig><dir>{}</dir><cachedir>{}/fontconfig</cachedir></fontconfig>",
                self.opt.font_directory.display(),
                self.env.get("XDG_CACHE_HOME").ok_or("cache home")?
            ),
        )?;
        self.env
            .insert("FONTCONFIG_FILE".into(), fontconfig.display().to_string());
        let egl = self
            .opt
            .software_driver
            .join("share/glvnd/egl_vendor.d/50_mesa.json");
        ensure(egl.is_file(), "Software EGL vendor missing")?;
        for (key, value) in [
            ("__EGL_VENDOR_LIBRARY_FILENAMES", egl.display().to_string()),
            (
                "LIBGL_DRIVERS_PATH",
                self.opt
                    .software_driver
                    .join("lib/dri")
                    .display()
                    .to_string(),
            ),
            ("MESA_LOADER_DRIVER_OVERRIDE", "llvmpipe".into()),
        ] {
            self.env.insert(key.into(), value);
        }
        Ok(())
    }
    fn start(&mut self) -> Result<()> {
        self.start_bus()?;
        self.setup_environment()?;
        if matches!(
            self.opt.workflow,
            Workflow::Permissions | Workflow::Library | Workflow::KeyboardSave
        ) {
            let prefix = self.opt.at_spi_prefix.clone();
            let mut updated = self.env.clone();
            self.a11y = Some(accessibility_dbus::start(&prefix, &mut updated, self)?);
            self.env = updated;
        }
        self.start_sway()?;
        self.start_browser()
    }
    fn common(&mut self) -> Result<()> {
        let outputs = self.swaymsg(&["-r", "-t", "get_outputs"])?;
        ensure(
            outputs
                .as_array()
                .is_some_and(|rows| rows.len() == 1 && rows[0]["scale"].as_f64() == Some(1.0)),
            "Unexpected compositor output",
        )?;
        save(&self.out.join("outputs.json"), &outputs)?;
        self.wait_capture("document.querySelector('#sample')", "main.png", None, None)?;
        self.navigate("brave://sandbox")?;
        let sandbox = wait(
            "complete actual browser sandbox status",
            Duration::from_secs(15),
            || {
                let href = self.evaluate("location.href")?;
                let body = self.evaluate("document.body.innerText")?;
                let text = body.as_str().unwrap_or("");
                let valid = href.as_str().is_some_and(|v| v.contains("sandbox"))
                    && text.contains("Seccomp-BPF sandbox")
                    && ["PID namespaces", "Namespace sandbox", "User namespaces"]
                        .iter()
                        .any(|v| text.contains(v));
                Ok(valid.then_some(text.to_string()))
            },
        )?;
        fs::write(self.out.join("sandbox-status.txt"), &sandbox)?;
        self.capture("sandbox.png")?;
        let reported_yes = |name: &str| {
            sandbox
                .lines()
                .any(|line| line.contains(name) && line.contains("Yes"))
        };
        ensure(
            reported_yes("Seccomp-BPF sandbox"),
            "Seccomp sandbox absent",
        )?;
        ensure(
            ["Namespace sandbox", "PID namespaces", "User namespaces"]
                .iter()
                .any(|name| reported_yes(name)),
            "Namespace sandbox absent",
        )?;
        self.checks.push("Actual browser status reports namespace and seccomp sandbox enabled without disabling sandbox flags".into());
        let origin = self.server.as_ref().ok_or("server")?.origin().to_string();
        self.navigate(&format!("{origin}/page"))?;
        self.wait_eval(
            "document.title",
            json!("Fixture browser workflow"),
            "return to synthetic page",
        )?;
        self.click_page("#sample")?;
        ensure(
            self.evaluate("document.hasFocus()")? == true,
            "Page focus missing",
        )?;
        let existing = self
            .windows()?
            .into_iter()
            .map(|node| node["id"].clone())
            .collect::<HashSet<_>>();
        self.key(&["-M", "ctrl", "-k", "s", "-m", "ctrl"])?;
        let chooser = self.wait_window(
            |node| !existing.contains(&node["id"]),
            "actual retained browser Save As chooser",
        )?;
        save(&self.out.join("save-chooser-tree.json"), &self.tree()?)?;
        self.capture("save-chooser.png")?;
        ensure(
            chooser["lumen_relationships"]["parent"] == self.main["id"],
            "Save chooser parent wrong",
        )?;
        ensure(
            chooser["id"] != self.main["id"],
            "Save chooser reused main window",
        )?;
        self.key(&["-k", "Escape"])?;
        wait("Save As cancellation", Duration::from_secs(15), || {
            Ok(self
                .windows()?
                .iter()
                .all(|node| node["id"] != chooser["id"])
                .then_some(()))
        })?;
        ensure(
            fs::read_dir(&self.downloads)?.next().is_none(),
            "Save cancellation wrote a file",
        )?;
        wait(
            "page focus after Save cancellation",
            Duration::from_secs(15),
            || Ok((self.evaluate("document.hasFocus()")? == true).then_some(())),
        )?;
        self.checks.push("Actual browser Save As uses a separate captured chooser; Escape commits no file and returns page focus".into());
        Ok(())
    }
    fn finish_result(&mut self) -> Result<()> {
        if self.opt.effects {
            let proof = [
                "Creating scenefx FX renderer",
                "Using OpenGL ES",
                "GL renderer: llvmpipe",
                "FX RENDERER: Shaders Initialized Successfully",
                "Trying udmabuf allocator",
                "Created GL FBO for buffer",
            ];
            let sway = text(&self.out.join("sway.log"))?;
            ensure(
                proof.iter().all(|phrase| sway.contains(phrase)),
                "Actual software effects renderer proof missing",
            )?;
            save(&self.out.join("software-effects-proof.json"), &json!(proof))?;
        }
        let mut actions: Vec<(&str, Vec<&str>)> =
            vec![("APP-BROWSE-02", vec!["edit_url", "select_url", "navigate"])];
        match self.opt.workflow {
            Workflow::Basic => {
                actions.push(("APP-BROWSE-01", vec!["new_tab", "close_tab", "reopen_tab"]));
                actions.push(("APP-BROWSE-08", vec!["zoom_in", "zoom_reset"]));
                actions.push((
                    "APP-BROWSE-09",
                    vec!["browser_fullscreen", "resume_page_input"],
                ));
            }
            Workflow::Downloads => actions.push((
                "APP-BROWSE-04",
                vec![
                    "running_download",
                    "completed_download_hash",
                    "pause_download",
                    "resume_download",
                    "cancel_download",
                    "downloads_manager",
                ],
            )),
            Workflow::Permissions => actions.push((
                "APP-BROWSE-05",
                vec![
                    "notifications_allow",
                    "notifications_deny",
                    "notifications_revoke",
                ],
            )),
            Workflow::Library => {
                actions.push((
                    "APP-BROWSE-03",
                    vec![
                        "bookmark_add",
                        "bookmark_edit",
                        "bookmark_delete",
                        "private_history_isolation",
                    ],
                ));
                actions.push((
                    "APP-BROWSE-06",
                    vec!["private_window", "cookie_isolation", "history_isolation"],
                ));
            }
            Workflow::PageFullscreen => actions.push((
                "APP-BROWSE-09",
                vec!["page_fullscreen", "escape_fullscreen", "resume_page_input"],
            )),
            _ => {}
        }
        for (capability, names) in actions {
            for name in names {
                self.inventory.record(
                    capability,
                    name,
                    "verified_compatible",
                    json!({"workflow":self.opt.workflow.as_str(),"checks":self.checks}),
                    "",
                )?;
            }
        }
        let raw = self.call(&["lumen-browser-invocation".into(), "--excluded-setup".into()])?;
        let excluded = raw
            .split('\0')
            .filter(|v| !v.is_empty())
            .collect::<Vec<_>>();
        save(
            &self.out.join("result.json"),
            &json!({"passed":true,"workflow":self.opt.workflow.as_str(),"checks":self.checks,"version":self.version,"browser":self.opt.browser,"command":self.command,"main":self.main,"profile":self.profile,"capabilities":self.inventory.snapshot(),"excludedSetup":excluded,"fontFixture":self.opt.font_directory,"isolation":"Private non-root filesystem/PID/network/IPC/UTS namespaces; loopback only; explicit private buses and profile","notClaimed":["Whole multi-action requirement completion","Reference-exact appearance","Consumer private assets","Whole save workflow completion including errors and mount loss","Physical devices, real accounts or external networks"]}),
        )?;
        Ok(())
    }
    fn basic(&mut self) -> Result<()> {
        let initial = self
            .targets()?
            .into_iter()
            .filter(|target| target["type"] == "page")
            .map(|target| target["targetId"].clone())
            .collect::<HashSet<_>>();
        self.key(&["-M", "ctrl", "-k", "t", "-m", "ctrl"])?;
        let new = wait("actual new tab", Duration::from_secs(15), || {
            Ok(self
                .targets()?
                .into_iter()
                .find(|target| target["type"] == "page" && !initial.contains(&target["targetId"])))
        })?;
        let page = format!("{}/page", self.server.as_ref().ok_or("server")?.origin());
        self.key(&[&page])?;
        self.key(&["-k", "Return"])?;
        wait("native new-tab navigation", Duration::from_secs(15), || {
            Ok(self
                .targets()?
                .iter()
                .any(|target| target["targetId"] == new["targetId"] && target["url"] == page)
                .then_some(()))
        })?;
        self.key(&["-M", "ctrl", "-k", "w", "-m", "ctrl"])?;
        wait("close captured tab", Duration::from_secs(15), || {
            Ok(self
                .targets()?
                .iter()
                .all(|target| target["targetId"] != new["targetId"])
                .then_some(()))
        })?;
        ensure(
            self.targets()?
                .iter()
                .any(|target| target["targetId"] == self.target["targetId"]),
            "Original tab lost",
        )?;
        self.key(&[
            "-M", "ctrl", "-M", "shift", "-k", "t", "-m", "shift", "-m", "ctrl",
        ])?;
        let reopened = wait("reopen closed tab", Duration::from_secs(15), || {
            Ok(self.targets()?.into_iter().find(|target| {
                target["type"] == "page"
                    && !initial.contains(&target["targetId"])
                    && target["targetId"] != new["targetId"]
                    && target["url"] == page
            }))
        })?;
        self.key(&["-M", "ctrl", "-k", "w", "-m", "ctrl"])?;
        wait("close reopened tab", Duration::from_secs(15), || {
            Ok(self
                .targets()?
                .iter()
                .all(|target| target["targetId"] != reopened["targetId"])
                .then_some(()))
        })?;
        wait(
            "original page input after tab lifecycle",
            Duration::from_secs(15),
            || Ok((self.evaluate("document.hasFocus()")? == true).then_some(())),
        )?;
        self.checks.push("Native new, close and reopen affect captured tab identities and preserve original tab/input".into());
        let ratio = self
            .evaluate("devicePixelRatio")?
            .as_f64()
            .ok_or("device ratio")?;
        self.key(&["-M", "ctrl", "-k", "=", "-m", "ctrl"])?;
        wait("native page zoom increase", Duration::from_secs(15), || {
            Ok((self.evaluate("devicePixelRatio")?.as_f64().unwrap_or(0.0) > ratio).then_some(()))
        })?;
        self.key(&["-M", "ctrl", "-k", "0", "-m", "ctrl"])?;
        wait("native zoom reset", Duration::from_secs(15), || {
            Ok((self.evaluate("devicePixelRatio")?.as_f64() == Some(ratio)).then_some(()))
        })?;
        self.checks
            .push("Native zoom increases actual page pixel ratio and reset restores it".into());
        self.key(&["-k", "F11"])?;
        wait("browser fullscreen", Duration::from_secs(15), || {
            Ok((self.current_main()?["fullscreen_mode"] == 1).then_some(()))
        })?;
        self.wait_capture(
            "document.querySelector('#sample')",
            "browser-fullscreen.png",
            None,
            None,
        )?;
        self.key(&["-k", "F11"])?;
        wait("browser fullscreen return", Duration::from_secs(15), || {
            Ok((self.current_main()?["fullscreen_mode"] == 0).then_some(()))
        })?;
        wait(
            "input restored after browser fullscreen",
            Duration::from_secs(15),
            || Ok((self.evaluate("document.hasFocus()")? == true).then_some(())),
        )?;
        self.checks.push("Native browser fullscreen enters/exits actual compositor state and restores page input".into());
        self.finish_result()
    }
    fn open_save(&mut self) -> Result<Value> {
        if self.keyboard_route_active {
            ensure(
                self.current_main()?["focused"] == true
                    && self.evaluate("document.hasFocus()")? == true,
                "Keyboard-only save requires the captured browser/page to retain focus",
            )?;
        } else {
            self.click_page("#sample")?;
        }
        let existing = self
            .windows()?
            .into_iter()
            .map(|node| node["id"].clone())
            .collect::<HashSet<_>>();
        self.key(&["-M", "ctrl", "-k", "s", "-m", "ctrl"])?;
        let chooser = self.wait_window(
            |node| !existing.contains(&node["id"]),
            "real Save As chooser",
        )?;
        ensure(
            chooser["lumen_relationships"]["parent"] == self.main["id"],
            "Save chooser parent wrong",
        )?;
        ensure(
            chooser["border"] == "csd" && chooser["deco_rect"]["height"] == 0,
            "Save chooser must use client decoration",
        )?;
        self.chooser_mapped
            .insert(as_i64(&chooser, "id")?, Instant::now());
        Ok(chooser)
    }
    fn choose_path(&mut self, path: &Path, chooser: &Value, finish_entry: bool) -> Result<()> {
        if self.keyboard_route_active {
            return self.choose_path_keyboard(path, chooser);
        }
        let id = as_i64(chooser, "id")?;
        self.chooser_mapped.entry(id).or_insert_with(Instant::now);
        ensure(
            path.parent() == Some(self.downloads.as_path()),
            "Unexpected chooser target directory",
        )?;
        self.key(&["-M", "ctrl", "-k", "l", "-m", "ctrl"])?;
        self.key(&["-M", "ctrl", "-k", "a", "-m", "ctrl"])?;
        self.key(&[&path.file_name().ok_or("target name")?.to_string_lossy()])?;
        if finish_entry {
            self.key(&["-k", "Tab"])?;
        }
        let mapped = self.chooser_mapped[&id];
        wait(
            "provider minimum chooser display duration",
            Duration::from_secs(3),
            || Ok((mapped.elapsed() >= Duration::from_millis(500)).then_some(())),
        )?;
        self.capture("chooser-accept.png")?;
        let image = Image::load_png(&self.out.join("chooser-accept.png"))?;
        let rect = &chooser["rect"];
        let x0 = as_i64(rect, "x")?.max(0) as usize;
        let y0 = as_i64(rect, "y")?.max(0) as usize;
        let x1 = (x0 + as_i64(rect, "width")? as usize).min(image.width);
        let y1 = (y0 + 100).min(image.height);
        let mut seen = HashSet::new();
        let mut components = Vec::new();
        for y in y0..y1 {
            for x in x0..x1 {
                if seen.contains(&(x, y))
                    || !chooser_pixel(image.pixel(x, y), self.portal_facts.is_some())
                {
                    continue;
                }
                let mut queue = VecDeque::from([(x, y)]);
                seen.insert((x, y));
                let (mut left, mut right, mut top, mut bottom) = (x, x, y, y);
                while let Some((px, py)) = queue.pop_front() {
                    left = left.min(px);
                    right = right.max(px);
                    top = top.min(py);
                    bottom = bottom.max(py);
                    for (nx, ny) in [
                        (px.wrapping_sub(1), py),
                        (px + 1, py),
                        (px, py.wrapping_sub(1)),
                        (px, py + 1),
                    ] {
                        if nx >= x0
                            && nx < x1
                            && ny >= y0
                            && ny < y1
                            && !seen.contains(&(nx, ny))
                            && chooser_pixel(image.pixel(nx, ny), self.portal_facts.is_some())
                        {
                            seen.insert((nx, ny));
                            queue.push_back((nx, ny));
                        }
                    }
                }
                if (50..=150).contains(&(right - left))
                    && (20..=60).contains(&(bottom - top))
                    && top - y0 < 20
                {
                    components.push([left, top, right, bottom]);
                }
            }
        }
        ensure(
            components.len() == 2,
            format!("Chooser painted buttons: {components:?}"),
        )?;
        let target = *components
            .iter()
            .max_by_key(|bounds| bounds[0])
            .ok_or("chooser button")?;
        save(
            &self.out.join("chooser-accept-bounds.json"),
            &json!({"native":chooser,"buttons":components,"target":target}),
        )?;
        self.pointer_click(
            ((target[0] + target[2]) / 2) as f64,
            ((target[1] + target[3]) / 2) as f64,
        )?;
        self.call(&[
            "grim".into(),
            "-c".into(),
            self.out
                .join("chooser-click-cursor.png")
                .display()
                .to_string(),
        ])?;
        Ok(())
    }
    fn choose_path_keyboard(&mut self, path: &Path, chooser: &Value) -> Result<()> {
        ensure(
            self.portal_facts.is_some(),
            "Keyboard save requires captured standard portal providers",
        )?;
        ensure(
            path.parent() == Some(self.downloads.as_path()),
            "Unexpected keyboard save target",
        )?;
        ensure(
            self.windows()?
                .iter()
                .any(|row| row["id"] == chooser["id"] && row["focused"] == true),
            "Captured chooser lost keyboard focus",
        )?;
        self.key(&["-M", "ctrl", "-k", "l", "-m", "ctrl"])?;
        self.key(&["-M", "ctrl", "-k", "a", "-m", "ctrl"])?;
        self.key(&[&path.file_name().ok_or("Target name")?.to_string_lossy()])?;
        self.key(&["-k", "Tab"])?;
        let mapped = self
            .chooser_mapped
            .get(&as_i64(chooser, "id")?)
            .copied()
            .ok_or("Chooser mapping time")?;
        wait(
            "keyboard chooser minimum display duration",
            Duration::from_secs(3),
            || Ok((mapped.elapsed() >= Duration::from_millis(500)).then_some(())),
        )?;
        self.capture(&format!(
            "keyboard-save-{}-before-accept.png",
            chooser["id"]
        ))?;
        // Observe focus through read-only accessibility, then traverse using
        // actual Tab events. Never call accessible focus/action methods.
        let backend = self.portal_facts.as_ref().ok_or("Portal facts")?["backendPid"].clone();
        let deadline = Instant::now() + Duration::from_secs(15);
        let mut captured = None;
        for step in 0..20 {
            ensure(
                Instant::now() < deadline,
                "Keyboard Save traversal exceeded observation budget",
            )?;
            ensure(
                self.windows()?
                    .iter()
                    .any(|row| row["id"] == chooser["id"] && row["focused"] == true),
                "Keyboard traversal lost captured chooser",
            )?;
            let rows = self
                .a11y
                .as_mut()
                .ok_or("Private accessibility observer")?
                .snapshot()?;
            save(
                &self
                    .out
                    .join(format!("keyboard-save-{}-focus-{step}.json", chooser["id"])),
                &json!(rows),
            )?;
            let targets = rows
                .iter()
                .filter(|row| {
                    row["pid"] == backend
                        && row["name"] == "Save"
                        && matches!(row["role"].as_str(), Some("button" | "push button"))
                        && row["states"]["showing"] == true
                        && row["states"]["sensitive"] == true
                })
                .collect::<Vec<_>>();
            ensure(
                targets.len() == 1,
                "Expected one captured standard-provider Save button",
            )?;
            let target = targets[0];
            if let Some(reference) = &captured {
                ensure(
                    reference == &target["reference"],
                    "Keyboard traversal retargeted Save identity",
                )?;
            } else {
                captured = Some(target["reference"].clone());
            }
            if target["states"]["focused"] == true {
                self.capture(&format!(
                    "keyboard-save-{}-focused-accept.png",
                    chooser["id"]
                ))?;
                return self.key(&["-k", "space"]);
            }
            self.key(&["-k", "Tab"])?;
            thread::sleep(Duration::from_millis(30));
        }
        Err(fail(
            "Save button not reached by bounded keyboard traversal",
        ))
    }
    fn ui_downloads(&mut self) -> Result<Vec<Value>> {
        Ok(self.evaluate("document.querySelector('downloads-manager')?.items_?.map(row=>({id:row.id,filePath:row.filePath,fileName:row.fileName,state:row.state,percent:row.percent,total:String(row.total)}))")?.as_array().cloned().unwrap_or_default())
    }
    fn new_save(&mut self) -> Result<()> {
        let target = self.downloads.join("new-saved-page.html");
        ensure(!target.exists(), "Save target already exists")?;
        let chooser = self.open_save()?;
        self.choose_path(&target, &chooser, false)?;
        wait(
            "new-file Save provider completion",
            Duration::from_secs(15),
            || {
                Ok(self
                    .windows()?
                    .iter()
                    .all(|node| node["id"] != chooser["id"])
                    .then_some(()))
            },
        )?;
        let files = fs::read_dir(&self.downloads)?
            .filter_map(|row| row.ok())
            .map(|row| row.path())
            .collect::<Vec<_>>();
        save(
            &self.out.join("new-save-observed.json"),
            &json!({"rows":crate::profile::download_rows(&self.profile)?,"files":files}),
        )?;
        if target.exists() {
            fs::copy(&target, self.out.join("new-saved-page.html"))?;
        }
        self.navigate("brave://downloads")?;
        wait(
            "actual downloads provider UI",
            Duration::from_secs(15),
            || {
                Ok(
                    (self.evaluate("!!document.querySelector('downloads-manager')")? == true)
                        .then_some(()),
                )
            },
        )?;
        let committed = wait(
            "new saved-page provider UI complete state",
            Duration::from_secs(15),
            || {
                Ok(self.ui_downloads()?.into_iter().find(|row| {
                    row["filePath"] == target.display().to_string() && row["state"] == 2
                }))
            },
        )?;
        let bytes = fs::read(&target)?;
        ensure(
            bytes
                .windows(b"Disposable local page".len())
                .any(|w| w == b"Disposable local page"),
            "Saved page content missing",
        )?;
        save(
            &self.out.join("new-save-provider.json"),
            &json!({"download":committed,"sha256":hash(&bytes)}),
        )?;
        self.checks.push(
            "Actual new-file Save commits exact captured target and completed provider record"
                .into(),
        );
        fs::copy(&target, self.out.join("new-saved-page.html"))?;
        if self.keyboard_route_active {
            Ok(())
        } else {
            self.finish_result()
        }
    }
    fn download_control(identity: &Value, selector: &str) -> Result<String> {
        Ok(format!(
            "(Array.from(document.querySelector('downloads-manager').shadowRoot.querySelectorAll('downloads-item')).find(item=>item.data.id==={})).shadowRoot.querySelector({})",
            serde_json::to_string(identity)?,
            serde_json::to_string(selector)?
        ))
    }
    fn download_row(&mut self, name: &str, state: i64) -> Result<Option<Value>> {
        Ok(self
            .ui_downloads()?
            .into_iter()
            .find(|row| row["fileName"] == format!("fixture-{name}.bin") && row["state"] == state))
    }
    fn begin_download(&mut self, name: &str) -> Result<()> {
        let existing = self
            .windows()?
            .into_iter()
            .map(|row| row["id"].clone())
            .collect::<HashSet<_>>();
        self.click_page(&format!("#{name}"))?;
        let chooser = self.wait_window(
            |row| !existing.contains(&row["id"]),
            "actual independent download target chooser",
        )?;
        ensure(
            chooser["lumen_relationships"]["parent"] == self.main["id"],
            "Download chooser parent wrong",
        )?;
        self.capture(&format!("download-{name}-chooser.png"))?;
        let target = self.downloads.join(format!("fixture-{name}.bin"));
        self.choose_path(&target, &chooser, false)?;
        wait(
            "download target chooser acceptance",
            Duration::from_secs(15),
            || {
                Ok(self
                    .windows()?
                    .iter()
                    .all(|row| row["id"] != chooser["id"])
                    .then_some(()))
            },
        )
    }
    fn download_action(&mut self, identity: &Value, selector: &str) -> Result<()> {
        self.click_expression(
            &Self::download_control(identity, "#more-actions")?,
            None,
            None,
        )?;
        self.click_expression(&Self::download_control(identity, selector)?, None, None)
    }
    fn downloads(&mut self) -> Result<()> {
        self.begin_download("fixed")?;
        self.navigate("brave://downloads")?;
        wait("actual downloads manager", Duration::from_secs(15), || {
            Ok(
                (self.evaluate("!!document.querySelector('downloads-manager')")? == true)
                    .then_some(()),
            )
        })?;
        let running = wait(
            "independent actual running download",
            Duration::from_secs(15),
            || self.download_row("fixed", 0),
        )?;
        let target = self.downloads.join("fixture-fixed.bin");
        ensure(!target.exists(), "Gated transfer completed early")?;
        ensure(
            fs::read_dir(&self.downloads)?
                .filter_map(|row| row.ok())
                .any(|row| row.file_name().to_string_lossy().ends_with(".crdownload")),
            "Missing partial transfer",
        )?;
        ensure(
            self.server
                .as_ref()
                .ok_or("server")?
                .snapshot()
                .iter()
                .all(|row| row["complete"] != true),
            "Gated response finished early",
        )?;
        self.wait_capture(
            &Self::download_control(&running["id"], "#content")?,
            "download-running.png",
            None,
            None,
        )?;
        self.checks.push("Independent gated download is genuinely running with partial file and no completed target".into());
        self.download_action(&running["id"], "#pause-or-resume")?;
        wait("actual paused download", Duration::from_secs(15), || {
            self.download_row("fixed", 3)
        })?;
        ensure(!target.exists(), "Paused transfer completed")?;
        self.wait_capture(
            &Self::download_control(&running["id"], "#content")?,
            "download-paused.png",
            None,
            None,
        )?;
        self.download_action(&running["id"], "#pause-or-resume")?;
        wait("actual resumed download", Duration::from_secs(15), || {
            self.download_row("fixed", 0)
        })?;
        self.checks.push(
            "Captured native download overflow actions pause and resume the same provider identity"
                .into(),
        );
        self.server.as_ref().ok_or("server")?.gate("fixed")?.open();
        let complete = wait(
            "actual completed independent download",
            Duration::from_secs(15),
            || self.download_row("fixed", 2),
        )?;
        let digest = crate::server::digest();
        wait(
            "complete download exact bytes",
            Duration::from_secs(15),
            || Ok((target.exists() && hash(&fs::read(&target)?) == digest).then_some(())),
        )?;
        ensure(
            !fs::read_dir(&self.downloads)?
                .filter_map(|row| row.ok())
                .any(|row| row.file_name().to_string_lossy().ends_with(".crdownload")),
            "Partial transfer remained",
        )?;
        self.wait_capture(
            &Self::download_control(&complete["id"], "#content")?,
            "download-complete.png",
            None,
            None,
        )?;
        self.checks.push("Independent download completion requires provider complete plus exact whole-file hash and no partial".into());
        let page = format!("{}/page", self.server.as_ref().ok_or("server")?.origin());
        self.navigate(&page)?;
        self.wait_eval(
            "document.title",
            json!("Fixture browser workflow"),
            "return to local page",
        )?;
        self.begin_download("cancel")?;
        self.navigate("brave://downloads")?;
        let cancel = wait(
            "second independent running download",
            Duration::from_secs(15),
            || self.download_row("cancel", 0),
        )?;
        self.download_action(&cancel["id"], "#cancel")?;
        let cancelled = wait(
            "actual cancelled provider row",
            Duration::from_secs(15),
            || self.download_row("cancel", 1),
        )?;
        self.server.as_ref().ok_or("server")?.gate("cancel")?.open();
        wait("cancelled partial cleanup", Duration::from_secs(15), || {
            Ok((!fs::read_dir(&self.downloads)?
                .filter_map(|row| row.ok())
                .any(|row| row.file_name().to_string_lossy().ends_with(".crdownload")))
            .then_some(()))
        })?;
        ensure(
            !self.downloads.join("fixture-cancel.bin").exists(),
            "Cancelled transfer wrote final target",
        )?;
        ensure(
            hash(&fs::read(&target)?) == digest,
            "Completed neighbor changed",
        )?;
        self.wait_capture(
            &Self::download_control(&cancelled["id"], "#content")?,
            "download-cancelled.png",
            None,
            None,
        )?;
        self.checks.push("Captured Cancel affects only second transfer and removes its partial without changing completed neighbor".into());
        save(
            &self.out.join("download-outcomes.json"),
            &json!({"running":running,"complete":complete,"cancelled":cancelled,"server":self.server.as_ref().unwrap().snapshot(),"completeSha256":digest}),
        )?;
        self.finish_result()
    }
    fn bookmark_rows(&self) -> Result<Vec<Value>> {
        let path = self.profile.join("Default/Bookmarks");
        if !path.exists() {
            return Ok(Vec::new());
        }
        let document: Value = serde_json::from_str(&text(&path)?)?;
        let mut rows = Vec::new();
        fn walk(node: &Value, rows: &mut Vec<Value>) {
            if node["type"] == "url" {
                rows.push(node.clone())
            }
            if let Some(children) = node["children"].as_array() {
                for child in children {
                    walk(child, rows)
                }
            }
        }
        if let Some(roots) = document["roots"].as_object() {
            for root in roots.values() {
                walk(root, &mut rows)
            }
        }
        Ok(rows)
    }
    fn control(
        &mut self,
        name: &str,
        role: &str,
        description: Option<&str>,
    ) -> Result<Option<Value>> {
        let rows = self.a11y.as_mut().ok_or("accessibility")?.snapshot()?;
        save(&self.out.join("bookmark-controls.json"), &json!(rows))?;
        let matching = rows
            .into_iter()
            .filter(|row| {
                row["name"] == name
                    && row["role"] == role
                    && row["pid"] == self.main["pid"]
                    && row["states"]["showing"] == true
                    && description.is_none_or(|value| row["description"] == value)
            })
            .collect::<Vec<_>>();
        ensure(
            matching.len() <= 1,
            format!("Ambiguous browser control {name}: {matching:?}"),
        )?;
        Ok(matching.into_iter().next())
    }
    fn activate(&mut self, name: &str, description: Option<&str>) -> Result<()> {
        let row = wait(
            &format!("actual bookmark {name}"),
            Duration::from_secs(15),
            || self.control(name, "button", description),
        )?;
        self.a11y
            .as_mut()
            .ok_or("accessibility")?
            .focus_captured(&row)?;
        self.key(&["-k", "space"])
    }
    fn click_accessible(&mut self, row: &Value) -> Result<()> {
        let (x, y, w, h) = self
            .a11y
            .as_mut()
            .ok_or("accessibility")?
            .bounds_captured(row)?;
        let native = self.current_main()?;
        let left = x as f64 + as_f64(&native["rect"], "x")? + as_f64(&native["window_rect"], "x")?;
        let top = y as f64 + as_f64(&native["rect"], "y")? + as_f64(&native["window_rect"], "y")?;
        save(
            &self.out.join("accessible-pointer-target.json"),
            &json!({"control":row,"bounds":[left,top,w,h]}),
        )?;
        self.pointer_click(left + (w / 2) as f64, top + (h / 2) as f64)
    }
    fn history_ui(&mut self) -> Result<Vec<Value>> {
        Ok(self.evaluate("(()=>{const roots=[document];while(roots.length){const root=roots.shift();const list=root.querySelector('history-list');if(list)return list.historyData_?.flatMap(row=>[row.url,...Object.keys(row.allTimestamps||{}).filter(value=>value.startsWith('http'))])||[];for(const node of root.querySelectorAll('*'))if(node.shadowRoot)roots.push(node.shadowRoot);}return null;})()")?.as_array().cloned().unwrap_or_default())
    }
    fn library(&mut self) -> Result<()> {
        let normal = format!(
            "{}/page?visit=normal",
            self.server.as_ref().ok_or("server")?.origin()
        );
        self.navigate(&normal)?;
        self.wait_eval("location.href", json!(normal), "normal history navigation")?;
        self.key(&["-M", "ctrl", "-k", "d", "-m", "ctrl"])?;
        wait("actual bookmark editor", Duration::from_secs(15), || {
            self.control("Done", "button", None)
        })?;
        self.capture("bookmark-add.png")?;
        self.activate("Done", None)?;
        let added = wait("actual bookmark persisted", Duration::from_secs(15), || {
            Ok(self
                .bookmark_rows()?
                .into_iter()
                .find(|row| row["url"] == normal))
        })?;
        self.checks.push("Native bookmark add commits exact current URL through actual editor and profile provider".into());
        self.key(&["-M", "ctrl", "-k", "d", "-m", "ctrl"])?;
        let entry = wait(
            "actual bookmark name editor",
            Duration::from_secs(15),
            || self.control("Bookmark name", "entry", None),
        )?;
        self.a11y
            .as_mut()
            .ok_or("accessibility")?
            .focus_captured(&entry)?;
        self.key(&["-M", "ctrl", "-k", "a", "-m", "ctrl"])?;
        self.key(&["Disposable bookmark"])?;
        self.activate("Done", None)?;
        let edited = wait(
            "captured bookmark edit persists",
            Duration::from_secs(15),
            || {
                Ok(self
                    .bookmark_rows()?
                    .into_iter()
                    .find(|row| row["id"] == added["id"] && row["name"] == "Disposable bookmark"))
            },
        )?;
        self.checks.push(
            "Native bookmark editor changes same bookmark identity and preserves exact URL".into(),
        );
        self.key(&["-M", "ctrl", "-k", "d", "-m", "ctrl"])?;
        self.activate("Remove", None)?;
        wait("captured bookmark removal", Duration::from_secs(15), || {
            Ok(self
                .bookmark_rows()?
                .iter()
                .all(|row| row["id"] != added["id"])
                .then_some(()))
        })?;
        self.checks.push(
            "Native bookmark Remove deletes captured bookmark without direct store mutation".into(),
        );
        self.navigate("brave://history")?;
        wait(
            "normal visit in actual history provider UI",
            Duration::from_secs(15),
            || Ok(self.history_ui()?.contains(&json!(normal)).then_some(())),
        )?;
        self.capture("normal-history.png")?;
        self.navigate(&normal)?;
        self.wait_eval(
            "location.href",
            json!(normal),
            "return to normal page for storage",
        )?;
        self.click_page("#set-marker")?;
        ensure(self.evaluate("document.cookie.includes('fixture_marker=normal') && localStorage.getItem('fixture_marker')==='normal'")?==true,"Normal storage marker absent")?;
        let existing = self
            .windows()?
            .into_iter()
            .map(|row| row["id"].clone())
            .collect::<HashSet<_>>();
        self.key(&[
            "-M", "ctrl", "-M", "shift", "-k", "n", "-m", "shift", "-m", "ctrl",
        ])?;
        let private = self.wait_window(
            |row| !existing.contains(&row["id"]),
            "actual private window",
        )?;
        self.capture("private-window-opening.png")?;
        let private_url = format!(
            "{}/page?visit=private&marker=private",
            self.server.as_ref().ok_or("server")?.origin()
        );
        self.key(&["-M", "ctrl", "-k", "l", "-m", "ctrl"])?;
        self.key(&[&private_url])?;
        self.key(&["-k", "Return"])?;
        let target = wait("actual private target", Duration::from_secs(15), || {
            Ok(self
                .targets()?
                .into_iter()
                .find(|row| row["url"] == private_url))
        })?;
        ensure(
            !target["browserContextId"].is_null()
                && target["browserContextId"] != self.target["browserContextId"],
            "Private browser context not distinct",
        )?;
        let session = self.cdp(
            "Target.attachToTarget",
            json!({"targetId":target["targetId"],"flatten":true}),
            None,
        )?["sessionId"]
            .as_str()
            .ok_or("private session")?
            .to_string();
        wait("private local page", Duration::from_secs(15), || {
            Ok(
                (self.evaluate_session("document.readyState", &session)? == "complete")
                    .then_some(()),
            )
        })?;
        ensure(
            self.evaluate_session(
                "document.cookie==='' && localStorage.getItem('fixture_marker')===null",
                &session,
            )? == true,
            "Private storage not isolated",
        )?;
        self.click_expression(
            "document.querySelector('#set-marker')",
            Some(&session),
            Some(&private),
        )?;
        wait(
            "private independent storage",
            Duration::from_secs(15),
            || {
                Ok((self.evaluate_session("document.cookie.includes('fixture_marker=private') && localStorage.getItem('fixture_marker')==='private'",&session)?==true).then_some(()))
            },
        )?;
        self.wait_capture(
            "document.querySelector('#status')",
            "private-window.png",
            Some(&session),
            Some(&private),
        )?;
        self.key(&[
            "-M", "ctrl", "-M", "shift", "-k", "w", "-m", "shift", "-m", "ctrl",
        ])?;
        wait("private window close", Duration::from_secs(15), || {
            Ok(self
                .windows()?
                .iter()
                .all(|row| row["id"] != private["id"])
                .then_some(()))
        })?;
        ensure(self.evaluate("document.cookie.includes('fixture_marker=normal') && localStorage.getItem('fixture_marker')==='normal'")?==true,"Normal storage marker changed")?;
        self.checks.push("Native private window has distinct browser context and isolated real cookies/local storage".into());
        let sentinel = format!(
            "{}/page?visit=after-private",
            self.server.as_ref().ok_or("server")?.origin()
        );
        self.navigate(&sentinel)?;
        self.wait_eval(
            "location.href",
            json!(sentinel),
            "post-private normal visit",
        )?;
        self.navigate("brave://history")?;
        let observed = wait(
            "actual history provider after private window close",
            Duration::from_secs(15),
            || {
                let rows = self.history_ui()?;
                Ok(rows.contains(&json!(sentinel)).then_some(rows))
            },
        )?;
        save(
            &self.out.join("history-provider-observed.json"),
            &json!({"normal":normal,"private":private_url,"sentinel":sentinel,"rows":observed}),
        )?;
        ensure(
            !observed.contains(&json!(private_url)) && observed.contains(&json!(normal)),
            "Private history leaked to normal provider",
        )?;
        self.capture("private-history-isolation.png")?;
        let menu = wait(
            "actual captured application menu button",
            Duration::from_secs(15),
            || self.control("Brave", "button", Some("Customize and control Brave")),
        )?;
        self.click_accessible(&menu)?;
        let exit = wait(
            "actual browser Exit menu item",
            Duration::from_secs(15),
            || self.control("Exit", "menu item", None),
        )?;
        self.click_accessible(&exit)?;
        wait(
            "native browser quit and profile flush",
            Duration::from_secs(15),
            || {
                Ok(self
                    .browser
                    .as_mut()
                    .ok_or("browser")?
                    .exited()?
                    .then_some(()))
            },
        )?;
        let persisted = crate::profile::history_urls(&self.profile)?;
        ensure(
            persisted.contains(&normal)
                && persisted.contains(&sentinel)
                && !persisted.contains(&private_url),
            "Private history leaked to durable history",
        )?;
        self.checks.push("Private visit remains absent from actual normal history UI and durable history after native quit".into());
        save(
            &self.out.join("library-outcomes.json"),
            &json!({"added":added,"edited":edited,"removedId":added["id"],"privateWindow":private,"privateTarget":target,"normalHistory":persisted,"providerHistory":observed}),
        )?;
        self.finish_result()
    }
    fn permission_snapshot(&mut self, name: &str) -> Result<Vec<Value>> {
        let a11y = self.a11y.as_mut().ok_or("accessibility")?;
        let rows = a11y.snapshot()?;
        let start = Instant::now();
        let owners = crate::accessibility::Observer {
            provider: a11y,
            clock: || start.elapsed().as_secs_f64(),
        }
        .diagnostics()?;
        save(
            &self.out.join(format!("{name}-owners.json")),
            &json!(owners),
        )?;
        save(&self.out.join(format!("{name}.json")), &json!(rows))?;
        Ok(rows)
    }
    fn permission_button(&mut self, name: &str, rows: Option<Vec<Value>>) -> Result<Option<Value>> {
        let rows = match rows {
            Some(rows) => rows,
            None => self.permission_snapshot("permission-current")?,
        };
        let found = rows
            .into_iter()
            .filter(|row| {
                row["states"]["showing"] == true
                    && row["pid"] == self.main["pid"]
                    && row["role"] == "button"
                    && row["name"] == name
            })
            .collect::<Vec<_>>();
        ensure(
            found.len() <= 1,
            format!("Ambiguous permission button {name}"),
        )?;
        Ok(found.into_iter().next())
    }
    fn permission_activate(&mut self, row: &Value) -> Result<()> {
        self.a11y
            .as_mut()
            .ok_or("accessibility")?
            .focus_captured(row)?;
        self.key(&["-k", "space"])
    }
    fn permission_request(&mut self, decision: &str, expected: &str) -> Result<Value> {
        ensure(
            self.evaluate("Notification.permission")? == "default",
            "Permission not default before request",
        )?;
        self.click_page("#permission")?;
        let row = wait(
            &format!("actual origin notification {decision} control"),
            Duration::from_secs(15),
            || {
                let rows =
                    self.permission_snapshot(&format!("notification-{expected}-controls"))?;
                let found = self.permission_button(decision, Some(rows.clone()))?;
                if found.is_some() {
                    let label = format!(
                        "{} wants to: Show notifications",
                        self.server.as_ref().ok_or("server")?.origin()
                    );
                    ensure(
                        rows.iter()
                            .any(|row| row["role"] == "alert" && row["name"] == label),
                        "Origin notification prompt absent",
                    )?;
                }
                Ok(found)
            },
        )?;
        self.capture(&format!("notification-{expected}-prompt.png"))?;
        self.permission_activate(&row)?;
        wait(
            &format!("actual origin notification {expected}"),
            Duration::from_secs(15),
            || {
                Ok(
                    (self.evaluate("document.querySelector('#status').textContent")? == expected
                        && self.evaluate("Notification.permission")? == expected)
                        .then_some(()),
                )
            },
        )?;
        self.capture(&format!("notification-{expected}.png"))?;
        Ok(row)
    }
    fn permission_reset(&mut self) -> Result<()> {
        let site = wait(
            "site controls after prompt closes",
            Duration::from_secs(15),
            || self.permission_button("View site information", None),
        )?;
        self.permission_activate(&site)?;
        let rows = self.permission_snapshot("site-permission-controls")?;
        let origin = self.server.as_ref().ok_or("server")?.origin();
        let host = origin.strip_prefix("http://").unwrap_or(origin);
        ensure(
            rows.iter()
                .any(|row| row["role"] == "alert" && row["name"] == host),
            "Site controls alert absent",
        )?;
        self.capture("site-permission-controls.png")?;
        let reset = self
            .permission_button("Reset permission", Some(rows))?
            .ok_or("Reset permission button")?;
        self.permission_activate(&reset)?;
        wait(
            "native origin permission reset",
            Duration::from_secs(15),
            || Ok((self.evaluate("Notification.permission")? == "default").then_some(())),
        )?;
        self.key(&["-k", "Escape"])?;
        self.key(&["-M", "ctrl", "-k", "r", "-m", "ctrl"])?;
        wait(
            "actual page reload after permission reset",
            Duration::from_secs(15),
            || {
                Ok(
                    (self.evaluate("document.querySelector('#status')?.textContent")?
                        == "No permission requested"
                        && self.evaluate("Notification.permission")? == "default")
                        .then_some(()),
                )
            },
        )
    }
    fn permissions(&mut self) -> Result<()> {
        let denied = self.permission_request("Block", "denied")?;
        self.checks.push("Real notification prompt denial updates requesting page and captured origin permission".into());
        self.permission_reset()?;
        self.checks.push("Actual site controls reset denied notification permission to default without a debugger grant".into());
        let granted = self.permission_request("Allow", "granted")?;
        self.checks.push(
            "Real captured-origin notification Allow updates provider and requesting page".into(),
        );
        self.permission_reset()?;
        self.checks.push(
            "Actual site controls revoke granted notification permission back to default".into(),
        );
        save(
            &self.out.join("notification-permission-result.json"),
            &json!({"origin":self.server.as_ref().unwrap().origin(),"denied":denied,"granted":granted,"finalPermission":self.evaluate("Notification.permission")?}),
        )?;
        self.finish_result()
    }
    fn page_fullscreen(&mut self) -> Result<()> {
        self.click_page("#fullscreen")?;
        wait("page fullscreen request", Duration::from_secs(15), || {
            Ok((self.evaluate("!!document.fullscreenElement")? == true).then_some(()))
        })?;
        wait(
            "page fullscreen native state",
            Duration::from_secs(15),
            || Ok((self.current_main()?["fullscreen_mode"] == 1).then_some(())),
        )?;
        self.capture("page-fullscreen-before-readback.png")?;
        let native = self.current_main()?;
        let layout = self.cdp(
            "Page.getLayoutMetrics",
            json!({}),
            Some(&self.session.clone()),
        )?;
        let dom = self.evaluate("({width:innerWidth,height:innerHeight,dpr:devicePixelRatio})")?;
        save(
            &self.out.join("page-fullscreen-before-readback.json"),
            &json!({"native":native,"layout":layout,"dom":dom}),
        )?;
        self.wait_capture(
            "document.querySelector('#sample')",
            "page-fullscreen.png",
            None,
            None,
        )?;
        self.key(&["-k", "Escape"])?;
        wait("page fullscreen Escape", Duration::from_secs(15), || {
            Ok((self.evaluate("!!document.fullscreenElement")? == false
                && self.current_main()?["fullscreen_mode"] == 0)
                .then_some(()))
        })?;
        ensure(
            self.evaluate("document.hasFocus()")? == true,
            "Input missing after page fullscreen",
        )?;
        self.checks.push("Actual page gesture fullscreen and native Escape reconcile page/native state and input".into());
        self.finish_result()
    }
    fn overwrite_confirmation(
        &mut self,
        chooser: &Value,
        existing: &HashSet<Value>,
        name: &str,
    ) -> Result<Value> {
        let question = wait(
            "real focused portal existing-target confirmation",
            Duration::from_secs(15),
            || {
                let rows = self
                    .windows()?
                    .into_iter()
                    .filter(|row| !existing.contains(&row["id"]))
                    .collect::<Vec<_>>();
                ensure(rows.len() <= 1, "Ambiguous overwrite confirmation")?;
                Ok(rows
                    .into_iter()
                    .find(|row| row["focused"] == true && row["visible"] == true))
            },
        )?;
        let facts = self.portal_facts.as_ref().ok_or("portal facts")?;
        ensure(
            question["pid"] == facts["backendPid"]
                && question["lumen_relationships"]["parent"] == chooser["id"]
                && question["id"] != chooser["id"],
            "Portal confirmation relationship wrong",
        )?;
        save(
            &self.out.join(format!("overwrite-{name}-tree.json")),
            &self.tree()?,
        )?;
        self.capture(&format!("overwrite-{name}.png"))?;
        Ok(question)
    }
    fn keyboard_save(&mut self) -> Result<()> {
        self.keyboard_route_active = true;
        let chooser = self.open_save()?;
        self.key(&["-k", "Escape"])?;
        wait(
            "keyboard-only initial save cancellation",
            Duration::from_secs(15),
            || {
                Ok(self
                    .windows()?
                    .iter()
                    .all(|row| row["id"] != chooser["id"])
                    .then_some(()))
            },
        )?;
        ensure(
            fs::read_dir(&self.downloads)?.next().is_none(),
            "Keyboard Cancel wrote a file",
        )?;
        wait(
            "keyboard-only Cancel returns page focus",
            Duration::from_secs(15),
            || Ok((self.evaluate("document.hasFocus()")? == true).then_some(())),
        )?;
        self.checks
            .push("Keyboard-only Save As and Escape Cancel preserve files and page focus".into());
        self.new_save()?;
        let origin = self.server.as_ref().ok_or("Server")?.origin().to_string();
        self.navigate(&format!("{origin}/page"))?;
        self.wait_eval(
            "document.title",
            json!("Fixture browser workflow"),
            "synthetic page before keyboard overwrite",
        )?;
        self.checks.push("Keyboard-only new target save uses captured chooser and exact complete provider/file identity".into());
        self.overwrite()
    }
    fn overwrite(&mut self) -> Result<()> {
        let target = self.downloads.join("report.html");
        fs::write(&target, "original disposable contents")?;
        let original = hash(&fs::read(&target)?);
        let chooser = self.open_save()?;
        let existing = self
            .windows()?
            .into_iter()
            .map(|row| row["id"].clone())
            .collect::<HashSet<_>>();
        self.choose_path(&target, &chooser, true)?;
        let question = self.overwrite_confirmation(&chooser, &existing, "cancel")?;
        ensure(
            hash(&fs::read(&target)?) == original,
            "Overwrite changed target before confirmation",
        )?;
        save(
            &self.out.join("overwrite-confirmation-tree.json"),
            &self.tree()?,
        )?;
        self.capture("overwrite-confirmation.png")?;
        self.key(&["-k", "Escape"])?;
        wait(
            "actual portal overwrite Cancel",
            Duration::from_secs(15),
            || {
                Ok(self
                    .windows()?
                    .iter()
                    .all(|row| row["id"] != question["id"])
                    .then_some(()))
            },
        )?;
        ensure(
            self.windows()?.iter().any(|row| row["id"] == chooser["id"])
                && hash(&fs::read(&target)?) == original,
            "Overwrite cancel did not preserve chooser and target",
        )?;
        self.key(&["-k", "Escape"])?;
        wait(
            "chooser cancellation after overwrite refusal",
            Duration::from_secs(15),
            || {
                Ok(self
                    .windows()?
                    .iter()
                    .all(|row| row["id"] != chooser["id"])
                    .then_some(()))
            },
        )?;
        wait(
            "page focus after overwrite refusal",
            Duration::from_secs(15),
            || Ok((self.evaluate("document.hasFocus()")? == true).then_some(())),
        )?;
        self.checks.push("Real portal overwrite Cancel preserves exact existing target hash and returns chooser/page focus".into());
        let chooser = self.open_save()?;
        let existing = self
            .windows()?
            .into_iter()
            .map(|row| row["id"].clone())
            .collect::<HashSet<_>>();
        self.choose_path(&target, &chooser, true)?;
        let question = self.overwrite_confirmation(&chooser, &existing, "replace")?;
        ensure(
            hash(&fs::read(&target)?) == original,
            "Overwrite changed target before Replace",
        )?;
        self.key(&["-M", "alt", "-k", "r", "-m", "alt"])?;
        wait(
            "explicit native portal Replace",
            Duration::from_secs(15),
            || {
                Ok(self
                    .windows()?
                    .iter()
                    .all(|row| row["id"] != question["id"] && row["id"] != chooser["id"])
                    .then_some(()))
            },
        )?;
        wait(
            "captured target replaced after confirmation",
            Duration::from_secs(15),
            || Ok((target.exists() && hash(&fs::read(&target)?) != original).then_some(())),
        )?;
        let bytes = fs::read(&target)?;
        ensure(
            bytes
                .windows(b"Disposable local page".len())
                .any(|row| row == b"Disposable local page"),
            "Replaced target content missing",
        )?;
        self.navigate("brave://downloads")?;
        let complete = wait(
            "actual saved-page provider complete after Replace",
            Duration::from_secs(15),
            || {
                Ok(self.ui_downloads()?.into_iter().find(|row| {
                    row["filePath"] == target.display().to_string() && row["state"] == 2
                }))
            },
        )?;
        self.wait_capture(
            &Self::download_control(&complete["id"], "#content")?,
            "overwrite-completed-download.png",
            None,
            None,
        )?;
        save(
            &self.out.join("overwrite-result.json"),
            &json!({"target":target,"beforeSha256":original,"afterSha256":hash(&bytes),"bytes":bytes.len(),"download":complete,"portal":self.portal_facts}),
        )?;
        self.checks.push("Explicit native portal Replace commits exact captured target and completed provider record after confirmation".into());
        self.finish_result()
    }
}
impl AccessibilityHost for Native {
    fn launch(
        &mut self,
        command: &[String],
        log: &str,
        _environment: &BTreeMap<String, String>,
    ) -> Result<u32> {
        Native::launch(self, command, log)
    }
    fn wait_name(&mut self, address: &str, name: &str, _owner_pid: u32) -> Result<()> {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        let builder = zbus::connection::Builder::address(address)?;
        let connection = runtime.block_on(async {
            tokio::time::timeout(Duration::from_secs(1), builder.build()).await
        })??;
        wait(name, Duration::from_secs(15), || {
            let connection = &connection;
            let owned = name.to_string();
            let result = runtime.block_on(async {
                tokio::time::timeout(Duration::from_millis(500), async move {
                    let proxy = zbus::Proxy::new(
                        connection,
                        "org.freedesktop.DBus",
                        "/org/freedesktop/DBus",
                        "org.freedesktop.DBus",
                    )
                    .await?;
                    proxy
                        .call::<_, _, bool>("NameHasOwner", &(owned.as_str(),))
                        .await
                })
                .await
            });
            Ok(result
                .ok()
                .and_then(std::result::Result::ok)
                .filter(|v| *v)
                .map(|_| ()))
        })
    }
}
impl PortalHost for Native {
    fn launch(
        &mut self,
        command: &[String],
        log: &str,
        environment: &BTreeMap<String, String>,
    ) -> Result<u32> {
        let file = File::create(self.out.join(log))?;
        let mut proc = Command::new(&command[0]);
        proc.args(&command[1..])
            .env_clear()
            .envs(environment)
            .stdout(Stdio::from(file.try_clone()?))
            .stderr(Stdio::from(file));
        unsafe {
            proc.pre_exec(|| {
                libc::setsid();
                Ok(())
            });
        }
        let child = proc.spawn()?;
        let pid = child.id();
        self.processes.push(Managed(child));
        Ok(pid)
    }
    fn wait_socket(&mut self, path: &Path, _owner_pid: u32) -> Result<()> {
        wait("owned chooser bus", Duration::from_secs(15), || {
            Ok(path.exists().then_some(()))
        })
    }
    fn wait_name(&mut self, address: &str, name: &str, pid: u32) -> Result<()> {
        <Native as AccessibilityHost>::wait_name(self, address, name, pid)
    }
}
fn chooser_pixel([r, g, b]: [u8; 3], portal: bool) -> bool {
    let lo = r.min(g).min(b);
    let hi = r.max(g).max(b);
    (lo > 225 && hi - lo < 8)
        || (portal && b > r.saturating_add(60) && b > g.saturating_add(20) && b > 120)
}
pub fn run(options: Options) -> Result<()> {
    let mut fixture = Native::new(options)?;
    fixture.start()?;
    fixture.common()?;
    match fixture.opt.workflow {
        Workflow::Basic => fixture.basic(),
        Workflow::NewSave => fixture.new_save(),
        Workflow::Downloads => fixture.downloads(),
        Workflow::Library => fixture.library(),
        Workflow::Permissions => fixture.permissions(),
        Workflow::PageFullscreen => fixture.page_fullscreen(),
        Workflow::Overwrite => fixture.overwrite(),
        Workflow::KeyboardSave => fixture.keyboard_save(),
    }
}
