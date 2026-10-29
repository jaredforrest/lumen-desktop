//! Delayed real browser handoff for the fixture's private PDF association.
use lumen_office_workflows::{Result, ensure};
use serde_json::Value;
use std::{env,fs::{self,OpenOptions},os::unix::process::CommandExt,path::Path,process::{Command,Stdio},thread,time::Duration};
fn main()->Result<()> {
    let config:Value=serde_json::from_slice(&fs::read("/home/fixture/pdf-viewer.json")?)?;
    let browser=Path::new(config["browser"].as_str().ok_or("Browser missing")?);
    ensure(browser.starts_with("/nix/store") && browser.is_file(),"Pinned browser required")?;
    let profile=config["profile"].as_str().ok_or("Private profile missing")?;
    let log=OpenOptions::new().create(true).append(true).open(config["log"].as_str().ok_or("Viewer log missing")?)?;
    thread::sleep(Duration::from_millis(400));
    let mut command=Command::new(browser);
    command.args([format!("--user-data-dir={profile}"),"--no-first-run".into(),"--no-default-browser-check".into(),"--ozone-platform=wayland".into(),"--force-renderer-accessibility".into()]);
    command.args(env::args_os().skip(1));
    command.env_remove("GTK_USE_PORTAL").env_remove("XDG_CURRENT_DESKTOP").stdout(Stdio::from(log.try_clone()?)).stderr(Stdio::from(log));
    Err(command.exec().into())
}
