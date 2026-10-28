use serde_json::{Value, json};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::io::{BufRead, BufReader};
use std::os::fd::AsRawFd;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::os::unix::net::UnixListener;
use std::process::{Child, ChildStdout, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

fn request(bin: &str, runtime: &str, value: Value) -> Value {
    let mut child = Command::new(bin)
        .arg("request")
        .env("XDG_RUNTIME_DIR", runtime)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    writeln!(child.stdin.take().unwrap(), "{value}").unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

struct Service(Option<Child>);
impl Drop for Service {
    fn drop(&mut self) {
        if let Some(mut child) = self.0.take() {
            unsafe {
                libc::kill(child.id() as i32, libc::SIGTERM);
            }
            let _ = child.wait();
        }
    }
}

#[test]
fn client_exit_does_not_stop_running_job() {
    let bin = env!("CARGO_BIN_EXE_lumen-file-jobs");
    // Unix socket paths have a small fixed kernel limit; Nix build TMPDIRs can be long.
    let root = std::path::Path::new("/tmp").join(format!("lumen-fj-{}", std::process::id()));
    fs::create_dir(&root).unwrap();
    let source = root.join("source");
    let target = root.join("target");
    let runtime = root.join("runtime");
    fs::create_dir(&source).unwrap();
    fs::create_dir(&target).unwrap();
    fs::create_dir(&runtime).unwrap();
    fs::write(source.join("payload"), b"retained").unwrap();
    let runtime_text = runtime.to_string_lossy().to_string();
    let mut service = Service(Some(
        Command::new(bin)
            .arg("serve")
            .env("XDG_RUNTIME_DIR", &runtime)
            .env("XDG_STATE_HOME", root.join("state"))
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap(),
    ));
    let deadline = Instant::now() + Duration::from_secs(5);
    while !runtime.join("lumen-file-operations.sock").exists() && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(10));
    }
    if !runtime.join("lumen-file-operations.sock").exists() {
        use std::io::Read;
        let child = service.0.as_mut().unwrap();
        let status = child.try_wait().unwrap();
        let mut error = String::new();
        if status.is_some() {
            child
                .stderr
                .as_mut()
                .unwrap()
                .read_to_string(&mut error)
                .unwrap();
        }
        panic!("service socket missing; status={status:?}; stderr={error}");
    }
    let id = request(
        bin,
        &runtime_text,
        json!({"action":"job-start","operation":"copy","paths":[source],"destination":target}),
    )["job"]
        .as_str()
        .unwrap()
        .to_owned();
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let state = request(bin, &runtime_text, json!({"action":"job-status"}));
        let job = state["jobs"]
            .as_array()
            .unwrap()
            .iter()
            .find(|job| job["id"] == id)
            .unwrap();
        if job["state"] == "complete" {
            break;
        }
        assert!(Instant::now() < deadline, "{job}");
        thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(
        fs::read(target.join("source/payload")).unwrap(),
        b"retained"
    );
    assert_eq!(fs::read(source.join("payload")).unwrap(), b"retained");
    let mut child = service.0.take().unwrap();
    unsafe {
        libc::kill(child.id() as i32, libc::SIGTERM);
    }
    let status = child.wait().unwrap();
    assert!(status.success(), "{status:?}");
    assert!(!runtime.join("lumen-file-operations.sock").exists());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn trash_client_preserves_ids_and_forwards_only_typed_requests() {
    let bin = env!("CARGO_BIN_EXE_lumen-file-jobs");
    let runtime = std::path::Path::new("/tmp").join(format!("lumen-tc-{}", std::process::id()));
    fs::create_dir(&runtime).unwrap();
    let socket = runtime.join("lumen-file-operations.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    let server = thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        let mut request = String::new();
        let mut reader = BufReader::new(stream);
        reader.read_line(&mut request).unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(&request).unwrap(),
            json!({"action":"trash-status"})
        );
        writeln!(
            reader.get_mut(),
            "{}",
            json!({"available":true,"busy":false})
        )
        .unwrap();
    });
    let mut child = Command::new(bin)
        .arg("trash-client")
        .env("XDG_RUNTIME_DIR", &runtime)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let stdin = child.stdin.as_mut().unwrap();
    writeln!(stdin, "{}", json!({"id":1,"action":"job-status"})).unwrap();
    writeln!(
        stdin,
        "{}",
        json!({"id":9_007_199_254_740_992_u64,"action":"trash-status"})
    )
    .unwrap();
    writeln!(stdin, "{}", json!({"id":2,"action":"trash-status"})).unwrap();
    drop(child.stdin.take());
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let replies: Vec<Value> = output
        .stdout
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
        .map(|line| serde_json::from_slice(line).unwrap())
        .collect();
    assert_eq!(replies.len(), 3);
    assert_eq!(replies[0]["id"], 1);
    assert!(
        replies[0]["error"]
            .as_str()
            .unwrap()
            .contains("Unsupported")
    );
    assert!(replies[1]["error"].as_str().unwrap().contains("Invalid"));
    assert_eq!(
        replies[2],
        json!({"available":true,"busy":false,"id":2,"action":"trash-status"})
    );
    server.join().unwrap();
    fs::remove_dir_all(runtime).unwrap();
}

struct TrashClient {
    process: Child,
    output: BufReader<ChildStdout>,
}

