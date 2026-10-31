use super::*;
use std::os::unix::{fs::PermissionsExt, net::UnixListener};

struct Directory(PathBuf);
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
struct Peer(libc::pid_t);
impl Peer {
    fn new() -> Self {
        let pid = unsafe { libc::fork() };
        assert!(pid >= 0);
        if pid == 0 {
            // No allocation, Rust destructors or exec transition in the child.
            unsafe {
                libc::pause();
                libc::_exit(0)
            };
        }
        Self(pid)
    }
}
impl Drop for Peer {
    fn drop(&mut self) {
        // This is our unreaped child; no other path waits on this PID.
        unsafe { libc::kill(self.0, libc::SIGKILL) };
        loop {
            let result = unsafe { libc::waitpid(self.0, std::ptr::null_mut(), 0) };
            if result == self.0
                || result < 0
                    && std::io::Error::last_os_error().raw_os_error() == Some(libc::ECHILD)
            {
                break;
            }
            if result < 0
                && std::io::Error::last_os_error().kind() != std::io::ErrorKind::Interrupted
            {
                break;
            }
        }
    }
}

#[test]
fn ordered_batch_preserves_focus_and_aba_without_intermediate_durable_publication() {
    let directory =
        Directory(std::env::temp_dir().join(format!("lumen-pending-batch-{}", std::process::id())));
    fs::create_dir(&directory.0).unwrap();
    fs::set_permissions(&directory.0, fs::Permissions::from_mode(0o700)).unwrap();
    let path = directory.0.join("compositor.sock");
    let _listener = UnixListener::bind(&path).unwrap();
    let identity = epoch(&path).unwrap();
    let peers = [Peer::new(), Peer::new(), Peer::new()];
    let nodes=peers.iter().enumerate().map(|(i,peer)|json!({"id":10+i,"pid":peer.0,"app_id":format!("app{i}"),"shell":"xdg_shell","window":null,"foreign_toplevel_identifier":format!("surface-{i}"),"focused":i==2,"lumen_relationships":{"version":1,"parent_supported":true,"parent":null,"modal":null},"lumen_state":{"version":1,"hidden":false,"minimised":false,"restore_workspace":"one"}})).collect::<Vec<_>>();
    let mut lifecycle = Lifecycle::default();
    lifecycle.refresh(&identity, &nodes);
    let before = lifecycle.native_observation().unwrap()["inventoryWitness"].clone();
    let mut session = SessionEnd::new(unsafe { libc::getpid() } as u32);
    let protocol = Cooperative;
    let mut store = Some(
        ObservationStore::new(
            &directory.0,
            &identity,
            "01234567-89ab-cdef-0123-456789abcdef",
        )
        .unwrap(),
    );
    let journal = store
        .as_ref()
        .unwrap()
        .directory
        .join(format!("{identity}.json"));
    let (mut events, mut writer) = UnixStream::pair().unwrap();
    // The same final tree hides this new/close ABA; both events must invalidate.
    let mut batch = vec![
        json!({"change":"new","container":nodes[0]}),
        json!({"change":"close","container":nodes[0]}),
    ];
    for node in &nodes {
        let mut focused = node.clone();
        focused["focused"] = json!(true);
        batch.push(json!({"change":"focus","container":focused}));
    }
    for event in &batch {
        send(&mut writer, 0x8000_0003, &event.to_string()).unwrap();
    }
    let count = batch.len();
    let (mut query, mut server) = UnixStream::pair().unwrap();
    server
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    let tree = json!({"id":1,"type":"root","nodes":[{"id":2,"type":"workspace","name":"one","nodes":nodes}],"floating_nodes":[]});
    let server = std::thread::spawn(move || {
        for _ in 0..count {
            let mut header = [0; 14];
            server.read_exact(&mut header).unwrap();
            assert_eq!(&header[..6], b"i3-ipc");
            assert_eq!(u32::from_le_bytes(header[6..10].try_into().unwrap()), 0);
            assert_eq!(u32::from_le_bytes(header[10..].try_into().unwrap()), 4);
            send(&mut server, 4, &tree.to_string()).unwrap();
        }
    });
    let mut latest = nodes.clone();
    assert_eq!(
        pending(
            &mut events,
            &mut latest,
            &mut query,
            &path,
            &identity,
            &mut lifecycle,
            &mut session,
            &protocol,
            &mut store
        )
        .unwrap(),
        Some(true)
    );
    server.join().unwrap();
    assert_eq!(
        lifecycle.activation_history.order(),
        ["app2", "app1", "app0"]
    );
    assert!(
        lifecycle.require_inventory(&before).is_err(),
        "Observed same-final-state ABA retained old authority"
    );
    assert!(
        !journal.exists(),
        "Event sampling performed intermediate durable publication"
    );
    let mut last = None;
    emit(true, &lifecycle, &session, &mut store, &mut last, "", None);
    assert!(journal.is_file());
    assert!(last.is_some());
    let recovered = store.as_mut().unwrap().load_state();
    assert_eq!(recovered.history.order(), ["app2", "app1", "app0"]);
    assert_eq!(
        recovered
            .windows
            .value()
            .as_array()
            .unwrap()
            .iter()
            .map(|row| row["conId"].as_i64().unwrap())
            .collect::<Vec<_>>(),
        vec![12, 11, 10]
    );
}
