//! A disposable real PipeWire graph used only inside the application boundary.
use serde_json::Value;
use std::collections::BTreeMap;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

const CONFIG: &str = r#"context.properties = { core.daemon = true core.name = capture-fixture support.dbus = false }
context.spa-libs = { audio.convert.* = audioconvert/libspa-audioconvert audio.adapt = audioconvert/libspa-audioconvert support.* = support/libspa-support audiotestsrc = audiotestsrc/libspa-audiotestsrc }
context.modules = [ { name = libpipewire-module-protocol-native } { name = libpipewire-module-metadata } { name = libpipewire-module-spa-node-factory } { name = libpipewire-module-client-node } { name = libpipewire-module-access } { name = libpipewire-module-adapter } { name = libpipewire-module-link-factory } ]
context.objects = [ { factory = spa-node-factory args = { factory.name = support.node.driver node.name = Synthetic-Driver node.group = pipewire.dummy priority.driver = 20000 } } ]
"#;

pub struct Graph {
    bin: PathBuf,
    runtime: PathBuf,
    output: PathBuf,
    env: BTreeMap<String, String>,
    child: Option<Child>,
    source: String,
    target: String,
}

impl Graph {
    pub fn new(
        bin: &Path,
        runtime: &Path,
        output: &Path,
        mut env: BTreeMap<String, String>,
    ) -> Self {
        env.insert("PIPEWIRE_REMOTE".into(), "capture-fixture".into());
        env.insert("PIPEWIRE_RUNTIME_DIR".into(), runtime.display().to_string());
        Self {
            bin: bin.into(),
            runtime: runtime.into(),
            output: output.into(),
            env,
            child: None,
            source: String::new(),
            target: String::new(),
        }
    }
    fn run(&self, binary: &str, args: &[&str]) -> Result<String> {
        let result = Command::new("timeout")
            .arg("5s")
            .arg(self.bin.join(binary))
            .args(args)
            .env_clear()
            .envs(&self.env)
            .output()?;
        if !result.status.success() {
            return Err(format!(
                "{binary} {args:?}: {}",
                String::from_utf8_lossy(&result.stderr)
            )
            .into());
        }
        Ok(String::from_utf8(result.stdout)?.trim().to_owned())
    }
    pub fn start(&mut self) -> Result<()> {
        let config = self.output.join("pipewire.conf");
        std::fs::write(&config, CONFIG)?;
        let log = File::create(self.output.join("pipewire.log"))?;
        self.child = Some(
            Command::new(self.bin.join("pipewire"))
                .args(["-c", &config.display().to_string()])
                .env_clear()
                .envs(&self.env)
                .stdout(Stdio::from(log.try_clone()?))
                .stderr(Stdio::from(log))
                .spawn()?,
        );
        let deadline = Instant::now() + Duration::from_secs(8);
        while !self.runtime.join("capture-fixture").exists() {
            if self
                .child
                .as_mut()
                .is_some_and(|child| child.try_wait().ok().flatten().is_some())
            {
                return Err("Private PipeWire exited".into());
            }
            if Instant::now() > deadline {
                return Err("Private PipeWire socket did not appear".into());
            }
            thread::sleep(Duration::from_millis(20));
        }
        Ok(())
    }
    pub fn prepare(&mut self) -> Result<()> {
        self.run("pw-cli", &["create-node", "adapter", "{ factory.name = audiotestsrc node.name = synthetic-source media.class = Audio/Source audio.position = [ MONO ] adapter.auto-port-config = { mode = dsp monitor = false position = preserve } object.linger = true }"])?;
        self.run("pw-cli", &["create-node", "adapter", "{ factory.name = support.null-audio-sink node.name = synthetic-capture media.class = Stream/Input/Audio audio.position = [ MONO ] adapter.auto-port-config = { mode = dsp monitor = false position = preserve } object.linger = true }"])?;
        let graph: Value = serde_json::from_str(&self.run("pw-dump", &["--no-colors"])?)?;
        let source = graph
            .as_array()
            .ok_or("PipeWire dump is not an array")?
            .iter()
            .find(|node| node["info"]["props"]["node.name"] == "synthetic-source")
            .and_then(|node| node["id"].as_u64())
            .ok_or("Synthetic source node absent")?;
        self.run("pw-cli", &["set-param", &source.to_string(), "PortConfig", "{ direction = Output mode = dsp format = { mediaType = audio mediaSubtype = raw format = F32 rate = 48000 channels = 1 position = [ MONO ] } }"])?;
        self.source = self
            .run("pw-link", &["-o"])?
            .lines()
            .find(|line| line.starts_with("synthetic-source:"))
            .ok_or("Synthetic source port absent")?
            .into();
        self.target = self
            .run("pw-link", &["-i"])?
            .lines()
            .find(|line| line.starts_with("synthetic-capture:"))
            .ok_or("Synthetic capture port absent")?
            .into();
        Ok(())
    }
    pub fn link(&self) -> Result<()> {
        self.run("pw-link", &[&self.source, &self.target])?;
        Ok(())
    }
    pub fn unlink(&self) -> Result<()> {
        self.run("pw-link", &["-d", &self.source, &self.target])?;
        Ok(())
    }
    pub fn close(&mut self) {
        if let Some(mut child) = self.child.take() {
            if child.try_wait().ok().flatten().is_none() {
                let _ = child.kill();
            }
            let _ = child.wait();
        }
    }
}
impl Drop for Graph {
    fn drop(&mut self) {
        self.close();
    }
}
