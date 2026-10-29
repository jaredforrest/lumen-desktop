//! Isolated installed office round trips; capability reports are not a parity verdict.
use lumen_office_workflows::{Result, corpus, ensure, error, formats, rasters};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::env;
use std::fs::{self, File};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;
use wait_timeout::ChildExt;

fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn save(path: &Path, value: &Value) -> Result<()> {
    fs::write(path, format!("{}\n", serde_json::to_string_pretty(value)?))?;
    Ok(())
}
fn text<'a>(value: &'a Value) -> Result<&'a str> {
    value
        .as_str()
        .ok_or_else(|| error(format!("Expected string: {value}")))
}
fn nonempty(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(v) => *v,
        Value::Array(v) => !v.is_empty(),
        Value::Object(v) => !v.is_empty(),
        Value::String(v) => !v.is_empty(),
        _ => true,
    }
}
fn command_str(program: &Path, args: &[&str]) -> Result<String> {
    let output = Command::new(program).args(args).output()?;
    ensure(
        output.status.success(),
        format!(
            "{} {:?}: {}",
            program.display(),
            args,
            String::from_utf8_lossy(&output.stderr)
        ),
    )?;
    Ok(String::from_utf8(output.stdout)?.trim().to_owned())
}

#[derive(Clone)]
struct Options {
    writer: PathBuf,
    sheet: PathBuf,
    slides: PathBuf,
    pdf_tools: PathBuf,
    output: PathBuf,
    source: PathBuf,
    inside: bool,
    source_sha: String,
}
impl Options {
    fn parse() -> Result<Self> {
        let mut writer = None;
        let mut sheet = None;
        let mut slides = None;
        let mut pdf_tools = None;
        let mut output = None;
        let mut source = None;
        let mut inside = false;
        let mut source_sha = String::new();
        let mut args = env::args_os().skip(1);
        while let Some(arg) = args.next() {
            match arg.to_str() {
                Some("--writer") => {
                    writer = Some(PathBuf::from(args.next().ok_or("Missing writer")?))
                }
                Some("--sheet") => sheet = Some(PathBuf::from(args.next().ok_or("Missing sheet")?)),
                Some("--slides") => {
                    slides = Some(PathBuf::from(args.next().ok_or("Missing slides")?))
                }
                Some("--pdf-tools") => {
                    pdf_tools = Some(PathBuf::from(args.next().ok_or("Missing PDF tools")?))
                }
                Some("--output") => {
                    output = Some(PathBuf::from(args.next().ok_or("Missing output")?))
                }
                Some("--source") => {
                    source = Some(PathBuf::from(args.next().ok_or("Missing source")?))
                }
                Some("--inside") => inside = true,
                Some("--source-sha") => {
                    source_sha = args
                        .next()
                        .ok_or("Missing source SHA")?
                        .to_string_lossy()
                        .into_owned()
                }
                _ => {
                    return Err(error(format!(
                        "Unknown office workflow option: {}",
                        arg.to_string_lossy()
                    )));
                }
            }
        }
        let current = env::current_dir()?;
        let output = output.ok_or("--output required")?;
        Ok(Self {
            writer: writer.ok_or("--writer required")?,
            sheet: sheet.ok_or("--sheet required")?,
            slides: slides.ok_or("--slides required")?,
            pdf_tools: pdf_tools.ok_or("--pdf-tools required")?,
            output: if output.is_absolute() {
                output
            } else {
                current.join(output)
            },
            source: source.unwrap_or(current),
            inside,
            source_sha,
        })
    }
    fn app(&self, name: &str) -> Result<&Path> {
        match name {
            "writer" => Ok(&self.writer),
            "sheet" => Ok(&self.sheet),
            "slides" => Ok(&self.slides),
            _ => Err(error("Unknown office provider")),
        }
    }
    fn extension(name: &str) -> Result<&'static str> {
        match name {
            "writer" => Ok("docx"),
            "sheet" => Ok("xlsx"),
            "slides" => Ok("pptx"),
            _ => Err(error("Unknown office provider")),
        }
    }
    fn pdf_tool(&self, name: &str) -> PathBuf {
        self.pdf_tools.join("bin").join(name)
    }
}
fn immutable_executable(path: &Path) -> Result<PathBuf> {
    let path = path.canonicalize()?;
    ensure(
        path.starts_with("/nix/store") && path.is_file(),
        "Use exact pinned installed tools",
    )?;
    Ok(path)
}
fn outer(mut options: Options) -> Result<()> {
    fs::create_dir(&options.output)?;
    fs::set_permissions(&options.output, fs::Permissions::from_mode(0o700))?;
    options.source = options.source.canonicalize()?;
    let sha = command_str(
        Path::new("git"),
        &[
            "-C",
            options.source.to_str().ok_or("Source path")?,
            "rev-parse",
            "HEAD",
        ],
    )?;
    for binary in [
        &options.writer,
        &options.sheet,
        &options.slides,
        &options.pdf_tool("pdfinfo"),
        &options.pdf_tool("pdftotext"),
        &options.pdf_tool("pdftoppm"),
    ] {
        immutable_executable(binary)?;
    }
    let current = env::current_exe()?;
    let executable = if current.starts_with(&options.source) {
        Path::new("/source").join(current.strip_prefix(&options.source)?)
    } else {
        current
    };
    let boundary = env::var_os("PATH")
        .and_then(|value| {
            env::split_paths(&value)
                .map(|dir| dir.join("lumen-application-isolation"))
                .find(|path| path.is_file())
        })
        .ok_or("Pinned Rust application isolation launcher required")?;
    let status = Command::new(boundary)
        .arg("run")
        .arg("--source")
        .arg(&options.source)
        .arg("--output")
        .arg(&options.output)
        .arg("--")
        .arg(executable)
        .args([
            "--inside",
            "--writer",
            options.writer.to_str().ok_or("Writer path")?,
            "--sheet",
            options.sheet.to_str().ok_or("Sheet path")?,
            "--slides",
            options.slides.to_str().ok_or("Slides path")?,
            "--pdf-tools",
            options.pdf_tools.to_str().ok_or("PDF tool path")?,
            "--output",
            "/artifacts",
            "--source-sha",
            &sha,
        ])
        .status()?;
    ensure(
        status.success(),
        format!("Installed office workflow failed: {status}"),
    )
}
fn isolated() -> Result<Value> {
    let uid = unsafe { libc::getuid() };
    let gid = unsafe { libc::getgid() };
    let pid = std::process::id();
    let hostname = fs::read_to_string("/proc/sys/kernel/hostname")?
        .trim()
        .to_owned();
    let home = env::var("HOME")?;
    ensure(
        uid != 0 && pid == 1 && hostname == "fixture" && home == "/home/fixture",
        "Private application boundary required",
    )?;
    let capabilities = fs::read_to_string("/proc/self/status")?
        .lines()
        .filter(|line| {
            ["CapEff:", "CapBnd:", "NoNewPrivs:"]
                .iter()
                .any(|prefix| line.starts_with(prefix))
        })
        .map(str::trim)
        .map(str::to_owned)
        .collect::<Vec<_>>();
    Ok(json!({"uid":uid,"gid":gid,"hostname":hostname,"home":home,"capabilities":capabilities}))
}
struct CallResult {
    stdout: String,
    stderr: String,
    returncode: i32,
}
struct Workflow {
    options: Options,
    commands: Vec<Value>,
    capabilities: Vec<Value>,
}
impl Workflow {
    fn new(options: Options) -> Self {
        Self {
            options,
            commands: Vec::new(),
            capabilities: Vec::new(),
        }
    }
    fn out(&self, name: &str) -> PathBuf {
        self.options.output.join(name)
    }
    fn call(&mut self, label: &str, command: Vec<String>, required: bool) -> Result<CallResult> {
        let (program, args) = command.split_first().ok_or("Empty office command")?;
        let stdout_path = self.out(&format!("{label}.stdout"));
        let stderr_path = self.out(&format!("{label}.stderr"));
        let mut child = Command::new(program)
            .args(args)
            .stdout(Stdio::from(File::create(&stdout_path)?))
            .stderr(Stdio::from(File::create(&stderr_path)?))
            .spawn()?;
        let status = match child.wait_timeout(Duration::from_secs(60))? {
            Some(status) => status,
            None => {
                child.kill()?;
                child.wait()?;
                return Err(error(format!("Office command timed out: {label}")));
            }
        };
        let stdout = String::from_utf8(fs::read(stdout_path)?)?;
        let stderr = String::from_utf8(fs::read(stderr_path)?)?;
        let returncode = status.code().unwrap_or(-1);
        self.commands.push(json!({"label":label,"command":command,"returncode":returncode,"stdoutSha256":sha(stdout.as_bytes()),"stderrSha256":sha(stderr.as_bytes())}));
        ensure(
            !required || returncode == 0,
            format!("{label} failed; inspect retained stderr"),
        )?;
        Ok(CallResult {
            stdout,
            stderr,
            returncode,
        })
    }
    fn capability(&mut self, id: &str, action: &str, status: &str, evidence: Value) {
        let mut item = json!({"id":id,"action":action,"status":status});
        if let (Some(target), Some(fields)) = (item.as_object_mut(), evidence.as_object()) {
            target.extend(fields.clone());
        }
        self.capabilities.push(item);
    }
    fn action_list(items: &[(String, Value)]) -> Result<Vec<String>> {
        let mut args = Vec::new();
        for (name, data) in items {
            args.push("--cmd".into());
            args.push(format!("{name}={}", serde_json::to_string(data)?));
        }
        Ok(args)
    }
    fn assert_source(path: &Path, expected: &str) -> Result<()> {
        ensure(
            formats::digest(path)? == expected,
            format!("Source mutated: {}", path.display()),
        )
    }
    fn inspect(name: &str, path: &Path) -> Result<Value> {
        match name {
            "writer" => formats::writer(path),
            "sheet" => formats::spreadsheet(path),
            "slides" => formats::presentation(path),
            _ => Err(error("Unknown office provider")),
        }
    }
    fn pdf(
        &mut self,
        app: &str,
        source: &Path,
        label: &str,
        expected_text: &[&str],
    ) -> Result<Value> {
        let before = formats::digest(source)?;
        let target = self.out(&format!("{label}.pdf"));
        let program = self.options.app(app)?.display().to_string();
        if app == "sheet" {
            let mut args = vec![
                program,
                "run".into(),
                "--in".into(),
                source.display().to_string(),
            ];
            args.extend(Self::action_list(&[(
                "file.exportPdf".into(),
                json!({"path":target,"sheets":"all"}),
            )])?);
            args.push("--quiet".into());
            self.call(label, args, true)?;
        } else {
            self.call(
                label,
                vec![
                    program,
                    "convert".into(),
                    source.display().to_string(),
                    target.display().to_string(),
                ],
                true,
            )?;
        }
        Self::assert_source(source, &before)?;
        ensure(
            formats::package_kind(&target) == "pdf",
            "PDF export has invalid container",
        )?;
        let metadata = self
            .call(
                &format!("{label}-info"),
                vec![
                    self.options.pdf_tool("pdfinfo").display().to_string(),
                    target.display().to_string(),
                ],
                true,
            )?
            .stdout;
        let pages = metadata
            .lines()
            .find_map(|line| line.strip_prefix("Pages:"))
            .ok_or("PDF page count missing")?
            .trim()
            .parse::<usize>()?;
        ensure(pages > 0, "PDF has no pages")?;
        let text = self
            .call(
                &format!("{label}-text"),
                vec![
                    self.options.pdf_tool("pdftotext").display().to_string(),
                    "-layout".into(),
                    target.display().to_string(),
                    "-".into(),
                ],
                true,
            )?
            .stdout;
        for phrase in expected_text {
            ensure(
                text.contains(phrase),
                format!("{label} PDF missing text: {phrase}"),
            )?;
        }
        let prefix = format!("{label}-page");
        self.call(
            &format!("{label}-raster"),
            vec![
                self.options.pdf_tool("pdftoppm").display().to_string(),
                "-scale-to".into(),
                "1000".into(),
                "-png".into(),
                target.display().to_string(),
                self.out(&prefix).display().to_string(),
            ],
            true,
        )?;
        let mut images = fs::read_dir(&self.options.output)?
            .filter_map(|row| row.ok().map(|row| row.path()))
            .filter(|path| {
                path.file_name().is_some_and(|name| {
                    name.to_string_lossy().starts_with(&format!("{prefix}-"))
                        && name.to_string_lossy().ends_with(".png")
                })
            })
            .collect::<Vec<_>>();
        images.sort();
        Ok(
            json!({"path":target,"sha256":formats::digest(&target)?,"pages":pages,"text":text,"sourceSha256":before,"images":images}),
        )
    }
}
impl Workflow {
    fn execute(&mut self, report: &mut Value) -> Result<()> {
        let corpus = corpus::create()?;
        fs::write(self.out("synthetic.png"), corpus.image)?;
        for app in ["writer", "sheet", "slides"] {
            let program = self.options.app(app)?.display().to_string();
            self.call(
                &format!("{app}-version"),
                vec![
                    program.clone(),
                    if app == "sheet" {
                        "version".into()
                    } else {
                        "--version".into()
                    },
                ],
                true,
            )?;
            let mut commands = vec![program, "commands".into()];
            if app != "slides" {
                commands.push("--json".into());
            }
            self.call(&format!("{app}-commands"), commands, true)?;
        }
        let mut sources = BTreeMap::<String, PathBuf>::new();
        let mut baseline = BTreeMap::<String, Value>::new();
        let mut copies = BTreeMap::<String, PathBuf>::new();
        let mut exports = BTreeMap::<String, Value>::new();
        for (app, actions) in [
            ("writer", &corpus.writer),
            ("sheet", &corpus.grid),
            ("slides", &corpus.deck),
        ] {
            let extension = Options::extension(app)?;
            let path = self.out(&format!("{app}-source.{extension}"));
            let mut args = vec![self.options.app(app)?.display().to_string(), "run".into()];
            args.extend(Self::action_list(actions)?);
            args.push(if app == "sheet" {
                "--out".into()
            } else {
                "--save".into()
            });
            args.push(path.display().to_string());
            if app == "writer" {
                args.extend(["--author".into(), "Fixture".into()]);
            }
            self.call(&format!("{app}-create"), args, true)?;
            ensure(
                formats::package_kind(&path) == extension,
                format!("{app} source container mismatch"),
            )?;
            let facts = Self::inspect(app, &path)?;
            save(&self.out(&format!("{app}-facts.json")), &facts)?;
            self.capability(
                &format!(
                    "APP-OFFICE-0{}",
                    ["writer", "sheet", "slides"]
                        .iter()
                        .position(|name| name == &app)
                        .ok_or("App index")?
                        + 1
                ),
                &format!("create {extension}"),
                "fixture_verified",
                json!({"facts":self.out(&format!("{app}-facts.json"))}),
            );
            sources.insert(app.into(), path);
            baseline.insert(app.into(), facts);
        }
        let writer = &baseline["writer"];
        ensure(
            writer["sections"].as_array().is_some_and(|v| v.len() == 2)
                && writer["tables"].as_array().is_some_and(|v| v.len() == 1),
            "Writer section or table corpus missing",
        )?;
        for key in ["media", "stories", "comments", "changes", "links"] {
            ensure(
                nonempty(&writer[key]),
                format!("Writer {key} corpus missing"),
            )?;
        }
        ensure(
            writer["paragraphs"].as_array().is_some_and(|paragraphs| {
                paragraphs.iter().any(|paragraph| {
                    paragraph["runs"]
                        .as_array()
                        .is_some_and(|runs| runs.iter().any(|run| run["format"].get("b").is_some()))
                })
            }),
            "Writer bold run missing",
        )?;
        let sheet = &baseline["sheet"];
        ensure(
            sheet["sheets"].as_array().is_some_and(|v| v.len() == 2)
                && nonempty(&sheet["charts"])
                && nonempty(&sheet["names"]),
            "Sheet corpus missing",
        )?;
        ensure(
            nonempty(&sheet["sheets"][0]["filter"])
                && sheet["sheets"][0]["cells"]["F2"]["numberFormat"] == "yyyy-mm-dd"
                && sheet["sheets"][0]["cells"]["E2"]["formula"] == "SUM(B2:B4)",
            "Sheet format/formula corpus missing",
        )?;
        ensure(
            sheet["sheets"][1]["cells"]["B1"]["type"] == "e"
                && sheet["sheets"][1]["cells"]["B1"]["value"] == "#NAME?",
            "Unsupported formula feedback missing",
        )?;
        let slides = &baseline["slides"];
        ensure(
            slides["slides"].as_array().is_some_and(|v| v.len() == 2)
                && nonempty(&slides["media"])
                && slides["notes"].as_array().is_some_and(|notes| {
                    notes.iter().any(|note| {
                        note.as_str()
                            .is_some_and(|text| text.contains("Synthetic speaker notes"))
                    })
                })
                && nonempty(&slides["slides"][0]["transitions"])
                && nonempty(&slides["fonts"]),
            "Presentation corpus missing",
        )?;
        self.capability(
            "APP-OFFICE-02",
            "unknown formula feedback",
            "fixture_verified",
            json!({"cell":"Summary!B1","value":"#NAME?"}),
        );
        for app in ["writer", "sheet", "slides"] {
            let source = sources[app].clone();
            let source_hash = formats::digest(&source)?;
            let extension = Options::extension(app)?;
            let copy = self.out(&format!("{app}-copy.{extension}"));
            let edited = self.out(&format!("{app}-edited.{extension}"));
            let edit = match app {
                "writer" => vec![
                    ("caret.docEnd".into(), json!({})),
                    (
                        "text.insert".into(),
                        json!({"text":" Temporary reversible edit."}),
                    ),
                ],
                "sheet" => vec![(
                    "cell.set".into(),
                    json!({"sheet":"Summary","cell":"C1","input":"Temporary reversible edit"}),
                )],
                "slides" => vec![(
                    "slide.notes".into(),
                    json!({"index":0,"text":"Temporary reversible edit"}),
                )],
                _ => unreachable!(),
            };
            let mut actions = edit;
            actions.push(("file.saveAs".into(), json!({"path":edited})));
            actions.push(("edit.undo".into(), json!({})));
            let mut args = vec![
                self.options.app(app)?.display().to_string(),
                "run".into(),
                if app == "writer" {
                    "--file".into()
                } else {
                    "--in".into()
                },
                source.display().to_string(),
            ];
            args.extend(Self::action_list(&actions)?);
            args.push(if app == "sheet" {
                "--out".into()
            } else {
                "--save".into()
            });
            args.push(copy.display().to_string());
            self.call(&format!("{app}-edit-undo"), args, true)?;
            Self::assert_source(&source, &source_hash)?;
            ensure(
                formats::package_kind(&copy) == extension,
                "Office copy container mismatch",
            )?;
            ensure(
                Self::inspect(app, &edited)? != baseline[app],
                format!("{app} edit was not observable"),
            )?;
            let facts = Self::inspect(app, &copy)?;
            let diff = formats::differences(&baseline[app], &facts);
            save(
                &self.out(&format!("{app}-roundtrip-differences.json")),
                &json!(diff),
            )?;
            self.capability(
                "WF-OFFICE-01",
                &format!("{extension} edit/save-copy/reopen/undo"),
                if diff.is_empty() {
                    "fixture_verified"
                } else {
                    "degraded"
                },
                json!({"differences":diff,"sourceSha256":source_hash}),
            );
            copies.insert(app.into(), copy);
        }
        for (app, phrases) in [
            ("writer", vec!["Workflow heading", "Second section."]),
            ("sheet", vec!["Item", "Amount"]),
            ("slides", vec!["Synthetic title", "Second slide"]),
        ] {
            let before = self.pdf(app, &sources[app], &format!("{app}-export"), &phrases)?;
            let after = self.pdf(app, &copies[app], &format!("{app}-copy-export"), &phrases)?;
            let before_images = before["images"]
                .as_array()
                .ok_or("Original PDF images missing")?
                .iter()
                .map(|path| PathBuf::from(text(path).unwrap_or("")))
                .collect::<Vec<_>>();
            let after_images = after["images"]
                .as_array()
                .ok_or("Copy PDF images missing")?
                .iter()
                .map(|path| PathBuf::from(text(path).unwrap_or("")))
                .collect::<Vec<_>>();
            let differences = rasters::compare_rasters(&before_images, &after_images)?;
            self.capability("APP-OFFICE-05",&format!("{app} PDF export/source preservation"),"fixture_verified",json!({"sourceSha256":before["sourceSha256"],"pages":before["pages"],"copyPages":after["pages"],"paintedDifferences":differences}));
            exports.insert(
                app.into(),
                json!({"original":before,"copy":after,"paintedDifferences":differences}),
            );
        }
        let original = &sources["writer"];
        let source_hash = formats::digest(original)?;
        let odt = self.out("writer.odt");
        let back = self.out("writer-odt-back.docx");
        self.call(
            "writer-odt",
            vec![
                self.options.writer.display().to_string(),
                "convert".into(),
                original.display().to_string(),
                odt.display().to_string(),
            ],
            true,
        )?;
        ensure(
            formats::package_kind(&odt) == "odt",
            "Writer ODT container mismatch",
        )?;
        self.call(
            "writer-odt-back",
            vec![
                self.options.writer.display().to_string(),
                "convert".into(),
                odt.display().to_string(),
                back.display().to_string(),
            ],
            true,
        )?;
        Self::assert_source(original, &source_hash)?;
        let diff = formats::differences(&baseline["writer"], &formats::writer(&back)?);
        save(&self.out("writer-odt-differences.json"), &json!(diff))?;
        self.capability(
            "APP-OFFICE-01",
            "DOCX to ODT and back",
            if diff.is_empty() {
                "fixture_verified"
            } else {
                "degraded"
            },
            json!({"differences":diff}),
        );
        let odt_before = formats::digest(&odt)?;
        let odt_copy = self.out("writer-copy.odt");
        let odt_copy_docx = self.out("writer-odt-copy.docx");
        let mut args = vec![
            self.options.writer.display().to_string(),
            "run".into(),
            "--file".into(),
            odt.display().to_string(),
        ];
        args.extend(Self::action_list(&[
            ("caret.docEnd".into(), json!({})),
            (
                "text.insert".into(),
                json!({"text":" Temporary reversible edit."}),
            ),
            ("edit.undo".into(), json!({})),
        ])?);
        args.extend(["--save".into(), odt_copy.display().to_string()]);
        self.call("writer-odt-edit-undo", args, true)?;
        Self::assert_source(&odt, &odt_before)?;
        ensure(
            formats::package_kind(&odt_copy) == "odt",
            "Writer ODT copy container mismatch",
        )?;
        self.call(
            "writer-odt-copy-check",
            vec![
                self.options.writer.display().to_string(),
                "convert".into(),
                odt_copy.display().to_string(),
                odt_copy_docx.display().to_string(),
            ],
            true,
        )?;
        let odt_diff =
            formats::differences(&formats::writer(&back)?, &formats::writer(&odt_copy_docx)?);
        self.capability("APP-OFFICE-01","ODT same-format edit/undo/save-copy/reopen",if odt_diff.is_empty(){"fixture_verified"}else{"degraded"},json!({"differences":odt_diff,"scope":"Starts with the separately measured degraded DOCX-to-ODT fixture; does not erase those conversion losses"}));
        let sheet_source = &sources["sheet"];
        let source_hash = formats::digest(sheet_source)?;
        let result = self.call(
            "sheet-ods",
            vec![
                self.options.sheet.display().to_string(),
                "convert".into(),
                sheet_source.display().to_string(),
                self.out("sheet.ods").display().to_string(),
            ],
            false,
        )?;
        Self::assert_source(sheet_source, &source_hash)?;
        self.capability(
            "APP-OFFICE-02",
            "ODS export",
            if result.returncode != 0 {
                "unsupported"
            } else {
                "format_probe_required"
            },
            json!({"providerReturncode":result.returncode,"reason":result.stderr.trim()}),
        );
        let slides_source = &sources["slides"];
        let source_hash = formats::digest(slides_source)?;
        let odp = self.out("slides.odp");
        let result = self.call(
            "slides-odp",
            vec![
                self.options.slides.display().to_string(),
                "convert".into(),
                slides_source.display().to_string(),
                odp.display().to_string(),
            ],
            false,
        )?;
        Self::assert_source(slides_source, &source_hash)?;
        let actual = if odp.exists() {
            formats::package_kind(&odp)
        } else {
            "absent"
        };
        self.capability("APP-OFFICE-03","ODP export",if result.returncode==0&&actual!="odp"{"provider_defect"}else if result.returncode!=0{"unsupported"}else{"fixture_verified"},json!({"providerReturncode":result.returncode,"expectedFormat":"odp","actualFormat":actual,"outputSha256":if odp.exists(){Some(formats::digest(&odp)?)}else{None}}));
        for (id, action) in [
            (
                "APP-OFFICE-04",
                "GUI template/recent/recovery and save conflict lifecycle",
            ),
            (
                "APP-OFFICE-05",
                "GUI format warning cancel and print-dialog acceptance",
            ),
            (
                "APP-OFFICE-06",
                "collaboration and versions; separate from file synchronization",
            ),
            (
                "APP-OFFICE-07",
                "interactive PDF selection/search/rotation/annotation/forms/signatures",
            ),
            (
                "WF-OFFICE-01",
                "GUI open/edit/save/unsaved close and killed-process recovery",
            ),
            ("APP-OFFICE-03", "interactive presenter view"),
        ] {
            self.capability(id, action, "not_yet_verified", json!({}));
        }
        report["harnessPassed"] = json!(true);
        report["exports"] = json!(exports);
        report["formatFacts"] = json!(baseline);
        report["sourceHashes"] = json!(
            sources
                .iter()
                .map(|(app, path)| Ok((app.clone(), formats::digest(path)?)))
                .collect::<Result<BTreeMap<_, _>>>()?
        );
        let module_root = Path::new("/source/tests/desktop/office-workflows-rust");
        let mut modules = BTreeMap::new();
        for name in [
            "src/main.rs",
            "src/lib.rs",
            "src/control.rs",
            "src/corpus.rs",
            "src/dialogs.rs",
            "src/formats.rs",
            "src/live.rs",
            "src/rasters.rs",
            "corpus.json",
            "synthetic.png",
        ] {
            let path = module_root.join(name);
            modules.insert(
                path.strip_prefix("/source")?.display().to_string(),
                formats::digest(&path)?,
            );
        }
        let isolation = Path::new("/source/tests/desktop/application-isolation/src/main.rs");
        modules.insert(
            isolation.strip_prefix("/source")?.display().to_string(),
            formats::digest(isolation)?,
        );
        report["fixtureModules"] = json!(modules);
        Ok(())
    }
}
fn inside(options: Options) -> Result<()> {
    let isolation = isolated()?;
    let mut tools = BTreeMap::new();
    for path in [
        &options.writer,
        &options.sheet,
        &options.slides,
        &options.pdf_tool("pdfinfo"),
        &options.pdf_tool("pdftotext"),
        &options.pdf_tool("pdftoppm"),
    ] {
        tools.insert(path.display().to_string(), formats::digest(path)?);
    }
    let mut workflow = Workflow::new(options.clone());
    let mut report = json!({"isolation":isolation,"installedTools":tools,"source":options.source_sha,"commands":[],"capabilities":[],"workflowComplete":false,"harnessPassed":false});
    let outcome = workflow.execute(&mut report);
    report["commands"] = json!(workflow.commands);
    report["capabilities"] = json!(workflow.capabilities);
    if let Err(error) = &outcome {
        report["failure"] = json!(error.to_string());
    }
    save(&workflow.out("result.json"), &report)?;
    println!(
        "{}",
        serde_json::to_string(
            &json!({"harnessPassed":report["harnessPassed"],"workflowComplete":false,"capabilityRecords":workflow.capabilities.len(),"result":"/artifacts/result.json"})
        )?
    );
    outcome
}
fn main() -> Result<()> {
    let options = Options::parse()?;
    if options.inside {
        inside(options)
    } else {
        outer(options)
    }
}
