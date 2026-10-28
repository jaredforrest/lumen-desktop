//! The public reader command boundary remains closed without a private bus.
use serde_json::Value;
use std::process::Command;

fn call(args: &[&str]) -> (i32, Value) {
    let output = Command::new(env!("CARGO_BIN_EXE_lumen-reader-commands"))
        .args(args)
        .env("DBUS_SESSION_BUS_ADDRESS", "")
        .output()
        .unwrap();
    let state = serde_json::from_slice(&output.stdout).unwrap();
    (output.status.code().unwrap(), state)
}

#[test]
fn disabled_unsupported_and_unavailable() {
    let (code, state) = call(&["where-am-i"]);
    assert_eq!(code, 1);
    assert_eq!(state["reason"], "disabled");

    let (code, state) = call(&["arbitrary"]);
    assert_eq!(code, 1);
    assert_eq!(state["reason"], "unsupported_command");

    let (code, state) = call(&["--enabled", "where-am-i"]);
    assert_eq!(code, 1);
    assert_eq!(state["reason"], "reader_unavailable");
    assert_eq!(state["dispatched"], false);

    let (code, state) = call(&["--enabled", "stop"]);
    assert_eq!(code, 1);
    assert_eq!(state["reason"], "reader_unavailable");
}
