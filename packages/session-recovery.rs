//! Independent recovery terminal after the shell exhausts its restart allowance.
use std::{env, io::{self, Write}, os::unix::process::CommandExt, process::Command};

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let arguments = env::args_os().skip(1).collect::<Vec<_>>();
    if arguments.as_slice() == ["--terminal"] {
        println!("The desktop shell stopped after repeated failures.");
        println!("Your applications remain open. This terminal is independent of the shell.");
        println!("Inspect its log with: journalctl --user -u lumen-desktop-shell.service");
        println!("When ready, retry with:");
        println!("  systemctl --user reset-failed lumen-desktop-shell.service");
        println!("  systemctl --user start lumen-desktop-shell.service");
        io::stdout().flush()?;
        return Err(Command::new(env!("LUMEN_RECOVERY_SHELL")).args(["--noprofile", "--norc"]).exec().into());
    }
    if !arguments.is_empty() { return Err("Expected no arguments or --terminal".into()); }
    let status = Command::new(env!("LUMEN_RECOVERY_SYSTEMCTL"))
        .args(["--user", "show", "--property=Result", "--value", "lumen-desktop-shell.service"])
        .output()?;
    if !status.status.success() { return Err("Could not read the shell service result".into()); }
    if std::str::from_utf8(&status.stdout)?.trim() != "start-limit-hit" { return Ok(()); }
    Err(Command::new(env!("LUMEN_RECOVERY_TERMINAL"))
        .args(["--app-id=lumen-session-recovery", "--title=Desktop Recovery"])
        .arg(env::current_exe()?).arg("--terminal").exec().into())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("Desktop recovery could not start: {error}");
        std::process::exit(1);
    }
}
