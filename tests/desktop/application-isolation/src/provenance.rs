//! Frozen source and immutable tool identity for disposable fixture runners.
use crate::Result;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

pub fn digest(path: &Path) -> Result<String> {
    Ok(format!("{:x}", Sha256::digest(fs::read(path)?)))
}

fn git(source: &Path, args: &[&str]) -> Result<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(source)
        .args(args)
        .output()?;
    if !output.status.success() {
        return Err("Git provenance command failed".into());
    }
    Ok(String::from_utf8(output.stdout)?.trim().to_owned())
}

pub struct FrozenSource {
    pub source: PathBuf,
    pub script: PathBuf,
    pub revision: String,
    pub source_hash: String,
    pub dependencies: BTreeMap<String, String>,
}

impl FrozenSource {
    pub fn new(source: &Path, script: &Path, dependencies: &[PathBuf]) -> Result<Self> {
        let source = source.canonicalize()?;
        let script = script.canonicalize()?;
        let revision = git(&source, &["rev-parse", "HEAD"])?;
        if !git(&source, &["status", "--porcelain"])?.is_empty() {
            return Err("Freeze source before native acceptance".into());
        }
        let source_hash = digest(&script)?;
        let mut tools = BTreeMap::new();
        for path in dependencies {
            let path = path.canonicalize()?;
            if !path.starts_with("/nix/store") {
                return Err("Immutable fixture dependencies required".into());
            }
            tools.insert(path.display().to_string(), digest(&path)?);
        }
        Ok(Self {
            source,
            script,
            revision,
            source_hash,
            dependencies: tools,
        })
    }
    pub fn finish(&self, output: &Path, exit_code: i32) -> Result<()> {
        let unchanged = git(&self.source, &["rev-parse", "HEAD"])? == self.revision
            && git(&self.source, &["status", "--porcelain"])?.is_empty()
            && digest(&self.script)? == self.source_hash;
        let tools_unchanged = self
            .dependencies
            .iter()
            .all(|(path, hash)| digest(Path::new(path)).is_ok_and(|actual| actual == *hash));
        let relative = self.script.strip_prefix(&self.source)?;
        let record = json!({"sourceRevision":self.revision,"sourceScript":relative,"sourceSha256":self.source_hash,"dependencies":self.dependencies,"sourceUnchanged":unchanged,"dependenciesUnchanged":tools_unchanged,"exitCode":exit_code});
        fs::write(
            output.join("native-source-boundary.json"),
            format!("{}\n", serde_json::to_string_pretty(&record)?),
        )?;
        if !unchanged || !tools_unchanged {
            return Err("Fixture source or dependency changed during native acceptance".into());
        }
        Ok(())
    }
}

pub fn generated(root: &Path, output: &Path) -> Result<()> {
    let mut files = BTreeMap::new();
    for entry in fs::read_dir(root)? {
        let path = entry?.path();
        if path.is_file()
            && matches!(
                path.extension().and_then(|ext| ext.to_str()),
                Some("qml" | "conf")
            )
        {
            files.insert(
                path.file_name().unwrap().to_string_lossy().into_owned(),
                digest(&path)?,
            );
        }
    }
    fs::write(
        output.join("generated-harnesses.json"),
        format!("{}\n", serde_json::to_string_pretty(&files)?),
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture_repo() -> (tempfile::TempDir, PathBuf, PathBuf, PathBuf) {
        let temp = tempfile::tempdir().unwrap();
        let repo = temp.path().join("source");
        let output = temp.path().join("artifacts");
        fs::create_dir(&repo).unwrap();
        fs::create_dir(&output).unwrap();
        let script = repo.join("fixture.rs");
        fs::write(&script, "fn main() {}\n").unwrap();
        for args in [vec!["init"],vec!["add","."],vec!["-c","user.name=Fixture","-c","user.email=fixture@example.invalid","commit","-m","Fixture"]] {
            let result=Command::new("git").args(args).current_dir(&repo).output().unwrap();
            assert!(result.status.success(),"{}",String::from_utf8_lossy(&result.stderr));
        }
        (temp,repo,script,output)
    }
    #[test]
    fn frozen_runner_refuses_dirty_source_before_dispatch() {
        let (_temp,repo,script,_output)=fixture_repo();
        fs::write(repo.join("pending"),"uncommitted").unwrap();
        assert!(FrozenSource::new(&repo,&script,&[]).is_err());
    }
    #[test]
    fn frozen_runner_detects_concurrent_source_mutation_and_keeps_initial_record() {
        let (_temp,repo,script,output)=fixture_repo();
        let frozen=FrozenSource::new(&repo,&script,&[]).unwrap();
        let before=frozen.source_hash.clone();
        fs::write(&script,"fn main() { changed(); }\n").unwrap();
        assert!(frozen.finish(&output,7).is_err());
        let record:serde_json::Value=serde_json::from_slice(&fs::read(output.join("native-source-boundary.json")).unwrap()).unwrap();
        assert_eq!(record["sourceSha256"],before);
        assert_eq!(record["sourceUnchanged"],false);
        assert_eq!(record["exitCode"],7);
    }
    #[test]
    fn frozen_runner_rejects_mutable_tool() {
        let (_temp,repo,script,output)=fixture_repo();
        let mutable=output.join("driver");fs::write(&mutable,"tool").unwrap();
        assert!(FrozenSource::new(&repo,&script,&[mutable]).is_err());
    }
    #[test]
    fn generated_hashes_only_harness_files() {
        let temp = tempfile::tempdir().unwrap();
        fs::write(temp.path().join("a.qml"), "a").unwrap();
        fs::write(temp.path().join("b.conf"), "b").unwrap();
        fs::write(temp.path().join("c.txt"), "c").unwrap();
        generated(temp.path(), temp.path()).unwrap();
        let record: serde_json::Value = serde_json::from_slice(
            &fs::read(temp.path().join("generated-harnesses.json")).unwrap(),
        )
        .unwrap();
        assert!(record.get("a.qml").is_some());
        assert!(record.get("b.conf").is_some());
        assert!(record.get("c.txt").is_none());
    }
}
