//! End to end PermissionStore protocol acceptance on an activation-free private bus.
use futures_util::StreamExt;
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashMap};
use std::env;
use std::error::Error;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};
use zbus::zvariant::OwnedValue;

const NAME: &str = "org.freedesktop.impl.portal.PermissionStore";
const PATH: &str = "/org/freedesktop/impl/portal/PermissionStore";
type Result<T> = std::result::Result<T, Box<dyn Error>>;

fn fail(message: impl Into<String>) -> Box<dyn Error> {
    message.into().into()
}
fn require(ok: bool, message: &str) -> Result<()> {
    if ok { Ok(()) } else { Err(fail(message)) }
}

struct Options {
    provider: PathBuf,
    client: PathBuf,
    inner: bool,
}
fn options() -> Result<Options> {
    let mut provider = None;
    let mut client = None;
    let mut inner = false;
    let mut args = env::args_os().skip(1);
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("--inner") => inner = true,
            Some("--provider") => {
                provider = Some(PathBuf::from(args.next().ok_or("Missing provider path")?))
            }
            Some("--client") => {
                client = Some(PathBuf::from(args.next().ok_or("Missing client path")?))
            }
            _ => return Err(fail("Unknown portal protocol option")),
        }
    }
    Ok(Options {
        provider: provider.ok_or("Missing provider")?,
        client: client.ok_or("Missing client")?,
        inner,
    })
}

fn private_environment(root: &Path) -> BTreeMap<String, String> {
    let mut environment = env::vars().collect::<BTreeMap<_, _>>();
    environment.insert("HOME".into(), root.display().to_string());
    environment.insert("XDG_RUNTIME_DIR".into(), root.display().to_string());
    for name in ["CONFIG", "STATE", "CACHE", "DATA"] {
        environment.insert(
            format!("XDG_{name}_HOME"),
            root.join(name.to_lowercase()).display().to_string(),
        );
    }
    for name in [
        "DBUS_SESSION_BUS_ADDRESS",
        "AT_SPI_BUS_ADDRESS",
        "DISPLAY",
        "WAYLAND_DISPLAY",
        "SWAYSOCK",
    ] {
        environment.remove(name);
    }
    environment
}

struct Provider {
    child: Child,
    log: PathBuf,
}
impl Provider {
    fn start(binary: &Path, root: &Path) -> Result<Self> {
        let ready = root.join("ready");
        let log = root.join("provider.log");
        let stream = File::create(&log)?;
        let mut child = Command::new(binary)
            .arg("--ready")
            .arg(&ready)
            .stdout(Stdio::from(stream.try_clone()?))
            .stderr(Stdio::from(stream))
            .spawn()?;
        let deadline = Instant::now() + Duration::from_secs(5);
        while !ready.exists() {
            if let Some(status) = child.try_wait()? {
                return Err(fail(format!(
                    "Permission provider exited early: {status}: {}",
                    fs::read_to_string(&log)?
                )));
            }
            if Instant::now() >= deadline {
                child.kill()?;
                child.wait()?;
                return Err(fail("Permission provider did not become ready"));
            }
            thread::sleep(Duration::from_millis(30));
        }
        Ok(Self { child, log })
    }
    fn stop(&mut self) -> Result<()> {
        if self.child.try_wait()?.is_none() {
            unsafe {
                libc::kill(self.child.id() as i32, libc::SIGTERM);
            }
        }
        self.child.wait()?;
        require(
            fs::read_to_string(&self.log)?.is_empty(),
            "Permission provider wrote diagnostics",
        )?;
        Ok(())
    }
}
impl Drop for Provider {
    fn drop(&mut self) {
        if self.child.try_wait().ok().flatten().is_none() {
            unsafe {
                libc::kill(self.child.id() as i32, libc::SIGTERM);
            }
        }
        let _ = self.child.wait();
    }
}

fn client_raw(binary: &Path, request: &Value) -> Result<String> {
    let mut child = Command::new("timeout")
        .arg("10s")
        .arg(binary)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    child
        .stdin
        .take()
        .ok_or("Missing permission client stdin")?
        .write_all(format!("{request}\n").as_bytes())?;
    let result = child.wait_with_output()?;
    if !result.status.success() {
        return Err(fail(format!(
            "Permission client failed: {}",
            String::from_utf8_lossy(&result.stderr)
        )));
    }
    Ok(String::from_utf8(result.stdout)?)
}
fn client(binary: &Path, request: &Value) -> Result<Value> {
    Ok(serde_json::from_str(&client_raw(binary, request)?)?)
}
fn snapshot(binary: &Path) -> Result<Value> {
    client(binary, &json!({"action":"list"}))
}
fn row<'a>(state: &'a Value, capability: &str, app: &str) -> Result<&'a Value> {
    state["rows"]
        .as_array()
        .and_then(|rows| {
            rows.iter()
                .find(|item| item["capability"] == capability && item["appId"] == app)
        })
        .ok_or_else(|| fail(format!("Missing {capability} row for {app}")))
}
fn action(binary: &Path, item: &Value, kind: &str, confirmed: bool) -> Result<Value> {
    client(
        binary,
        &json!({"action":kind,"id":item["id"],"revision":item["revision"],"confirmed":confirmed}),
    )
}
fn contains(value: &Value, text: &str) -> bool {
    value.as_str().is_some_and(|value| value.contains(text))
}

