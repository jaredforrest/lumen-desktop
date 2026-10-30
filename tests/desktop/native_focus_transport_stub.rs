//! Owned stdio child for transport faults; never opens a listener or compositor.
use std::{
    env,
    fs::{self, OpenOptions},
    io::{self, BufRead, Write},
    path::PathBuf,
};
fn append(path: PathBuf, text: &str) {
    writeln!(
        OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .unwrap(),
        "{text}"
    )
    .unwrap();
}
fn emit(text: &str) {
    println!("{text}");
    io::stdout().flush().unwrap();
}
fn main() {
    let root = PathBuf::from(env::var("LUMEN_NATIVE_FOCUS_ROOT").unwrap());
    let starts = root.join("starts");
    let previous = fs::read_to_string(&starts).unwrap_or_default();
    if let Some(pid) = previous.lines().last() {
        if PathBuf::from(format!("/proc/{pid}")).exists() {
            fs::write(root.join("overlapping-owned-child"), pid).unwrap();
        }
    }
    let generation = previous.lines().count();
    let stat = fs::read_to_string("/proc/self/stat").unwrap();
    let start = stat
        .rsplit_once(')')
        .unwrap()
        .1
        .split_whitespace()
        .nth(19)
        .unwrap();
    append(
        root.join("identities"),
        &format!("{} {start}", std::process::id()),
    );
    append(starts, &std::process::id().to_string());
    let mode = if generation == 0 {
        env::var("LUMEN_NATIVE_FOCUS_MODE").unwrap()
    } else {
        "success".into()
    };
    let mut count = 0;
    for line in io::stdin().lock().lines() {
        let line = line.unwrap();
        if line.contains("\"action\":\"status\"") {
            emit(
                "{\"available\":true,\"sessionEpoch\":\"synthetic-owned-epoch\",\"applications\":[],\"activationHistory\":[],\"nativeWindows\":[]}",
            );
            continue;
        }
        if !line.contains("\"action\":\"focus_native\"") {
            continue;
        }
        let marker = "\"requestId\":\"";
        let rest = line.split_once(marker).unwrap().1;
        let request = rest.split('"').next().unwrap();
        append(
            root.join("requests"),
            &format!("{} {request}", std::process::id()),
        );
        count += 1;
        match mode.as_str() {
            "malformed" => emit("{"),
            "unavailable" => emit("{\"available\":false}"),
            "wrong-id" => emit(
                "{\"available\":true,\"sessionEpoch\":\"synthetic-owned-epoch\",\"result\":{\"nativeFocus\":{\"requestId\":\"wrong-owned-request\",\"requested\":true}}}",
            ),
            "stall" => {}
            "exit" => return,
            _ => {
                emit(&format!(
                    "{{\"available\":true,\"sessionEpoch\":\"synthetic-owned-epoch\",\"result\":{{\"nativeFocus\":{{\"requestId\":\"{request}\",\"requested\":true}}}}}}"
                ));
                if count >= 2 || generation > 0 {
                    emit(
                        "{\"available\":true,\"sessionEpoch\":\"synthetic-owned-epoch\",\"result\":{\"nativeFocus\":{\"requestId\":\"unsolicited-idle-result\",\"requested\":true}}}",
                    );
                }
            }
        }
    }
}
