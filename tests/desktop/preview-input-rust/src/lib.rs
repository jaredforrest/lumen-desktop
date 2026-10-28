//! Shared private Wayland session and RGB evidence helpers for display fixtures.
use png::{BitDepth, ColorType, Decoder, Encoder, Transformations};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::env;
use std::error::Error;
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::{PermissionsExt, symlink};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};
use tempfile::TempDir;
use walkdir::WalkDir;

pub type Result<T> = std::result::Result<T, Box<dyn Error>>;
pub fn fail(message: impl Into<String>) -> Box<dyn Error> {
    message.into().into()
}
pub fn ensure(ok: bool, message: impl Into<String>) -> Result<()> {
    if ok { Ok(()) } else { Err(fail(message)) }
}
pub fn s(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}
pub fn pause(ms: u64) {
    thread::sleep(Duration::from_millis(ms));
}
pub fn number(v: &Value, key: &str) -> Result<f64> {
    v[key]
        .as_f64()
        .ok_or_else(|| fail(format!("Missing numeric {key}: {v}")))
}
pub fn integer(v: &Value, key: &str) -> Result<i64> {
    number(v, key).map(|v| v as i64)
}
pub fn round(v: f64) -> i64 {
    v.round_ties_even() as i64
}
pub fn wait<T>(
    name: &str,
    seconds: u64,
    mut probe: impl FnMut() -> Result<Option<T>>,
) -> Result<T> {
    let until = Instant::now() + Duration::from_secs(seconds);
    let mut last = String::new();
    loop {
        match probe() {
            Ok(Some(v)) => return Ok(v),
            Ok(None) => {}
            Err(e) => last = e.to_string(),
        }
        if Instant::now() >= until {
            return Err(fail(format!("Timed out waiting for {name}: {last}")));
        }
        pause(50)
    }
}
pub fn parse_env0(bytes: &[u8]) -> BTreeMap<String, String> {
    bytes
        .split(|v| *v == 0)
        .filter_map(|part| {
            part.iter().position(|v| *v == b'=').map(|at| {
                (
                    String::from_utf8_lossy(&part[..at]).into_owned(),
                    String::from_utf8_lossy(&part[at + 1..]).into_owned(),
                )
            })
        })
        .collect()
}
pub fn copy_tree(from: &Path, to: &Path) -> Result<()> {
    fs::create_dir_all(to)?;
    for item in WalkDir::new(from).follow_links(false).min_depth(1) {
        let item = item?;
        let dest = to.join(item.path().strip_prefix(from)?);
        if item.file_type().is_dir() {
            fs::create_dir_all(&dest)?;
        } else if item.file_type().is_symlink() {
            symlink(fs::read_link(item.path())?, &dest)?;
        } else {
            fs::copy(item.path(), &dest)?;
        }
    }
    Ok(())
}
pub fn sha_file(path: &Path) -> Result<String> {
    let mut hash = Sha256::new();
    hash.update(fs::read(path)?);
    Ok(format!("{:x}", hash.finalize()))
}
pub fn sha_bytes(bytes: &[u8]) -> String {
    let mut hash = Sha256::new();
    hash.update(bytes);
    format!("{:x}", hash.finalize())
}
pub fn sha_assets(root: &Path) -> Result<Value> {
    let mut files = WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_map(|v| v.ok())
        .filter(|v| v.file_type().is_file())
        .map(|v| v.path().to_owned())
        .collect::<Vec<_>>();
    files.sort();
    let mut map = serde_json::Map::new();
    for path in files {
        map.insert(s(path.strip_prefix(root)?), json!(sha_file(&path)?));
    }
    Ok(Value::Object(map))
}
fn call_with_env(
    program: &str,
    args: &[&str],
    values: &BTreeMap<String, String>,
) -> Result<Vec<u8>> {
    let result = Command::new("timeout")
        .arg("10s")
        .arg(program)
        .args(args)
        .env_clear()
        .envs(values)
        .output()?;
    ensure(
        result.status.success(),
        format!(
            "{program} {args:?} failed: {}",
            String::from_utf8_lossy(&result.stderr)
        ),
    )?;
    Ok(result.stdout)
}
struct Managed {
    child: Child,
    argv: Vec<String>,
}
pub struct Session {
    pub out: PathBuf,
    pub env: BTreeMap<String, String>,
    pub root: TempDir,
    pub sway: PathBuf,
    children: Vec<Managed>,
}
impl Session {
    pub fn child_count(&self) -> usize {
        self.children.len()
    }
    pub fn child_pid(&self, index: usize) -> u32 {
        self.children[index].child.id()
    }
    pub fn existing(output: &Path, launcher: &Path, sway: &Path, prefix: &str) -> Result<Self> {
        let scratch = output.join(".native-input-init");
        let mut session = Self::new(&scratch, launcher, sway, prefix)?;
        fs::remove_dir(&scratch)?;
        session.out = fs::canonicalize(output)?;
        Ok(session)
    }
    pub fn new(output: &Path, launcher: &Path, sway: &Path, prefix: &str) -> Result<Self> {
        ensure(
            !output.exists(),
            format!("Fresh output directory required: {}", output.display()),
        )?;
        fs::create_dir_all(output)?;
        let out = fs::canonicalize(output)?;
        let root = tempfile::Builder::new().prefix(prefix).tempdir()?;
        fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700))?;
        let mut values = env::vars().collect::<BTreeMap<_, _>>();
        values.insert("HOME".into(), s(root.path()));
        values.insert("XDG_RUNTIME_DIR".into(), s(root.path()));
        for key in ["CONFIG", "STATE", "CACHE", "DATA"] {
            values.insert(
                format!("XDG_{key}_HOME"),
                s(&root.path().join(key.to_lowercase())),
            );
        }
        for key in [
            "DISPLAY",
            "WAYLAND_DISPLAY",
            "SWAYSOCK",
            "DBUS_SESSION_BUS_ADDRESS",
            "AT_SPI_BUS_ADDRESS",
            "PULSE_SERVER",
            "PIPEWIRE_REMOTE",
        ] {
            values.remove(key);
        }
        let printed = call_with_env(&s(launcher), &["--print-env0"], &values)?;
        values.extend(parse_env0(&printed));
        // The launcher's packaged PATH is retained; all writable state stays private.
        values.insert("HOME".into(), s(root.path()));
        values.insert("XDG_RUNTIME_DIR".into(), s(root.path()));
        for key in ["CONFIG", "STATE", "CACHE", "DATA"] {
            values.insert(
                format!("XDG_{key}_HOME"),
                s(&root.path().join(key.to_lowercase())),
            );
        }
        for key in [
            "DISPLAY",
            "WAYLAND_DISPLAY",
            "SWAYSOCK",
            "DBUS_SESSION_BUS_ADDRESS",
            "AT_SPI_BUS_ADDRESS",
            "PULSE_SERVER",
            "PIPEWIRE_REMOTE",
        ] {
            values.remove(key);
        }
        Ok(Self {
            out,
            env: values,
            root,
            sway: sway.to_owned(),
            children: Vec::new(),
        })
    }
    pub fn set(&mut self, key: &str, value: impl Into<String>) {
        self.env.insert(key.into(), value.into());
    }
    pub fn set_many(&mut self, pairs: &[(&str, &str)]) {
        for (key, value) in pairs {
            self.set(key, *value);
        }
    }
    pub fn call(&self, program: &str, args: &[&str]) -> Result<String> {
        Ok(String::from_utf8(call_with_env(program, args, &self.env)?)?
            .trim()
            .to_owned())
    }
    pub fn swaymsg(&self, args: &[&str]) -> Result<Value> {
        let executable = self
            .sway
            .parent()
            .map(|p| p.join("swaymsg"))
            .filter(|p| p.exists())
            .unwrap_or_else(|| PathBuf::from("swaymsg"));
        serde_json::from_str(&self.call(&s(&executable), args)?).map_err(Into::into)
    }
    pub fn ipc(&self, command: &str) -> Result<()> {
        let result = self.swaymsg(&[command])?;
        ensure(
            result
                .as_array()
                .is_some_and(|rows| rows.iter().all(|v| v["success"] == true)),
            format!("Sway rejected {command}: {result}"),
        )
    }
    pub fn launch(
        &mut self,
        program: &str,
        args: &[&str],
        log: &str,
        interactive: bool,
    ) -> Result<usize> {
        let file = File::create(self.out.join(log))?;
        let child = Command::new(program)
            .args(args)
            .env_clear()
            .envs(&self.env)
            .stdin(if interactive {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::from(file.try_clone()?))
            .stderr(Stdio::from(file))
            .process_group(0)
            .spawn()?;
        let mut argv = vec![program.to_owned()];
        argv.extend(args.iter().map(|v| (*v).to_owned()));
        self.children.push(Managed { child, argv });
        Ok(self.children.len() - 1)
    }
    pub fn write(&mut self, index: usize, value: &str) -> Result<()> {
        let stream = self.children[index]
            .child
            .stdin
            .as_mut()
            .ok_or("Interactive stdin missing")?;
        stream.write_all(value.as_bytes())?;
        stream.flush()?;
        Ok(())
    }
    pub fn process_status(&mut self) -> Value {
        Value::Array(self.children.iter_mut().map(|m|json!({"argv":m.argv,"returncode":m.child.try_wait().ok().flatten().and_then(|s|s.code())})).collect())
    }
    pub fn start_bus(&mut self) -> Result<()> {
        let runtime = self.root.path().to_owned();
        self.set(
            "DBUS_SYSTEM_BUS_ADDRESS",
            format!("unix:path={}", runtime.join("no-system-bus").display()),
        );
        let config = self.out.join("bus.conf");
        fs::write(
            &config,
            format!(
                "<busconfig><type>session</type><listen>unix:tmpdir={}</listen><policy context=\"default\"><allow own=\"*\"/><allow send_destination=\"*\"/><allow receive_sender=\"*\"/></policy></busconfig>",
                runtime.display()
            ),
        )?;
        let log = File::create(self.out.join("bus.log"))?;
        let mut child = Command::new("dbus-daemon")
            .arg("--nofork")
            .arg("--print-address=1")
            .arg(format!("--config-file={}", config.display()))
            .env_clear()
            .envs(&self.env)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::from(log))
            .process_group(0)
            .spawn()?;
        let mut address = String::new();
        BufReader::new(child.stdout.take().ok_or("Private bus stdout missing")?)
            .read_line(&mut address)?;
        ensure(
            address.starts_with("unix:"),
            format!("Private bus did not publish address: {address}"),
        )?;
        self.set("DBUS_SESSION_BUS_ADDRESS", address.trim().to_owned());
        self.children.push(Managed {
            child,
            argv: vec!["dbus-daemon".into(), "--nofork".into()],
        });
        Ok(())
    }
    pub fn wait_sway(&mut self, expected_outputs: usize) -> Result<()> {
        let socket = wait("Sway socket", 15, || {
            Ok(fs::read_dir(self.root.path())?
                .filter_map(|v| v.ok().map(|e| e.path()))
                .find(|p| {
                    p.file_name().is_some_and(|v| {
                        v.to_string_lossy().starts_with("sway-ipc")
                            && v.to_string_lossy().ends_with(".sock")
                    })
                }))
        })?;
        self.set("SWAYSOCK", s(&socket));
        wait("Sway output count", 15, || {
            Ok((self.outputs()?.len() == expected_outputs).then_some(()))
        })?;
        let wayland = wait("Wayland socket", 15, || {
            Ok(fs::read_dir(self.root.path())?
                .filter_map(|v| v.ok().map(|e| e.path()))
                .find(|p| {
                    p.file_name().is_some_and(|v| {
                        let n = v.to_string_lossy();
                        n.starts_with("wayland-") && !n.ends_with(".lock")
                    })
                }))
        })?;
        self.set(
            "WAYLAND_DISPLAY",
            wayland.file_name().unwrap().to_string_lossy().into_owned(),
        );
        Ok(())
    }
    pub fn tree(&self) -> Result<Value> {
        self.swaymsg(&["-t", "get_tree", "-r"])
    }
    pub fn outputs(&self) -> Result<Vec<Value>> {
        Ok(self
            .swaymsg(&["-t", "get_outputs", "-r"])?
            .as_array()
            .ok_or("Output list missing")?
            .iter()
            .filter(|v| v["active"] == true)
            .cloned()
            .collect())
    }
    pub fn window(&self, title: &str) -> Result<Option<Value>> {
        fn walk(node: &Value, title: &str) -> Option<Value> {
            if node["name"] == title && node["app_id"].as_str().is_some() {
                return Some(node.clone());
            }
            for key in ["nodes", "floating_nodes"] {
                if let Some(children) = node[key].as_array() {
                    for child in children {
                        if let Some(found) = walk(child, title) {
                            return Some(found);
                        }
                    }
                }
            }
            None
        }
        Ok(walk(&self.tree()?, title))
    }
    pub fn capture(&self, path: &Path, args: &[&str]) -> Result<()> {
        let mut values = args.to_vec();
        let target = s(path);
        values.push(&target);
        self.call("grim", &values)?;
        Ok(())
    }
}
impl Drop for Session {
    fn drop(&mut self) {
        for managed in self.children.iter_mut().rev() {
            if managed.child.try_wait().ok().flatten().is_none() {
                unsafe {
                    libc::killpg(managed.child.id() as i32, libc::SIGTERM);
                }
            }
        }
        for managed in self.children.iter_mut().rev() {
            let until = Instant::now() + Duration::from_secs(5);
            while managed.child.try_wait().ok().flatten().is_none() {
                if Instant::now() >= until {
                    unsafe {
                        libc::killpg(managed.child.id() as i32, libc::SIGKILL);
                    }
                    let _ = managed.child.wait();
                    break;
                }
                pause(20);
            }
        }
    }
}
pub struct Raster {
    pub width: i64,
    pub height: i64,
    pub pixels: Vec<u8>,
}
impl Raster {
    pub fn read(path: &Path) -> Result<Self> {
        let mut decoder = Decoder::new(BufReader::new(File::open(path)?));
        decoder.set_transformations(Transformations::EXPAND | Transformations::STRIP_16);
        let mut reader = decoder.read_info()?;
        let mut data = vec![0; reader.output_buffer_size()];
        let info = reader.next_frame(&mut data)?;
        let channels = match info.color_type {
            ColorType::Rgb => 3,
            ColorType::Rgba => 4,
            ColorType::Grayscale => 1,
            ColorType::GrayscaleAlpha => 2,
            _ => return Err(fail("Unsupported PNG color type")),
        };
        let mut pixels = Vec::with_capacity((info.width * info.height * 3) as usize);
        for pixel in data[..info.buffer_size()].chunks_exact(channels) {
            if channels <= 2 {
                pixels.extend_from_slice(&[pixel[0], pixel[0], pixel[0]])
            } else {
                pixels.extend_from_slice(&pixel[..3])
            }
        }
        Ok(Self {
            width: info.width as i64,
            height: info.height as i64,
            pixels,
        })
    }
    pub fn pixel(&self, x: i64, y: i64) -> [u8; 3] {
        if x < 0 || y < 0 || x >= self.width || y >= self.height {
            return [0; 3];
        }
        let at = ((y * self.width + x) * 3) as usize;
        [self.pixels[at], self.pixels[at + 1], self.pixels[at + 2]]
    }
    pub fn crop(&self, x0: i64, y0: i64, x1: i64, y1: i64) -> Self {
        let width = (x1 - x0).max(0);
        let height = (y1 - y0).max(0);
        let mut pixels = Vec::with_capacity((width * height * 3) as usize);
        for y in 0..height {
            for x in 0..width {
                pixels.extend_from_slice(&self.pixel(x0 + x, y0 + y));
            }
        }
        Self {
            width,
            height,
            pixels,
        }
    }
    pub fn save(&self, path: &Path) -> Result<()> {
        let mut encoder = Encoder::new(File::create(path)?, self.width as u32, self.height as u32);
        encoder.set_color(ColorType::Rgb);
        encoder.set_depth(BitDepth::Eight);
        encoder.write_header()?.write_image_data(&self.pixels)?;
        Ok(())
    }
    pub fn deviation(&self) -> [f64; 3] {
        let count = (self.width * self.height) as f64;
        if count == 0.0 {
            return [0.0; 3];
        }
        let (mut sum, mut sq) = ([0.0; 3], [0.0; 3]);
        for p in self.pixels.chunks_exact(3) {
            for c in 0..3 {
                let v = p[c] as f64;
                sum[c] += v;
                sq[c] += v * v
            }
        }
        let mut out = [0.0; 3];
        for c in 0..3 {
            out[c] = (sq[c] / count - (sum[c] / count).powi(2)).max(0.0).sqrt()
        }
        out
    }
    pub fn each(&self) -> impl Iterator<Item = [u8; 3]> + '_ {
        self.pixels.chunks_exact(3).map(|p| [p[0], p[1], p[2]])
    }
}
