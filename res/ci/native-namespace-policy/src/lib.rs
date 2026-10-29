//! Temporary, exact-executable user namespace policy for disposable hosted CI.
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::env;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

const PARAMETERS: [&str; 4] = [
    "kernel/apparmor_restrict_unprivileged_userns",
    "kernel/apparmor_restrict_unprivileged_unconfined",
    "kernel/unprivileged_userns_clone",
    "user/max_user_namespaces",
];

fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

pub fn hosted(
    environment: &BTreeMap<String, String>,
    release: &BTreeMap<String, String>,
) -> Result<(), String> {
    for (name, expected) in [
        ("GITHUB_ACTIONS", "true"),
        ("RUNNER_ENVIRONMENT", "github-hosted"),
        ("RUNNER_OS", "Linux"),
    ] {
        if environment.get(name).map(String::as_str) != Some(expected) {
            return Err("Only disposable GitHub-hosted Linux jobs may prepare this policy".into());
        }
    }
    if release.get("ID").map(String::as_str) != Some("ubuntu")
        || release.get("VERSION_ID").map(String::as_str) != Some("24.04")
    {
        return Err("Revalidate policy for this runner distribution".into());
    }
    Ok(())
}

pub fn profile_for(executable: &str) -> Result<(String, String), String> {
    let rest = executable
        .strip_prefix("/nix/store/")
        .ok_or("Exact immutable Nix unshare required")?;
    let (hash, version) = rest.split_once("-util-linux-")
        .and_then(|(hash, rest)| rest.strip_suffix("/bin/unshare").map(|version| (hash, version)))
        .or_else(|| rest.split_once("-bubblewrap-")
            .and_then(|(hash, rest)| rest.strip_suffix("/bin/bwrap").map(|version| (hash, version))))
        .ok_or("Exact immutable namespace executable required")?;
    if hash.len() != 32
        || !hash
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
        || version.is_empty()
        || !version
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"_.+-".contains(&byte))
    {
        return Err("Exact immutable Nix unshare required".into());
    }
    let name = format!(
        "lumen-native-tests-{}",
        &digest(executable.as_bytes())[..16]
    );
    let text = format!(
        "abi <abi/4.0>,\nprofile {name} \"{executable}\" flags=(unconfined) {{\n  userns,\n}}\n"
    );
    Ok((name, text))
}

pub fn parameters(root: &Path) -> Result<BTreeMap<String, Option<String>>, String> {
    PARAMETERS
        .into_iter()
        .map(|name| {
            let value = match fs::read_to_string(root.join(name)) {
                Ok(text) => Some(text.trim().to_owned()),
                Err(error) if error.kind() == io::ErrorKind::NotFound => None,
                Err(error) => return Err(error.to_string()),
            };
            Ok((name.to_owned(), value))
        })
        .collect()
}

pub struct ProcessResult {
    pub status: i32,
    pub stdout: String,
    pub stderr: String,
}

pub fn execute_with<F>(
    parser: &str,
    operation: &str,
    text: &str,
    mut process: F,
) -> Result<String, String>
where
    F: FnMut(&[String], &str) -> Result<ProcessResult, String>,
{
    let args = [
        "--non-interactive".to_owned(),
        parser.to_owned(),
        operation.to_owned(),
    ];
    let output = process(&args, text)?;
    if output.status != 0 {
        return Err(format!(
            "Namespace policy {operation} failed ({}): {}",
            output.status, output.stderr
        ));
    }
    if !output.stderr.is_empty() {
        return Err(format!(
            "Namespace policy parser emitted a diagnostic: {}",
            output.stderr
        ));
    }
    Ok(output.stdout)
}

fn command(
    program: &str,
    args: &[String],
    input: Option<&str>,
    timeout: Duration,
) -> Result<ProcessResult, String> {
    // Bound the entire process group, including helpers that inherit stdout.
    let mut child = Command::new(option_env!("LUMEN_TIMEOUT_BIN").unwrap_or("timeout"))
        .args([
            "--kill-after=1s",
            &format!("{}s", timeout.as_secs()),
            program,
        ])
        .args(args)
        .stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| error.to_string())?;
    if let Some(input) = input {
        let mut stdin = child.stdin.take().ok_or("Parser stdin is unavailable")?;
        stdin
            .write_all(input.as_bytes())
            .map_err(|error| error.to_string())?;
    }
    let output = child
        .wait_with_output()
        .map_err(|error| error.to_string())?;
    if matches!(output.status.code(), Some(124 | 137)) {
        return Err(format!(
            "Namespace policy command timed out after {} seconds",
            timeout.as_secs()
        ));
    }
    Ok(ProcessResult {
        status: output.status.code().unwrap_or(1),
        stdout: String::from_utf8(output.stdout).map_err(|_| "Invalid parser output")?,
        stderr: String::from_utf8(output.stderr).map_err(|_| "Invalid parser diagnostic")?,
    })
}

