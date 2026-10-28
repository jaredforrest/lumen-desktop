//! Real Orca speech and input over owned installed controls in a disposable namespace.
use super::*;
use futures_util::StreamExt;
use sha2::{Digest, Sha256};
use std::io::Write;
use std::os::unix::fs::MetadataExt;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};

type Reference = (String, String);
fn hash(path: &Path) -> Result<String> {
    Ok(format!("{:x}", Sha256::digest(fs::read(path)?)))
}
fn scan_xml(orca: &Path) -> Result<PathBuf> {
    for version in fs::read_dir(orca.join("share/gsettings-schemas"))? {
        let schemas = version?.path().join("glib-2.0/schemas");
        if schemas.is_dir() {
            for item in fs::read_dir(schemas)? {
                let path = item?.path();
                if path.extension().is_some_and(|v| v == "xml") {
                    return Ok(path);
                }
            }
        }
    }
    Err(fail("Orca GSettings schema missing"))
}
impl Harness {
    fn capture_events(
        &self,
    ) -> Result<(
        Arc<Mutex<Vec<Value>>>,
        Arc<AtomicBool>,
        thread::JoinHandle<()>,
    )> {
        let bus = self.a11y.as_ref().ok_or("AT-SPI")?;
        for event in ["object:", "window:", "focus:"] {
            self.rt.block_on(async {
                let registry = Proxy::new(
                    bus,
                    "org.a11y.atspi.Registry",
                    "/org/a11y/atspi/registry",
                    "org.a11y.atspi.Registry",
                )
                .await?;
                let result: zbus::Result<()> = registry
                    .call("RegisterEvent", &(event, Vec::<String>::new()))
                    .await;
                if result.is_err() {
                    let _: () = registry.call("RegisterEvent", &(event,)).await?;
                }
                Ok::<(), zbus::Error>(())
            })?;
        }
        let collected = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let connection = bus.clone();
        let output = Arc::clone(&collected);
        let stopped = Arc::clone(&stop);
        let handle = thread::spawn(move || {
            let Ok(rt) = Builder::new_current_thread().enable_all().build() else {
                return;
            };
            rt.block_on(async {
                let rule = zbus::MatchRule::builder().msg_type(zbus::message::Type::Signal).build();
                let Ok(mut stream) = zbus::MessageStream::for_match_rule(rule, &connection, Some(1024)).await else { return; };
                let started = Instant::now();
                while !stopped.load(Ordering::Relaxed) {
                    if let Ok(Some(Ok(message))) = tokio::time::timeout(Duration::from_millis(100), stream.next()).await {
                        let header = message.header();
                        let interface = header.interface().map(|v| v.to_string()).unwrap_or_default();
                        if !interface.starts_with("org.a11y.atspi.Event.") { continue; }
                        let body = String::from_utf8_lossy(message.data().bytes()).to_string();
                        let value = json!({"time":started.elapsed().as_secs_f64(), "arguments":body, "sender":header.sender().map(|v| v.to_string()), "interface":interface, "member":header.member().map(|v| v.to_string()), "path":header.path().map(|v| v.to_string())});
                        if let Ok(mut events) = output.lock() { events.push(value); }
                    }
                }
            });
        });
        Ok((collected, stop, handle))
    }
    fn reader_state(&self, fixture: &Path) -> Result<Value> {
        Ok(serde_json::from_str(&self.ipc(
            fixture,
            "readerFixture",
            "state",
        )?)?)
    }
    fn reader_ipc(&self, fixture: &Path, method: &str) -> Result<()> {
        self.ipc(fixture, "readerFixture", method)?;
        Ok(())
    }
    fn speech_messages(&self, tool: &Path, protocol: &Path) -> Result<Vec<Value>> {
        let command = vec![
            tool.display().to_string(),
            "messages".into(),
            protocol.display().to_string(),
        ];
        Ok(serde_json::from_str(&self.call(&command)?)?)
    }
    fn utterances(&self, tool: &Path) -> Result<Vec<String>> {
        let command = vec![
            tool.display().to_string(),
            "utterances".into(),
            self.opt.output.join("orca-debug.log").display().to_string(),
        ];
        Ok(serde_json::from_str(&self.call(&command)?)?)
    }
    fn spoken(&self, tool: &Path, protocol: &Path, needle: &str, after: usize) -> Result<bool> {
        Ok(self
            .speech_messages(tool, protocol)?
            .iter()
            .skip(after)
            .any(|message| {
                message["plainText"]
                    .as_str()
                    .is_some_and(|v| v.contains(needle))
            }))
    }
    fn focus_ref(&self, reference: &Reference) -> Result<()> {
        let bus = self.a11y.as_ref().ok_or("AT-SPI")?;
        let result: bool = self.rt.block_on(async {
            let proxy = Proxy::new(
                bus,
                reference.0.as_str(),
                reference.1.as_str(),
                "org.a11y.atspi.Component",
            )
            .await?;
            proxy.call("GrabFocus", &()).await
        })?;
        ensure(result, "Captured accessible focus failed")
    }
    fn action_ref(&self, reference: &Reference) -> Result<bool> {
        let bus = self.a11y.as_ref().ok_or("AT-SPI")?;
        Ok(self.rt.block_on(async {
            let proxy = Proxy::new(
                bus,
                reference.0.as_str(),
                reference.1.as_str(),
                "org.a11y.atspi.Action",
            )
            .await?;
            proxy.call("DoAction", &(0_i32,)).await
        })?)
    }
    fn state_bits(&self, reference: &Reference) -> Result<Vec<u32>> {
        let bus = self.a11y.as_ref().ok_or("AT-SPI")?;
        Ok(self.rt.block_on(async {
            let proxy = Proxy::new(
                bus,
                reference.0.as_str(),
                reference.1.as_str(),
                "org.a11y.atspi.Accessible",
            )
            .await?;
            proxy.call("GetState", &()).await
        })?)
    }
    fn state_has(&self, reference: &Reference, bit: usize) -> Result<bool> {
        let states = self.state_bits(reference)?;
        Ok(states
            .get(bit / 32)
            .is_some_and(|word| word & (1 << (bit % 32)) != 0))
    }
    fn text_caret(&self, reference: &Reference) -> Result<i32> {
        let bus = self.a11y.as_ref().ok_or("AT-SPI")?;
        let value: zbus::zvariant::OwnedValue = self.rt.block_on(async {
            let proxy = Proxy::new(
                bus,
                reference.0.as_str(),
                reference.1.as_str(),
                "org.freedesktop.DBus.Properties",
            )
            .await?;
            proxy
                .call("Get", &("org.a11y.atspi.Text", "CaretOffset"))
                .await
        })?;
        Ok(i32::try_from(value)?)
    }
    fn set_caret(&self, reference: &Reference, index: i32) -> Result<bool> {
        let bus = self.a11y.as_ref().ok_or("AT-SPI")?;
        Ok(self.rt.block_on(async {
            let proxy = Proxy::new(
                bus,
                reference.0.as_str(),
                reference.1.as_str(),
                "org.a11y.atspi.Text",
            )
            .await?;
            proxy.call("SetCaretOffset", &(index,)).await
        })?)
    }
    fn reader_command(
        &self,
        path: &Path,
        command: &str,
        env_override: Option<&BTreeMap<String, String>>,
    ) -> Result<(i32, Value)> {
        let output = Command::new(path)
            .arg(command)
            .env_clear()
            .envs(env_override.unwrap_or(&self.env))
            .output()?;
        let value: Value = serde_json::from_slice(&output.stdout)?;
        Ok((output.status.code().unwrap_or(1), value))
    }
    fn reader_sway(&mut self, shortcut: Option<&Path>) -> Result<()> {
        let mut config =
            "output HEADLESS-1 resolution 1100x1000\nseat seat0 fallback true\n".to_string();
        if let Some(path) = shortcut {
            config.push_str(&format!(
                "bindsym Mod4+Control+F8 exec {} where-am-i > {}\n",
                path.display(),
                self.opt.output.join("shortcut-result.json").display()
            ));
        }
        let config_path = self.opt.output.join("sway.conf");
        fs::write(&config_path, config)?;
        self.launch(
            &[
                self.opt.sway.display().to_string(),
                "--unsupported-gpu".into(),
                "-c".into(),
                config_path.display().to_string(),
            ],
            "sway.log",
        )?;
        let runtime = self.runtime.clone();
        let socket = wait("private compositor", || {
            Ok(fs::read_dir(&runtime)?
                .filter_map(|entry| entry.ok())
                .map(|entry| entry.path())
                .find(|path| {
                    path.file_name()
                        .is_some_and(|v| v.to_string_lossy().starts_with("sway-ipc."))
                }))
        })?;
        self.env
            .insert("SWAYSOCK".into(), socket.display().to_string());
        let display = wait("private display", || {
            Ok(fs::read_dir(&runtime)?
                .filter_map(|entry| entry.ok())
                .map(|entry| entry.file_name().to_string_lossy().to_string())
                .find(|name| name.starts_with("wayland-") && !name.ends_with(".lock")))
        })?;
        self.env.insert("WAYLAND_DISPLAY".into(), display);
        self.launch(
            &["wtype".into(), "-s".into(), "30000".into()],
            "keyboard.log",
        )?;
        Ok(())
    }
    pub(super) fn screen_reader_input(&mut self) -> Result<()> {
        let live = Path::new(&format!("/run/user/{}/at-spi/bus_0", unsafe {
            libc::getuid()
        }))
        .to_path_buf();
        let live_before = fs::metadata(&live).ok().map(|v| v.ino());
        let tool = self.opt.extra("--speech-tool")?.canonicalize()?;
        let orca = self.opt.extra("--orca")?.canonicalize()?;
        let dispatcher = self.opt.extra("--dispatcher")?.canonicalize()?;
        let dispatcher_source = self.opt.extra("--dispatcher-source")?.canonicalize()?;
        let schema_compiler = self.opt.extra("--schema-compiler")?.canonicalize()?;
        let reader_command = self.opt.extras.get("--reader-command").map(PathBuf::from);
        let lock_client = self.opt.extras.get("--lock-client").map(PathBuf::from);
        ensure(
            tool.starts_with("/nix/store") && tool.is_file(),
            "Immutable speech tool required",
        )?;
        let proof: Value = serde_json::from_str(&self.call(&[
            tool.display().to_string(),
            "source-proof".into(),
            dispatcher_source.display().to_string(),
        ])?)?;
        save(&self.opt.output.join("source-proof.json"), &proof)?;
        self.launcher_env()?;
        let assets = PathBuf::from(
            self.env
                .get("LUMEN_DESKTOP_ASSETS")
                .ok_or("desktop assets")?,
        );
        ensure(
            assets.starts_with("/nix/store") && assets.join("shell/AppLauncher.qml").is_file(),
            "Immutable installed QML assets required",
        )?;
        let source = assets.join("shell");
        let palette: Value =
            serde_json::from_slice(&fs::read(assets.join("appearance/palette.json"))?)?;
        let appearance = Path::new(self.env.get("XDG_STATE_HOME").ok_or("state home")?)
            .join("lumen-desktop/appearance.json");
        fs::create_dir_all(appearance.parent().ok_or("appearance parent")?)?;
        save(
            &appearance,
            &json!({"font":palette["font"],"palette":palette["light"],"preferences":{}}),
        )?;
        for (key, value) in [
            ("QT_QPA_PLATFORM", "wayland"),
            ("QT_QPA_PLATFORMTHEME", "generic"),
            ("QT_ACCESSIBILITY", "1"),
            ("QT_LINUX_ACCESSIBILITY_ALWAYS_ON", "1"),
            ("GLOBAL_MENU_ENABLED", "0"),
            ("LUMEN_FILE_SERVICE", "0"),
            ("LUMEN_LAUNCH_SCOPES", "0"),
            ("WLR_BACKENDS", "headless"),
            ("WLR_LIBINPUT_NO_DEVICES", "1"),
            (
                "WLR_RENDERER",
                if self.opt.effects { "gles2" } else { "pixman" },
            ),
            ("WLR_RENDERER_ALLOW_SOFTWARE", "1"),
            ("WLR_RENDERER_FORCE_SOFTWARE", "1"),
            ("QT_QUICK_BACKEND", "software"),
            ("GST_REGISTRY_FORK", "no"),
            ("GSETTINGS_BACKEND", "memory"),
            ("GDK_BACKEND", "wayland"),
            ("XDG_SESSION_TYPE", "wayland"),
        ] {
            self.env.insert(key.into(), value.into());
        }
        for key in [
            "NO_AT_BRIDGE",
            "GTK_A11Y",
            "DISPLAY",
            "WAYLAND_DISPLAY",
            "SWAYSOCK",
        ] {
            self.env.remove(key);
        }
        self.env.insert(
            "BRLAPI_HOST".into(),
            self.runtime.join("absent-braille").display().to_string(),
        );
        self.env.insert(
            "PULSE_SERVER".into(),
            format!("unix:{}", self.runtime.join("absent-audio").display()),
        );
        self.env.insert(
            "ALSA_CONFIG_PATH".into(),
            self.runtime.join("absent-alsa").display().to_string(),
        );
        self.start_a11y()?;
        let (events, event_stop, event_handle) = self.capture_events()?;
        let speech: Value = serde_json::from_str(&self.call(&[
            tool.display().to_string(),
            "configure".into(),
            self.opt.output.display().to_string(),
            dispatcher.display().to_string(),
        ])?)?;
        self.env.insert(
            "SPEECHD_ADDRESS".into(),
            speech["address"].as_str().ok_or("speech address")?.into(),
        );
        let speech_command = speech["command"]
            .as_array()
            .ok_or("speech command")?
            .iter()
            .map(|value| {
                value
                    .as_str()
                    .ok_or_else(|| fail("speech command token"))
                    .map(str::to_string)
            })
            .collect::<Result<Vec<_>>>()?;
        let saved_path = self.env.insert(
            "PATH".into(),
            speech["emptyBin"]
                .as_str()
                .ok_or("empty speech PATH")?
                .into(),
        );
        self.launch(&speech_command, "speech.log")?;
        if let Some(path) = saved_path {
            self.env.insert("PATH".into(), path);
        }
        wait("private speech socket", || {
            Ok(Path::new(speech["socket"].as_str().ok_or("speech socket")?)
                .exists()
                .then_some(()))
        })?;
        let schemas = self.opt.output.join("schemas");
        fs::create_dir(&schemas)?;
        let xml = scan_xml(&orca)?;
        fs::copy(&xml, schemas.join(xml.file_name().ok_or("schema name")?))?;
        fs::write(
            schemas.join("fixture.gschema.override"),
            "[org.gnome.Orca.Braille]\nenabled=false\n[org.gnome.Orca.Sound]\nenabled=false\n[org.gnome.Orca.Speech]\nsynthesizer='observer'\n",
        )?;
        self.call(&[
            schema_compiler.display().to_string(),
            schemas.display().to_string(),
        ])?;
        self.env
            .insert("GSETTINGS_SCHEMA_DIR".into(), schemas.display().to_string());
        self.reader_sway(reader_command.as_deref())?;
        let fixture = self.qml(include_str!("../reader.qml"), &source)?;
        let installed = [
            "AppLauncher.qml",
            "NotificationCentre.qml",
            "NotificationCard.qml",
            "SessionOptions.qml",
            "StatusMessage.qml",
            "DialogButton.qml",
        ]
        .into_iter()
        .map(|name| {
            Ok((
                name.to_string(),
                json!({"path":source.join(name),"sha256":hash(&source.join(name))?}),
            ))
        })
        .collect::<Result<serde_json::Map<String, Value>>>()?;
        let command_proof = if let Some(path) = &reader_command {
            Some(json!({"path":path,"wrapperSha256":hash(path)?}))
        } else {
            None
        };
        save(
            &self.opt.output.join("runtime-manifest.json"),
            &json!({"launcher":self.opt.launcher,"launcherSha256":hash(&self.opt.launcher)?,"fixtureSha256":hash(&fixture)?,"installedComponents":installed,"providers":"Synthetic application/notification/lifecycle boundary only; production controls imported unchanged","reader":orca,"readerCommand":command_proof,"lockClient":lock_client,"atspi":self.opt.atspi,"dispatcher":dispatcher,"namespace":"Private PID, mount/proc, user and network namespaces"}),
        )?;
        let orca_pid = self.launch(
            &[
                orca.join("bin/orca").display().to_string(),
                "--speech-system=speechdispatcherfactory".into(),
                format!(
                    "--debug-file={}",
                    self.opt.output.join("orca-debug.log").display()
                ),
            ],
            "orca.log",
        )?;
        let session = self.connection(
            self.env
                .get("DBUS_SESSION_BUS_ADDRESS")
                .ok_or("session bus")?,
        )?;
        wait("real Orca event loop", || {
            let process = self
                .processes
                .iter_mut()
                .find(|p| p.0.id() == orca_pid)
                .ok_or("Orca process")?;
            ensure(
                process.0.try_wait()?.is_none(),
                "Orca exited before readiness",
            )?;
            if !self.name_owner(&session, "org.gnome.Orca.Service")? {
                return Ok(None);
            }
            let version: String = self.rt.block_on(async {
                let proxy = Proxy::new(
                    &session,
                    "org.gnome.Orca.Service",
                    "/org/gnome/Orca/Service",
                    "org.gnome.Orca.Service",
                )
                .await?;
                proxy.call("GetVersion", &()).await
            })?;
            Ok((version == "50.2").then_some(()))
        })?;
        let shell_pid = self.launch(
            &[
                "quickshell".into(),
                "-p".into(),
                fixture.display().to_string(),
            ],
            "shell.log",
        )?;
        let tree = wait("exact owned mapped window", || {
            let tree: Value = serde_json::from_str(&self.call(&[
                "swaymsg".into(),
                "-r".into(),
                "-t".into(),
                "get_tree".into(),
            ])?)?;
            let mut pending = vec![tree];
            while let Some(node) = pending.pop() {
                if node["pid"] == shell_pid
                    && node["name"] == "Reader fixture"
                    && node["rect"]["width"].as_i64().unwrap_or(0) > 0
                {
                    return Ok(Some(node));
                }
                for key in ["nodes", "floating_nodes"] {
                    if let Some(rows) = node[key].as_array() {
                        pending.extend(rows.iter().cloned())
                    }
                }
            }
            Ok(None)
        })?;
        self.call(&["swaymsg".into(), format!("[con_id={}] focus", tree["id"])])?;
        let protocol = PathBuf::from(speech["protocol"].as_str().ok_or("speech protocol")?);
        wait("actual owned window", || {
            Ok(self.reader_state(&fixture)?["visible"]
                .as_bool()
                .filter(|v| *v)
                .map(|_| ()))
        })?;
        self.reader_ipc(&fixture, "search")?;
        self.call(&["wtype".into(), "Sentinel".into()])?;
        wait("acknowledged physical query results", || {
            let state = self.reader_state(&fixture)?;
            Ok(
                (state["query"] == "Sentinel" && state["selectionQuery"] == "Sentinel")
                    .then_some(()),
            )
        })?;
        let a11y = self.a11y.as_ref().ok_or("AT-SPI")?.clone();
        let alpha = wait("production search result", || {
            self.find_named(&a11y, "Sentinel Alpha")
        })?;
        self.focus_ref(&alpha)?;
        wait("real reader first result speech", || {
            Ok(self
                .spoken(&tool, &protocol, "Sentinel Alpha", 0)?
                .then_some(()))
        })?;
        self.call(&["wtype".into(), "-k".into(), "Tab".into()])?;
        wait("real keyboard next result speech", || {
            Ok(self
                .spoken(&tool, &protocol, "Sentinel Beta", 0)?
                .then_some(()))
        })?;
        let mut checks =
            vec!["physical search focus speaks results in traversal order".to_string()];
        let query = wait("query accessible", || {
            self.find_named(
                &a11y,
                "Search applications, files, actions and clipboard history",
            )
        })?;
        let caret = self.text_caret(&query)?;
        let selected = self.reader_state(&fixture)?["selected"].clone();
        if let Some(command) = &reader_command {
            ensure(
                lock_client.is_some(),
                "Reader commands require a private lock peer",
            )?;
            let checkpoint = self.speech_messages(&tool, &protocol)?.len();
            self.call(&[
                "wtype".into(),
                "-M".into(),
                "logo".into(),
                "-M".into(),
                "ctrl".into(),
                "-k".into(),
                "F8".into(),
                "-m".into(),
                "ctrl".into(),
                "-m".into(),
                "logo".into(),
            ])?;
            let shortcut = self.opt.output.join("shortcut-result.json");
            wait("actual private consumer key dispatch", || {
                Ok(shortcut.metadata().ok().filter(|m| m.len() > 0).map(|_| ()))
            })?;
            ensure(
                serde_json::from_slice::<Value>(&fs::read(&shortcut)?)?["accepted"] == true,
                "Shortcut was not accepted",
            )?;
            wait("typed reader current object", || {
                Ok(self
                    .spoken(&tool, &protocol, "Sentinel Beta", checkpoint)?
                    .then_some(()))
            })?;
            let checkpoint = self.speech_messages(&tool, &protocol)?.len();
            self.focus_ref(&query)?;
            wait("reader focused owned text", || {
                Ok(self
                    .spoken(
                        &tool,
                        &protocol,
                        "Search applications, files, actions and clipboard history",
                        checkpoint,
                    )?
                    .then_some(()))
            })?;
            ensure(self.set_caret(&query, 0)?, "Could not set query caret")?;
            let checkpoint = self.speech_messages(&tool, &protocol)?.len();
            let (code, result) = self.reader_command(command, "read-all", None)?;
            ensure(
                code == 0 && result["accepted"] == true,
                "Read-all command failed",
            )?;
            wait("typed reader read-all", || {
                Ok(self
                    .spoken(&tool, &protocol, "Sentinel", checkpoint)?
                    .then_some(()))
            })?;
            let cancel_offset = fs::metadata(&protocol)?.len() as usize;
            let (code, result) = self.reader_command(command, "stop", None)?;
            ensure(
                code == 0 && result["accepted"] == true,
                "Stop command failed",
            )?;
            wait("typed reader cancel transport", || {
                Ok(String::from_utf8_lossy(&fs::read(&protocol)?)
                    .get(cancel_offset..)
                    .filter(|s| s.contains("CANCEL"))
                    .map(|_| ()))
            })?;
            ensure(
                self.set_caret(&query, caret)?,
                "Could not restore query caret",
            )?;
            let beta = wait("captured result after reader command", || {
                self.find_named(&a11y, "Sentinel Beta")
            })?;
            let checkpoint = self.speech_messages(&tool, &protocol)?.len();
            self.focus_ref(&beta)?;
            wait("restored production result focus", || {
                Ok(self
                    .spoken(&tool, &protocol, "Sentinel Beta", checkpoint)?
                    .then_some(()))
            })?;
            checks.push("typed owned reader commands produce actual current-object/read-all speech and cancel transport".into());
        }
        self.reader_ipc(&fixture, "update")?;
        wait("actual provider update", || {
            Ok(self.reader_state(&fixture)?["results"]
                .as_array()
                .is_some_and(|rows| rows.contains(&json!("fixture.gamma")))
                .then_some(()))
        })?;
        let state = self.reader_state(&fixture)?;
        ensure(
            state["query"] == "Sentinel" && state["selected"] == selected,
            "Provider update changed query or selection",
        )?;
        ensure(
            self.text_caret(&query)? == caret,
            "Provider update changed query caret",
        )?;
        checks.push("provider update preserves query caret and captured selected identity".into());
        self.reader_ipc(&fixture, "remove")?;
        wait("removed captured result", || {
            Ok(self.reader_state(&fixture)?["results"]
                .as_array()
                .is_some_and(|rows| !rows.contains(&json!("fixture.alpha")))
                .then_some(()))
        })?;
        let _ = self.action_ref(&alpha);
        ensure(
            self.reader_state(&fixture)?["launched"]
                .as_array()
                .is_some_and(|rows| rows.is_empty()),
            "Retired accessible result launched a replacement",
        )?;
        checks.push("retired accessible result cannot launch a replacement".into());
        let idle_start = self.speech_messages(&tool, &protocol)?.len();
        thread::sleep(Duration::from_secs(2));
        let idle = self.speech_messages(&tool, &protocol)?;
        ensure(
            idle.iter()
                .skip(idle_start)
                .filter(|message| {
                    message["plainText"]
                        .as_str()
                        .is_some_and(|v| v.contains("Sentinel"))
                })
                .count()
                <= 1,
            "Idle results repeat endlessly",
        )?;
        checks.push("settled search produces no repeated idle result stream".into());
        self.reader_ipc(&fixture, "notifications")?;
        let collapsed = wait("collapsed production notification", || {
            self.find_named(
                &a11y,
                "Sentinel Inbox, Sentinel First, 2 notifications. Expand stack",
            )
        })?;
        self.focus_ref(&collapsed)?;
        wait("collapsed stack speech", || {
            Ok(self
                .spoken(&tool, &protocol, "Expand stack", 0)?
                .then_some(()))
        })?;
        self.call(&["wtype".into(), "-k".into(), "Return".into()])?;
        let first = wait("expanded first notification", || {
            self.find_named(&a11y, "Sentinel Inbox, Sentinel First")
        })?;
        self.focus_ref(&first)?;
        wait("first notification speech", || {
            Ok(self
                .spoken(&tool, &protocol, "Sentinel First", 0)?
                .then_some(()))
        })?;
        wait("first notification body", || {
            Ok(self
                .spoken(&tool, &protocol, "Sentinel Orchard", 0)?
                .then_some(()))
        })?;
        let second = wait("expanded second notification", || {
            self.find_named(&a11y, "Sentinel Inbox, Sentinel Second")
        })?;
        self.focus_ref(&second)?;
        wait("second notification speech", || {
            Ok((self.spoken(&tool, &protocol, "Sentinel Second", 0)?
                && self.spoken(&tool, &protocol, "Sentinel Harbour", 0)?)
            .then_some(()))
        })?;
        checks.push(
            "production notification stack expands by keyboard and reader speaks ordered cards"
                .into(),
        );
        self.reader_ipc(&fixture, "restore")?;
        wait("fresh query on launcher return", || {
            Ok((self.reader_state(&fixture)?["query"] == "").then_some(()))
        })?;
        self.reader_ipc(&fixture, "modalShow")?;
        wait("production confirmation visible", || {
            Ok((self.reader_state(&fixture)?["modal"] == true).then_some(()))
        })?;
        wait("modal context before initial cancel speech", || {
            Ok((self.spoken(&tool, &protocol, "Restart confirmation", 0)?
                && self.spoken(&tool, &protocol, "Cancel", 0)?)
            .then_some(()))
        })?;
        self.call(&["wtype".into(), "-k".into(), "Tab".into()])?;
        wait("modal keyboard continue speech", || {
            Ok(self.spoken(&tool, &protocol, "Continue", 0)?.then_some(()))
        })?;
        self.call(&["wtype".into(), "-k".into(), "Return".into()])?;
        wait("real actionable modal error", || {
            Ok(self.reader_state(&fixture)?["error"]
                .as_str()
                .is_some_and(|v| !v.is_empty())
                .then_some(()))
        })?;
        wait("modal actionable error speech", || {
            Ok(self
                .spoken(
                    &tool,
                    &protocol,
                    "Application observation is unavailable",
                    0,
                )?
                .then_some(()))
        })?;
        self.call(&["wtype".into(), "-k".into(), "Return".into()])?;
        thread::sleep(Duration::from_secs(2));
        let error_messages = self
            .speech_messages(&tool, &protocol)?
            .iter()
            .filter(|message| {
                message["plainText"]
                    .as_str()
                    .is_some_and(|v| v.contains("Application observation is unavailable"))
            })
            .count();
        ensure(
            error_messages == 1,
            "Unchanged error was repeatedly announced",
        )?;
        let continuation = wait("original continuation control", || {
            self.find_named(&a11y, "Continue")
        })?;
        ensure(
            self.state_has(&continuation, 12)?,
            "Announcement stole keyboard focus",
        )?;
        checks
            .push("actionable error is announced once without focus steal or idle repeats".into());
        let return_checkpoint = self.speech_messages(&tool, &protocol)?.len();
        self.call(&["wtype".into(), "-k".into(), "Escape".into()])?;
        wait("physical modal cancel", || {
            Ok((self.reader_state(&fixture)?["modal"] == false).then_some(()))
        })?;
        wait("actual opener focus return", || {
            Ok(self.state_has(&query, 12)?.then_some(()))
        })?;
        wait("reader speaks restored opener", || {
            Ok(self
                .spoken(
                    &tool,
                    &protocol,
                    "Search applications, files, actions and clipboard history",
                    return_checkpoint,
                )?
                .then_some(()))
        })?;
        let state = self.reader_state(&fixture)?;
        ensure(
            state["launched"]
                .as_array()
                .is_some_and(|rows| rows.is_empty())
                && state["finalActions"]
                    .as_array()
                    .is_some_and(|rows| rows.is_empty()),
            "Modal caused an unintended action",
        )?;
        checks.push("production modal context error and keyboard cancellation are spoken without final action".into());
        if let Some(command) = &reader_command {
            let lock = lock_client.as_ref().ok_or("Lock peer required")?;
            let log = File::create(self.opt.output.join("command-lock-peer.log"))?;
            let mut process = Command::new(lock);
            process
                .env_clear()
                .envs(&self.env)
                .stdin(Stdio::piped())
                .stdout(Stdio::from(log.try_clone()?))
                .stderr(Stdio::from(log));
            unsafe {
                process.pre_exec(|| {
                    libc::setsid();
                    Ok(())
                });
            }
            let child = process.spawn()?;
            let lock_pid = child.id();
            self.processes.push(ChildProcess(child));
            wait("actual native locked authority", || {
                let value: Value = serde_json::from_str(&self.call(&[
                    "swaymsg".into(),
                    "-r".into(),
                    "-t".into(),
                    "get_version".into(),
                ])?)?;
                Ok((value["lumen_session_lock"]["locked"] == true).then_some(()))
            })?;
            for action in ["read-all", "where-am-i"] {
                let (code, result) = self.reader_command(command, action, None)?;
                ensure(
                    code == 1 && result["reason"] == "session_unavailable",
                    "Locked compositor disclosed reader content",
                )?;
            }
            let (code, result) = self.reader_command(command, "stop", None)?;
            ensure(
                code == 0 && result["accepted"] == true,
                "Locked stop failed",
            )?;
            checks.push(
                "actual locked compositor denies reader content commands while allowing safe stop"
                    .into(),
            );
            let peer = self
                .processes
                .iter_mut()
                .find(|p| p.0.id() == lock_pid)
                .ok_or("Lock peer missing")?;
            peer.0
                .stdin
                .as_mut()
                .ok_or("Lock peer stdin")?
                .write_all(b"u\n")?;
            wait("actual native unlock", || {
                let value: Value = serde_json::from_str(&self.call(&[
                    "swaymsg".into(),
                    "-r".into(),
                    "-t".into(),
                    "get_version".into(),
                ])?)?;
                Ok((value["lumen_session_lock"]["locked"] == false).then_some(()))
            })?;
            let mut missing = self.env.clone();
            missing.insert(
                "SWAYSOCK".into(),
                self.runtime
                    .join("missing-compositor.sock")
                    .display()
                    .to_string(),
            );
            let (code, result) = self.reader_command(command, "where-am-i", Some(&missing))?;
            ensure(
                code == 1 && result["reason"] == "session_unavailable",
                "Disconnected compositor disclosed reader content",
            )?;
            let (code, result) = self.reader_command(command, "stop", Some(&missing))?;
            ensure(
                code == 0 && result["accepted"] == true,
                "Disconnected stop failed",
            )?;
            checks.push(
                "disconnected compositor fails closed for reading but preserves safe owned stop"
                    .into(),
            );
        }
        unsafe {
            libc::killpg(orca_pid as i32, libc::SIGTERM);
        }
        let utterances = self.utterances(&tool)?;
        save(
            &self.opt.output.join("initial-speech.json"),
            &json!({"state":self.reader_state(&fixture)?,"utterances":utterances,"events":events.lock().map_err(|_| "Event lock")?.clone()}),
        )?;
        ensure(
            utterances.iter().any(|v| v.contains("Sentinel Alpha"))
                && utterances.iter().any(|v| v.contains("Sentinel Beta")),
            "No owned result speech",
        )?;
        checks.push("real Orca speech from production owned search".into());
        let accepted = self.speech_messages(&tool, &protocol)?;
        let ordered = [
            "Sentinel Alpha",
            "Sentinel Beta",
            "Expand stack",
            "Sentinel Orchard",
            "Sentinel Harbour",
            "Restart confirmation",
            "Cancel",
            "Continue",
            "Application observation is unavailable",
        ];
        let positions: Vec<usize> = ordered
            .iter()
            .map(|needle| {
                accepted
                    .iter()
                    .position(|message| {
                        message["plainText"]
                            .as_str()
                            .is_some_and(|v| v.contains(needle))
                            && message["events"]
                                .as_array()
                                .is_some_and(|items| items.contains(&json!("BEGIN")))
                    })
                    .ok_or_else(|| fail(format!("Missing begun speech: {needle}")))
            })
            .collect::<Result<_>>()?;
        ensure(
            positions.windows(2).all(|v| v[0] <= v[1]),
            "Reader transcript out of order",
        )?;
        wait("reader transport completed selected utterances", || {
            let messages = self.speech_messages(&tool, &protocol)?;
            Ok(positions
                .iter()
                .all(|index| {
                    let id = &accepted[*index]["id"];
                    messages
                        .iter()
                        .filter(|message| &message["id"] == id)
                        .all(|message| {
                            message["events"].as_array().is_some_and(|v| {
                                v.contains(&json!("END")) || v.contains(&json!("CANCELED"))
                            })
                        })
                })
                .then_some(()))
        })?;
        event_stop.store(true, Ordering::Relaxed);
        event_handle.join().map_err(|_| "Event capture panicked")?;
        let captured = events.lock().map_err(|_| "Event lock")?.clone();
        save(&self.opt.output.join("events.json"), &json!(captured))?;
        ensure(
            captured.iter().any(|event| {
                event["member"] == "Announcement"
                    && event["arguments"]
                        .as_str()
                        .is_some_and(|v| v.contains("Application observation is unavailable"))
            }),
            "Timestamped announcement event missing",
        )?;
        checks.push("ordered reader transcript has real accepted begin/end speech and timestamped announcement event".into());
        save(
            &self.opt.output.join("transcript.json"),
            &json!(self.speech_messages(&tool, &protocol)?),
        )?;
        save(
            &self.opt.output.join("final-state.json"),
            &self.reader_state(&fixture)?,
        )?;
        save(
            &self.opt.output.join("result.json"),
            &json!({"passed":true,"checks":checks,"utterances":utterances,"orderedSentinels":ordered,"positions":positions,"events":captured}),
        )?;
        self.call(&[
            tool.display().to_string(),
            "retire".into(),
            self.opt.output.display().to_string(),
        ])?;
        let live_after = fs::metadata(&live).ok().map(|v| v.ino());
        ensure(
            live_before == live_after,
            "Live accessibility socket changed",
        )?;
        Ok(())
    }
}
