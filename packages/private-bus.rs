//! Start a disposable session bus while preserving native argument bytes.
use std::{env, os::unix::process::CommandExt, process::Command};

fn main() {
    let arguments = env::args_os().skip(1).collect::<Vec<_>>();
    let mut command = Command::new(env!("LUMEN_DBUS_RUN_SESSION"));
    if !arguments.iter().any(|argument| argument.as_encoded_bytes().starts_with(b"--config-file=")) {
        command.arg(concat!("--config-file=", env!("LUMEN_DBUS_TEST_CONFIG")));
    }
    let error = command.args(arguments).exec();
    eprintln!("Private session bus could not start: {error}");
    std::process::exit(1);
}
