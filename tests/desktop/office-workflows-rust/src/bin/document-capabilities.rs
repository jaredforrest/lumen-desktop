//! Installed CLI version ownership and print-to-file checks in a private boundary.
use lumen_office_workflows::{Result, ensure, error, formats};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{env, fs, io::Read, os::unix::fs::PermissionsExt, path::{Path, PathBuf}, process::{Command, Output, Stdio}, thread, time::Duration};
use wait_timeout::ChildExt;

struct Options { writer: PathBuf, sheet: PathBuf, slides: PathBuf, pdf_tools: PathBuf, output: PathBuf, inside: bool }
fn options() -> Result<Options> {
    let mut writer = None; let mut sheet = None; let mut slides = None; let mut pdf_tools = None; let mut output = None; let mut inside = false;
    let mut args = env::args_os().skip(1);
    while let Some(arg) = args.next() { match arg.to_str() {
        Some("--writer") => writer = Some(PathBuf::from(args.next().ok_or("Missing writer")?)),
        Some("--sheet") => sheet = Some(PathBuf::from(args.next().ok_or("Missing sheet")?)),
        Some("--slides") => slides = Some(PathBuf::from(args.next().ok_or("Missing slides")?)),
        Some("--pdf-tools") => pdf_tools = Some(PathBuf::from(args.next().ok_or("Missing PDF tools")?)),
        Some("--output") => output = Some(PathBuf::from(args.next().ok_or("Missing output")?)),
        Some("--inside") => inside = true,
        _ => return Err(error(format!("Unknown option: {}", arg.to_string_lossy()))),
    }}
    Ok(Options { writer: writer.ok_or("--writer required")?, sheet: sheet.ok_or("--sheet required")?, slides: slides.ok_or("--slides required")?, pdf_tools: pdf_tools.ok_or("--pdf-tools required")?, output: output.ok_or("--output required")?, inside })
}
fn pinned(path: &Path) -> Result<PathBuf> {
    let path = path.canonicalize()?;
    ensure(path.starts_with("/nix/store") && path.is_file(), "Pinned installed executable required")?;
    Ok(path)
}
fn outer(mut o: Options) -> Result<()> {
    o.writer = pinned(&o.writer)?; o.sheet = pinned(&o.sheet)?; o.slides = pinned(&o.slides)?;
    let pdfinfo = pinned(&o.pdf_tools.join("bin/pdfinfo"))?;
    let pdftotext = pinned(&o.pdf_tools.join("bin/pdftotext"))?;
    fs::create_dir(&o.output)?; fs::set_permissions(&o.output, fs::Permissions::from_mode(0o700))?;
    let source = env::current_dir()?.canonicalize()?;
    let exe = env::current_exe()?;
    let inside = if exe.starts_with(&source) { Path::new("/source").join(exe.strip_prefix(&source)?) } else { exe };
    let status = Command::new("lumen-application-isolation").args(["run", "--source"])
        .arg(&source).arg("--output").arg(&o.output).arg("--").arg(inside).arg("--inside")
        .arg("--writer").arg(&o.writer).arg("--sheet").arg(&o.sheet).arg("--slides").arg(&o.slides)
        .arg("--pdf-tools").arg(pdfinfo.parent().ok_or("PDF tools bin")?.parent().ok_or("PDF tools root")?)
        .arg("--output").arg("/artifacts").status()?;
    ensure(status.success(), format!("Document capabilities fixture failed: {status}"))?;
    let _ = pdftotext;
    Ok(())
}
fn actions(items: &[(&str, Value)]) -> Vec<String> {
    items.iter().flat_map(|(name, value)| ["--cmd".to_owned(), format!("{name}={value}")]).collect()
}
struct Runner { o: Options, commands: Vec<Value>, checks: Vec<Value> }
impl Runner {
    fn call(&mut self, label: &str, tool: &Path, args: &[String]) -> Result<Output> {
        let mut child = Command::new(tool).args(args).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn()?;
        let stdout = child.stdout.take().ok_or("Provider stdout unavailable")?;
        let stderr = child.stderr.take().ok_or("Provider stderr unavailable")?;
        let capture = |mut pipe: Box<dyn Read + Send>| thread::spawn(move || { let mut data=Vec::new(); pipe.read_to_end(&mut data).map(|_|data) });
        let out_task = capture(Box::new(stdout)); let err_task = capture(Box::new(stderr));
        let status = match child.wait_timeout(Duration::from_secs(30))? { Some(status)=>status, None=>{child.kill()?;let _=child.wait();return Err(error(format!("{label} timed out")));} };
        let result = Output { status, stdout:out_task.join().map_err(|_|error("Provider stdout capture failed"))??, stderr:err_task.join().map_err(|_|error("Provider stderr capture failed"))?? };
        fs::write(self.o.output.join(format!("{label}.stdout")), &result.stdout)?;
        fs::write(self.o.output.join(format!("{label}.stderr")), &result.stderr)?;
        self.commands.push(json!({"label":label,"argv":args,"returncode":result.status.code(),"stdoutSha256":format!("{:x}",Sha256::digest(&result.stdout)),"stderrSha256":format!("{:x}",Sha256::digest(&result.stderr))}));
        Ok(result)
    }
    fn check(&mut self, name: &str, passed: bool) -> Result<()> {
        self.checks.push(json!({"name":name,"passed":passed}));
        ensure(passed, name)
    }
    fn word_text(path: &Path) -> Result<String> {
        Ok(formats::writer(path)?["paragraphs"].as_array().ok_or("DOCX paragraphs missing")?.iter().filter_map(|p|p["text"].as_str()).collect::<Vec<_>>().join("\n"))
    }
    fn list(output: &Output, name: &str) -> Result<Vec<Value>> {
        let text = String::from_utf8(output.stdout.clone())?;
        text.lines().filter_map(|line|line.strip_prefix(&format!("{name}: "))).map(|json|Ok(serde_json::from_str(json)?)).collect()
    }
    fn writer_run(&mut self, label: &str, source: Option<&Path>, target: Option<&Path>, items: &[(&str, Value)]) -> Result<Output> {
        let mut args = vec!["run".to_owned()];
        if let Some(source) = source { args.extend(["--file".to_owned(), source.display().to_string()]); }
        args.extend(actions(items));
        if let Some(target) = target { args.extend(["--save".to_owned(), target.display().to_string()]); }
        args.push("--print".to_owned());
        let tool = self.o.writer.clone(); self.call(label, &tool, &args)
    }
    fn run(&mut self) -> Result<()> {
        let out = self.o.output.clone();
        for (name, tool) in [("writer",self.o.writer.clone()),("sheet",self.o.sheet.clone()),("slides",self.o.slides.clone())] {
            let version = if name == "sheet" { "version" } else { "--version" };
            let got = self.call(&format!("{name}-version"), &tool, &[version.into()])?;
            self.check(&format!("{name} installed version"), got.status.success())?;
            let mut command_args = vec!["commands".to_owned()]; if name != "slides" { command_args.push("--json".to_owned()); }
            let got = self.call(&format!("{name}-commands"), &tool, &command_args)?;
            self.check(&format!("{name} installed commands"), got.status.success())?;
        }
        let first = out.join("first.docx"); let second = out.join("second.docx");
        for (label, path, marker) in [("first",&first,"First document marker"),("second",&second,"Second document marker")] {
            let got = self.writer_run(&format!("create-{label}"),None,Some(path),&[("text.insert",json!({"text":marker}))])?;
            self.check("Create version source",got.status.success() && Self::word_text(path)?.contains(marker))?;
        }
        let hashes = [formats::digest(&first)?,formats::digest(&second)?];
        for (name, transition, marker) in [
            ("new",vec![("file.new",json!({})),("text.insert",json!({"text":"New document marker"}))],"New document marker"),
            ("open",vec![("file.open",json!({"path":second}))],"Second document marker"),
        ] {
            let copy = out.join(format!("{name}-copy.docx"));
            let mut items = vec![("file.versions",json!({"save":"First only"}))]; items.extend(transition.clone()); items.extend([("file.versions",json!({})),("file.recover",json!({}))]);
            let got = self.writer_run(&format!("{name}-list"),Some(&first),Some(&copy),&items)?;
            let versions = Self::list(&got,"file.versions")?; let recover = Self::list(&got,"file.recover")?;
            self.check(&format!("{name} clears previous document snapshots"), got.status.success() && versions.last()==Some(&json!([])) && recover==vec![json!([])] && Self::word_text(&copy)?.contains(marker) && !Self::word_text(&copy)?.contains("First document marker"))?;
            let refused = out.join(format!("{name}-misleading-restore.docx"));
            let mut items = vec![("file.versions",json!({"save":"First only"}))]; items.extend(transition); items.push(("file.versions",json!({"restore":0})));
            let got = self.writer_run(&format!("{name}-restore"),Some(&first),Some(&refused),&items)?;
            self.check(&format!("{name} refuses predecessor snapshot"),!got.status.success() && !refused.exists())?;
        }
        let restored = out.join("same-document-restored.docx");
        let got = self.writer_run("same-document",Some(&first),Some(&restored),&[
            ("file.versions",json!({"save":"First"})),("caret.docEnd",json!({})),("text.insert",json!({"text":" Edited marker"})),
            ("file.versions",json!({"restore":0})),("file.versions",json!({"save":"Second"})),("caret.docEnd",json!({})),
            ("text.insert",json!({"text":" Edited marker"})),("file.versions",json!({"restore":0})),("file.versions",json!({})),
            ("file.versions",json!({"restore":0})),("file.versions",json!({})),("file.recover",json!({})),
        ])?;
        let versions = Self::list(&got,"file.versions")?; let recovery = Self::list(&got,"file.recover")?;
        let labels = |value:&Value| value.as_array().map(|rows|rows.iter().filter_map(|row|row["label"].as_str().map(str::to_owned)).collect::<Vec<_>>()).unwrap_or_default();
        self.check("Repeated same-document restores retain both snapshots",got.status.success() && Self::word_text(&restored)?.contains("First document marker") && !Self::word_text(&restored)?.contains("Edited marker") && versions.iter().rev().take(2).all(|v|labels(v)==["First","Second"]) && recovery.last().is_some_and(|v|labels(v)==["First","Second"]))?;
        let failed = out.join("failed-open-output.docx");
        let got = self.writer_run("failed-open",Some(&first),Some(&failed),&[("file.versions",json!({"save":"First only"})),("file.open",json!({"path":out.join("missing.docx")}))])?;
        self.check("Failed open leaves saved documents intact",!got.status.success() && !failed.exists() && formats::digest(&first)?==hashes[0] && formats::digest(&second)?==hashes[1])?;
        let source=out.join("version-source.docx");let copy=out.join("restored-copy.docx");
        let got=self.writer_run("writer-create",None,Some(&source),&[("text.insert",json!({"text":"Original version marker."}))])?;
        self.check("Version source created",got.status.success())?;let source_hash=formats::digest(&source)?;
        let got=self.writer_run("writer-version-restore",Some(&source),Some(&copy),&[("file.versions",json!({"save":"Captured version"})),("caret.docEnd",json!({})),("text.insert",json!({"text":" Later unsaved marker."})),("file.versions",json!({"restore":0})),("file.versions",json!({}))])?;
        self.check("Session restore writes a separate original-content copy",got.status.success() && String::from_utf8_lossy(&got.stdout).contains("Captured version") && Self::word_text(&copy)?.contains("Original version marker.") && !Self::word_text(&copy)?.contains("Later unsaved marker.") && formats::digest(&source)?==source_hash)?;
        let got=self.writer_run("writer-version-reopen",Some(&copy),None,&[("file.versions",json!({}))])?;
        self.check("Saved copy has no persistent version history",got.status.success() && Self::list(&got,"file.versions")?.last()==Some(&json!([])))?;
        let invalid=out.join("invalid.docx");let got=self.writer_run("writer-invalid-version",Some(&source),Some(&invalid),&[("file.versions",json!({"restore":99}))])?;
        self.check("Unavailable snapshot cannot publish output",!got.status.success() && !invalid.exists() && formats::digest(&source)?==source_hash)?;
        let sheet=out.join("print-source.xlsx");let pdf=out.join("printed.pdf");
        let got=self.call("sheet-create",&self.o.sheet.clone(),&[vec!["run".to_owned()],actions(&[("cell.set",json!({"cell":"A1","input":"Document print fixture"})),("cell.set",json!({"cell":"B2","input":"42"}))]),vec!["--out".to_owned(),sheet.display().to_string(),"--quiet".to_owned()]].concat())?;
        self.check("Spreadsheet print source created",got.status.success())?;let sheet_hash=formats::digest(&sheet)?;
        let got=self.call("sheet-print-file",&self.o.sheet.clone(),&[vec!["run".to_owned(),"--in".to_owned(),sheet.display().to_string()],actions(&[("file.print",json!({"path":pdf}))]),vec!["--quiet".to_owned()]].concat())?;
        self.check("Spreadsheet print produces PDF without source mutation",got.status.success() && formats::package_kind(&pdf)=="pdf" && formats::digest(&sheet)?==sheet_hash)?;
        let info=self.call("sheet-print-info",&self.o.pdf_tools.join("bin/pdfinfo"),&[pdf.display().to_string()])?;
        let text=self.call("sheet-print-text",&self.o.pdf_tools.join("bin/pdftotext"),&[pdf.display().to_string(),"-".to_owned()])?;
        self.check("Independent PDF tools parse pages and contents",info.status.success() && text.status.success() && String::from_utf8_lossy(&info.stdout).contains("Pages:") && String::from_utf8_lossy(&text.stdout).contains("Document print fixture") && String::from_utf8_lossy(&text.stdout).contains("42"))?;
        let unavailable=out.join("missing-parent/failed.pdf");
        let got=self.call("sheet-print-unavailable-file",&self.o.sheet.clone(),&[vec!["run".to_owned(),"--in".to_owned(),sheet.display().to_string()],actions(&[("file.print",json!({"path":unavailable}))]),vec!["--quiet".to_owned()]].concat())?;
        self.check("Unavailable PDF destination preserves source",!got.status.success() && !unavailable.exists() && formats::digest(&sheet)?==sheet_hash)?;
        Ok(())
    }
}
fn inside(o:Options)->Result<()> {
    ensure(unsafe{libc::getpid()}==1 && unsafe{libc::geteuid()}!=0 && env::var("HOME")?=="/home/fixture" && !Path::new("/dev/dri").exists(),"Private document boundary required")?;
    let mut runner=Runner{o,commands:Vec::new(),checks:Vec::new()};
    let result=runner.run();
    let report=json!({"passed":result.is_ok(),"scope":"CLI session versions and print-to-file only; no persistent history or printer submission", "checks":runner.checks,"commands":runner.commands,"installedTools":{"writer":formats::digest(&runner.o.writer)?,"sheet":formats::digest(&runner.o.sheet)?,"slides":formats::digest(&runner.o.slides)?},"error":result.as_ref().err().map(|e|e.to_string())});
    fs::write(runner.o.output.join("result.json"),format!("{}\n",serde_json::to_string_pretty(&report)?))?;
    result
}
fn main()->Result<()> {let o=options()?; if o.inside {inside(o)} else {outer(o)}}
