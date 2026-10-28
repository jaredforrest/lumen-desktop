use serde_json::{Value,json};
use std::fs;
use std::io::{Read,Write};
use std::os::unix::net::UnixListener;
use std::process::Command;
use std::time::{Duration,Instant};

#[test]
fn exact_disposable_process_requires_one_use_confirmation() {
    let root=std::env::temp_dir().join(format!("lumen-force-quit-cli-{}",std::process::id()));
    fs::create_dir(&root).unwrap();
    let socket=root.join("sway.sock");
    let listener=UnixListener::bind(&socket).unwrap();
    listener.set_nonblocking(true).unwrap();
    let mut child=Command::new("sleep").arg("30").spawn().unwrap();
    let tree=json!({"id":1,"type":"root","nodes":[{"id":2,"type":"con","app_id":"org.example.Fixture","name":"Fixture","pid":child.id(),"shell":"xdg_shell","nodes":[],"floating_nodes":[]}],"floating_nodes":[]});
    let server=std::thread::spawn(move||{
        let deadline=Instant::now()+Duration::from_secs(8);
        for _ in 0..2 {
            let mut stream=loop{match listener.accept(){Ok((stream,_))=>break stream,Err(error)if error.kind()==std::io::ErrorKind::WouldBlock&&Instant::now()<deadline=>std::thread::sleep(Duration::from_millis(10)),Err(error)=>panic!("fake compositor accept: {error}")}};
            let mut header=[0_u8;14];stream.read_exact(&mut header).unwrap();
            assert_eq!(&header[..6],b"i3-ipc");
            let size=u32::from_le_bytes(header[6..10].try_into().unwrap()) as usize;
            let kind=u32::from_le_bytes(header[10..14].try_into().unwrap());
            assert_eq!(kind,4);
            let mut body=vec![0_u8;size];stream.read_exact(&mut body).unwrap();
            let bytes=serde_json::to_vec(&tree).unwrap();
            let mut reply=b"i3-ipc".to_vec();reply.extend_from_slice(&(bytes.len() as u32).to_le_bytes());reply.extend_from_slice(&kind.to_le_bytes());reply.extend_from_slice(&bytes);
            stream.write_all(&reply).unwrap();
        }
    });
    let binary=env!("CARGO_BIN_EXE_lumen-application-control");
    let prepare=Command::new(binary).env("SWAYSOCK",&socket).env("XDG_RUNTIME_DIR",&root).args(["--request-id","11","prepare","[{\"appId\":\"org.example.Fixture\",\"title\":\"Fixture\"}]"]).output().unwrap();
    assert!(prepare.status.success(),"{}",String::from_utf8_lossy(&prepare.stdout));
    let offer:Value=serde_json::from_slice(&prepare.stdout).unwrap();
    assert_eq!(offer["processCount"],1);
    let token=offer["token"].as_str().unwrap();
    let confirm=Command::new(binary).env("SWAYSOCK",&socket).env("XDG_RUNTIME_DIR",&root).args(["--request-id","12","confirm",token]).output().unwrap();
    assert!(confirm.status.success(),"{}",String::from_utf8_lossy(&confirm.stdout));
    let result:Value=serde_json::from_slice(&confirm.stdout).unwrap();
    assert_eq!(result["terminationRequested"],1);
    assert!(!child.wait().unwrap().success());
    let repeat=Command::new(binary).env("SWAYSOCK",&socket).env("XDG_RUNTIME_DIR",&root).args(["confirm",token]).output().unwrap();
    assert!(!repeat.status.success());
    server.join().unwrap();
    fs::remove_dir_all(root).unwrap();
}
