//! Real service/client endpoints, isolated files and ordinary personal Trash recovery.
use serde_json::{Value, json};
use std::{
    fs,
    io::Write,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};
struct Service {
    root: PathBuf,
    child: Child,
}
impl Service {
    fn new() -> Self {
        let uuid = fs::read_to_string("/proc/sys/kernel/random/uuid").unwrap();
        let root = Path::new("/tmp").join(format!("lcu-{}", uuid.trim()));
        fs::create_dir(&root).unwrap();
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
        for folder in ["runtime", "home", "source", "target", "state", "data"] {
            fs::create_dir(root.join(folder)).unwrap();
        }
        let child = Command::new(env!("CARGO_BIN_EXE_lumen-file-jobs"))
            .arg("serve")
            .env("HOME", root.join("home"))
            .env("XDG_RUNTIME_DIR", root.join("runtime"))
            .env("XDG_STATE_HOME", root.join("state"))
            .env("XDG_DATA_HOME", root.join("data"))
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let value = Self { root, child };
        let deadline = Instant::now() + Duration::from_secs(5);
        while !value
            .root
            .join("runtime/lumen-file-operations.sock")
            .exists()
        {
            assert!(Instant::now() < deadline, "service readiness");
            thread::sleep(Duration::from_millis(5));
        }
        value
    }
    fn request(&self, value: Value) -> Value {
        let mut child = Command::new(env!("CARGO_BIN_EXE_lumen-file-jobs"))
            .arg("request")
            .env("XDG_RUNTIME_DIR", self.root.join("runtime"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        writeln!(child.stdin.take().unwrap(), "{value}").unwrap();
        let output = child.wait_with_output().unwrap();
        assert!(output.status.success(), "client failed");
        serde_json::from_slice(&output.stdout).unwrap()
    }
    fn state(&self) -> Value {
        self.request(json!({"action":"job-status"}))
    }
    fn run(&self, operation: &str, paths: &[PathBuf], conflict: &str) -> Value {
        let response=self.request(json!({"action":"job-start","operation":operation,"paths":paths,"destination":self.root.join("target"),"conflict":conflict}));
        let id = response["job"].as_str().expect("job admitted");
        // Multi-item jobs perform a durable publish/history flush per item.
        // Keep individual Undo bounded separately from the 25-item retention case.
        let deadline = Instant::now() + Duration::from_secs(20 + 2 * paths.len() as u64);
        loop {
            let value = self.state();
            let job = value["jobs"]
                .as_array()
                .unwrap()
                .iter()
                .find(|job| job["id"] == id)
                .unwrap();
            if !["running", "cancelling"].contains(&job["state"].as_str().unwrap()) {
                return job.clone();
            }
            assert!(Instant::now() < deadline, "job deadline");
            thread::sleep(Duration::from_millis(5));
        }
    }
}
impl Drop for Service {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = fs::remove_dir_all(&self.root);
    }
}
#[test]
fn endpoint_copy_history_retains_only_last_twenty_created_items() {
    let s = Service::new();
    let mut paths = Vec::new();
    for index in 0..25 {
        let path = s.root.join(format!("source/item-{index:02}"));
        fs::write(&path, b"source remains").unwrap();
        paths.push(path);
    }
    assert_eq!(s.run("copy", &paths, "skip")["state"], "complete");
    for index in (5..25).rev() {
        assert_eq!(
            s.state()["undoLabel"],
            format!("Undo Copy of “item-{index:02}”")
        );
        assert_eq!(s.run("undo", &[], "skip")["state"], "complete");
    }
    assert_eq!(s.state()["canUndo"], false);
    for index in 0..25 {
        assert!(paths[index].exists());
        assert_eq!(
            s.root.join(format!("target/item-{index:02}")).exists(),
            index < 5
        );
    }
}
#[test]
fn endpoint_ineligible_copy_preserves_named_earlier_undo() {
    let s = Service::new();
    let first = s.root.join("source/earlier");
    fs::write(&first, b"eligible").unwrap();
    assert_eq!(s.run("copy", &[first], "skip")["state"], "complete");
    let large = s.root.join("source/large");
    fs::File::create(&large)
        .unwrap()
        .set_len(65 * 1024 * 1024)
        .unwrap();
    let result = s.run("copy", &[large], "skip");
    assert_eq!(result["state"], "complete");
    assert!(
        result["undoWarning"]
            .as_str()
            .unwrap()
            .contains("Earlier eligible")
    );
    assert_eq!(s.state()["undoLabel"], "Undo Copy of “earlier”");
    assert_eq!(s.run("undo", &[], "skip")["state"], "complete");
    assert!(s.root.join("target/large").exists());
    assert!(!s.root.join("target/earlier").exists());
}
#[test]
fn endpoint_copy_undo_uses_trash_and_existing_put_back() {
    let s = Service::new();
    let source = s.root.join("source/a");
    fs::write(&source, b"original").unwrap();
    assert_eq!(
        s.run("copy", &[source.clone()], "skip")["state"],
        "complete"
    );
    assert_eq!(
        s.state()["canUndo"],
        true,
        "successful eligible Copy has an Undo record"
    );
    assert_eq!(s.state()["undoKind"], "copy");
    assert_eq!(s.state()["undoLabel"], "Undo Copy of “a”");
    let result = s.run("undo", &[], "skip");
    assert_eq!(result["state"], "complete", "{result}");
    assert!(!s.root.join("target/a").exists());
    assert_eq!(fs::read(&source).unwrap(), b"original");
    let recovered = PathBuf::from(
        result["recoveryPath"]
            .as_str()
            .expect("ordinary Trash recovery path"),
    );
    assert_eq!(fs::read(&recovered).unwrap(), b"original");
    let id = recovered.file_name().unwrap().to_str().unwrap();
    assert!(
        s.root
            .join(format!("data/Trash/info/{id}.trashinfo"))
            .is_file()
    );
    assert_eq!(s.run("restore", &[recovered], "skip")["state"], "complete");
    assert_eq!(fs::read(s.root.join("target/a")).unwrap(), b"original");
    assert_eq!(fs::read(source).unwrap(), b"original");
}
#[test]
fn endpoint_tree_and_symlink_undo_preserve_originals() {
    let s = Service::new();
    let tree = s.root.join("source/tree");
    fs::create_dir_all(tree.join("nested")).unwrap();
    fs::write(tree.join("nested/file"), b"retained tree").unwrap();
    std::os::unix::fs::symlink("nested/file", tree.join("link")).unwrap();
    assert_eq!(s.run("copy", &[tree.clone()], "skip")["state"], "complete");
    let undo = s.run("undo", &[], "skip");
    assert_eq!(undo["state"], "complete", "{undo}");
    let recovered = PathBuf::from(undo["recoveryPath"].as_str().unwrap());
    assert_eq!(
        fs::read(recovered.join("nested/file")).unwrap(),
        b"retained tree"
    );
    assert_eq!(
        fs::read_link(recovered.join("link")).unwrap(),
        Path::new("nested/file")
    );
    assert!(tree.join("nested/file").exists());
    assert_eq!(s.run("restore", &[recovered], "skip")["state"], "complete");
    let link = s.root.join("source/top-link");
    std::os::unix::fs::symlink("tree/nested/file", &link).unwrap();
    assert_eq!(s.run("copy", &[link.clone()], "skip")["state"], "complete");
    let undo = s.run("undo", &[], "skip");
    assert_eq!(undo["state"], "complete", "{undo}");
    assert_eq!(
        fs::read_link(Path::new(undo["recoveryPath"].as_str().unwrap())).unwrap(),
        Path::new("tree/nested/file")
    );
    assert!(link.is_symlink());
    assert_eq!(
        fs::read(tree.join("nested/file")).unwrap(),
        b"retained tree"
    );
}
#[test]
fn endpoint_changed_replaced_new_child_and_parent_refuse() {
    for kind in ["changed", "replaced", "new-child", "parent"] {
        let s = Service::new();
        let source = s.root.join("source/item");
        if kind == "new-child" {
            fs::create_dir(&source).unwrap();
            fs::write(source.join("a"), b"source").unwrap();
        } else {
            fs::write(&source, b"source").unwrap();
        }
        assert_eq!(
            s.run("copy", &[source.clone()], "skip")["state"],
            "complete"
        );
        let target = s.root.join("target/item");
        match kind {
            "changed" => {
                let meta = fs::metadata(&target).unwrap();
                fs::write(&target, b"edited").unwrap();
                use std::os::unix::fs::MetadataExt;
                let name = std::ffi::CString::new(target.as_os_str().as_encoded_bytes()).unwrap();
                let times = [
                    libc::timespec {
                        tv_sec: 0,
                        tv_nsec: libc::UTIME_OMIT,
                    },
                    libc::timespec {
                        tv_sec: meta.mtime(),
                        tv_nsec: meta.mtime_nsec(),
                    },
                ];
                assert_eq!(
                    unsafe {
                        libc::utimensat(
                            libc::AT_FDCWD,
                            name.as_ptr(),
                            times.as_ptr(),
                            libc::AT_SYMLINK_NOFOLLOW,
                        )
                    },
                    0
                );
            }
            "replaced" => {
                fs::remove_file(&target).unwrap();
                fs::write(&target, b"replacement").unwrap();
            }
            "new-child" => fs::write(target.join("new"), b"new work").unwrap(),
            "parent" => {
                fs::rename(s.root.join("target"), s.root.join("old-target")).unwrap();
                fs::create_dir(s.root.join("target")).unwrap();
                fs::write(&target, b"foreign").unwrap();
            }
            _ => unreachable!(),
        }
        assert_eq!(s.run("undo", &[], "skip")["state"], "failed", "{kind}");
        assert!(target.exists(), "{kind}");
        assert!(source.exists());
        if kind == "new-child" {
            assert_eq!(fs::read(target.join("new")).unwrap(), b"new work");
        }
    }
}
#[test]
fn endpoint_collisions_and_partial_batch_only_undo_created_outputs() {
    let s = Service::new();
    let a = s.root.join("source/a");
    let b = s.root.join("source/b");
    fs::write(&a, b"copy a").unwrap();
    fs::write(&b, b"copy b").unwrap();
    fs::write(s.root.join("target/a"), b"pre-existing").unwrap();
    assert_eq!(
        s.run("copy", &[a.clone()], "keep-both")["state"],
        "complete"
    );
    assert!(s.root.join("target/a copy 2").exists());
    assert_eq!(s.run("undo", &[], "skip")["state"], "complete");
    assert!(!s.root.join("target/a copy 2").exists());
    assert_eq!(fs::read(s.root.join("target/a")).unwrap(), b"pre-existing");
    let partial = s.run("copy", &[a.clone(), b.clone()], "skip");
    assert_eq!(partial["state"], "partial");
    assert_eq!(partial["completed"], 1);
    assert_eq!(s.run("undo", &[], "skip")["state"], "complete");
    assert!(!s.root.join("target/b").exists());
    assert_eq!(fs::read(s.root.join("target/a")).unwrap(), b"pre-existing");
    assert_eq!(fs::read(a).unwrap(), b"copy a");
    assert_eq!(fs::read(b).unwrap(), b"copy b");
}
#[test]
fn endpoint_copy_limits_warn_without_failing_successful_copy() {
    for kind in ["bytes", "depth"] {
        let s = Service::new();
        let source = s.root.join("source/item");
        if kind == "bytes" {
            let file = fs::File::create(&source).unwrap();
            file.set_len(64 * 1024 * 1024 + 1).unwrap();
        } else {
            let mut path = source.clone();
            for _ in 0..66 {
                path = path.join("d");
            }
            fs::create_dir_all(path).unwrap();
        }
        let copy = s.run("copy", &[source.clone()], "skip");
        assert_eq!(copy["state"], "complete", "{copy}");
        assert!(copy["errors"].as_array().unwrap().is_empty());
        assert!(
            copy["undoWarning"]
                .as_str()
                .unwrap()
                .contains("Undo unavailable")
        );
        assert_eq!(s.state()["canUndo"], false);
        assert!(source.exists());
        assert!(s.root.join("target/item").exists());
    }
}
#[test]
fn endpoint_trash_preflight_and_post_publication_failure_preserve_data() {
    for kind in ["bad-root", "unwritable-info"] {
        let s = Service::new();
        let source = s.root.join("source/a");
        fs::write(&source, b"source remains").unwrap();
        assert_eq!(
            s.run("copy", &[source.clone()], "skip")["state"],
            "complete"
        );
        if kind == "bad-root" {
            fs::write(s.root.join("data/Trash"), b"existing non-folder").unwrap();
        } else {
            fs::create_dir_all(s.root.join("data/Trash/info")).unwrap();
            fs::set_permissions(
                s.root.join("data/Trash/info"),
                fs::Permissions::from_mode(0o500),
            )
            .unwrap();
        }
        let undo = s.run("undo", &[], "skip");
        assert_eq!(undo["state"], "failed", "{kind}: {undo}");
        assert_eq!(fs::read(&source).unwrap(), b"source remains");
        if kind == "bad-root" {
            assert_eq!(
                fs::read(s.root.join("target/a")).unwrap(),
                b"source remains"
            );
        } else {
            let recovery = PathBuf::from(
                undo["recoveryPath"]
                    .as_str()
                    .expect("failed publish exposes data"),
            );
            assert_eq!(fs::read(recovery).unwrap(), b"source remains");
            assert!(
                undo["undoMessage"]
                    .as_str()
                    .unwrap()
                    .to_lowercase()
                    .contains("recoverable")
            );
            assert_eq!(s.state()["canUndo"], false);
        }
    }
}
#[test]
fn endpoint_cross_volume_trash_refuses_before_claim() {
    use std::os::unix::fs::MetadataExt;
    let s = Service::new();
    let shm = Path::new("/dev/shm");
    if !shm.is_dir() || fs::metadata(shm).unwrap().dev() == fs::metadata(&s.root).unwrap().dev() {
        eprintln!("Cross-volume endpoint case unavailable in this environment");
        return;
    }
    let other = shm.join(format!("lcu-{}", s.child.id()));
    fs::create_dir(&other).unwrap();
    fs::set_permissions(&other, fs::Permissions::from_mode(0o700)).unwrap();
    fs::remove_dir(s.root.join("data")).unwrap();
    std::os::unix::fs::symlink(&other, s.root.join("data")).unwrap();
    let source = s.root.join("source/a");
    fs::write(&source, b"cross-volume retained").unwrap();
    assert_eq!(
        s.run("copy", &[source.clone()], "skip")["state"],
        "complete"
    );
    let undo = s.run("undo", &[], "skip");
    assert_eq!(undo["state"], "failed");
    assert!(undo["errors"].to_string().contains("same volume"));
    assert_eq!(
        fs::read(s.root.join("target/a")).unwrap(),
        b"cross-volume retained"
    );
    assert_eq!(fs::read(source).unwrap(), b"cross-volume retained");
    fs::remove_dir_all(other).unwrap();
}
