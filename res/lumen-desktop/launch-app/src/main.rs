//! Launch an application outside the desktop shell's service cgroup.

use std::env;
use std::ffi::OsString;
use std::fs;
use std::os::unix::process::CommandExt;
use std::process::{Command, ExitCode};

fn command_args(
    mut args: Vec<OsString>,
    scope_id: Option<&str>,
) -> Option<(OsString, Vec<OsString>)> {
    if args.first().is_some_and(|arg| arg == "--") {
        args.remove(0);
    }
    if args.is_empty() {
        return None;
    }
    if let Some(id) = scope_id {
        let mut scoped = [
            "--user".into(),
            "--scope".into(),
            "--collect".into(),
            "--quiet".into(),
            "--expand-environment=no".into(),
            format!("--unit=app-lumen-desktop-{id}").into(),
            "--".into(),
        ]
        .to_vec();
        scoped.extend(args);
        Some(("systemd-run".into(), scoped))
    } else {
        Some((args.remove(0), args))
    }
}

fn random_scope_id() -> std::io::Result<String> {
    let uuid = fs::read_to_string("/proc/sys/kernel/random/uuid")?;
    let id: String = uuid.trim().chars().filter(|c| *c != '-').collect();
    if id.len() == 32 && id.bytes().all(|b| b.is_ascii_hexdigit()) {
        Ok(id)
    } else {
        Err(std::io::Error::other("invalid kernel UUID"))
    }
}

fn main() -> ExitCode {
    let args = env::args_os().skip(1).collect();
    let scoped = env::var_os("LUMEN_LAUNCH_SCOPES").is_some_and(|value| value == "1");
    let scope_id = if scoped {
        match random_scope_id() {
            Ok(id) => Some(id),
            Err(error) => {
                eprintln!("Unable to launch application: {error}");
                return ExitCode::FAILURE;
            }
        }
    } else {
        None
    };
    let Some((program, argv)) = command_args(args, scope_id.as_deref()) else {
        eprintln!("An application command is required");
        return ExitCode::from(2);
    };
    // exec preserves the caller's environment and working directory. A failed
    // production scope launch must never fall back into the shell's cgroup.
    let error = Command::new(program).args(argv).exec();
    eprintln!("Unable to launch application: {error}");
    ExitCode::FAILURE
}

#[cfg(test)]
mod tests {
    use super::command_args;

    #[test]
    fn scoped_arguments_stay_literal() {
        let input = vec!["--", "literal command", "$HOME", "$(touch no)"];
        let (program, args) =
            command_args(input.into_iter().map(Into::into).collect(), Some("abc"))
                .expect("command");
        assert_eq!(program, "systemd-run");
        assert_eq!(
            args,
            [
                "--user",
                "--scope",
                "--collect",
                "--quiet",
                "--expand-environment=no",
                "--unit=app-lumen-desktop-abc",
                "--",
                "literal command",
                "$HOME",
                "$(touch no)",
            ]
        );
    }

    #[test]
    fn direct_command_and_missing_command() {
        let (program, args) =
            command_args(vec!["--".into(), "echo".into(), "hello".into()], None).expect("command");
        assert_eq!(program, "echo");
        assert_eq!(args, ["hello"]);
        assert!(command_args(vec!["--".into()], None).is_none());
    }
}