impl TrashClient {
    fn start(bin: &str, root: &std::path::Path) -> Self {
        let runtime = root.join("runtime");
        let mut process = Command::new(bin)
            .arg("trash-client")
            .env("HOME", root.join("home"))
            .env("XDG_RUNTIME_DIR", runtime)
            .env("XDG_DATA_HOME", root.join("data"))
            .env("XDG_STATE_HOME", root.join("state"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let output = BufReader::new(process.stdout.take().unwrap());
        Self { process, output }
    }

    fn send(&mut self, value: Value) -> Value {
        writeln!(self.process.stdin.as_mut().unwrap(), "{value}").unwrap();
        let mut pollfd = libc::pollfd {
            fd: self.output.get_ref().as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        };
        assert_eq!(unsafe { libc::poll(&mut pollfd, 1, 3000) }, 1, "Trash client response timed out");
        let mut line = String::new();
        assert!(self.output.read_line(&mut line).unwrap() > 0, "Trash client closed early");
        serde_json::from_str(&line).unwrap()
    }

    fn finish(mut self) {
        drop(self.process.stdin.take());
        let result = self.process.wait().unwrap();
        assert!(result.success(), "Trash client exited: {result}");
    }
}

impl Drop for TrashClient {
    fn drop(&mut self) {
        if self.process.try_wait().unwrap().is_none() {
            let _ = self.process.kill();
            let _ = self.process.wait();
        }
    }
}

fn launch_trash_service(bin: &str, root: &std::path::Path) -> Service {
    let log = OpenOptions::new().create(true).append(true).open(root.join("server.log")).unwrap();
    let runtime = root.join("runtime");
    let mut service = Service(Some(
        Command::new(bin)
            .arg("serve")
            .env("HOME", root.join("home"))
            .env("XDG_RUNTIME_DIR", &runtime)
            .env("XDG_DATA_HOME", root.join("data"))
            .env("XDG_STATE_HOME", root.join("state"))
            .stdout(Stdio::from(log.try_clone().unwrap()))
            .stderr(Stdio::from(log))
            .spawn()
            .unwrap(),
    ));
    let deadline = Instant::now() + Duration::from_secs(3);
    while !runtime.join("lumen-file-operations.sock").exists() && Instant::now() < deadline {
        assert!(service.0.as_mut().unwrap().try_wait().unwrap().is_none(), "Trash service exited: {}", fs::read_to_string(root.join("server.log")).unwrap());
        thread::sleep(Duration::from_millis(10));
    }
    assert!(runtime.join("lumen-file-operations.sock").exists(), "Trash socket absent: {}", fs::read_to_string(root.join("server.log")).unwrap());
    service
}

#[test]
fn confirmed_trash_outlives_client_and_restart_never_replays() {
    let bin = env!("CARGO_BIN_EXE_lumen-file-jobs");
    let root = std::path::Path::new("/tmp").join(format!("lumen-trash-client-{}", std::process::id()));
    fs::create_dir(&root).unwrap();
    fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
    for name in ["home", "runtime", "data", "state", "data/Trash", "data/Trash/files", "data/Trash/info"] {
        let path = root.join(name);
        fs::create_dir(&path).unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
    }
    let item = root.join("data/Trash/files/Literal # é.txt");
    fs::write(&item, "disposable").unwrap();
    let info = root.join("data/Trash/info/Literal # é.txt.trashinfo");
    fs::write(&info, format!("[Trash Info]\nPath={}/Literal%20%23%20%C3%A9.txt\n", root.join("home").display())).unwrap();
    fs::set_permissions(&info, fs::Permissions::from_mode(0o600)).unwrap();
    let metadata = item.metadata().unwrap();
    let mut server = launch_trash_service(bin, &root);
    let mut client = TrashClient::start(bin, &root);
    let prepared = client.send(json!({"id":1,"action":"trash-prepare","mode":"selected","items":[{
        "path":item,"identity":{"device":metadata.dev().to_string(),"inode":metadata.ino().to_string()}
    }]}));
    assert!(prepared.get("error").is_none(), "{prepared}");
    assert_eq!(prepared["count"], 1);
    assert!(item.exists());
    let token = prepared["token"].as_str().unwrap().to_owned();
    let confirmed = client.send(json!({"id":2,"action":"trash-confirm","token":token}));
    assert!(confirmed.get("error").is_none(), "{confirmed}");
    client.finish();
    let mut client = TrashClient::start(bin, &root);
    let deadline = Instant::now() + Duration::from_secs(4);
    loop {
        let status = client.send(json!({"id":3,"action":"trash-status"}));
        if status["busy"] == false {
            assert_eq!(status["operations"].as_array().unwrap().last().unwrap()["state"], "complete", "{status}");
            break;
        }
        assert!(Instant::now() < deadline, "Trash operation did not finish: {status}");
        thread::sleep(Duration::from_millis(10));
    }
    assert!(!item.exists());
    client.finish();
    let mut process = server.0.take().unwrap();
    unsafe { libc::kill(process.id() as i32, libc::SIGTERM); }
    assert!(process.wait().unwrap().success());
    let replacement = root.join("data/Trash/files/Literal # é.txt");
    fs::write(&replacement, "later arrival").unwrap();
    let _restarted = launch_trash_service(bin, &root);
    let mut client = TrashClient::start(bin, &root);
    let replay = client.send(json!({"id":4,"action":"trash-confirm","token":token}));
    assert_eq!(replay["operations"].as_array().unwrap().last().unwrap()["state"], "complete", "{replay}");
    assert_eq!(fs::read_to_string(&replacement).unwrap(), "later arrival");
    let invalid = client.send(json!({"id":5,"action":"trash-confirm","token":token,"path":root.join("home")}));
    assert!(invalid.get("error").is_some(), "{invalid}");
    client.finish();
    drop(_restarted);
    let log = fs::read_to_string(root.join("server.log")).unwrap();
    assert!(!log.contains("Warning") && !log.contains("Traceback"), "{log}");
    fs::remove_dir_all(root).unwrap();
}
