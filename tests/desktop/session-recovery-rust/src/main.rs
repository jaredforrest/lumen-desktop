//! Exercise compiled recovery logic with immutable private command endpoints.
use std::{env, fs, io::Write, path::Path, process::{Command, Stdio}, time::{SystemTime, UNIX_EPOCH}};
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

fn main() -> Result<()> {
    let executable = env::args().next().unwrap_or_default();
    match Path::new(&executable).file_name().and_then(|name| name.to_str()) {
        Some("systemctl") => return status_endpoint(),
        Some("foot") => return terminal_endpoint(),
        Some("recovery-shell") => {
            if env::args().skip(1).collect::<Vec<_>>() != ["--noprofile", "--norc"] { return Err("Unexpected interactive shell arguments".into()); }
            fs::write(env::var("RECOVERY_SHELL_RECEIPT")?, b"opened")?;
            if env::var_os("RECOVERY_SHELL_HOLD").is_some() {
                loop { std::thread::sleep(std::time::Duration::from_secs(1)); }
            }
            return Ok(());
        }
        _ => {}
    }
    let arguments = env::args().skip(1).collect::<Vec<_>>();
    match arguments.as_slice() {
        [flag, command] if flag == "--command" => run(Path::new(command)),
        _ => Err("Expected --command /nix/store/.../bin/lumen-session-recovery".into()),
    }
}

fn status_endpoint() -> Result<()> {
    let arguments = env::args().skip(1).collect::<Vec<_>>();
    if arguments != ["--user", "show", "--property=Result", "--value", "lumen-desktop-shell.service"] { return Err("Unexpected status request".into()); }
    fs::write(env::var("RECOVERY_REQUESTS")?, arguments.join("\n"))?;
    if env::var_os("RECOVERY_STATUS_ERROR").is_some() { return Err("Disposable status failure".into()); }
    println!("{}", env::var("RECOVERY_RESULT")?);
    Ok(())
}

fn terminal_endpoint() -> Result<()> {
    let arguments = env::args().skip(1).collect::<Vec<_>>();
    if arguments.len() != 4 || arguments[0] != "--app-id=lumen-session-recovery" || arguments[1] != "--title=Desktop Recovery" || arguments[3] != "--terminal" { return Err("Unexpected terminal request".into()); }
    fs::write(env::var("RECOVERY_TERMINAL")?, arguments.join("\n"))?;
    if env::var_os("RECOVERY_TERMINAL_ERROR").is_some() { return Err("Disposable terminal failure".into()); }
    let output = Command::new(&arguments[2]).args(&arguments[3..]).stdin(Stdio::null()).output()?;
    if !output.status.success() { return Err("Recovery terminal child failed".into()); }
    std::io::stdout().write_all(&output.stdout)?;
    std::io::stderr().write_all(&output.stderr)?;
    Ok(())
}

fn run(command: &Path) -> Result<()> {
    let command = command.canonicalize()?;
    if !command.starts_with("/nix/store/") { return Err("Immutable recovery binary required".into()); }
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let root = env::temp_dir().join(format!("lumen-recovery-{}-{nonce}", std::process::id()));
    fs::create_dir(&root)?;
    let result = cases(&command, &root);
    fs::remove_dir_all(root)?;
    result
}

fn cases(command: &Path, root: &Path) -> Result<()> {
    let bytes = fs::read(command)?;
    for (index, (state, failure)) in [
        ("success", ""), ("exit-code", ""), ("start-limit-hit", ""),
        ("unknown", ""), ("", ""), ("start-limit-hit\nextra", ""),
        ("start-limit-hit", "status"), ("start-limit-hit", "terminal"),
    ].iter().enumerate() {
        let requests = root.join(format!("{index}-status"));
        let terminal = root.join(format!("{index}-terminal"));
        let shell = root.join(format!("{index}-shell"));
        let mut child = Command::new(command);
        child.env("RECOVERY_RESULT", state).env("RECOVERY_REQUESTS", &requests)
            .env("RECOVERY_TERMINAL", &terminal).env("RECOVERY_SHELL_RECEIPT", &shell)
            .env_remove("RECOVERY_STATUS_ERROR").env_remove("RECOVERY_TERMINAL_ERROR").env_remove("RECOVERY_SHELL_HOLD");
        if *failure == "status" { child.env("RECOVERY_STATUS_ERROR", "1"); }
        if *failure == "terminal" { child.env("RECOVERY_TERMINAL_ERROR", "1"); }
        let output = child.stdin(Stdio::null()).output()?;
        if output.status.success() != failure.is_empty() { return Err(format!("Wrong recovery outcome for {index}: {output:?}").into()); }
        if !requests.is_file() { return Err("Missing status receipt".into()); }
        let opened = *state == "start-limit-hit" && *failure != "status";
        if terminal.exists() != opened || shell.exists() != (opened && failure.is_empty()) { return Err(format!("Unexpected terminal/shell for case {index}").into()); }
        if opened && failure.is_empty() {
            let body = String::from_utf8(output.stdout)?;
            let lines = body.lines().collect::<Vec<_>>();
            if lines.len() != 6 || lines[0] != "The desktop shell stopped after repeated failures." || lines[5] != "  systemctl --user start lumen-desktop-shell.service" || !output.stderr.is_empty() { return Err(format!("Wrong terminal disclosure: {body:?}").into()); }
        } else if !output.stdout.is_empty() { return Err("Unexpected recovery output".into()); }
    }
    if fs::read(command)? != bytes { return Err("Recovery executable changed".into()); }
    println!("Compiled recovery: eight real status/terminal cases, body and shell handoff passed");
    Ok(())
}
