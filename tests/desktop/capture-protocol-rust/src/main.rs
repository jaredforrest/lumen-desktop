//! Real PipeWire capture observation in the disposable application boundary.
use serde_json::{Value, json};
use std::env;
use std::error::Error;
use std::fs::{self, File};
use std::io::Write;
use std::os::unix::fs::{FileTypeExt, PermissionsExt};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

type Result<T> = std::result::Result<T, Box<dyn Error>>;
const CONFIG: &str = "context.properties = { core.daemon = true core.name = capture-fixture support.dbus = false }\n\
context.spa-libs = { audio.convert.* = audioconvert/libspa-audioconvert audio.adapt = audioconvert/libspa-audioconvert support.* = support/libspa-support audiotestsrc = audiotestsrc/libspa-audiotestsrc }\n\
context.modules = [ { name = libpipewire-module-protocol-native } { name = libpipewire-module-metadata } { name = libpipewire-module-spa-node-factory } { name = libpipewire-module-client-node } { name = libpipewire-module-access } { name = libpipewire-module-adapter } { name = libpipewire-module-link-factory } ]\n\
context.objects = [ { factory = spa-node-factory args = { factory.name = support.node.driver node.name = Synthetic-Driver node.group = pipewire.dummy priority.driver = 20000 } } ]\n";

fn fail(message: impl Into<String>) -> Box<dyn Error> {
    message.into().into()
}
fn require(condition: bool, message: &str) -> Result<()> {
    if condition {
        Ok(())
    } else {
        Err(fail(message))
    }
}
fn executable(name: &str) -> Result<PathBuf> {
    env::split_paths(&env::var_os("PATH").ok_or("PATH missing")?)
        .map(|directory| directory.join(name))
        .find(|candidate| candidate.is_file())
        .ok_or_else(|| fail(format!("Missing executable: {name}")))
}
fn immutable(path: &Path, label: &str) -> Result<PathBuf> {
    let canonical = fs::canonicalize(path)?;
    require(
        canonical.starts_with("/nix/store"),
        &format!("Pinned immutable {label} required"),
    )?;
    Ok(canonical)
}

struct Options {
    pipewire: PathBuf,
    observer: PathBuf,
    output: Option<PathBuf>,
    parent_net: Option<String>,
    inside: bool,
}
fn options() -> Result<Options> {
    let mut pipewire = None;
    let mut observer = None;
    let mut output = None;
    let mut parent_net = None;
    let mut inside = false;
    let mut args = env::args_os().skip(1);
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("--pipewire") => {
                pipewire = Some(PathBuf::from(args.next().ok_or("Missing PipeWire path")?))
            }
            Some("--observer") => {
                observer = Some(PathBuf::from(args.next().ok_or("Missing observer path")?))
            }
            Some("--output") => {
                output = Some(PathBuf::from(args.next().ok_or("Missing output path")?))
            }
            Some("--parent-net") => {
                parent_net = Some(
                    args.next()
                        .ok_or("Missing parent namespace")?
                        .to_string_lossy()
                        .into_owned(),
                )
            }
            Some("--inside") => inside = true,
            _ => return Err(fail("Unknown capture protocol option")),
        }
    }
    Ok(Options {
        pipewire: pipewire.ok_or("--pipewire is required")?,
        observer: match observer {
            Some(path) => path,
            None => executable("lumen-capture-observation")?,
        },
        output,
        parent_net,
        inside,
    })
}

fn outer(options: &Options) -> Result<()> {
    let output = options.output.as_ref().ok_or("--output is required")?;
    fs::create_dir(output)?;
    fs::set_permissions(output, fs::Permissions::from_mode(0o700))?;
    let output = fs::canonicalize(output)?;
    let pipewire = immutable(&options.pipewire, "PipeWire tools")?;
    for tool in ["pipewire", "pw-dump", "pw-cli", "pw-link"] {
        require(
            pipewire.join(tool).is_file(),
            "Pinned PipeWire tool missing",
        )?;
    }
    let observer = immutable(&options.observer, "Rust capture observer")?;
    let boundary = immutable(
        &executable("lumen-application-isolation")?,
        "Rust application boundary",
    )?;
    let source = fs::canonicalize(env::current_dir()?)?;
    let parent_net = fs::read_link("/proc/self/ns/net")?
        .to_string_lossy()
        .into_owned();
    let status = Command::new("timeout")
        .arg("45s")
        .arg(boundary)
        .args(["run", "--source"])
        .arg(source)
        .arg("--output")
        .arg(output)
        .arg("--")
        .arg(env::current_exe()?)
        .arg("--inside")
        .arg("--pipewire")
        .arg(pipewire)
        .arg("--observer")
        .arg(observer)
        .arg("--parent-net")
        .arg(parent_net)
        .status()?;
    require(
        status.success(),
        &format!("Capture boundary exited {status}"),
    )
}

