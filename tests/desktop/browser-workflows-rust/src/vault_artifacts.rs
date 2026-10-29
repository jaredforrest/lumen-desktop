//! Credential-free diagnostics for disposable vault fixtures.
use crate::{Result, fail};
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

pub struct ArtifactSecrets {
    values: BTreeSet<String>,
}
impl ArtifactSecrets {
    pub fn new() -> Self {
        Self {
            values: BTreeSet::new(),
        }
    }
    pub fn add(&mut self, value: String) -> Result<String> {
        if value.is_empty() {
            return Err(fail("Nonempty disposable sentinel required"));
        }
        self.values.insert(value.clone());
        Ok(value)
    }
    pub fn redact(&self, text: &str) -> String {
        let mut values = self.values.iter().collect::<Vec<_>>();
        values.sort_by_key(|value| std::cmp::Reverse(value.len()));
        let mut redacted = text.to_owned();
        for value in values {
            redacted = redacted.replace(value.as_str(), "[disposable credential redacted]");
        }
        redacted
    }
    pub fn audit(&self, root: &Path) -> Result<usize> {
        let mut stack = vec![root.to_path_buf()];
        let mut files = 0;
        let mut leaks = false;
        while let Some(dir) = stack.pop() {
            for entry in fs::read_dir(dir)? {
                let entry = entry?;
                let path = entry.path();
                let info = fs::symlink_metadata(&path)?;
                if info.file_type().is_dir() {
                    stack.push(path);
                    continue;
                }
                if !info.file_type().is_file() || info.len() > 8 * 1024 * 1024 {
                    return Err(fail("Unexpected retained vault artifact"));
                }
                files += 1;
                let bytes = fs::read(&path)?;
                if bytes
                    .windows(b"-----BEGIN ".len())
                    .any(|part| part == b"-----BEGIN ")
                    && bytes
                        .windows(b"PRIVATE KEY-----".len())
                        .any(|part| part == b"PRIVATE KEY-----")
                {
                    fs::write(&path, b"[private key artifact removed]\n")?;
                    leaks = true;
                    continue;
                }
                let mut cleaned = bytes.clone();
                let mut values = self.values.iter().collect::<Vec<_>>();
                values.sort_by_key(|value| std::cmp::Reverse(value.len()));
                for value in values {
                    cleaned = replace_bytes(
                        &cleaned,
                        value.as_bytes(),
                        b"[disposable credential redacted]",
                    );
                }
                if cleaned != bytes {
                    fs::write(&path, cleaned)?;
                    leaks = true;
                }
            }
        }
        if leaks {
            Err(fail(
                "Disposable credential appeared in retained artifacts; removed",
            ))
        } else {
            Ok(files)
        }
    }
}
impl Default for ArtifactSecrets {
    fn default() -> Self {
        Self::new()
    }
}
fn replace_bytes(haystack: &[u8], needle: &[u8], replacement: &[u8]) -> Vec<u8> {
    if needle.is_empty() {
        return haystack.to_vec();
    }
    let mut result = Vec::new();
    let mut start = 0;
    while let Some(offset) = haystack[start..]
        .windows(needle.len())
        .position(|part| part == needle)
    {
        let end = start + offset;
        result.extend_from_slice(&haystack[start..end]);
        result.extend_from_slice(replacement);
        start = end + needle.len();
    }
    result.extend_from_slice(&haystack[start..]);
    result
}
pub fn run_cli(
    program: &Path,
    arguments: &[&str],
    environment: &[(&str, &str)],
    timeout: Duration,
) -> Result<Output> {
    let mut child = Command::new(program)
        .args(arguments)
        .env_clear()
        .envs(environment.iter().copied())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let deadline = Instant::now() + timeout;
    while child.try_wait()?.is_none() {
        if Instant::now() >= deadline {
            child.kill()?;
            let _ = child.wait_with_output();
            return Err(fail("Disposable CLI command exceeded its deadline"));
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    Ok(child.wait_with_output()?)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn nested_redaction_and_clean_receipt() {
        let mut guard = ArtifactSecrets::new();
        guard.add("synthetic-long-sentinel".into()).unwrap();
        guard.add("synthetic-long".into()).unwrap();
        assert_eq!(
            guard.redact("synthetic-long-sentinel"),
            "[disposable credential redacted]"
        );
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("result.json"), b"{\"passed\":true}").unwrap();
        assert_eq!(guard.audit(dir.path()).unwrap(), 1);
    }
    #[test]
    fn reflected_secrets_are_removed_and_fail_acceptance() {
        let mut guard = ArtifactSecrets::new();
        for value in [
            "synthetic-account@example.invalid",
            "synthetic-password-sentinel",
            "synthetic-session-sentinel",
            "synthetic-registration-sentinel",
        ] {
            guard.add(value.into()).unwrap();
        }
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("failure.json");
        fs::write(
            &path,
            guard.values.iter().cloned().collect::<Vec<_>>().join(" "),
        )
        .unwrap();
        assert!(guard.audit(dir.path()).is_err());
        let text = fs::read_to_string(&path).unwrap();
        assert!(guard.values.iter().all(|value| !text.contains(value)));
        assert_eq!(guard.audit(dir.path()).unwrap(), 1);
    }
    #[test]
    fn private_key_is_removed_and_symlink_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("key.log");
        fs::write(
            &path,
            b"-----BEGIN PRIVATE KEY-----\nsynthetic material\n-----END PRIVATE KEY-----",
        )
        .unwrap();
        assert!(ArtifactSecrets::new().audit(dir.path()).is_err());
        assert!(
            !fs::read_to_string(&path)
                .unwrap()
                .contains("synthetic material")
        );
        std::os::unix::fs::symlink("/nonexistent", dir.path().join("link")).unwrap();
        assert!(ArtifactSecrets::new().audit(dir.path()).is_err());
    }
    #[test]
    fn timeout_never_reflects_argument() {
        let exe = std::env::current_exe().unwrap();
        let result = run_cli(
            &exe,
            &[
                "--exact",
                "vault_artifacts::tests::sleep_helper",
                "--ignored",
                "--skip",
                "synthetic-secret",
            ],
            &[],
            Duration::from_millis(20),
        );
        let message = result.unwrap_err().to_string();
        assert!(message.contains("exceeded its deadline"));
        assert!(!message.contains("synthetic-secret"));
    }
    #[test]
    #[ignore]
    fn sleep_helper() {
        std::thread::sleep(Duration::from_secs(1));
    }
}