fn execute(parser: &str, operation: &str, text: &str) -> Result<(), String> {
    let stdout = execute_with(parser, operation, text, |args, input| {
        command("sudo", args, Some(input), Duration::from_secs(20))
    })?;
    if !stdout.is_empty() {
        print!("{stdout}");
    }
    Ok(())
}

pub fn loaded_from_output(name: &str, output: &str) -> Result<bool, String> {
    let matches = output
        .lines()
        .filter(|line| line.starts_with(&format!("{name} (")))
        .count();
    if matches > 1 {
        return Err("Ambiguous test profile identity".into());
    }
    Ok(matches == 1)
}

fn profile_loaded(name: &str) -> Result<bool, String> {
    let args = [
        "--non-interactive".to_owned(),
        "/usr/bin/cat".to_owned(),
        "/sys/kernel/security/apparmor/profiles".to_owned(),
    ];
    let output = command("sudo", &args, None, Duration::from_secs(5))?;
    if output.status != 0 || !output.stderr.is_empty() {
        return Err(format!(
            "Unable to inspect loaded profiles: {}",
            output.stderr
        ));
    }
    loaded_from_output(name, &output.stdout)
}

pub fn cleanup_policy<P, A>(
    name: &str,
    text: &str,
    attempted: bool,
    mut present: P,
    mut apply: A,
) -> Result<(), String>
where
    P: FnMut(&str) -> Result<bool, String>,
    A: FnMut(&str, &str) -> Result<(), String>,
{
    if !attempted {
        return Ok(());
    }
    if present(name)? {
        apply("--remove", text)?;
    }
    if present(name)? {
        return Err("Test profile remains loaded after cleanup".into());
    }
    Ok(())
}

pub fn save_record_with<F>(out: &Path, record: &Value, publish: F) -> Result<(), String>
where
    F: FnOnce(&Path, &Path) -> io::Result<()>,
{
    let id =
        fs::read_to_string("/proc/sys/kernel/random/uuid").map_err(|error| error.to_string())?;
    let temporary = out.join(format!("record-{}", id.trim()));
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&temporary)
            .map_err(|error| error.to_string())?;
        serde_json::to_writer_pretty(&mut file, record).map_err(|error| error.to_string())?;
        file.write_all(b"\n").map_err(|error| error.to_string())?;
        file.flush().map_err(|error| error.to_string())?;
        file.sync_all().map_err(|error| error.to_string())?;
        publish(&temporary, &out.join("result.json")).map_err(|error| error.to_string())?;
        File::open(out)
            .and_then(|directory| directory.sync_all())
            .map_err(|error| error.to_string())
    })();
    if temporary.exists() {
        let _ = fs::remove_file(temporary);
    }
    result
}

fn save_record(out: &Path, record: &Value) -> Result<(), String> {
    save_record_with(out, record, |from, to| fs::rename(from, to))
}

fn release() -> Result<BTreeMap<String, String>, String> {
    let text = fs::read_to_string("/etc/os-release").map_err(|error| error.to_string())?;
    Ok(text
        .lines()
        .filter_map(|line| line.split_once('='))
        .map(|(key, value)| (key.to_owned(), value.trim_matches('"').to_owned()))
        .collect())
}

fn out_path(value: &str) -> Result<PathBuf, String> {
    let path = PathBuf::from(value);
    let path = if path.is_absolute() {
        path
    } else {
        env::current_dir()
            .map_err(|error| error.to_string())?
            .join(path)
    };
    if fs::symlink_metadata(&path).is_ok_and(|info| info.file_type().is_symlink()) {
        return Err("Policy artifacts cannot be symlinks".into());
    }
    Ok(path)
}

