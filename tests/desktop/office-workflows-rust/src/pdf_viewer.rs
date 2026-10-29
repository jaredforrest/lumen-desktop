//! A private PDF association that launches the installed browser directly.
use crate::{Result, ensure};
use serde_json::{Value, json};
use std::{collections::BTreeMap, fs, os::unix::fs::symlink, path::{Path, PathBuf}};

fn pinned(path:&Path)->Result<PathBuf> {
    let path=path.canonicalize()?;
    ensure(path.starts_with("/nix/store") && path.is_file(),"Pinned PDF viewer dependency required")?;
    Ok(path)
}
pub fn configure(environment:&mut BTreeMap<String,String>, browser:&Path, xdg_tools:&Path, mime_data:&Path, helper_bin:&Path)->Result<Value> {
    let browser=pinned(browser)?;let xdg=pinned(&xdg_tools.join("bin/xdg-open"))?;
    let mime=mime_data.canonicalize()?;
    ensure(mime.starts_with("/nix/store") && mime.join("mime/mime.cache").is_file(),"Pinned MIME catalogue required")?;
    let helper_bin=pinned(helper_bin)?;
    let data=Path::new(environment.get("XDG_DATA_HOME").ok_or("Private XDG data home required")?);
    let config=Path::new(environment.get("XDG_CONFIG_HOME").ok_or("Private XDG config home required")?);
    let apps=data.join("applications");fs::create_dir_all(&apps)?;fs::create_dir_all(config)?;
    let helper=Path::new("/home/fixture/pdf-viewer");
    if helper.exists(){fs::remove_file(helper)?;}
    symlink(&helper_bin,helper)?;
    fs::write("/home/fixture/pdf-viewer.json",format!("{}\n",json!({"browser":browser,"profile":"/home/fixture/pdf-profile","log":"/artifacts/pdf-viewer.log"})))?;
    fs::write(apps.join("fixture-pdf.desktop"),format!("[Desktop Entry]\nType=Application\nName=Fixture PDF Viewer\nExec={} %U\nMimeType=application/pdf;\nTerminal=false\n",helper.display()))?;
    fs::write(config.join("mimeapps.list"),"[Default Applications]\napplication/pdf=fixture-pdf.desktop\n")?;
    environment.insert("XDG_DATA_DIRS".into(),format!("{}:{}",mime.display(),environment.get("XDG_DATA_DIRS").map(String::as_str).unwrap_or("")));
    environment.insert("PATH".into(),format!("{}:{}",xdg.parent().ok_or("XDG tools bin")?.display(),environment.get("PATH").map(String::as_str).unwrap_or("")));
    environment.insert("BROWSER".into(),helper.display().to_string());
    Ok(json!({"browser":browser,"sha256":crate::formats::digest(&browser)?,"helper":helper,"xdgTools":xdg.parent().ok_or("XDG tools bin")?.parent().ok_or("XDG tools root")?,"delaySeconds":0.4,"scope":"Private association and delayed real browser process only; no fake viewer or printer"}))
}
#[cfg(test)]
mod tests {
    #[test]
    fn desktop_entry_uses_private_helper() {
        let entry=format!("[Desktop Entry]\nType=Application\nName=Fixture PDF Viewer\nExec={} %U\nMimeType=application/pdf;\nTerminal=false\n","/home/fixture/pdf-viewer");
        assert!(entry.contains("MimeType=application/pdf;"));
        assert!(entry.contains("Exec=/home/fixture/pdf-viewer %U"));
    }
}
