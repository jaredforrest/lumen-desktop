//! Installed provider publication and format integrity in a disposable namespace.
use lumen_office_workflows::{Result, corpus, ensure, error, formats};
use serde_json::{Value, json};
use std::env;
use std::fs::{self, File};
use std::io::Read;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;
use wait_timeout::ChildExt;
use zip::ZipArchive;

struct Options {
    writer: PathBuf,
    grid: PathBuf,
    deck: PathBuf,
    output: PathBuf,
    inside: bool,
}
fn arguments() -> Result<Options> {
    let mut writer = None;
    let mut grid = None;
    let mut deck = None;
    let mut output = None;
    let mut inside = false;
    let mut args = env::args_os().skip(1);
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("--wordcraft") => writer = Some(PathBuf::from(args.next().ok_or("Missing wordcraft")?)),
            Some("--gridcraft") => grid = Some(PathBuf::from(args.next().ok_or("Missing gridcraft")?)),
            Some("--deckcraft") => deck = Some(PathBuf::from(args.next().ok_or("Missing deckcraft")?)),
            Some("--output") => output = Some(PathBuf::from(args.next().ok_or("Missing output")?)),
            Some("--inside") => inside = true,
            _ => return Err(error(format!("Unknown option: {}", arg.to_string_lossy()))),
        }
    }
    let cwd = env::current_dir()?;
    let output = output.ok_or("Missing output")?;
    Ok(Options {
        writer: writer.ok_or("Missing wordcraft")?,
        grid: grid.ok_or("Missing gridcraft")?,
        deck: deck.ok_or("Missing deckcraft")?,
        output: if output.is_absolute() { output } else { cwd.join(output) },
        inside,
    })
}
fn save(path: &Path, value: &Value) -> Result<()> {
    fs::write(path, format!("{}\n", serde_json::to_string_pretty(value)?))?;
    Ok(())
}
fn check_tool(path: &Path) -> Result<PathBuf> {
    let canonical = path.canonicalize()?;
    ensure(canonical.starts_with("/nix/store") && canonical.is_file(), "Pinned installed provider required")?;
    Ok(canonical)
}
fn outer(mut o: Options) -> Result<()> {
    o.writer = check_tool(&o.writer)?;
    o.grid = check_tool(&o.grid)?;
    o.deck = check_tool(&o.deck)?;
    fs::create_dir(&o.output)?;
    fs::set_permissions(&o.output, fs::Permissions::from_mode(0o700))?;
    let source = env::current_dir()?.canonicalize()?;
    let exe = env::current_exe()?;
    let exe_inside = if exe.starts_with(&source) { Path::new("/source").join(exe.strip_prefix(&source)?) } else { exe };
    let status = Command::new("lumen-application-isolation")
        .arg("run").arg("--source").arg(&source).arg("--output").arg(&o.output).arg("--")
        .arg(exe_inside).arg("--inside")
        .arg("--wordcraft").arg(&o.writer).arg("--gridcraft").arg(&o.grid)
        .arg("--deckcraft").arg(&o.deck).arg("--output").arg("/artifacts")
        .status()?;
    ensure(status.success(), format!("Provider fixture failed: {status}"))
}
fn actions(items: &[(String, Value)]) -> Vec<String> {
    items.iter().flat_map(|(name, data)| ["--cmd".to_owned(), format!("{name}={data}")]).collect()
}
fn invoke(tool: &Path, args: &[String], out: &Path, label: &str) -> Result<std::process::Output> {
    let mut child = Command::new(tool).args(args).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn()?;
    let deadline = if label.ends_with("-fifo") { 5 } else { 60 };
    if child.wait_timeout(Duration::from_secs(deadline))?.is_none() {
        child.kill()?;
        let _ = child.wait_with_output();
        return Err(error(format!("{label} exceeded provider deadline")));
    }
    let result = child.wait_with_output()?;
    fs::write(out.join(format!("{label}.stdout")), &result.stdout)?;
    fs::write(out.join(format!("{label}.stderr")), &result.stderr)?;
    Ok(result)
}
fn run_args(actions: &[(String, Value)], save: &Path, flag: &str, source: Option<&Path>, source_flag: &str) -> Vec<String> {
    let mut args = vec!["run".to_owned()];
    if let Some(source) = source { args.extend([source_flag.to_owned(), source.display().to_string()]); }
    args.extend(self::actions(actions));
    args.extend([flag.to_owned(), save.display().to_string()]);
    args
}
fn package_contains(path: &Path, main: &str, marker: &str) -> Result<()> {
    let mut zip = ZipArchive::new(File::open(path)?)?;
    ensure(zip.len() < 4096 && zip.by_name(main).is_ok(), "Expected bounded package and main part")?;
    let mut size = 0_u64;
    let mut text = String::new();
    for index in 0..zip.len() {
        let mut part = zip.by_index(index)?;
        size += part.size();
        ensure(size < 64 * 1024 * 1024, "Package exceeds fixture budget")?;
        if part.name().ends_with(".xml") {
            let mut xml = String::new();
            part.read_to_string(&mut xml)?;
            let doc = roxmltree::Document::parse(&xml)?;
            for node in doc.descendants().filter(|n| n.is_text()) { text.push_str(node.text().unwrap_or("")); }
        }
    }
    ensure(text.contains(marker), "Edited marker absent from package")
}
fn provider(o: &Options, app: &str, tool: &Path, items: &[(String, Value)], extension: &str, suffix: &str, edit: (&str, Value), main: &str, records: &mut Vec<Value>) -> Result<()> {
    let out = &o.output;
    let original = out.join(format!("{app}-original.{extension}"));
    let flag = if app == "gridcraft" { "--out" } else { "--save" };
    let source_flag = if app == "wordcraft" { "--file" } else { "--in" };
    let created = invoke(tool, &run_args(items, &original, flag, None, source_flag), out, &format!("{app}-create"))?;
    ensure(created.status.success(), format!("{app} creation failed: {}", String::from_utf8_lossy(&created.stderr)))?;
    let before = formats::digest(&original)?;
    for kind in ["legacy-hardlink", "legacy-symlink", "destination-hardlink", "destination-symlink"] {
        let target = out.join(format!("{app}-{kind}.{extension}"));
        let alias = PathBuf::from(format!("{}{suffix}", target.display()));
        match kind {
            "legacy-hardlink" => fs::hard_link(&original, &alias)?,
            "legacy-symlink" => std::os::unix::fs::symlink(&original, &alias)?,
            "destination-hardlink" => fs::hard_link(&original, &target)?,
            "destination-symlink" => std::os::unix::fs::symlink(&original, &target)?,
            _ => unreachable!(),
        }
        let result = invoke(tool, &run_args(&[(edit.0.to_owned(), edit.1.clone())], &target, flag, Some(&original), source_flag), out, &format!("{app}-{kind}"))?;
        ensure(result.status.success(), format!("{app} {kind} failed: {}", String::from_utf8_lossy(&result.stderr)))?;
        ensure(formats::digest(&original)? == before, "Original changed during save copy")?;
        package_contains(&target, main, "Edited copy marker")?;
        if kind.starts_with("legacy") {
            ensure(formats::digest(&alias)? == before && alias.is_symlink() == (kind == "legacy-symlink"), "Legacy alias changed")?;
            ensure(target.metadata()?.permissions().mode() & 0o777 == 0o600, "New target permissions not private")?;
        }
        ensure(!target.is_symlink() && target.metadata()?.ino() != original.metadata()?.ino(), "Target retains alias")?;
        records.push(json!({"app":app,"case":kind,"passed":true}));
    }
    let fifo = out.join(format!("{app}-fifo"));
    let path = std::ffi::CString::new(fifo.as_os_str().as_encoded_bytes())?;
    ensure(unsafe { libc::mkfifo(path.as_ptr(), 0o600) } == 0, "FIFO creation failed")?;
    let invalid = fifo.join(format!("copy.{extension}"));
    let result = invoke(tool, &run_args(&[(edit.0.to_owned(), edit.1.clone())], &invalid, flag, Some(&original), source_flag), out, &format!("{app}-fifo"))?;
    ensure(!result.status.success() && formats::digest(&original)? == before, "Non-directory parent accepted")?;
    records.push(json!({"app":app,"case":"non-directory-parent-refused","passed":true}));
    let directory = out.join(format!("{app}-directory.{extension}"));
    fs::create_dir(&directory)?;
    let result = invoke(tool, &run_args(&[(edit.0.to_owned(), edit.1.clone())], &directory, flag, Some(&original), source_flag), out, &format!("{app}-directory"))?;
    ensure(!result.status.success() && directory.is_dir() && formats::digest(&original)? == before, "Directory publication not refused")?;
    ensure(!fs::read_dir(out)?.any(|entry| entry.is_ok_and(|e| e.file_name().to_string_lossy().starts_with(".lumen-save-"))), "Temporary save file leaked")?;
    records.push(json!({"app":app,"case":"failed-publication-cleaned","passed":true}));
    fs::set_permissions(&original, fs::Permissions::from_mode(0o640))?;
    let result = invoke(tool, &run_args(&[(edit.0.to_owned(), edit.1)], &original, flag, Some(&original), source_flag), out, &format!("{app}-ordinary-save"))?;
    ensure(result.status.success(), "Ordinary save failed")?;
    package_contains(&original, main, "Edited copy marker")?;
    ensure(original.metadata()?.permissions().mode() & 0o777 == 0o640, "Existing permissions changed")?;
    records.push(json!({"app":app,"case":"ordinary-save","passed":true}));
    Ok(())
}
fn inside(o: Options) -> Result<()> {
    ensure(unsafe { libc::getuid() } != 0 && env::var("HOME")? == "/home/fixture" && !Path::new("/.old-root").exists(), "Disposable boundary missing")?;
    let corpus = corpus::create()?;
    let mut records = Vec::new();
    provider(&o, "wordcraft", &o.writer, &corpus.writer, "docx", ".tmp", ("text.insert", json!({"text":"Edited copy marker"})), "word/document.xml", &mut records)?;
    provider(&o, "gridcraft", &o.grid, &corpus.grid, "xlsx", ".sctmp", ("cell.set", json!({"cell":"A1","input":"Edited copy marker"})), "xl/workbook.xml", &mut records)?;
    provider(&o, "deckcraft", &o.deck, &corpus.deck, "pptx", ".saving", ("slide.notes", json!({"text":"Edited copy marker"})), "ppt/presentation.xml", &mut records)?;
    let original = o.output.join("deck-format-source.pptx");
    let created = invoke(&o.deck, &run_args(&corpus.deck, &original, "--save", None, "--in"), &o.output, "deck-format-create")?;
    ensure(created.status.success() && formats::package_kind(&original) == "pptx", "Presentation source invalid")?;
    let original_hash = formats::digest(&original)?;
    for (label, name, data) in [
        ("inferred", "file.saveAs", json!({})),
        ("explicit-native", "file.export", json!({"format":"deckcraft"})),
        ("unknown-format", "file.export", json!({"format":"imaginary"})),
    ] {
        let target = o.output.join(format!("{label}.odp"));
        fs::write(&target, b"Existing destination sentinel")?;
        let before = formats::digest(&target)?;
        let mut data = data;
        data["path"] = json!(target.display().to_string());
        let mut args = vec!["run".to_owned(), "--in".to_owned(), original.display().to_string()];
        args.extend(actions(&[(name.to_owned(), data)]));
        let result = invoke(&o.deck, &args, &o.output, label)?;
        let stderr = String::from_utf8_lossy(&result.stderr);
        ensure(!stderr.contains("unknown option") && !stderr.contains("unknown command"), "Fixture missed format dispatch")?;
        ensure(!result.status.success() && formats::digest(&target)? == before && formats::digest(&original)? == original_hash, "Unsupported format altered source or target")?;
        records.push(json!({"case":label,"passed":true}));
    }
    for (suffix, expected) in [("deckcraft","deck-native"),("slidecraft","deck-native"),("PPTX","pptx"),("potx","pptx"),("ppsx","pptx"),("pdf","pdf")] {
        let target = o.output.join(format!("copy.{suffix}"));
        let result = invoke(&o.deck, &["convert".into(), original.display().to_string(), target.display().to_string()], &o.output, &format!("convert-{suffix}"))?;
        ensure(result.status.success() && formats::package_kind(&target) == expected && formats::digest(&original)? == original_hash, "Declared format mismatch")?;
        records.push(json!({"case":format!("declared-{suffix}"),"passed":true}));
    }
    let target = o.output.join("unsupported.odp");
    let result = invoke(&o.deck, &["convert".into(), original.display().to_string(), target.display().to_string()], &o.output, "convert-odp")?;
    ensure(!result.status.success() && !target.exists() && formats::digest(&original)? == original_hash, "Unsupported convert emitted output")?;
    records.push(json!({"case":"unsupported-convert-has-no-output","passed":true}));
    save(&o.output.join("result.json"), &json!({"passed":true,"cases":records,"scope":"Installed provider atomic publication and truthful format declarations in private namespaces"}))
}
fn main() {
    let result = arguments().and_then(|o| if o.inside { inside(o) } else { outer(o) });
    if let Err(err) = result { eprintln!("{err}"); std::process::exit(1); }
}
