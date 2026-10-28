//! Explicit immutable dependencies, clean-source provenance and private application boundary.
use crate::{Result, fail};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Workflow {
    NewSave,
    Basic,
    Downloads,
    Permissions,
    Library,
    PageFullscreen,
    Overwrite,
}
impl Workflow {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NewSave => "new-save",
            Self::Basic => "basic",
            Self::Downloads => "downloads",
            Self::Permissions => "permissions",
            Self::Library => "library",
            Self::PageFullscreen => "page-fullscreen",
            Self::Overwrite => "overwrite",
        }
    }
    pub fn parse(value: &str) -> Result<Self> {
        Ok(match value {
            "new-save" => Self::NewSave,
            "basic" => Self::Basic,
            "downloads" => Self::Downloads,
            "permissions" => Self::Permissions,
            "library" => Self::Library,
            "page-fullscreen" => Self::PageFullscreen,
            "overwrite" => Self::Overwrite,
            _ => return Err(fail("Unknown browser workflow")),
        })
    }
}
pub struct Options {
    pub browser: PathBuf,
    pub launcher: PathBuf,
    pub sway: PathBuf,
    pub pointer_tool: PathBuf,
    pub keyboard_tool: PathBuf,
    pub font_directory: PathBuf,
    pub at_spi_prefix: PathBuf,
    pub software_driver: PathBuf,
    pub portal_tools: Option<PathBuf>,
    pub output: PathBuf,
    pub effects: bool,
    pub trace_wayland: bool,
    pub workflow: Workflow,
    pub inside: bool,
}
fn immutable(path: &Path, directory: bool) -> Result<PathBuf> {
    let canonical = path.canonicalize()?;
    if !canonical.starts_with("/nix/store")
        || if directory {
            !canonical.is_dir()
        } else {
            !canonical.is_file()
        }
    {
        return Err(fail(format!(
            "Provide an existing immutable dependency: {}",
            path.display()
        )));
    }
    Ok(canonical)
}
impl Options {
    pub fn parse(args: impl IntoIterator<Item = String>) -> Result<Self> {
        let mut values = BTreeMap::new();
        let mut effects = false;
        let mut trace_wayland = false;
        let mut inside = false;
        let mut workflow = Workflow::Basic;
        let mut iter = args.into_iter();
        while let Some(key) = iter.next() {
            match key.as_str() {
                "--effects" => effects = true,
                "--trace-wayland" => trace_wayland = true,
                "--inside" => inside = true,
                "--workflow" => {
                    workflow = Workflow::parse(&iter.next().ok_or("--workflow value")?)?
                }
                "--browser" | "--launcher" | "--sway" | "--pointer-tool" | "--keyboard-tool"
                | "--font-directory" | "--at-spi-prefix" | "--software-driver"
                | "--portal-tools" | "--output" => {
                    values.insert(key, iter.next().ok_or("option value")?);
                }
                _ => return Err(fail(format!("Unknown browser option: {key}"))),
            }
        }
        let mut required = |key: &str, directory: bool| -> Result<PathBuf> {
            immutable(
                Path::new(&values.remove(key).ok_or(format!("{key} required"))?),
                directory,
            )
        };
        let browser = required("--browser", false)?;
        let launcher = required("--launcher", false)?;
        let sway = required("--sway", false)?;
        let pointer_tool = required("--pointer-tool", false)?;
        let keyboard_tool = required("--keyboard-tool", false)?;
        let font_directory = required("--font-directory", true)?;
        let at_spi_prefix = required("--at-spi-prefix", true)?;
        let software_driver = required("--software-driver", true)?;
        let portal_tools = values
            .remove("--portal-tools")
            .map(|v| immutable(Path::new(&v), false))
            .transpose()?;
        if workflow == Workflow::Overwrite && portal_tools.is_none() {
            return Err(fail(
                "Overwrite acceptance requires explicit real portal providers",
            ));
        }
        let output = PathBuf::from(values.remove("--output").ok_or("--output required")?);
        let output = if output.is_absolute() {
            output
        } else {
            env::current_dir()?.join(output)
        };
        Ok(Self {
            browser,
            launcher,
            sway,
            pointer_tool,
            keyboard_tool,
            font_directory,
            at_spi_prefix,
            software_driver,
            portal_tools,
            output,
            effects,
            trace_wayland,
            workflow,
            inside,
        })
    }
    pub fn dependency_manifest(&self) -> Value {
        json!({"browser":self.browser,"launcher":self.launcher,"sway":self.sway,"pointer_tool":self.pointer_tool,"keyboard_tool":self.keyboard_tool,"font_directory":self.font_directory,"at_spi_prefix":self.at_spi_prefix,"software_driver":self.software_driver})
    }
    pub fn child_args(&self) -> Vec<String> {
        let mut args = vec!["--inside".into()];
        for (name, path) in [
            ("browser", &self.browser),
            ("launcher", &self.launcher),
            ("sway", &self.sway),
            ("pointer-tool", &self.pointer_tool),
            ("keyboard-tool", &self.keyboard_tool),
            ("font-directory", &self.font_directory),
            ("at-spi-prefix", &self.at_spi_prefix),
            ("software-driver", &self.software_driver),
        ] {
            args.push(format!("--{name}"));
            args.push(path.display().to_string());
        }
        args.extend([
            "--output".into(),
            "/artifacts".into(),
            "--workflow".into(),
            self.workflow.as_str().into(),
        ]);
        if let Some(path) = &self.portal_tools {
            args.extend(["--portal-tools".into(), path.display().to_string()]);
        }
        if self.effects {
            args.push("--effects".into());
        }
        if self.trace_wayland {
            args.push("--trace-wayland".into());
        }
        args
    }
}
pub fn validate_inside(output: &Path) -> Result<()> {
    if env::current_dir()? != Path::new("/source")
        || output != Path::new("/artifacts")
        || env::var("HOME").ok().as_deref() != Some("/home/fixture")
        || env::var("XDG_RUNTIME_DIR").ok().as_deref() != Some("/run/fixture")
        || unsafe { libc::getuid() } == 0
    {
        return Err(fail(
            "Private application boundary required before internal entry",
        ));
    }
    let init = fs::metadata("/proc/1/status")?;
    use std::os::unix::fs::MetadataExt;
    if init.uid() != unsafe { libc::getuid() } {
        return Err(fail(
            "Private application boundary required before internal entry",
        ));
    }
    let status = fs::read_to_string("/proc/self/status")?;
    for expected in [
        "CapEff:\t0000000000000000",
        "CapBnd:\t0000000000000000",
        "NoNewPrivs:\t1",
    ] {
        if !status.contains(expected) {
            return Err(fail("Private application capabilities were not dropped"));
        }
    }
    Ok(())
}
fn check(program: &str, args: &[&str], cwd: &Path) -> Result<String> {
    let output = Command::new(program).args(args).current_dir(cwd).output()?;
    if !output.status.success() {
        return Err(fail(format!(
            "{program} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        )));
    }
    Ok(String::from_utf8(output.stdout)?.trim().into())
}
fn hash(path: &Path) -> Result<String> {
    Ok(format!("{:x}", Sha256::digest(fs::read(path)?)))
}
fn source_files(
    directory: &Path,
    repo: &Path,
    records: &mut BTreeMap<String, String>,
) -> Result<()> {
    for entry in fs::read_dir(directory)? {
        let path = entry?.path();
        if path.is_dir() {
            if path.file_name().is_some_and(|name| name == "target") {
                continue;
            }
            source_files(&path, repo, records)?;
        } else if path.is_file() {
            records.insert(path.strip_prefix(repo)?.display().to_string(), hash(&path)?);
        }
    }
    Ok(())
}
pub fn source_provenance(repo: &Path, options: &Options, exit_code: i32) -> Result<Value> {
    let revision = check("git", &["rev-parse", "HEAD"], repo)?;
    let dirty = check("git", &["status", "--porcelain"], repo)?;
    if !dirty.is_empty() {
        return Err(fail("Freeze application fixture source before acceptance"));
    }
    let mut sources = BTreeMap::new();
    for folder in [
        "tests/desktop/browser-workflows-rust",
        "tests/desktop/application_fixture",
    ] {
        let directory = repo.join(folder);
        if directory.exists() {
            source_files(&directory, repo, &mut sources)?;
        }
    }
    Ok(
        json!({"revision":revision,"sources":sources,"dependencies":options.dependency_manifest(),"workflow":options.workflow.as_str(),"effects":options.effects,"portalTools":options.portal_tools,"exitCode":exit_code}),
    )
}
pub fn launcher_environment(launcher: &Path) -> Result<BTreeMap<String, String>> {
    let output = Command::new(launcher).arg("--print-env0").output()?;
    if !output.status.success() {
        return Err(fail("Launcher environment failed"));
    }
    let mut values = BTreeMap::new();
    for entry in output.stdout.split(|byte| *byte == 0) {
        if let Some(i) = entry.iter().position(|byte| *byte == b'=') {
            values.insert(
                String::from_utf8(entry[..i].to_vec())?,
                String::from_utf8(entry[i + 1..].to_vec())?,
            );
        }
    }
    Ok(values)
}
pub fn find_immutable_tool(name: &str, path: &str) -> Result<PathBuf> {
    for dir in path.split(':') {
        let candidate = Path::new(dir).join(name);
        if candidate.is_file() {
            return immutable(&candidate, false);
        }
    }
    Err(fail(format!(
        "Run within pinned nix develop runtime; missing immutable {name}"
    )))
}
pub fn run_outer(options: &Options) -> Result<i32> {
    let repo = env::current_dir()?;
    let mut provenance = source_provenance(&repo, options, -1)?;
    fs::create_dir(&options.output)?;
    let environment = launcher_environment(&options.launcher)?;
    for name in ["dbus-daemon", "grim", "wtype"] {
        find_immutable_tool(name, environment.get("PATH").ok_or("launcher PATH")?)?;
    }
    let boundary = find_immutable_tool(
        "lumen-application-isolation",
        environment.get("PATH").ok_or("launcher PATH")?,
    )?;
    let executable = immutable(&env::current_exe()?, false)?;
    let mut command = Command::new(boundary);
    command
        .arg("run")
        .arg("--source")
        .arg(&repo)
        .arg("--output")
        .arg(&options.output);
    if options.effects {
        command.arg("--software-graphics");
    }
    command
        .arg("--")
        .arg(executable)
        .args(options.child_args())
        .env_clear()
        .envs(&environment);
    let result = command.status()?;
    let code = result.code().unwrap_or(1);
    provenance["exitCode"] = json!(code);
    fs::write(
        options.output.join("source-provenance.json"),
        serde_json::to_vec_pretty(&provenance)?,
    )?;
    Ok(code)
}
