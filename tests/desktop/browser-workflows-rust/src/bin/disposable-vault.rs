//! Disposable Vaultwarden web registration and Bitwarden CLI compatibility.
use base64::{Engine as _, engine::general_purpose::STANDARD};
use lumen_browser_workflows_fixture::boundary::validate_inside;
use lumen_browser_workflows_fixture::protocol::PipeTransport;
use lumen_browser_workflows_fixture::vault_artifacts::{ArtifactSecrets, run_cli};
use lumen_browser_workflows_fixture::vault_https::{PrivateHttps, client_config, connect};
use lumen_browser_workflows_fixture::{Result, fail};
use rustls::ClientConfig;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::env;
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Read, Write};
use std::os::fd::AsRawFd;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

const ORIGIN: &str = "https://127.0.0.1:8237";
#[derive(Clone)]
struct Options {
    server: PathBuf,
    web: PathBuf,
    cli: PathBuf,
    browser: PathBuf,
    bus: PathBuf,
    tls_tool: PathBuf,
    trust_tool: PathBuf,
    font_config: PathBuf,
    output: PathBuf,
    inside: bool,
}
impl Options {
    fn parse() -> Result<Self> {
        let mut args = env::args_os().skip(1);
        let mut values = BTreeMap::new();
        let mut inside = false;
        while let Some(arg) = args.next() {
            match arg.to_str() {
                Some("--inside") => inside = true,
                Some(key) if key.starts_with("--") => {
                    values.insert(
                        key.to_owned(),
                        PathBuf::from(args.next().ok_or("Missing vault option value")?),
                    );
                }
                _ => return Err(fail("Invalid vault option")),
            }
        }
        let mut required = |name: &str| {
            values
                .remove(name)
                .ok_or_else(|| fail(format!("{name} required")))
        };
        let output = required("--output")?;
        Ok(Self {
            server: required("--server")?,
            web: required("--web")?,
            cli: required("--cli")?,
            browser: required("--browser")?,
            bus: required("--bus")?,
            tls_tool: required("--tls-tool")?,
            trust_tool: required("--trust-tool")?,
            font_config: required("--font-config")?,
            output: if output.is_absolute() {
                output
            } else {
                env::current_dir()?.join(output)
            },
            inside,
        })
    }
    fn tools(&self) -> [(&'static str, &Path); 8] {
        [
            ("--server", &self.server),
            ("--web", &self.web),
            ("--cli", &self.cli),
            ("--browser", &self.browser),
            ("--bus", &self.bus),
            ("--tls-tool", &self.tls_tool),
            ("--trust-tool", &self.trust_tool),
            ("--font-config", &self.font_config),
        ]
    }
}
fn outer(mut o: Options) -> Result<()> {
    let tools = o.tools().map(|(name, path)| (name, path.to_path_buf()));
    for (name, path) in tools {
        let canonical = path.canonicalize()?;
        if !canonical.starts_with("/nix/store")
            || (name == "--web" && !canonical.is_dir())
            || (name != "--web" && !canonical.is_file())
        {
            return Err(fail(format!("Pinned immutable {name} required")));
        }
        match name {
            "--server" => o.server = canonical,
            "--web" => o.web = canonical,
            "--cli" => o.cli = canonical,
            "--browser" => o.browser = canonical,
            "--bus" => o.bus = canonical,
            "--tls-tool" => o.tls_tool = canonical,
            "--trust-tool" => o.trust_tool = canonical,
            "--font-config" => o.font_config = canonical,
            _ => unreachable!(),
        }
    }
    fs::create_dir(&o.output)?;
    fs::set_permissions(&o.output, fs::Permissions::from_mode(0o700))?;
    let source = env::current_dir()?.canonicalize()?;
    let exe = env::current_exe()?;
    let executable = if exe.starts_with(&source) {
        Path::new("/source").join(exe.strip_prefix(&source)?)
    } else {
        exe
    };
    let mut command = Command::new("lumen-application-isolation");
    command
        .arg("run")
        .arg("--source")
        .arg(source)
        .arg("--output")
        .arg(&o.output)
        .arg("--")
        .arg(executable)
        .arg("--inside")
        .arg("--output")
        .arg("/artifacts");
    for (name, path) in o.tools() {
        command.arg(name).arg(path);
    }
    let status = command.status()?;
    if !status.success() {
        return Err(fail(format!(
            "Disposable vault compatibility failed: {status}"
        )));
    }
    Ok(())
}
fn generate_certificate(o: &Options) -> Result<(PathBuf, PathBuf, PathBuf)> {
    let cert = PathBuf::from("/home/fixture/fixture-cert.pem");
    let key = PathBuf::from("/home/fixture/fixture-key.pem");
    let ca = PathBuf::from("/home/fixture/fixture-ca.pem");
    let ca_key = PathBuf::from("/home/fixture/fixture-ca-key.pem");
    let request = PathBuf::from("/home/fixture/fixture.csr");
    let extensions = PathBuf::from("/home/fixture/fixture.ext");
    let root = Command::new(&o.tls_tool)
        .args([
            "req",
            "-x509",
            "-newkey",
            "rsa:2048",
            "-nodes",
            "-days",
            "1",
            "-subj",
            "/CN=Disposable fixture CA",
            "-addext",
            "basicConstraints=critical,CA:TRUE",
            "-addext",
            "keyUsage=critical,keyCertSign,cRLSign",
            "-keyout",
        ])
        .arg(&ca_key)
        .arg("-out")
        .arg(&ca)
        .output()?;
    if !root.status.success() {
        return Err(fail("Disposable CA generation failed"));
    }
    let leaf = Command::new(&o.tls_tool)
        .args([
            "req",
            "-new",
            "-newkey",
            "rsa:2048",
            "-nodes",
            "-subj",
            "/CN=127.0.0.1",
            "-keyout",
        ])
        .arg(&key)
        .arg("-out")
        .arg(&request)
        .output()?;
    if !leaf.status.success() {
        return Err(fail("Disposable leaf request failed"));
    }
    fs::write(
        &extensions,
        "subjectAltName=IP:127.0.0.1,DNS:localhost\nbasicConstraints=critical,CA:FALSE\nextendedKeyUsage=serverAuth\nkeyUsage=digitalSignature,keyEncipherment\n",
    )?;
    let signed = Command::new(&o.tls_tool)
        .args(["x509", "-req", "-days", "1", "-CAcreateserial", "-in"])
        .arg(&request)
        .arg("-CA")
        .arg(&ca)
        .arg("-CAkey")
        .arg(&ca_key)
        .arg("-extfile")
        .arg(&extensions)
        .arg("-out")
        .arg(&cert)
        .output()?;
    if !signed.status.success() {
        return Err(fail("Disposable leaf signing failed"));
    }
    let trust = PathBuf::from("/home/fixture/.pki/nssdb");
    fs::create_dir_all(&trust)?;
    for args in [
        vec![
            "-N",
            "-d",
            "sql:/home/fixture/.pki/nssdb",
            "--empty-password",
        ],
        vec![
            "-A",
            "-d",
            "sql:/home/fixture/.pki/nssdb",
            "-n",
            "Disposable fixture",
            "-t",
            "C,,",
            "-i",
            "/home/fixture/fixture-ca.pem",
        ],
    ] {
        if !Command::new(&o.trust_tool)
            .args(args)
            .output()?
            .status
            .success()
        {
            return Err(fail("Private browser trust setup failed"));
        }
    }
    Ok((cert, key, ca))
}
fn run_child(mut command: Command, log: &Path) -> Result<Child> {
    let logfile = File::create(log)?;
    command
        .stdout(Stdio::from(logfile.try_clone()?))
        .stderr(Stdio::from(logfile));
    unsafe {
        command.pre_exec(|| {
            libc::setsid();
            Ok(())
        });
    }
    Ok(command.spawn()?)
}
fn bus(o: &Options, environment: &mut BTreeMap<String, String>) -> Result<Child> {
    let config = o.output.join("bus.conf");
    fs::write(
        &config,
        "<busconfig><type>session</type><listen>unix:tmpdir=/run/fixture</listen><policy context=\"default\"><allow own=\"*\"/><allow send_destination=\"*\"/><allow receive_sender=\"*\"/></policy></busconfig>",
    )?;
    let log = File::create(o.output.join("bus.log"))?;
    let mut command = Command::new(&o.bus);
    command.args([
        "--nofork",
        "--print-address=1",
        &format!("--config-file={}", config.display()),
    ]);
    command.stdout(Stdio::piped()).stderr(Stdio::from(log));
    unsafe {
        command.pre_exec(|| {
            libc::setsid();
            Ok(())
        });
    }
    let mut child = command.spawn()?;
    let stdout = child.stdout.take().ok_or("Private bus stdout missing")?;
    let mut poll = libc::pollfd {
        fd: stdout.as_raw_fd(),
        events: libc::POLLIN,
        revents: 0,
    };
    if unsafe { libc::poll(&mut poll, 1, 5000) } <= 0 {
        return Err(fail("Private bus startup timed out"));
    }
    let mut address = String::new();
    BufReader::new(stdout).read_line(&mut address)?;
    if !address.starts_with("unix:") {
        return Err(fail("Private bus address missing"));
    }
    environment.insert("DBUS_SESSION_BUS_ADDRESS".into(), address.trim().into());
    Ok(child)
}
fn random() -> Result<String> {
    let mut bytes = [0; 24];
    File::open("/dev/urandom")?.read_exact(&mut bytes)?;
    Ok(base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes))
}
fn body(response: &[u8]) -> Result<(u16, Vec<u8>)> {
    let index = response
        .windows(4)
        .position(|part| part == b"\r\n\r\n")
        .ok_or("HTTP response headers missing")?;
    let headers = std::str::from_utf8(&response[..index])?;
    let status = headers
        .split_whitespace()
        .nth(1)
        .ok_or("HTTP status missing")?
        .parse()?;
    let bytes = &response[index + 4..];
    if headers
        .to_ascii_lowercase()
        .contains("transfer-encoding: chunked")
    {
        let mut decoded = Vec::new();
        let mut rest = bytes;
        loop {
            let end = rest
                .windows(2)
                .position(|part| part == b"\r\n")
                .ok_or("Chunk length missing")?;
            let line = std::str::from_utf8(&rest[..end])?
                .split(';')
                .next()
                .ok_or("Chunk size missing")?;
            let length = usize::from_str_radix(line, 16)?;
            rest = &rest[end + 2..];
            if length == 0 {
                break;
            }
            if length > 8 * 1024 * 1024 || rest.len() < length + 2 {
                return Err(fail("Invalid chunked response"));
            }
            decoded.extend_from_slice(&rest[..length]);
            rest = &rest[length + 2..];
        }
        Ok((status, decoded))
    } else {
        Ok((status, bytes.to_vec()))
    }
}
fn http(
    config: Arc<ClientConfig>,
    method: &str,
    path: &str,
    payload: Option<&[u8]>,
) -> Result<(u16, Vec<u8>)> {
    let mut stream = connect(config)?;
    let data = payload.unwrap_or_default();
    let header = format!(
        "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1:8237\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n",
        data.len()
    );
    stream.write_all(header.as_bytes())?;
    stream.write_all(data)?;
    let mut response = Vec::new();
    stream.read_to_end(&mut response)?;
    body(&response)
}
fn wait<T>(label: &str, seconds: u64, mut attempt: impl FnMut() -> Result<Option<T>>) -> Result<T> {
    let deadline = Instant::now() + Duration::from_secs(seconds);
    loop {
        if let Some(value) = attempt()? {
            return Ok(value);
        }
        if Instant::now() >= deadline {
            return Err(fail(format!("{label} timed out")));
        }
        thread::sleep(Duration::from_millis(50));
    }
}
fn evaluate(browser: &mut PipeTransport, session: &str, expression: &str) -> Result<Value> {
    let value = browser.request(
        "Runtime.evaluate",
        json!({"expression":expression,"returnByValue":true}),
        Some(session),
        Duration::from_secs(5),
    )?;
    if !value["exceptionDetails"].is_null() {
        return Err(fail("Disposable web form script rejected"));
    }
    Ok(value["result"]["value"].clone())
}
fn selector(browser: &mut PipeTransport, session: &str, css: &str) -> Result<()> {
    let expression = format!("!!document.querySelector({})", serde_json::to_string(css)?);
    wait("Expected registration field", 15, || {
        Ok(evaluate(browser, session, &expression)?
            .as_bool()
            .filter(|value| *value))
    })?;
    Ok(())
}
fn fill(browser: &mut PipeTransport, session: &str, css: &str, value: &str) -> Result<()> {
    selector(browser, session, css)?;
    evaluate(
        browser,
        session,
        &format!(
            "document.querySelector({}).focus()",
            serde_json::to_string(css)?
        ),
    )?;
    for character in value.chars() {
        browser.request("Input.dispatchKeyEvent", json!({"type":"char","text":character.to_string(),"unmodifiedText":character.to_string()}), Some(session), Duration::from_secs(5))?;
    }
    for typ in ["keyDown", "keyUp"] {
        browser.request(
            "Input.dispatchKeyEvent",
            json!({"type":typ,"key":"Tab","code":"Tab","windowsVirtualKeyCode":9}),
            Some(session),
            Duration::from_secs(5),
        )?;
    }
    Ok(())
}
fn cli(
    o: &Options,
    environment: &BTreeMap<String, String>,
    args: &[&str],
    session: Option<&str>,
    success: bool,
) -> Result<String> {
    let mut local = environment.clone();
    local.insert(
        "BITWARDENCLI_APPDATA_DIR".into(),
        "/home/fixture/cli".into(),
    );
    local.insert("BW_NOINTERACTION".into(), "true".into());
    if let Some(session) = session {
        local.insert("BW_SESSION".into(), session.into());
    }
    let pairs = local
        .iter()
        .map(|(key, value)| (key.as_str(), value.as_str()))
        .collect::<Vec<_>>();
    let result = run_cli(&o.cli, args, &pairs, Duration::from_secs(30))?;
    if result.status.success() != success {
        return Err(fail(format!(
            "Unexpected disposable CLI outcome: {}",
            args.first().unwrap_or(&"?")
        )));
    }
    let output = String::from_utf8(result.stdout)?.trim().to_owned();
    Ok(output)
}
fn percent(value: &str) -> String {
    value
        .bytes()
        .map(|byte| {
            if byte.is_ascii_alphanumeric() || b"-._~".contains(&byte) {
                (byte as char).to_string()
            } else {
                format!("%{byte:02X}")
            }
        })
        .collect()
}
fn workflow(
    o: &Options,
    guard: &mut ArtifactSecrets,
    processes: &mut Vec<Child>,
    browser_slot: &mut Option<PipeTransport>,
    https_slot: &mut Option<PrivateHttps>,
) -> Result<Value> {
    validate_inside(&o.output)?;
    let (cert, key, ca) = generate_certificate(o)?;
    let tls = client_config(&ca)?;
    let mut environment: BTreeMap<String, String> = env::vars().collect();
    environment.insert(
        "FONTCONFIG_FILE".into(),
        o.font_config.display().to_string(),
    );
    environment.insert("NODE_EXTRA_CA_CERTS".into(), ca.display().to_string());
    environment.insert("LUMEN_FIXTURE_PASSWORD".into(), String::new());
    environment.insert(
        "DBUS_SYSTEM_BUS_ADDRESS".into(),
        "unix:path=/run/fixture/no-system-bus".into(),
    );
    let path = format!(
        "{}:{}",
        o.bus.parent().ok_or("Bus parent missing")?.display(),
        environment.get("PATH").map(String::as_str).unwrap_or("")
    );
    environment.insert("PATH".into(), path);
    processes.push(bus(o, &mut environment)?);
    let data = Path::new("/home/fixture/vault");
    fs::create_dir(data)?;
    let mut server_env = environment.clone();
    for (key, value) in [
        ("DATA_FOLDER", data.display().to_string()),
        ("WEB_VAULT_FOLDER", o.web.display().to_string()),
        ("ROCKET_ADDRESS", "127.0.0.1".into()),
        ("ROCKET_PORT", "8238".into()),
        ("DOMAIN", ORIGIN.into()),
        ("SIGNUPS_ALLOWED", "true".into()),
        ("SIGNUPS_VERIFY", "false".into()),
        ("LOG_LEVEL", "error".into()),
        ("IP_HEADER", "none".into()),
    ] {
        server_env.insert(key.into(), value);
    }
    let mut server = Command::new(&o.server);
    server.env_clear().envs(&server_env);
    processes.push(run_child(server, &o.output.join("server.log"))?);
    *https_slot = Some(PrivateHttps::start(&cert, &key)?);
    wait("Private vault readiness", 15, || {
        Ok(http(Arc::clone(&tls), "GET", "/alive", None)
            .ok()
            .filter(|(status, _)| *status == 200))
    })?;
    let profile = Path::new("/home/fixture/browser");
    fs::create_dir(profile)?;
    let command = vec![
        o.browser.display().to_string(),
        format!("--user-data-dir={}", profile.display()),
        "--headless".into(),
        "--disable-gpu".into(),
        "--no-first-run".into(),
        "--no-default-browser-check".into(),
        "--remote-debugging-pipe".into(),
        "about:blank".into(),
    ];
    *browser_slot = Some(PipeTransport::start(
        &command,
        &environment,
        &o.output.join("browser.log"),
    )?);
    let browser = browser_slot.as_mut().ok_or("Browser process missing")?;
    browser.request(
        "Browser.getVersion",
        json!({}),
        None,
        Duration::from_secs(15),
    )?;
    let targets = browser.request("Target.getTargets", json!({}), None, Duration::from_secs(5))?;
    let target = targets["targetInfos"]
        .as_array()
        .ok_or("Browser targets missing")?
        .iter()
        .find(|row| row["type"] == "page")
        .ok_or("Browser page missing")?;
    let session = browser.request(
        "Target.attachToTarget",
        json!({"targetId":target["targetId"],"flatten":true}),
        None,
        Duration::from_secs(5),
    )?["sessionId"]
        .as_str()
        .ok_or("Browser session missing")?
        .to_owned();
    let email = guard.add(format!("fixture-{}@example.invalid", &random()?[..12]))?;
    let password = guard.add(random()?)?;
    environment.insert("LUMEN_FIXTURE_PASSWORD".into(), password.clone());
    let provision =
        json!({"email":email,"name":"Disposable fixture","receiveMarketingEmails":false})
            .to_string();
    let (status, payload) = http(
        Arc::clone(&tls),
        "POST",
        "/identity/accounts/register/send-verification-email",
        Some(provision.as_bytes()),
    )?;
    if status >= 400 {
        return Err(fail("Private signup token provisioning failed"));
    }
    let token = std::str::from_utf8(&payload)?.trim();
    let token = if token.starts_with('"') {
        serde_json::from_str::<String>(token)?
    } else {
        token.to_owned()
    };
    if token.is_empty() {
        return Err(fail("Disposable registration token missing"));
    }
    guard.add(token.clone())?;
    let (status, config) = http(Arc::clone(&tls), "GET", "/api/config", None)?;
    if status != 200 || !serde_json::from_slice::<Value>(&config)?.is_object() {
        return Err(fail("Private vault config missing"));
    }
    for method in ["Page.enable", "Network.enable", "Runtime.enable"] {
        browser.request(method, json!({}), Some(&session), Duration::from_secs(5))?;
    }
    let url = format!(
        "{ORIGIN}/#/finish-signup?email={}&token={}",
        percent(&email),
        percent(&token)
    );
    browser.request(
        "Page.navigate",
        json!({"url":url}),
        Some(&session),
        Duration::from_secs(10),
    )?;
    fill(
        browser,
        &session,
        "input#input-password-form_new-password",
        &password,
    )?;
    fill(
        browser,
        &session,
        "input#input-password-form_new-password-confirm",
        &password,
    )?;
    evaluate(
        browser,
        &session,
        "(()=>{const e=document.querySelector('input[formcontrolname=checkForBreaches]');if(e&&e.checked)e.click();return true})()",
    )?;
    let button = "Array.from(document.querySelectorAll('button,a')).find(e=>e.textContent.trim()==='Create account')";
    wait("Create account button", 15, || {
        Ok(evaluate(browser, &session, &format!("!!({button})"))?
            .as_bool()
            .filter(|value| *value))
    })?;
    evaluate(
        browser,
        &session,
        &format!(
            "(()=>{{const b=({button});if(b.disabled||!b.form)throw Error('Registration unavailable');b.form.requestSubmit(b);return true}})()"
        ),
    )?;
    wait("Web registration completion", 15, || {
        Ok(evaluate(
            browser,
            &session,
            "!document.querySelector('input#input-password-form_new-password')",
        )?
        .as_bool()
        .filter(|value| *value))
    })?;
    cli(o, &environment, &["config", "server", ORIGIN], None, true)?;
    let mut session_token = guard.add(cli(
        o,
        &environment,
        &[
            "login",
            &email,
            "--passwordenv",
            "LUMEN_FIXTURE_PASSWORD",
            "--raw",
        ],
        None,
        true,
    )?)?;
    if session_token.contains('\n') || session_token.is_empty() {
        return Err(fail("Disposable CLI session invalid"));
    }
    let status: Value = serde_json::from_str(&cli(
        o,
        &environment,
        &["status"],
        Some(&session_token),
        true,
    )?)?;
    if status["status"] != "unlocked" || status["serverUrl"] != ORIGIN {
        return Err(fail("Disposable CLI login state invalid"));
    }
    let mut item: Value = serde_json::from_str(&cli(
        o,
        &environment,
        &["get", "template", "item"],
        Some(&session_token),
        true,
    )?)?;
    let item_password = guard.add(random()?)?;
    item["name"] = json!("Disposable fixture item");
    item["type"] = json!(1);
    item["login"] = json!({"username":"disposable-user","password":item_password,"totp":null,"uris":[{"uri":"https://example.invalid","match":null}]});
    let encoded = guard.add(STANDARD.encode(item.to_string()))?;
    let created: Value = serde_json::from_str(&cli(
        o,
        &environment,
        &["create", "item", &encoded],
        Some(&session_token),
        true,
    )?)?;
    let id = created["id"]
        .as_str()
        .ok_or("Created item ID missing")?
        .to_owned();
    if created["login"]["password"] != item_password {
        return Err(fail("Created item password mismatch"));
    }
    let mut fetched: Value = serde_json::from_str(&cli(
        o,
        &environment,
        &["get", "item", &id],
        Some(&session_token),
        true,
    )?)?;
    if fetched["login"]["password"] != item_password {
        return Err(fail("Fetched item password mismatch"));
    }
    let replacement = guard.add(random()?)?;
    fetched["login"]["password"] = json!(replacement);
    let encoded = guard.add(STANDARD.encode(fetched.to_string()))?;
    let edited: Value = serde_json::from_str(&cli(
        o,
        &environment,
        &["edit", "item", &id, &encoded],
        Some(&session_token),
        true,
    )?)?;
    if edited["login"]["password"] != replacement {
        return Err(fail("Edited item password mismatch"));
    }
    cli(o, &environment, &["sync"], Some(&session_token), true)?;
    cli(o, &environment, &["lock"], Some(&session_token), true)?;
    let status: Value = serde_json::from_str(&cli(o, &environment, &["status"], None, true)?)?;
    if status["status"] != "locked" {
        return Err(fail("CLI lock state invalid"));
    }
    cli(o, &environment, &["get", "item", &id], None, false)?;
    session_token = guard.add(cli(
        o,
        &environment,
        &["unlock", "--passwordenv", "LUMEN_FIXTURE_PASSWORD", "--raw"],
        None,
        true,
    )?)?;
    let retained: Value = serde_json::from_str(&cli(
        o,
        &environment,
        &["get", "item", &id],
        Some(&session_token),
        true,
    )?)?;
    if retained["login"]["password"] != replacement {
        return Err(fail("Edited item not retained after unlock"));
    }
    cli(
        o,
        &environment,
        &["delete", "item", &id],
        Some(&session_token),
        true,
    )?;
    cli(o, &environment, &["sync"], Some(&session_token), true)?;
    let listed: Value = serde_json::from_str(&cli(
        o,
        &environment,
        &["list", "items"],
        Some(&session_token),
        true,
    )?)?;
    if listed
        .as_array()
        .ok_or("Item list invalid")?
        .iter()
        .any(|row| row["id"] == id)
    {
        return Err(fail("Deleted item remains in list"));
    }
    cli(o, &environment, &["logout"], Some(&session_token), true)?;
    let status: Value = serde_json::from_str(&cli(o, &environment, &["status"], None, true)?)?;
    if status["status"] != "unauthenticated" {
        return Err(fail("CLI logout state invalid"));
    }
    Ok(
        json!({"passed":true,"checks":["private server signup-token provisioning","real web password registration","CLI password login","create and decrypt synthetic login item","edit synthetic login item","private server sync","lock refuses item access","password unlock retains edited item","trash item and sync","logout"],"scope":"Disposable loopback Vaultwarden and actual CLI with real web signup; no live account or extension claims","credentialsRetained":false}),
    )
}
fn cleanup(
    processes: &mut Vec<Child>,
    browser: &mut Option<PipeTransport>,
    https: &mut Option<PrivateHttps>,
    guard: &ArtifactSecrets,
    output: &Path,
) -> Result<usize> {
    let mut errors = Vec::new();
    if let Some(browser) = browser.take() {
        if browser.close().is_err() {
            errors.push("browser");
        }
    }
    if let Some(https) = https.as_mut() {
        if https.close().is_err() {
            errors.push("https");
        }
    }
    for child in processes.iter_mut().rev() {
        if child.try_wait().ok().flatten().is_none() {
            unsafe {
                libc::killpg(child.id() as i32, libc::SIGTERM);
            }
        }
        let deadline = Instant::now() + Duration::from_secs(5);
        while child.try_wait().ok().flatten().is_none() && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(20));
        }
        if child.try_wait().ok().flatten().is_none() {
            unsafe {
                libc::killpg(child.id() as i32, libc::SIGKILL);
            }
        }
        if child.wait().is_err() {
            errors.push("child");
        }
    }
    let audited = match guard.audit(output) {
        Ok(files) => files,
        Err(_) => {
            errors.push("artifact-audit");
            0
        }
    };
    if !errors.is_empty() {
        return Err(fail(format!(
            "Disposable fixture cleanup failed: {}",
            errors.join(",")
        )));
    }
    Ok(audited)
}
fn inside(o: Options) -> Result<()> {
    let mut guard = ArtifactSecrets::new();
    let mut processes = Vec::new();
    let mut browser = None;
    let mut https = None;
    let work = workflow(&o, &mut guard, &mut processes, &mut browser, &mut https);
    let audited = cleanup(&mut processes, &mut browser, &mut https, &guard, &o.output);
    let mut result = work?;
    result["retainedFilesAudited"] = json!(audited?);
    result["serverWarnings"] = json!(
        fs::read_to_string(o.output.join("server.log"))?
            .lines()
            .filter(|line| line.contains("[WARN]"))
            .collect::<Vec<_>>()
    );
    fs::write(
        o.output.join("result.json"),
        format!("{}\n", serde_json::to_string_pretty(&result)?),
    )?;
    Ok(())
}
fn main() {
    let result = Options::parse().and_then(|o| if o.inside { inside(o) } else { outer(o) });
    if let Err(error) = result {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