fn run_profile(operation: &str, output: &str, executable: &str) -> Result<(), String> {
    hosted(&env::vars().collect(), &release()?)?;
    let parser = "/usr/sbin/apparmor_parser";
    let info = fs::metadata(parser).map_err(|error| error.to_string())?;
    if info.uid() != 0 || info.mode() & 0o022 != 0 || info.mode() & 0o111 == 0 {
        return Err("Root-owned runner AppArmor parser required".into());
    }
    let out = out_path(output)?;
    match operation {
        "prepare" => {
            let info = fs::metadata(executable).map_err(|error| error.to_string())?;
            if !info.is_file() || info.mode() & 0o111 == 0 { return Err("Immutable namespace executable missing".into()); }
            let (name, text) = profile_for(executable)?;
            fs::create_dir_all(out.parent().ok_or("Invalid artifact path")?)
                .map_err(|error| error.to_string())?;
            fs::create_dir(&out).map_err(|error| error.to_string())?;
            fs::set_permissions(&out, fs::Permissions::from_mode(0o700))
                .map_err(|error| error.to_string())?;
            fs::write(out.join("profile"), &text).map_err(|error| error.to_string())?;
            let before = parameters(Path::new("/proc/sys"))?;
            let mut record = json!({"profile":name,"executable":executable,
                "executableSha256":digest(&fs::read(executable).map_err(|error| error.to_string())?),
                "profileSha256":digest(text.as_bytes()),"parametersBefore":before,
                "loaded":false,"attempted":false});
            save_record(&out, &record)?;
            if profile_loaded(&name)? {
                return Err("Refusing to replace a preexisting test profile".into());
            }
            record["attempted"] = json!(true);
            save_record(&out, &record)?;
            execute(parser, "--add", &text)?;
            record["loaded"] = json!(profile_loaded(&name)?);
            save_record(&out, &record)?;
            if record["loaded"] != true {
                return Err("Namespace profile was not loaded".into());
            }
            record["parametersAfter"] = json!(parameters(Path::new("/proc/sys"))?);
            save_record(&out, &record)?;
            if record["parametersAfter"] != record["parametersBefore"] {
                return Err("Global namespace controls changed unexpectedly".into());
            }
            println!("Prepared exact immutable test executable policy; global controls unchanged");
        }
        "cleanup" => {
            if !out.exists() {
                return Ok(());
            }
            let info = fs::symlink_metadata(&out).map_err(|error| error.to_string())?;
            if !info.is_dir() || info.uid() != unsafe { libc::getuid() } || info.mode() & 0o077 != 0
            {
                return Err("Policy artifact ownership changed".into());
            }
            let mut record: Value = serde_json::from_slice(
                &fs::read(out.join("result.json")).map_err(|error| error.to_string())?,
            )
            .map_err(|error| error.to_string())?;
            let executable = record["executable"]
                .as_str()
                .ok_or("Missing saved executable")?;
            let (name, text) = profile_for(executable)?;
            let profile = out.join("profile");
            if name != record["profile"].as_str().unwrap_or("")
                || fs::symlink_metadata(&profile)
                    .map_err(|error| error.to_string())?
                    .file_type()
                    .is_symlink()
                || fs::read_to_string(&profile).map_err(|error| error.to_string())? != text
            {
                return Err("Refusing changed policy cleanup".into());
            }
            cleanup_policy(
                &name,
                &text,
                record["attempted"]
                    .as_bool()
                    .ok_or("Missing attempted state")?,
                profile_loaded,
                |operation, text| execute(parser, operation, text),
            )?;
            record["loaded"] = json!(false);
            record["cleaned"] = json!(true);
            record["parametersAfterCleanup"] = json!(parameters(Path::new("/proc/sys"))?);
            save_record(&out, &record)?;
            if record["parametersAfterCleanup"] != record["parametersBefore"] {
                return Err("Global namespace controls changed during job".into());
            }
            println!("Removed only the test executable policy; global controls unchanged");
        }
        _ => return Err("Expected prepare or cleanup".into()),
    }
    Ok(())
}

fn for_each_profile(mut action: impl FnMut(&str, &str) -> Result<(), String>) -> Result<(), String> {
    let mut errors = Vec::new();
    for (name, executable) in [("bwrap", env!("LUMEN_BWRAP_BIN")), ("unshare", env!("LUMEN_UNSHARE_BIN"))] {
        if let Err(error) = action(name, executable) { errors.push(format!("{name}: {error}")); }
    }
    if errors.is_empty() { Ok(()) } else { Err(errors.join("; ")) }
}

