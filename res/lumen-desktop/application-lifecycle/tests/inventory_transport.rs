//! Real broker stdin/stdout and IPC framing against an explicitly fake provider.
//! This proves request ownership and refusal; it does not prove compositor guards.
use lumen_application_lifecycle::{receive, send};
use serde_json::{Value, json};
use std::{
    fs,
    io::{BufRead, BufReader, Read, Write},
    os::unix::{fs::PermissionsExt, net::UnixListener},
    process::{Child, Command, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};
struct Owned(Child);
impl Drop for Owned {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
fn query_kind(stream: &mut std::os::unix::net::UnixStream) -> std::io::Result<u32> {
    let mut header = [0; 14];
    stream.read_exact(&mut header)?;
    assert_eq!(&header[..6], b"i3-ipc");
    let size = u32::from_le_bytes(header[6..10].try_into().unwrap()) as usize;
    assert!(size <= 65536);
    let mut payload = vec![0; size];
    stream.read_exact(&mut payload)?;
    Ok(u32::from_le_bytes(header[10..14].try_into().unwrap()))
}
fn wait_value(rx: &mpsc::Receiver<Value>, predicate: impl Fn(&Value) -> bool) -> Value {
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        let value = rx
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .expect("bounded broker reply");
        if predicate(&value) {
            return value;
        }
    }
}
#[test]
fn real_broker_requires_current_inventory_and_observed_revision_before_dispatch() {
    let owner = Owned(Command::new("sleep").arg("30").spawn().unwrap());
    let directory =
        std::env::temp_dir().join(format!("lumen-inventory-wire-{}", std::process::id()));
    fs::create_dir(&directory).unwrap();
    fs::set_permissions(&directory, fs::Permissions::from_mode(0o700)).unwrap();
    let socket = directory.join("provider.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    listener.set_nonblocking(true).unwrap();
    let node = json!({"id":11,"type":"con","pid":owner.0.id(),"app_id":"fixture.native","name":"Owned fixture","shell":"xdg_shell","window":null,"foreign_toplevel_identifier":"owned-surface","focused":true,"lumen_state":{"version":1,"hidden":false,"minimised":false,"restore_workspace":"one"},"lumen_relationships":{"version":1,"parent_supported":true,"parent":null,"modal":null}});
    let tree = Arc::new(Mutex::new(
        json!({"type":"root","nodes":[{"type":"workspace","name":"one","nodes":[node]}]}),
    ));
    let commands = Arc::new(AtomicUsize::new(0));
    let inject_tree = Arc::new(AtomicBool::new(false));
    let inject_command = Arc::new(AtomicBool::new(false));
    let (events_tx, events_rx) = mpsc::channel();
    let provider_tree = tree.clone();
    let provider_commands = commands.clone();
    let tree_injection = inject_tree.clone();
    let command_injection = inject_command.clone();
    let provider = std::thread::spawn(move || {
        let accept = || {
            let deadline = Instant::now() + Duration::from_secs(8);
            loop {
                match listener.accept() {
                    Ok((stream, _)) => return stream,
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        assert!(Instant::now() < deadline, "bounded provider connection");
                        std::thread::sleep(Duration::from_millis(5));
                    }
                    Err(e) => panic!("{e}"),
                }
            }
        };
        let mut events = accept();
        events
            .set_read_timeout(Some(Duration::from_secs(8)))
            .unwrap();
        assert_eq!(receive(&mut events).unwrap().0, 2);
        send(&mut events, 2, "{\"success\":true}").unwrap();
        let mut injected_events = events.try_clone().unwrap();
        events_tx.send(events).unwrap();
        let mut query = accept();
        query
            .set_read_timeout(Some(Duration::from_secs(8)))
            .unwrap();
        while let Ok(kind) = query_kind(&mut query) {
            let reply = match kind {
                7 => json!({"native_focus_guard":2}),
                4 => {
                    if tree_injection.swap(false, Ordering::SeqCst) {
                        for change in ["title", "new", "close"] {
                            send(
                                &mut injected_events,
                                0x8000_0003,
                                &json!({"change":change,"container":{}}).to_string(),
                            )
                            .unwrap();
                        }
                    }
                    provider_tree.lock().unwrap().clone()
                }
                0 => {
                    provider_commands.fetch_add(1, Ordering::SeqCst);
                    if command_injection.swap(false, Ordering::SeqCst) {
                        for change in ["focus", "new", "close"] {
                            send(
                                &mut injected_events,
                                0x8000_0003,
                                &json!({"change":change,"container":{}}).to_string(),
                            )
                            .unwrap();
                        }
                    }
                    json!([{"success":true}])
                }
                _ => panic!("Unexpected provider query"),
            };
            if send(&mut query, kind, &reply.to_string()).is_err() {
                break;
            }
        }
    });
    let mut broker = Owned(
        Command::new(env!("CARGO_BIN_EXE_lumen-application-lifecycle"))
            .env("SWAYSOCK", &socket)
            .env("XDG_RUNTIME_DIR", &directory)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );
    let mut input = broker.0.stdin.take().unwrap();
    let output = broker.0.stdout.take().unwrap();
    let (tx, rx) = mpsc::channel();
    let reader = std::thread::spawn(move || {
        for line in BufReader::new(output).lines() {
            let Ok(line) = line else { break };
            if let Ok(value) = serde_json::from_str::<Value>(&line) {
                if tx.send(value).is_err() {
                    break;
                }
            }
        }
    });
    let initial = wait_value(&rx, |v| {
        v["nativeObservation"]["captures"]
            .as_array()
            .is_some_and(|a| a.len() == 1)
    });
    let observation = initial["nativeObservation"].clone();
    let request = |id: &str, witness: Value| json!({"action":"focus_native","requestId":id,"capture":observation["captures"][0],"initialFocus":observation["source"],"switchSpace":true,"inventoryWitness":witness});
    let mut invoke = |id: &str, witness: Value| {
        writeln!(input, "{}", request(id, witness)).unwrap();
        input.flush().unwrap();
        wait_value(&rx, |v| v["result"]["nativeFocus"]["requestId"] == id)
    };
    let old = observation["inventoryWitness"].clone();
    assert_eq!(
        invoke("valid", old.clone())["result"]["nativeFocus"]["requested"],
        true
    );
    assert_eq!(commands.load(Ordering::SeqCst), 1);
    assert_eq!(
        invoke("missing", Value::Null)["result"]["nativeFocus"]["requested"],
        false
    );
    let mut changed = old.clone();
    changed["digest"] = json!("0".repeat(64));
    assert_eq!(
        invoke("changed", changed)["result"]["nativeFocus"]["requested"],
        false
    );
    assert_eq!(commands.load(Ordering::SeqCst), 1);
    // An unrelated new view followed by restoration is observed before dispatch.
    let mut events = events_rx.recv_timeout(Duration::from_secs(8)).unwrap();
    for change in ["title", "focus"] {
        send(
            &mut events,
            0x8000_0003,
            &json!({"change":change,"container":{}}).to_string(),
        )
        .unwrap();
    }
    send(
        &mut events,
        0x8000_0003,
        "{\"change\":\"new\",\"container\":{}}",
    )
    .unwrap();
    let observed = wait_value(&rx, |v| {
        v["nativeObservation"]["inventoryWitness"]["revision"]
            .as_u64()
            .is_some_and(|r| r > old["revision"].as_u64().unwrap())
    });
    assert_eq!(
        observed["nativeObservation"]["inventoryWitness"]["digest"],
        old["digest"]
    );
    assert_eq!(
        invoke("observed-aba", old)["result"]["nativeFocus"]["requested"],
        false
    );
    assert_eq!(commands.load(Ordering::SeqCst), 1);
    drop(invoke);
    // Pause the actual broker so both IPC events and stdin are readable in the
    // same poll iteration. Event priority must invalidate the fresh witness.
    assert_eq!(
        unsafe { libc::kill(broker.0.id() as i32, libc::SIGSTOP) },
        0
    );
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        let status = fs::read_to_string(format!("/proc/{}/status", broker.0.id())).unwrap();
        if status
            .lines()
            .any(|line| line.starts_with("State:") && line.contains("T (stopped)"))
        {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "Broker did not stop before queue proof"
        );
        std::thread::sleep(Duration::from_millis(2));
    }
    for change in ["title", "focus"] {
        send(
            &mut events,
            0x8000_0003,
            &json!({"change":change,"container":{}}).to_string(),
        )
        .unwrap();
    }
    send(
        &mut events,
        0x8000_0003,
        "{\"change\":\"new\",\"container\":{}}",
    )
    .unwrap();
    send(
        &mut events,
        0x8000_0003,
        "{\"change\":\"close\",\"container\":{}}",
    )
    .unwrap();
    writeln!(
        input,
        "{}",
        request(
            "queued-aba",
            observed["nativeObservation"]["inventoryWitness"].clone()
        )
    )
    .unwrap();
    writeln!(
        input,
        "{}",
        request(
            "queued-second",
            observed["nativeObservation"]["inventoryWitness"].clone()
        )
    )
    .unwrap();
    input.flush().unwrap();
    assert_eq!(
        unsafe { libc::kill(broker.0.id() as i32, libc::SIGCONT) },
        0
    );
    let queued = wait_value(&rx, |v| {
        v["result"]["nativeFocus"]["requestId"] == "queued-aba"
    });
    assert_eq!(queued["result"]["nativeFocus"]["requested"], false);
    assert_eq!(commands.load(Ordering::SeqCst), 1);
    let second = wait_value(&rx, |v| {
        v["result"]["nativeFocus"]["requestId"] == "queued-second"
    });
    assert_eq!(second["result"]["nativeFocus"]["requested"], false);
    inject_tree.store(true, Ordering::SeqCst);
    writeln!(
        input,
        "{}",
        request(
            "barrier",
            second["nativeObservation"]["inventoryWitness"].clone()
        )
    )
    .unwrap();
    input.flush().unwrap();
    let barrier = wait_value(&rx, |v| {
        v["result"]["nativeFocus"]["requestId"] == "barrier"
    });
    assert_eq!(barrier["result"]["nativeFocus"]["requested"], false);
    assert_eq!(commands.load(Ordering::SeqCst), 1);
    inject_command.store(true, Ordering::SeqCst);
    let fresh = barrier["nativeObservation"]["inventoryWitness"].clone();
    writeln!(
        input,
        "{}\n{}",
        request("buffered-first", fresh.clone()),
        request("buffered-second", fresh)
    )
    .unwrap();
    input.flush().unwrap();
    let first = wait_value(&rx, |v| {
        v["result"]["nativeFocus"]["requestId"] == "buffered-first"
    });
    assert_eq!(first["result"]["nativeFocus"]["requested"], true);
    let second = wait_value(&rx, |v| {
        v["result"]["nativeFocus"]["requestId"] == "buffered-second"
    });
    assert_eq!(second["result"]["nativeFocus"]["requested"], false);
    assert_eq!(commands.load(Ordering::SeqCst), 2);
    // An incomplete queued event must retire the real observer before a later
    // native command; header waiting cannot block forever or skip the stream.
    events.write_all(b"i3-").unwrap();
    writeln!(
        input,
        "{}",
        request(
            "partial-event",
            second["nativeObservation"]["inventoryWitness"].clone()
        )
    )
    .unwrap();
    input.flush().unwrap();
    wait_value(&rx, |v| v["available"] == false);
    assert_eq!(commands.load(Ordering::SeqCst), 2);
    drop(input);
    drop(broker);
    drop(events);
    provider.join().unwrap();
    reader.join().unwrap();
    fs::remove_dir_all(directory).unwrap();
}
