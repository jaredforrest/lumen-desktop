//! Start a disposable session bus while preserving native argument bytes.
use std::{env, ffi::OsString, os::unix::process::CommandExt, process::Command};

fn bus_options(arguments: &[OsString]) -> (bool, bool) {
    let (mut config, mut daemon) = (false, false);
    let mut index = 0;
    while let Some(argument) = arguments.get(index) {
        let bytes = argument.as_encoded_bytes();
        if bytes == b"--" || !bytes.starts_with(b"-") { break; }
        let split = bytes == b"--config-file" || bytes == b"--dbus-daemon";
        config |= bytes == b"--config-file" || bytes.starts_with(b"--config-file=");
        daemon |= bytes == b"--dbus-daemon" || bytes.starts_with(b"--dbus-daemon=");
        index += if split { 2 } else { 1 };
    }
    (config, daemon)
}

fn main() {
    let arguments = env::args_os().skip(1).collect::<Vec<_>>();
    let (config, daemon) = bus_options(&arguments);
    let mut command = Command::new(env!("LUMEN_DBUS_RUN_SESSION"));
    if !daemon {
        command.arg(concat!("--dbus-daemon=", env!("LUMEN_DBUS_DAEMON")));
    }
    if !config {
        command.arg(concat!("--config-file=", env!("LUMEN_DBUS_TEST_CONFIG")));
    }
    let error = command.args(arguments).exec();
    eprintln!("Private session bus could not start: {error}");
    std::process::exit(1);
}

#[cfg(test)]
mod tests {
    use super::*;
    fn options(values: &[&str]) -> (bool, bool) {
        bus_options(&values.iter().map(OsString::from).collect::<Vec<_>>())
    }
    #[test]
    fn child_literals_do_not_override_bus_defaults() {
        assert_eq!(options(&["--", "printf", "--config-file=child", "--dbus-daemon=child"]), (false, false));
        assert_eq!(options(&["printf", "--config-file=child"]), (false, false));
    }
    #[test]
    fn leading_bus_options_accept_both_forms() {
        assert_eq!(options(&["--config-file", "config", "--dbus-daemon", "daemon", "--", "printf"]), (true, true));
        assert_eq!(options(&["--config-file=config", "--dbus-daemon=daemon", "printf"]), (true, true));
        assert_eq!(options(&["--config-file", "--dbus-daemon=filename", "printf"]), (true, false));
    }
}