async fn run_inner(options: &Options) -> Result<()> {
    let private = tempfile::tempdir()?;
    let root = private.path();
    let mut provider = Provider::start(&options.provider, root)?;
    let connection = zbus::Connection::session().await?;
    let store = zbus::Proxy::new(&connection, NAME, PATH, NAME).await?;
    let admin = zbus::Proxy::new(&connection, NAME, PATH, "org.example.PermissionFixture").await?;

    let missing: zbus::Result<(HashMap<String, Vec<String>>, OwnedValue)> =
        store.call("Lookup", &("devices", "missing")).await;
    require(
        matches!(missing, Err(zbus::Error::MethodError(ref name, _, _)) if name.as_str()=="org.freedesktop.portal.Error.NotFound"),
        "Missing entry lost portal NotFound error",
    )?;
    let mut changes = store.receive_signal("Changed").await?;

    let initial = snapshot(&options.client)?;
    require(
        initial["available"] == true
            && initial["version"] == 2
            && initial["rows"]
                .as_array()
                .is_some_and(|rows| rows.len() == 8),
        "Initial permission rows/version changed",
    )?;
    let encoded = initial.to_string();
    for secret in [
        "private-restore-resource",
        "private-remote-resource",
        "PRIVATE_PROVIDER_METADATA_DO_NOT_EXPORT",
    ] {
        require(
            !encoded.contains(secret),
            "Private provider resource leaked into client output",
        )?;
    }
    require(
        !encoded
            .to_lowercase()
            .replace(
                initial["scope"]
                    .as_str()
                    .unwrap_or("")
                    .to_lowercase()
                    .as_str(),
                "",
            )
            .contains("microphone"),
        "Undeclared microphone capability appeared",
    )?;
    require(
        initial["rows"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["appId"] == "" && item["actions"] == json!([])),
        "Unidentified app gained actions",
    )?;
    require(
        row(&initial, "Screenshots", "org.example.Unknown")?["actions"] == json!([]),
        "Unknown provider value gained actions",
    )?;

    let camera = row(&initial, "Camera", "org.example.First")?.clone();
    require(
        contains(
            &action(&options.client, &camera, "deny", false)?["error"],
            "confirmation",
        ),
        "Deny without confirmation was accepted",
    )?;
    let before: Value = serde_json::from_str(&admin.call::<_, _, String>("Dump", &()).await?)?;
    let changed = action(&options.client, &camera, "deny", true)?;
    require(
        changed["available"] == true
            && contains(&changed["message"], "Existing capture may continue"),
        "Deny was not confirmed",
    )?;
    let after: Value = serde_json::from_str(&admin.call::<_, _, String>("Dump", &()).await?)?;
    let mut expected = before["tables"].clone();
    expected["devices"]["camera"]["org.example.First"] = json!(["no"]);
    require(
        after["tables"] == expected
            && after["calls"].as_array().and_then(|calls| calls.last())
                == Some(&json!([
                    "SetPermission",
                    "devices",
                    "camera",
                    "org.example.First",
                    ["no"]
                ])),
        "Deny changed a sibling or recorded the wrong target",
    )?;
    let signal = tokio::time::timeout(Duration::from_secs(2), changes.next())
        .await?
        .ok_or("Missing Changed signal")?;
    let (table, resource, deleted, data, permissions): (
        String,
        String,
        bool,
        OwnedValue,
        HashMap<String, Vec<String>>,
    ) = signal.body().deserialize()?;
    require(
        table == "devices"
            && resource == "camera"
            && !deleted
            && String::try_from(data)? == "private"
            && permissions.get("org.example.First") == Some(&vec!["no".into()]),
        "Changed signal signature or target changed",
    )?;

    require(
        contains(
            &action(&options.client, &camera, "reset", true)?["error"],
            "changed",
        ),
        "Stale revision was accepted",
    )?;
    let fresh = row(&snapshot(&options.client)?, "Camera", "org.example.First")?.clone();
    require(
        action(&options.client, &fresh, "reset", true)?["available"] == true,
        "Reset failed",
    )?;
    let reset: Value = serde_json::from_str(&admin.call::<_, _, String>("Dump", &()).await?)?;
    require(
        reset["calls"].as_array().and_then(|calls| calls.last())
            == Some(&json!([
                "DeletePermission",
                "devices",
                "camera",
                "org.example.First"
            ])),
        "Reset reached the wrong target",
    )?;
    let saved = row(
        &snapshot(&options.client)?,
        "Saved screen sharing",
        "org.example.First",
    )?
    .clone();
    let saved_id = saved["id"].clone();
    let forgotten = action(&options.client, &saved, "forget", true)?;
    require(
        forgotten["available"] == true
            && !forgotten["rows"]
                .as_array()
                .unwrap()
                .iter()
                .any(|item| item["id"] == saved_id)
            && forgotten["rows"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|item| item["capability"] == "Saved screen sharing")
                .count()
                == 1,
        "Forget did not remove only its saved entry",
    )?;

    admin
        .call::<_, _, ()>("Configure", &(1u32, false, ""))
        .await?;
    let v1 = snapshot(&options.client)?;
    require(
        !row(&v1, "Saved remote desktop", "org.example.Second")?["actions"]
            .as_array()
            .unwrap()
            .contains(&json!("forget")),
        "Version 1 gained deletion",
    )?;
    admin
        .call::<_, _, ()>("Configure", &(2u32, true, ""))
        .await?;
    let screenshot = row(
        &snapshot(&options.client)?,
        "Screenshots",
        "org.example.First",
    )?
    .clone();
    require(
        contains(
            &action(&options.client, &screenshot, "deny", true)?["error"],
            "did not confirm",
        ),
        "Ignored provider write looked successful",
    )?;
    admin
        .call::<_, _, ()>("Configure", &(2u32, false, "screenshot"))
        .await?;
    let partial = snapshot(&options.client)?;
    require(
        partial["available"] == true
            && partial["coverage"]
                .as_array()
                .unwrap()
                .iter()
                .any(|item| item["state"] == "unavailable")
            && !partial["rows"]
                .as_array()
                .unwrap()
                .iter()
                .any(|item| item["capability"] == "Screenshots"),
        "Blocked table did not fail partially",
    )?;
    admin
        .call::<_, _, ()>("Configure", &(2u32, false, ""))
        .await?;
    require(
        snapshot(&options.client)?["available"] == true,
        "Provider did not recover after unblock",
    )?;
    let raw = client_raw(&options.client, &json!({"action":"list"}))?;
    require(
        serde_json::from_str::<Value>(&raw)?["available"] == true
            && !raw.contains("private-restore-resource"),
        "CLI leaked private resource",
    )?;

    let stale = row(
        &snapshot(&options.client)?,
        "Screenshots",
        "org.example.First",
    )?
    .clone();
    provider.stop()?;
    fs::remove_file(root.join("ready"))?;
    provider = Provider::start(&options.provider, root)?;
    require(
        contains(
            &action(&options.client, &stale, "deny", true)?["error"],
            "changed",
        ),
        "Old owner revision was accepted",
    )?;
    provider.stop()?;
    let unavailable = snapshot(&options.client)?;
    require(
        unavailable["available"] == false
            && contains(&unavailable["error"], "unavailable")
            && unavailable["rows"] == json!([]),
        "Missing provider did not fail closed",
    )?;
    println!("PASS private PermissionStore protocol, exact target, signal and restart");
    Ok(())
}