pub fn run(operation: &str, output: &str) -> Result<(), String> {
    hosted(&env::vars().collect(), &release()?)?;
    let out = out_path(output)?;
    match operation {
        "prepare" => {
            fs::create_dir_all(out.parent().ok_or("Invalid artifact path")?).map_err(|error| error.to_string())?;
            fs::create_dir(&out).map_err(|error| error.to_string())?;
            fs::set_permissions(&out, fs::Permissions::from_mode(0o700)).map_err(|error| error.to_string())?;
            let before = parameters(Path::new("/proc/sys"))?;
            for_each_profile(|name, executable| run_profile("prepare", out.join(name).to_str().ok_or("Invalid policy artifact")?, executable))?;
            let source = env::current_dir().map_err(|error| error.to_string())?;
            let probe = out.join("isolation-probe");
            let status = Command::new(env!("LUMEN_TIMEOUT_BIN"))
                .args(["60", env!("LUMEN_ISOLATION_BIN"), "probe", "--source"])
                .arg(source).arg("--output").arg(probe)
                .status().map_err(|error| error.to_string())?;
            let after = parameters(Path::new("/proc/sys"))?;
            save_record(&out, &json!({"profiles":["bwrap","unshare"],"parametersBefore":before,"parametersAfter":after,"isolationProbePassed":status.success()}))?;
            if before != after { return Err("Global namespace controls changed during real isolation probe".into()); }
            if !status.success() { return Err(format!("Actual namespace isolation probe failed: {status}")); }
            println!("Actual default and private-PTY isolation probes passed under exact executable policies");
            Ok(())
        }
        "cleanup" => {
            if !out.exists() { return Ok(()); }
            let info = fs::symlink_metadata(&out).map_err(|error| error.to_string())?;
            if !info.is_dir() || info.uid() != unsafe { libc::getuid() } || info.mode() & 0o077 != 0 {
                return Err("Policy artifact ownership changed".into());
            }
            // Always attempt both removals, even if one saved profile was damaged.
            for_each_profile(|name, executable| run_profile("cleanup", out.join(name).to_str().ok_or("Invalid policy artifact")?, executable))
        }
        _ => Err("Expected prepare or cleanup".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    fn fixture() -> PathBuf {
        let path = env::temp_dir().join(format!(
            "lumen-policy-{}",
            fs::read_to_string("/proc/sys/kernel/random/uuid")
                .unwrap()
                .trim()
        ));
        fs::create_dir(&path).unwrap();
        path
    }
    #[test]
    fn only_hosted_ubuntu_is_allowed() {
        let environment = BTreeMap::from([
            ("GITHUB_ACTIONS".into(), "true".into()),
            ("RUNNER_ENVIRONMENT".into(), "github-hosted".into()),
            ("RUNNER_OS".into(), "Linux".into()),
        ]);
        let release = BTreeMap::from([
            ("ID".into(), "ubuntu".into()),
            ("VERSION_ID".into(), "24.04".into()),
        ]);
        assert!(hosted(&environment, &release).is_ok());
        for (key, value) in [
            ("RUNNER_ENVIRONMENT", "self-hosted"),
            ("RUNNER_OS", "Windows"),
        ] {
            let mut wrong = environment.clone();
            wrong.insert(key.into(), value.into());
            assert!(hosted(&wrong, &release).is_err());
        }
        let mut wrong = release.clone();
        wrong.insert("ID".into(), "nixos".into());
        assert!(hosted(&environment, &wrong).is_err());
        assert!(hosted(&BTreeMap::new(), &release).is_err());
    }
    #[test]
    fn profile_attaches_only_the_exact_immutable_executable() {
        let path = format!(
            "/nix/store/{}-util-linux-2.42.3-bin/bin/unshare",
            "a".repeat(32)
        );
        let (name, text) = profile_for(&path).unwrap();
        assert!(
            text.contains(&format!("\"{path}\""))
                && text.contains("userns,")
                && !text.contains('*')
        );
        assert_eq!(profile_for(&path).unwrap(), (name, text));
        for wrong in [
            "/usr/bin/unshare".to_owned(),
            format!("{path}\"\n}}"),
            path.replace("/bin/unshare", "/bin/bash"),
        ] {
            assert!(profile_for(&wrong).is_err());
        }
    }
    #[test]
    fn bubblewrap_profile_is_exact_and_does_not_match_other_tools() {
        let path = format!("/nix/store/{}-bubblewrap-0.11.0/bin/bwrap", "b".repeat(32));
        let (_, text) = profile_for(&path).unwrap();
        assert!(text.contains(&format!("\"{path}\"")) && !text.contains('*'));
        for wrong in [path.replace("/bin/bwrap", "/bin/unshare"), path.replace("-bubblewrap-", "-other-"), format!("{path}/../bwrap"), "/usr/bin/bwrap".into()] {
            assert!(profile_for(&wrong).is_err());
        }
    }
    #[test]
    fn both_exact_profiles_are_attempted_when_first_action_fails() {
        let mut attempts = Vec::new();
        let error = for_each_profile(|name, executable| {
            attempts.push(name.to_owned());
            assert!(profile_for(executable).is_ok());
            if name == "bwrap" { Err("retained failure".into()) } else { Ok(()) }
        }).unwrap_err();
        assert_eq!(attempts, ["bwrap", "unshare"]);
        assert!(error.contains("bwrap: retained failure"));
    }
    #[test]
    fn global_parameters_are_read_only() {
        let root = fixture();
        for name in PARAMETERS {
            let path = root.join(name);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, "1\n").unwrap();
        }
        let values = parameters(&root).unwrap();
        assert!(values.values().all(|value| value.as_deref() == Some("1")));
        for name in PARAMETERS {
            assert_eq!(fs::read_to_string(root.join(name)).unwrap(), "1\n");
        }
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn parser_receives_fixed_stdin_and_rejects_diagnostics() {
        let observed = RefCell::new(Vec::new());
        let output = execute_with(
            "/usr/sbin/apparmor_parser",
            "--add",
            "fixed profile",
            |args, input| {
                observed
                    .borrow_mut()
                    .push((args.to_vec(), input.to_owned()));
                Ok(ProcessResult {
                    status: 0,
                    stdout: String::new(),
                    stderr: String::new(),
                })
            },
        )
        .unwrap();
        assert!(output.is_empty());
        assert_eq!(
            observed.into_inner(),
            vec![(
                vec!["--non-interactive", "/usr/sbin/apparmor_parser", "--add"]
                    .into_iter()
                    .map(str::to_owned)
                    .collect(),
                "fixed profile".into()
            )]
        );
        assert!(
            execute_with("parser", "--add", "fixed", |_, _| Ok(ProcessResult {
                status: 0,
                stdout: String::new(),
                stderr: "warning".into()
            }))
            .is_err()
        );
        assert!(
            execute_with("parser", "--add", "fixed", |_, _| Ok(ProcessResult {
                status: 1,
                stdout: String::new(),
                stderr: "syntax rejection".into()
            }))
            .unwrap_err()
            .contains("syntax rejection")
        );
    }
    #[test]
    fn cleanup_reconciles_kernel_state_after_interruption() {
        let calls = RefCell::new(Vec::new());
        let mut states = [true, false].into_iter();
        cleanup_policy(
            "name",
            "fixed",
            true,
            |_| Ok(states.next().unwrap()),
            |operation, text| {
                calls
                    .borrow_mut()
                    .push((operation.to_owned(), text.to_owned()));
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(
            calls.into_inner(),
            vec![("--remove".into(), "fixed".into())]
        );
        cleanup_policy(
            "name",
            "fixed",
            false,
            |_| panic!("untouched profile queried"),
            |_, _| panic!("untouched profile changed"),
        )
        .unwrap();
        assert!(cleanup_policy("name", "fixed", true, |_| Ok(true), |_, _| Ok(())).is_err());
    }
    #[test]
    fn interrupted_record_publication_keeps_previous_receipt() {
        let root = fixture();
        let record = json!({"attempted":true,"loaded":false});
        save_record_with(&root, &record, |from, to| fs::rename(from, to)).unwrap();
        assert!(
            save_record_with(&root, &json!({"attempted":true,"loaded":true}), |_, _| Err(
                io::Error::other("synthetic interruption")
            ))
            .is_err()
        );
        assert_eq!(
            serde_json::from_slice::<Value>(&fs::read(root.join("result.json")).unwrap()).unwrap(),
            record
        );
        assert_eq!(fs::read_dir(&root).unwrap().count(), 1);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn loaded_profile_count_is_exact() {
        assert!(!loaded_from_output("name", "other (enforce)\n").unwrap());
        assert!(loaded_from_output("name", "name (unconfined)\n").unwrap());
        assert!(loaded_from_output("name", "name (unconfined)\nname (enforce)\n").is_err());
    }
}
