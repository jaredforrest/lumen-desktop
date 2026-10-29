//! Exercise the built recovery script with private status and terminal endpoints.
use std::env;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

fn main() -> Result<()> {
    match Path::new(&env::args().next().unwrap_or_default()).file_name().and_then(|name| name.to_str()) {
        Some("systemctl") => return status_stub(),
        Some("foot") => return terminal_stub(),
        _ => {}
    }
    let args: Vec<String> = env::args().skip(1).collect();
    match args.as_slice() {
        [command, script] if command == "--command" => run(Path::new(script)),
        _ => Err("Expected --command /nix/store/.../bin/lumen-session-recovery".into()),
    }
}

fn status_stub() -> Result<()> {
    let args: Vec<String> = env::args().skip(1).collect();
    let expected = ["--user", "show", "--property=Result", "--value", "lumen-desktop-shell.service"];
    if args != expected { return Err(format!("Unexpected status request: {args:?}").into()); }
    fs::write(env::var("RECOVERY_REQUESTS")?, args.join("\n"))?;
    println!("{}", env::var("RECOVERY_RESULT")?);
    Ok(())
}

fn terminal_stub() -> Result<()> {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.len() < 4 || args[0] != "--app-id=lumen-session-recovery" || args[1] != "--title=Desktop Recovery" {
        return Err(format!("Unexpected terminal request: {args:?}").into());
    }
    fs::write(env::var("RECOVERY_TERMINAL")?, args.join("\n"))?;
    let output = Command::new(&args[2]).args(&args[3..]).stdin(Stdio::null()).output()?;
    if !output.status.success() { return Err(format!("Recovery shell failed: {output:?}").into()); }
    use std::io::Write;
    std::io::stdout().write_all(&output.stdout)?;
    std::io::stderr().write_all(&output.stderr)?;
    Ok(())
}

fn replace_foot(script: &str, terminal: &Path) -> Result<String> {
    let suffix = "/bin/foot";
    let mut matches = script.match_indices(suffix).filter_map(|(end, _)| {
        let prefix = &script[..end];
        let start = prefix.rfind("/nix/store/")?;
        prefix[start..].contains("-foot-").then_some((start, end + suffix.len()))
    });
    let (start, end) = matches.next().ok_or("No built foot invocation")?;
    if matches.next().is_some() { return Err("Multiple built foot invocations".into()); }
    Ok(format!("{}{}{}", &script[..start], terminal.display(), &script[end..]))
}

fn run(command: &Path) -> Result<()> {
    let command = fs::canonicalize(command)?;
    if !command.starts_with("/nix/store/") { return Err("Command must be a built store script".into()); }
    let original = fs::read_to_string(&command)?;
    let unique = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let root = env::temp_dir().join(format!("lumen-recovery-{}-{unique}", std::process::id()));
    fs::create_dir(&root)?;
    let result = run_cases(&original, &root);
    fs::remove_dir_all(&root)?;
    result
}

fn run_cases(original: &str, root: &Path) -> Result<()> {
    let stubs = root.join("stubs");
    fs::create_dir(&stubs)?;
    let executable = env::current_exe()?;
    for name in ["systemctl", "foot"] { fs::copy(&executable, stubs.join(name))?; }
    let mut inserted = 0;
    let modified = original.lines().map(|line| {
        if line.starts_with("export PATH=\"") {
            inserted += 1;
            format!("{line}\nexport PATH=\"{}:$PATH\"", stubs.display())
        } else { line.to_owned() }
    }).collect::<Vec<_>>().join("\n");
    if inserted != 1 { return Err(format!("Expected one package PATH declaration, found {inserted}").into()); }
    let modified = replace_foot(&modified, &stubs.join("foot"))?;
    let script = root.join("recovery");
    fs::write(&script, format!("{modified}\n"))?;
    fs::set_permissions(&script, fs::Permissions::from_mode(0o700))?;
    let bash = original.lines().next().and_then(|line| line.strip_prefix("#!")).ok_or("Missing script shebang")?;
    for outcome in ["success", "exit-code", "start-limit-hit"] {
        let request = root.join(format!("{outcome}-request"));
        let terminal = root.join(format!("{outcome}-terminal"));
        let output = Command::new(bash).arg(&script).env("RECOVERY_RESULT", outcome)
            .env("RECOVERY_REQUESTS", &request).env("RECOVERY_TERMINAL", &terminal)
            .stdin(Stdio::null()).output()?;
        if !output.status.success() || !output.stderr.is_empty() {
            return Err(format!("Recovery script failed for {outcome}: {output:?}").into());
        }
        let status = fs::read_to_string(&request)?;
        if status != "--user\nshow\n--property=Result\n--value\nlumen-desktop-shell.service" {
            return Err(format!("Unexpected status request: {status}").into());
        }
        let opened = terminal.exists();
        if opened != (outcome == "start-limit-hit") { return Err(format!("Wrong terminal status for {outcome}").into()); }
        let body = String::from_utf8(output.stdout)?;
        if opened {
            let lines: Vec<&str> = body.lines().collect();
            if lines.len() != 6 || lines[0] != "The desktop shell stopped after repeated failures."
                || lines[5] != "  systemctl --user start lumen-desktop-shell.service" {
                return Err(format!("Unexpected recovery body: {body:?}").into());
            }
        } else if !body.is_empty() { return Err(format!("Unexpected output for {outcome}: {body:?}").into()); }
    }
    println!("Built recovery script: three status cases and terminal body passed");
    Ok(())
}