fn run_outer(options: &Options) -> Result<()> {
    let private = tempfile::tempdir()?;
    let root = private.path();
    let config = root.join("bus.conf");
    fs::write(
        &config,
        format!(
            "<busconfig><type>session</type><listen>unix:tmpdir={}</listen><auth>EXTERNAL</auth><policy context=\"default\"><allow send_destination=\"*\"/><allow receive_sender=\"*\"/><allow own=\"*\"/></policy></busconfig>",
            root.display()
        ),
    )?;
    let output = Command::new("timeout")
        .arg("35s")
        .arg("dbus-run-session")
        .arg(format!("--config-file={}", config.display()))
        .arg("--")
        .arg(env::current_exe()?)
        .arg("--inner")
        .arg("--provider")
        .arg(&options.provider)
        .arg("--client")
        .arg(&options.client)
        .env_clear()
        .envs(private_environment(root))
        .output()?;
    if !output.status.success() {
        return Err(fail(format!(
            "Private portal protocol failed: {}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )));
    }
    print!("{}", String::from_utf8_lossy(&output.stdout));
    Ok(())
}

#[tokio::main]
async fn main() {
    let result = options().and_then(|options| {
        if options.inner {
            Ok(options)
        } else {
            run_outer(&options).map(|_| options)
        }
    });
    let result = match result {
        Ok(options) if options.inner => run_inner(&options).await,
        Ok(_) => Ok(()),
        Err(error) => Err(error),
    };
    if let Err(error) = result {
        eprintln!("portal protocol: {error}");
        std::process::exit(1);
    }
}
