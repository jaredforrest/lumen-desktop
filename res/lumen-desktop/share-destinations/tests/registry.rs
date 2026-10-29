include!("../src/main.rs");
use std::os::unix::fs::symlink;
struct Fixture {
    root: tempfile::TempDir,
    file: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let file = root.path().join("literal $(no-command) ; name.txt");
        fs::write(&file, b"Original synthetic bytes").unwrap();
        Self { root, file }
    }
    fn config(&self, p: &Provider, edit: impl FnOnce(&mut Value)) {
        let path = p.executable.with_extension("json");
        let mut v: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        edit(&mut v);
        fs::write(path, serde_json::to_vec(&v).unwrap()).unwrap()
    }
    fn provider(&self, id: &str, state: &str) -> Provider {
        let executable = self.root.path().join(id);
        symlink(
            env!("CARGO_BIN_EXE_lumen-share-provider-fixture"),
            &executable,
        )
        .unwrap();
        let advert = json!({"schema":1,"generation":"fixture-epoch-1","label":format!("Synthetic {id}"),"state":state,"detail":"Choose synthetic recipient","actions":["files"]});
        fs::write(executable.with_extension("json"),serde_json::to_vec(&json!({"advertisement":advert.to_string(),"request":self.root.path().join(format!("{id}.request")),"outcome":json!({"schema":1,"state":"cancelled","detail":"Synthetic cancellation"}).to_string()})).unwrap()).unwrap();
        Provider {
            id: id.into(),
            label: format!("Configured {id}"),
            executable,
            mode: "protocol".into(),
        }
    }
    fn request(&self, p: &Provider) -> Request {
        let m = self.file.metadata().unwrap();
        Request {
            id: p.id.clone(),
            fingerprint: describe(p).unwrap().fingerprint,
            files: vec![File {
                path: RawValue::from_string(serde_json::to_string(&self.file).unwrap()).unwrap(),
                device: m.dev().to_string(),
                inode: m.ino().to_string(),
            }],
        }
    }
    fn request_exists(&self, id: &str) -> bool {
        self.root.path().join(format!("{id}.request")).exists()
    }
}
#[test]
fn two_actual_endpoints_independent_and_offline() {
    let f = Fixture::new();
    let a = f.provider("first", "ready");
    let b = f.provider("second", "offline");
    let rows = listing(&[a, b]);
    assert_eq!(rows["destinations"][0]["state"], "ready");
    assert_eq!(rows["destinations"][1]["state"], "offline");
    assert_ne!(
        rows["destinations"][0]["fingerprint"],
        rows["destinations"][1]["fingerprint"]
    );
    assert!(!f.request_exists("first"));
}
#[test]
fn cancellation_preserves_literal_selection_and_original() {
    let f = Fixture::new();
    let p = f.provider("first", "ready");
    let req = f.request(&p);
    assert_eq!(invoke(&[p], &req).unwrap()["state"], "cancelled");
    let got: Value =
        serde_json::from_slice(&fs::read(f.root.path().join("first.request")).unwrap()).unwrap();
    assert_eq!(got["files"][0]["path"], f.file.to_str().unwrap());
    assert_eq!(fs::read(&f.file).unwrap(), b"Original synthetic bytes");
    assert!(!f.root.path().join("no-command").exists());
}
#[test]
fn declared_outcomes_are_explicit() {
    for state in ["completed", "opened", "offline", "error"] {
        let f = Fixture::new();
        let p = f.provider("first", "ready");
        f.config(&p, |c| {
            c["outcome"] =
                json!(json!({"schema":1,"state":state,"detail":"Synthetic outcome"}).to_string())
        });
        assert_eq!(
            invoke(std::slice::from_ref(&p), &f.request(&p)).unwrap()["state"],
            state
        );
    }
}
#[test]
fn going_offline_refuses_before_dispatch() {
    let f = Fixture::new();
    let p = f.provider("first", "ready");
    let r = f.request(&p);
    f.config(&p, |c| {
        let mut a: Value = serde_json::from_str(c["advertisement"].as_str().unwrap()).unwrap();
        a["state"] = json!("offline");
        c["advertisement"] = json!(a.to_string());
    });
    assert_eq!(invoke(&[p], &r).unwrap()["state"], "offline");
    assert!(!f.request_exists("first"));
}
fn changed(field: &str) {
    let f = Fixture::new();
    let p = f.provider("first", "ready");
    let r = f.request(&p);
    f.config(&p, |c| {
        let mut a: Value = serde_json::from_str(c["advertisement"].as_str().unwrap()).unwrap();
        a[field] = json!("changed");
        c["advertisement"] = json!(a.to_string())
    });
    assert!(
        invoke(&[p], &r)
            .unwrap_err()
            .contains("Destination changed")
    );
    assert!(!f.request_exists("first"));
}
#[test]
fn changed_advertisement_does_not_retarget() {
    changed("label")
}
#[test]
fn generation_change_same_label_readiness_refuses() {
    changed("generation")
}
#[test]
fn replaced_and_symlink_refuse() {
    for link in [false, true] {
        let f = Fixture::new();
        let p = f.provider("first", "ready");
        let r = f.request(&p);
        let old = f.file.with_extension("original");
        fs::rename(&f.file, &old).unwrap();
        if link {
            symlink(&old, &f.file).unwrap()
        } else {
            fs::write(&f.file, "replacement").unwrap()
        }
        assert!(invoke(&[p], &r).is_err());
        assert!(!f.request_exists("first"));
        assert_eq!(fs::read(old).unwrap(), b"Original synthetic bytes");
    }
}
#[test]
fn malformed_selection_is_refused() {
    let f = Fixture::new();
    let p = f.provider("first", "ready");
    for value in [
        json!({"path":"relative","device":"1","inode":"2"}),
        json!({"path":f.file,"device":true,"inode":"2"}),
        json!({"path":f.file,"device":"18446744073709551616","inode":"2"}),
        json!({"path":f.file,"device":"01","inode":"2"}),
        json!({"path":f.file,"device":"1","inode":"2","extra":1}),
    ] {
        let parsed = serde_json::from_value::<File>(value);
        assert!(parsed.is_err() || selection(&[parsed.unwrap()]).is_err())
    }
    assert!(selection(&[]).is_err());
    let r = f.request(&p);
    let mut repeated = r.files;
    let copy: File = serde_json::from_str(&serde_json::to_string(&repeated[0]).unwrap()).unwrap();
    repeated.push(copy);
    assert!(selection(&repeated).is_err());
}
#[test]
fn malformed_advertisements_fail_independently() {
    let f = Fixture::new();
    let p = f.provider("first", "ready");
    let base = json!({"schema":1,"generation":"epoch","label":"Fixture","state":"ready","detail":"Choose","actions":["files"]});
    for (field, value) in [
        ("schema", json!(true)),
        ("label", json!("bad\nlabel")),
        ("state", json!(9)),
        ("generation", json!("")),
        ("command", json!(["sh"])),
    ] {
        let mut a = base.clone();
        a[field] = value;
        f.config(&p, |c| c["advertisement"] = json!(a.to_string()));
        assert!(describe(&p).is_err(), "{field}");
    }
    let mut a = base;
    a.as_object_mut().unwrap().remove("generation");
    f.config(&p, |c| c["advertisement"] = json!(a.to_string()));
    assert!(describe(&p).is_err());
}
fn legacy(f: &Fixture) -> Provider {
    let mut p = f.provider("first", "ready");
    p.mode = "nearby".into();
    f.config(&p, |c| {
        c["advertisement"] =
            json!("{\"available\":true,\"canSend\":true,\"detail\":\"Choose recipient\"}")
    });
    p
}
#[test]
fn legacy_capability_absence_is_unavailable() {
    let f = Fixture::new();
    let p = legacy(&f);
    let r = f.request(&p);
    f.config(&p, |c| {
        c["advertisement"] = json!("{\"available\":true,\"canSend\":false}")
    });
    assert_eq!(describe(&p).unwrap().state, "unavailable");
    assert_eq!(invoke(&[p], &r).unwrap()["state"], "error");
}
#[test]
fn legacy_handoff_never_executes_send() {
    let f = Fixture::new();
    let p = legacy(&f);
    let r = f.request(&p);
    let result = invoke(std::slice::from_ref(&p), &r).unwrap();
    assert_eq!(result["command"], json!([p.executable, "send", f.file]));
    assert!(!f.request_exists("first"));
}
fn raw_filename(f: &Fixture, p: &Provider) -> Request {
    let mut bytes = f.root.path().as_os_str().as_encoded_bytes().to_vec();
    bytes.extend_from_slice(b"/raw-\xff.txt");
    let raw = PathBuf::from(std::ffi::OsString::from_vec(bytes));
    fs::write(&raw, b"raw unchanged").unwrap();
    let m = raw.metadata().unwrap();
    Request {
        id: p.id.clone(),
        fingerprint: describe(p).unwrap().fingerprint,
        files: vec![File {
            path: RawValue::from_string(format!("\"{}/raw-\\udcff.txt\"", f.root.path().display()))
                .unwrap(),
            device: m.dev().to_string(),
            inode: m.ino().to_string(),
        }],
    }
}
#[test]
fn json_protocol_preserves_non_utf8_filename() {
    let f = Fixture::new();
    let p = f.provider("first", "ready");
    let r = raw_filename(&f, &p);
    assert_eq!(invoke(&[p], &r).unwrap()["state"], "cancelled");
    assert!(
        fs::read_to_string(f.root.path().join("first.request"))
            .unwrap()
            .contains("\\udcff")
    );
}
#[test]
fn legacy_refuses_non_utf8_argv() {
    let f = Fixture::new();
    let p = legacy(&f);
    let r = raw_filename(&f, &p);
    assert!(invoke(&[p], &r).unwrap_err().contains("cannot preserve"));
    assert!(!f.request_exists("first"));
}
#[test]
fn diagnostic_flood_failure_are_bounded_and_reaped() {
    for behavior in ["diagnostic", "flood", "failure"] {
        let f = Fixture::new();
        let p = f.provider("first", "ready");
        let pid = f.root.path().join("pid");
        f.config(&p, |c| {
            c["behavior"] = json!(behavior);
            c["pid"] = json!(pid)
        });
        assert!(
            exchange(
                &p.executable,
                "share-describe",
                None,
                Duration::from_secs(1)
            )
            .is_err()
        );
        let id: u32 = fs::read_to_string(pid).unwrap().parse().unwrap();
        assert!(!Path::new(&format!("/proc/{id}")).exists());
    }
}
#[test]
fn blocked_stdin_and_forked_pipes_reap_before_return() {
    for behavior in ["nonreader", "descendant"] {
        let f = Fixture::new();
        let p = f.provider("first", "ready");
        let child = f.root.path().join("child");
        let pid = f.root.path().join("pid");
        f.config(&p, |c| {
            c["behavior"] = json!(behavior);
            c["pid"] = json!(pid);
            c["child"] = json!(child)
        });
        assert!(
            exchange(
                &p.executable,
                "share-files",
                Some(&vec![b'x'; LIMIT]),
                Duration::from_millis(180)
            )
            .is_err()
        );
        for path in [pid, child] {
            if path.exists() {
                let id: u32 = fs::read_to_string(path).unwrap().parse().unwrap();
                assert!(!Path::new(&format!("/proc/{id}")).exists())
            }
        }
    }
}
#[test]
fn duplicate_and_injected_outcomes_refused() {
    let f = Fixture::new();
    let p = f.provider("first", "ready");
    for raw in [
        "{\"schema\":1,\"state\":\"handoff\",\"detail\":\"no\"}",
        "{\"schema\":1,\"state\":\"completed\",\"state\":\"error\",\"detail\":\"no\"}",
    ] {
        f.config(&p, |c| c["outcome"] = json!(raw));
        assert!(invoke(std::slice::from_ref(&p), &f.request(&p)).is_err());
    }
}
#[test]
fn scoped_subreaper_role_restored() {
    let f = Fixture::new();
    let p = f.provider("first", "ready");
    let mut old = 0;
    unsafe {
        libc::prctl(libc::PR_GET_CHILD_SUBREAPER, &mut old, 0, 0, 0);
    }
    describe(&p).unwrap();
    let mut now = 0;
    unsafe {
        libc::prctl(libc::PR_GET_CHILD_SUBREAPER, &mut now, 0, 0, 0);
    }
    assert_eq!(old, now);
    f.config(&p, |c| c["behavior"] = json!("failure"));
    assert!(describe(&p).is_err());
    unsafe {
        libc::prctl(libc::PR_GET_CHILD_SUBREAPER, &mut now, 0, 0, 0);
    }
    assert_eq!(old, now);
}
#[test]
fn missing_configuration_empty_mutable_refused() {
    assert!(configured(None).unwrap().is_empty());
    let f = Fixture::new();
    assert!(configured(Some(f.file.clone())).is_err());
}
#[test]
fn real_interrupt_reaps_endpoint_and_preserves_original() {
    let f = Fixture::new();
    let p = f.provider("first", "ready");
    let pid = f.root.path().join("pid");
    f.config(&p, |c| {
        c["behavior"] = json!("hang");
        c["pid"] = json!(pid)
    });
    let executable = std::env::current_exe().unwrap();
    let test = Command::new(executable)
        .args(["--exact", "interrupt_child", "--ignored", "--nocapture"])
        .env("SHARE_TEST_ENDPOINT", &p.executable)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    while !pid.exists() {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(5));
    }
    unsafe {
        libc::kill(test.id() as i32, libc::SIGTERM);
    }
    let output = test.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("unconfirmed"));
    let id: u32 = fs::read_to_string(pid).unwrap().parse().unwrap();
    assert!(!Path::new(&format!("/proc/{id}")).exists());
    assert_eq!(fs::read(&f.file).unwrap(), b"Original synthetic bytes");
}
#[test]
#[ignore]
fn interrupt_child() {
    unsafe {
        libc::signal(libc::SIGTERM, interrupt as *const () as usize);
    }
    let endpoint = PathBuf::from(std::env::var_os("SHARE_TEST_ENDPOINT").unwrap());
    let error = exchange(&endpoint, "share-describe", None, Duration::from_secs(10)).unwrap_err();
    assert!(error.contains("unconfirmed"));
    println!("{error}");
}