fn run(
    program: &Path,
    args: &[&str],
    input: Option<&str>,
    extra: &[(&str, &Path)],
) -> Result<String> {
    let mut command = Command::new("timeout");
    command.arg("4s").arg(program).args(args);
    command.env("PIPEWIRE_REMOTE", "capture-fixture");
    command.env(
        "PIPEWIRE_RUNTIME_DIR",
        env::var_os("XDG_RUNTIME_DIR").ok_or("XDG_RUNTIME_DIR missing")?,
    );
    for (key, value) in extra {
        command.env(key, value);
    }
    let mut child = command
        .stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    if let Some(value) = input {
        child
            .stdin
            .take()
            .ok_or("Missing stdin")?
            .write_all(value.as_bytes())?;
    }
    let output = child.wait_with_output()?;
    if !output.status.success() {
        return Err(fail(format!(
            "{} failed: {}",
            program.display(),
            String::from_utf8_lossy(&output.stderr)
        )));
    }
    Ok(String::from_utf8(output.stdout)?.trim().to_owned())
}
fn graph(options: &Options) -> Result<Value> {
    Ok(serde_json::from_str(&run(
        &options.observer,
        &["graph"],
        None,
        &[("LUMEN_PW_DUMP", &options.pipewire.join("pw-dump"))],
    )?)?)
}
fn describe(options: &Options, graph: &Value) -> Result<Value> {
    Ok(serde_json::from_str(&run(
        &options.observer,
        &["describe"],
        Some(&graph.to_string()),
        &[],
    )?)?)
}
fn tool(options: &Options, name: &str, args: &[&str]) -> Result<String> {
    run(&options.pipewire.join(name), args, None, &[])
}
fn wait_for<T>(description: &str, mut attempt: impl FnMut() -> Result<Option<T>>) -> Result<T> {
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        if let Some(value) = attempt()? {
            return Ok(value);
        }
        if Instant::now() >= deadline {
            return Err(fail(format!("Timed out: {description}")));
        }
        thread::sleep(Duration::from_millis(50));
    }
}

struct Server(Child);
impl Drop for Server {
    fn drop(&mut self) {
        unsafe { libc::killpg(self.0.id() as i32, libc::SIGTERM) };
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            if self.0.try_wait().ok().flatten().is_some() {
                break;
            }
            if Instant::now() >= deadline {
                let _ = self.0.kill();
                break;
            }
            thread::sleep(Duration::from_millis(25));
        }
        let _ = self.0.wait();
    }
}

