use std::{
    collections::BTreeSet,
    env,
    ffi::{CString, OsStr, OsString},
    fs,
    io::{self, Read},
    os::{
        fd::{AsRawFd, FromRawFd},
        unix::{
            ffi::OsStrExt,
            fs::{FileTypeExt, MetadataExt, OpenOptionsExt},
        },
    },
    path::{Path, PathBuf},
};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
#[derive(Clone, Copy)]
struct Limits {
    files: usize,
    bytes: u64,
    entries: usize,
    depth: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            files: 100_000,
            bytes: 2 * 1024 * 1024 * 1024,
            entries: 200_000,
            depth: 128,
        }
    }
}
#[derive(Debug, Default)]
struct Report {
    files: usize,
    bytes: u64,
    entries: usize,
    metadata_bytes: u64,
    skipped: Vec<String>,
}
fn patterns(input: &str) -> Result<Vec<&str>> {
    let mut result = BTreeSet::new();
    for line in input.lines().map(str::trim).filter(|line| !line.is_empty()) {
        let name = line
            .strip_prefix("artifacts/")
            .ok_or("Artifact pattern must start with artifacts/")?;
        let plain = name.strip_suffix('*').unwrap_or(name);
        if plain.is_empty()
            || plain == "."
            || plain == ".."
            || plain.contains(['/', '\\', '*', '?', '[', ']'])
        {
            return Err(format!("Unsupported artifact pattern: {line:?}").into());
        }
        result.insert(name);
    }
    if result.is_empty() {
        return Err("Empty artifact include policy".into());
    }
    Ok(result.into_iter().collect())
}
fn matches(name: &[u8], pattern: &str) -> bool {
    pattern.strip_suffix('*').map_or_else(
        || name == pattern.as_bytes(),
        |prefix| name.starts_with(prefix.as_bytes()),
    )
}
fn open_child(parent: &fs::File, name: &OsStr, flags: i32) -> io::Result<fs::File> {
    let name = CString::new(name.as_bytes())?;
    let fd = unsafe {
        libc::openat(
            parent.as_raw_fd(),
            name.as_ptr(),
            flags | libc::O_CLOEXEC | libc::O_NOFOLLOW | libc::O_NONBLOCK,
        )
    };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(unsafe { fs::File::from_raw_fd(fd) })
}
fn read_names(directory: &fs::File, limits: Limits, report: &mut Report) -> Result<Vec<OsString>> {
    let mut names = Vec::new();
    for entry in fs::read_dir(format!("/proc/self/fd/{}", directory.as_raw_fd()))? {
        report.entries += 1;
        if report.entries > limits.entries {
            return Err("Artifact traversal entry budget exceeded".into());
        }
        names.push(entry?.file_name());
    }
    names.sort();
    Ok(names)
}
fn copy_bounded(
    source: &mut impl Read,
    target: &mut impl io::Write,
    remaining: u64,
) -> Result<u64> {
    let copied = io::copy(&mut source.take(remaining + 1), target)?;
    if copied > remaining {
        return Err("Artifact byte budget exceeded during copy".into());
    }
    Ok(copied)
}
fn walk(
    parent: &fs::File,
    name: &OsStr,
    relative: &Path,
    stage: &Path,
    limits: Limits,
    report: &mut Report,
    depth: usize,
) -> Result<()> {
    if depth > limits.depth {
        return Err("Artifact traversal budget exceeded".into());
    }
    let identity = open_child(parent, name, libc::O_PATH)?;
    let metadata = identity.metadata()?;
    let kind = metadata.file_type();
    if kind.is_dir() {
        let directory = open_child(parent, name, libc::O_RDONLY | libc::O_DIRECTORY)?;
        let current = directory.metadata()?;
        if current.dev() != metadata.dev() || current.ino() != metadata.ino() {
            return Err("Artifact directory changed during staging".into());
        }
        let children = read_names(&directory, limits, report)?;
        for child in children {
            walk(
                &directory,
                &child,
                &relative.join(&child),
                stage,
                limits,
                report,
                depth + 1,
            )?;
        }
    } else if kind.is_file() {
        if report.files >= limits.files.saturating_sub(2)
            || metadata.len()
                > limits
                    .bytes
                    .saturating_sub(report.bytes.saturating_add(report.metadata_bytes))
        {
            return Err(format!("Artifact file/byte budget exceeded at {relative:?}").into());
        }
        let target = stage.join(relative);
        fs::create_dir_all(target.parent().ok_or("Artifact has no parent")?)?;
        let mut source = open_child(parent, name, libc::O_RDONLY)?;
        let current = source.metadata()?;
        if !current.is_file() || current.dev() != metadata.dev() || current.ino() != metadata.ino()
        {
            return Err("Artifact file changed during staging".into());
        }
        let mut target = fs::File::create(target)?;
        let copied = copy_bounded(
            &mut source,
            &mut target,
            limits
                .bytes
                .saturating_sub(report.bytes.saturating_add(report.metadata_bytes)),
        )?;
        target.set_permissions(metadata.permissions())?;
        report.files += 1;
        report.bytes += copied;
    } else {
        let name = if kind.is_symlink() {
            "symlink"
        } else if kind.is_socket() {
            "socket"
        } else if kind.is_fifo() {
            "fifo"
        } else if kind.is_block_device() {
            "block-device"
        } else if kind.is_char_device() {
            "character-device"
        } else {
            "other"
        };
        let record = format!("{name}\t{relative:?}");
        let size = record.len() as u64 + 1;
        if size
            > limits
                .bytes
                .saturating_sub(report.bytes.saturating_add(report.metadata_bytes))
        {
            return Err("Artifact manifest byte budget exceeded".into());
        }
        report.metadata_bytes += size;
        report.skipped.push(record);
    }
    Ok(())
}
fn destination_parent(root: &Path, output: &Path) -> Result<(PathBuf, PathBuf)> {
    if output
        .components()
        .any(|part| part == std::path::Component::ParentDir)
    {
        return Err("Artifact output cannot contain parent traversal".into());
    }
    let output = if output.is_absolute() {
        output.to_path_buf()
    } else {
        env::current_dir()?.join(output)
    };
    let mut ancestor = output.parent().ok_or("Output has no parent")?.to_path_buf();
    let mut missing = Vec::new();
    loop {
        match fs::symlink_metadata(&ancestor) {
            Ok(_) => break,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                missing.push(
                    ancestor
                        .file_name()
                        .ok_or("Output ancestor has no name")?
                        .to_owned(),
                );
                ancestor = ancestor
                    .parent()
                    .ok_or("Output ancestor missing")?
                    .to_path_buf();
            }
            Err(error) => return Err(error.into()),
        }
    }
    let mut parent = ancestor.canonicalize()?;
    if !parent.is_dir() {
        return Err("Output ancestor is not a directory".into());
    }
    for part in missing.into_iter().rev() {
        parent.push(part);
    }
    let destination = parent.join(output.file_name().ok_or("Output has no name")?);
    if destination.starts_with(root.join("artifacts")) {
        return Err("Output must be outside the original artifacts tree".into());
    }
    Ok((parent, destination))
}
fn stage(root: &Path, includes: &str, output: &Path, limits: Limits) -> Result<Report> {
    let policy = patterns(includes)?;
    let root = root.canonicalize()?;
    let (parent, output) = destination_parent(&root, output)?;
    fs::create_dir_all(&parent)?;
    if fs::symlink_metadata(&output).is_ok() {
        return Err("Artifact staging output already exists".into());
    }
    let temporary = parent.join(format!(".lumen-artifact-stage-{}", std::process::id()));
    fs::create_dir(&temporary)?;
    let result = (|| {
        let mut report = Report {
            metadata_bytes: includes.len() as u64 + 512,
            ..Report::default()
        };
        if report.metadata_bytes > limits.bytes {
            return Err("Artifact manifest byte budget exceeded".into());
        }
        let artifacts = root.join("artifacts");
        match fs::symlink_metadata(&artifacts) {
            Ok(meta) if meta.file_type().is_dir() => {
                let directory = fs::OpenOptions::new()
                    .read(true)
                    .custom_flags(
                        libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC,
                    )
                    .open(&artifacts)?;
                let current = directory.metadata()?;
                if current.dev() != meta.dev() || current.ino() != meta.ino() {
                    return Err("Artifact root changed during staging".into());
                }
                for name in read_names(&directory, limits, &mut report)? {
                    if policy
                        .iter()
                        .any(|pattern| matches(name.as_bytes(), pattern))
                    {
                        walk(
                            &directory,
                            &name,
                            &Path::new("artifacts").join(&name),
                            &temporary,
                            limits,
                            &mut report,
                            0,
                        )?;
                    }
                }
            }
            Ok(_) => return Err("Original artifacts root is not a real directory".into()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        if report.files == 0 {
            return Err(format!(
                "No regular fixture evidence selected ({} special entries omitted)",
                report.skipped.len()
            )
            .into());
        }
        let text = format!(
            "Regular files: {}\nBytes: {}\nSkipped entries (type, repository-relative path; links never followed):\n{}\n",
            report.files,
            report.bytes,
            report.skipped.join("\n")
        );
        if report.files + 2 > limits.files
            || (text.len() as u64).saturating_add(includes.len() as u64)
                > limits.bytes.saturating_sub(report.bytes)
        {
            return Err("Artifact manifest file/byte budget exceeded".into());
        }
        fs::write(temporary.join("artifact-staging-manifest.txt"), text)?;
        fs::write(temporary.join("artifact-includes.txt"), includes)?;
        fs::rename(&temporary, &output)?;
        Ok(report)
    })();
    if result.is_err() {
        let _ = fs::remove_dir_all(temporary);
    }
    result
}
fn main() -> Result<()> {
    let args: Vec<_> = env::args_os().skip(1).collect();
    if args.len() != 3 {
        return Err("Usage: lumen-stage-artifacts REPOSITORY INCLUDE_FILE OUTPUT".into());
    }
    let report = stage(
        Path::new(&args[0]),
        &fs::read_to_string(&args[1])?,
        Path::new(&args[2]),
        Limits::default(),
    )?;
    println!(
        "Staged {} regular files ({} bytes); {} special entries recorded without dereferencing",
        report.files,
        report.bytes,
        report.skipped.len()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        os::unix::{
            fs::{PermissionsExt, symlink},
            net::UnixListener,
        },
        path::PathBuf,
        sync::atomic::{AtomicUsize, Ordering},
    };
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            let path = env::temp_dir().join(format!(
                "lumen-artifact-test-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    #[test]
    fn real_socket_and_external_links_never_enter_snapshot() -> Result<()> {
        let f = Fixture::new();
        let root = &f.0;
        fs::create_dir_all(root.join("artifacts/owned-case/nested"))?;
        let file = root.join("artifacts/owned-case/nested/frame.bin");
        fs::write(&file, [0, 1, 255, 10])?;
        fs::set_permissions(&file, fs::Permissions::from_mode(0o640))?;
        fs::write(root.join("outside-secret"), "MUST NOT READ")?;
        symlink(
            root.join("outside-secret"),
            root.join("artifacts/owned-case/external"),
        )?;
        symlink(
            "/nonexistent-must-not-resolve",
            root.join("artifacts/owned-case/dangling"),
        )?;
        symlink(root, root.join("artifacts/owned-case/loop"))?;
        let _socket = UnixListener::bind(root.join("artifacts/owned-case/runtime.sock"))?;
        fs::create_dir(root.join("artifacts/excluded"))?;
        fs::write(root.join("artifacts/excluded/not-in-policy"), "EXCLUDED")?;
        let out = root.join("upload/backend");
        let report = stage(
            root,
            "artifacts/owned-*\nartifacts/owned-case\n",
            &out,
            Limits::default(),
        )?;
        assert_eq!(
            (report.files, report.bytes, report.skipped.len()),
            (1, 4, 4)
        );
        assert_eq!(
            fs::read(out.join("artifacts/owned-case/nested/frame.bin"))?,
            [0, 1, 255, 10]
        );
        assert_eq!(
            fs::metadata(out.join("artifacts/owned-case/nested/frame.bin"))?
                .permissions()
                .mode()
                & 0o777,
            0o640
        );
        assert!(!out.join("artifacts/excluded").exists());
        for name in ["external", "dangling", "loop", "runtime.sock"] {
            assert!(fs::symlink_metadata(out.join("artifacts/owned-case").join(name)).is_err());
        }
        assert!(
            report
                .skipped
                .iter()
                .any(|line| line == "socket\t\"artifacts/owned-case/runtime.sock\"")
        );
        assert!(
            report
                .skipped
                .iter()
                .any(|line| line == "symlink\t\"artifacts/owned-case/external\"")
        );
        assert_eq!(
            fs::read_to_string(root.join("outside-secret"))?,
            "MUST NOT READ"
        );
        assert!(root.join("artifacts/owned-case/runtime.sock").exists());
        Ok(())
    }
    #[test]
    fn budgets_fail_without_publishing_partial_snapshot() -> Result<()> {
        let f = Fixture::new();
        fs::create_dir_all(f.0.join("artifacts/case"))?;
        fs::write(f.0.join("artifacts/case/file"), "12345")?;
        for limits in [
            Limits {
                bytes: 4,
                ..Limits::default()
            },
            Limits {
                files: 0,
                ..Limits::default()
            },
            Limits {
                entries: 1,
                ..Limits::default()
            },
            Limits {
                depth: 0,
                ..Limits::default()
            },
        ] {
            let out = f.0.join("upload/backend");
            assert!(
                stage(&f.0, "artifacts/case", &out, limits)
                    .unwrap_err()
                    .to_string()
                    .contains("budget")
            );
            assert!(!out.exists());
        }
        assert_eq!(
            fs::read_to_string(f.0.join("artifacts/case/file"))?,
            "12345"
        );
        Ok(())
    }
    #[test]
    fn source_growth_copy_is_bounded_before_budget_error() {
        let mut source = &b"1234567890"[..];
        let mut target = Vec::new();
        assert!(copy_bounded(&mut source, &mut target, 4).is_err());
        assert_eq!(target, b"12345");
    }
    #[test]
    fn descriptor_ownership_rejects_leaf_links_and_keeps_original_parent() -> Result<()> {
        let f = Fixture::new();
        fs::create_dir(f.0.join("parent"))?;
        fs::create_dir(f.0.join("external"))?;
        fs::write(f.0.join("parent/file"), "INSIDE")?;
        fs::write(f.0.join("external/file"), "EXTERNAL MUST NOT READ")?;
        let parent = fs::File::open(f.0.join("parent"))?;
        fs::rename(f.0.join("parent"), f.0.join("captured-parent"))?;
        symlink(f.0.join("external"), f.0.join("parent"))?;
        let mut content = String::new();
        open_child(&parent, OsStr::new("file"), libc::O_RDONLY)?.read_to_string(&mut content)?;
        assert_eq!(content, "INSIDE");
        let captured = open_child(&parent, OsStr::new("file"), libc::O_PATH)?;
        fs::remove_file(f.0.join("captured-parent/file"))?;
        symlink(f.0.join("external/file"), f.0.join("captured-parent/file"))?;
        assert!(captured.metadata()?.is_file());
        assert_eq!(
            open_child(&parent, OsStr::new("file"), libc::O_RDONLY)
                .unwrap_err()
                .raw_os_error(),
            Some(libc::ELOOP)
        );
        Ok(())
    }
    #[test]
    fn empty_missing_and_special_only_inputs_fail_without_publishing() -> Result<()> {
        let f = Fixture::new();
        let out = f.0.join("upload/backend");
        for _ in 0..2 {
            assert!(
                stage(&f.0, "artifacts/case", &out, Limits::default())
                    .unwrap_err()
                    .to_string()
                    .contains("No regular fixture evidence")
            );
            assert!(!out.exists());
            fs::create_dir_all(f.0.join("artifacts/case"))?;
        }
        let _socket = UnixListener::bind(f.0.join("artifacts/case/runtime.sock"))?;
        assert!(
            stage(&f.0, "artifacts/case", &out, Limits::default())
                .unwrap_err()
                .to_string()
                .contains("1 special entries")
        );
        assert!(!out.exists());
        Ok(())
    }
    #[test]
    fn rejected_output_does_not_create_original_artifact_directories() -> Result<()> {
        let f = Fixture::new();
        fs::create_dir_all(f.0.join("artifacts/case"))?;
        fs::write(f.0.join("artifacts/case/file"), "UNCHANGED")?;
        symlink(f.0.join("artifacts/case"), f.0.join("output-link"))?;
        for out in [
            f.0.join("artifacts/case/direct/new"),
            f.0.join("output-link/indirect/new"),
        ] {
            assert!(
                stage(&f.0, "artifacts/case", &out, Limits::default())
                    .unwrap_err()
                    .to_string()
                    .contains("outside")
            );
        }
        assert!(!f.0.join("artifacts/case/direct").exists());
        assert!(!f.0.join("artifacts/case/indirect").exists());
        assert_eq!(
            fs::read_to_string(f.0.join("artifacts/case/file"))?,
            "UNCHANGED"
        );
        Ok(())
    }
    #[test]
    fn policy_rejects_escape_and_unsupported_patterns() {
        for value in [
            "",
            "../private",
            "/artifacts/case",
            "artifacts/../secret",
            "artifacts/case/other",
            "artifacts/a?",
            "artifacts/*",
        ] {
            assert!(patterns(value).is_err(), "{value}");
        }
    }
}
