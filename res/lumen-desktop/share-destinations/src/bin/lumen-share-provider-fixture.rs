//! Disposable protocol endpoint used only by the package's behavioral tests.
use serde_json::{Value, json};
use std::{
    fs,
    io::{Read, Write},
    time::Duration,
};
fn main() {
    let exe = std::env::args_os().next().unwrap();
    let cfg: Value = serde_json::from_slice(
        &fs::read(std::path::PathBuf::from(exe).with_extension("json")).unwrap(),
    )
    .unwrap();
    let action = std::env::args().nth(1).unwrap();
    let behavior = cfg["behavior"].as_str().unwrap_or("");
    if let Some(path) = cfg["pid"].as_str() {
        fs::write(path, std::process::id().to_string()).unwrap();
    }
    if behavior == "descendant" {
        let pid = unsafe { libc::fork() };
        if pid == 0 {
            if let Some(path) = cfg["child"].as_str() {
                fs::write(path, std::process::id().to_string()).unwrap();
            }
            loop {
                std::thread::sleep(Duration::from_secs(1));
            }
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    if matches!(behavior, "hang" | "descendant" | "nonreader") {
        loop {
            std::thread::sleep(Duration::from_secs(1));
        }
    }
    if behavior == "diagnostic" {
        eprintln!("synthetic diagnostic");
        return;
    }
    if behavior == "flood" {
        let _ = std::io::stdout().write_all(&vec![b'x'; 131072]);
        return;
    }
    if behavior == "failure" {
        std::process::exit(3)
    }
    if action == "share-describe" || action == "status" {
        print!("{}", cfg["advertisement"].as_str().unwrap());
        return;
    }
    if action == "share-files" {
        let mut data = vec![];
        std::io::stdin().read_to_end(&mut data).unwrap();
        fs::write(cfg["request"].as_str().unwrap(), &data).unwrap();
        println!(
            "{}",
            cfg["outcome"].as_str().unwrap_or(
                &json!({"schema":1,"state":"cancelled","detail":"Synthetic cancellation"})
                    .to_string()
            )
        );
        return;
    }
    panic!("Unexpected provider action");
}