fn inner(options: &Options) -> Result<()> {
    require(
        unsafe { libc::getuid() } != 0,
        "Capture fixture ran as root",
    )?;
    let parent = options
        .parent_net
        .as_deref()
        .ok_or("Missing parent namespace")?;
    require(
        fs::read_link("/proc/self/ns/net")?.to_string_lossy() != parent,
        "Capture fixture kept the host network namespace",
    )?;
    require(
        !Path::new("/dev/snd").exists(),
        "Host sound hardware visible",
    )?;
    let home = env::var_os("HOME").ok_or("HOME missing")?;
    let entries = fs::read_dir("/home")?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<std::io::Result<Vec<_>>>()?;
    require(entries == [PathBuf::from(home)], "Unexpected home visible")?;

    let output = Path::new("/artifacts");
    let runtime = PathBuf::from(env::var_os("XDG_RUNTIME_DIR").ok_or("XDG_RUNTIME_DIR missing")?);
    let config = output.join("pipewire.conf");
    fs::write(&config, CONFIG)?;
    let log = File::create(output.join("server.log"))?;
    let server = Command::new(options.pipewire.join("pipewire"))
        .arg("-c")
        .arg(&config)
        .stdout(Stdio::from(log.try_clone()?))
        .stderr(Stdio::from(log))
        .env("PIPEWIRE_REMOTE", "capture-fixture")
        .env("PIPEWIRE_RUNTIME_DIR", &runtime)
        .process_group(0)
        .spawn()?;
    let _server = Server(server);
    wait_for("Private PipeWire socket", || {
        Ok(fs::metadata(runtime.join("capture-fixture"))
            .map(|metadata| metadata.file_type().is_socket())
            .unwrap_or(false)
            .then_some(()))
    })?;
    let baseline = describe(options, &graph(options)?)?;
    require(
        baseline["available"] == true && baseline["rows"] == json!([]),
        "Capture existed before synthetic link",
    )?;

    tool(
        options,
        "pw-cli",
        &[
            "create-node",
            "adapter",
            "{ factory.name = audiotestsrc node.name = synthetic-source media.class = Audio/Source audio.position = [ MONO ] adapter.auto-port-config = { mode = dsp monitor = false position = preserve } object.linger = true }",
        ],
    )?;
    tool(
        options,
        "pw-cli",
        &[
            "create-node",
            "adapter",
            "{ factory.name = support.null-audio-sink node.name = synthetic-capture media.class = Stream/Input/Audio audio.position = [ MONO ] adapter.auto-port-config = { mode = dsp monitor = false position = preserve } object.linger = true }",
        ],
    )?;
    let nodes = graph(options)?;
    fs::write(
        output.join("created.json"),
        format!("{}\n", serde_json::to_string_pretty(&nodes)?),
    )?;
    let source_id = nodes
        .as_array()
        .and_then(|nodes| {
            nodes
                .iter()
                .find(|node| node["info"]["props"]["node.name"] == "synthetic-source")
        })
        .and_then(|node| node["id"].as_u64())
        .ok_or("Synthetic source missing")?;
    tool(
        options,
        "pw-cli",
        &[
            "set-param",
            &source_id.to_string(),
            "PortConfig",
            "{ direction = Output mode = dsp format = { mediaType = audio mediaSubtype = raw format = F32 rate = 48000 channels = 1 position = [ MONO ] } }",
        ],
    )?;
    let outputs = tool(options, "pw-link", &["-o"])?;
    let inputs = tool(options, "pw-link", &["-i"])?;
    let source = outputs
        .lines()
        .find(|port| port.starts_with("synthetic-source:"))
        .ok_or("Synthetic source port missing")?;
    let target = inputs
        .lines()
        .find(|port| port.starts_with("synthetic-capture:"))
        .ok_or("Synthetic capture port missing")?;
    tool(options, "pw-link", &[source, target])?;
    let active = wait_for("Actual active synthetic capture link", || {
        let state = describe(options, &graph(options)?)?;
        Ok(state["rows"]
            .as_array()
            .is_some_and(|rows| !rows.is_empty())
            .then_some(state))
    })?;
    require(
        active["rows"]
            .as_array()
            .is_some_and(|rows| rows.len() == 1)
            && active["rows"][0]["kind"] == "audio-input",
        "Expected one audio capture row",
    )?;
    tool(options, "pw-link", &["-d", source, target])?;
    let cleared = wait_for("Unlink clears capture observation", || {
        let state = describe(options, &graph(options)?)?;
        Ok((state["rows"] == json!([])).then_some(state))
    })?;
    let result = json!({
        "passed": true,
        "checks": [
            "Private real PipeWire graph starts without capture",
            "Synthetic linked capture is observed through actual pw-dump",
            "Actual unlink retires observation"
        ],
        "active": active,
        "cleared": cleared,
        "scope": "Synthetic graph in isolated non-root namespaces with no hardware or host PipeWire; not camera/microphone/portal/hardware acceptance"
    });
    fs::write(
        output.join("result.json"),
        format!("{}\n", serde_json::to_string_pretty(&result)?),
    )?;
    Ok(())
}

fn main() {
    let result = options().and_then(|options| {
        if options.inside {
            inner(&options)
        } else {
            outer(&options)
        }
    });
    if let Err(error) = result {
        eprintln!("capture protocol: {error}");
        std::process::exit(1);
    }
}
