use serde_json::{Value, json};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn describe(configured: Option<&str>, home: &Path) -> Result<Value, &'static str> {
    let path = configured.filter(|path| !path.is_empty()).map(PathBuf::from).unwrap_or_else(|| home.join("Desktop"));
    if !path.is_absolute() { return Err("The desktop directory must be an absolute local path"); }
    let disabled = configured.is_some_and(|path| !path.is_empty()) && path == home;
    let exists = !disabled && fs::metadata(&path).is_ok_and(|metadata| metadata.is_dir());
    let writable = exists && {
        let cpath = std::ffi::CString::new(path.as_os_str().as_encoded_bytes()).map_err(|_| "Invalid desktop directory")?;
        unsafe { libc::access(cpath.as_ptr(), libc::W_OK) == 0 }
    };
    Ok(json!({"path":path.to_string_lossy(),"disabled":disabled,"exists":exists,"writable":writable}))
}

fn home_list(home: &Path) -> Value {
    let mut rows = Vec::new();
    if let Ok(entries) = fs::read_dir(home) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') { continue; }
            let directory = entry.metadata().is_ok_and(|info| info.is_dir());
            rows.push((name, directory));
        }
    }
    rows.sort_by(|a, b| (!a.1, a.0.to_lowercase()).cmp(&(!b.1, b.0.to_lowercase())));
    Value::Array(rows.into_iter().take(60)
        .map(|(name, directory)| json!({"name":name,"directory":directory})).collect())
}

fn main() {
    let home = env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
    if env::args().nth(1).as_deref() == Some("--home-list") {
        println!("{}", home_list(&home));
        return;
    }
    let configured = Command::new("xdg-user-dir").arg("DESKTOP").output().ok()
        .filter(|output| output.status.success()).and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|output| output.trim_end_matches(['\n', '\r']).to_owned());
    match describe(configured.as_deref(), &home) {
        Ok(result) => println!("{result}"),
        Err(error) => { println!("{}",json!({"error":error}));std::process::exit(1); }
    }
}

#[cfg(test)] mod tests {
    use super::*;
    #[test] fn location_does_not_create_missing_folder() {
        let root=env::temp_dir().join(format!("lumen-desktop-files-{}",std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let path=root.join("custom desktop");
        let row=describe(Some(path.to_str().unwrap()),&root).unwrap();
        assert_eq!(row["path"],path.to_string_lossy().as_ref());
        assert_eq!(row["exists"],false);
        assert!(!path.exists());
        fs::create_dir(&path).unwrap();
        assert_eq!(describe(Some(path.to_str().unwrap()),&root).unwrap()["exists"],true);
        fs::remove_dir_all(root).unwrap();
    }
    #[test] fn home_is_disabled_and_relative_paths_fail() {
        let home=Path::new("/fixture/home");
        let row=describe(Some("/fixture/home"),home).unwrap();
        assert_eq!(row["disabled"],true);
        assert_eq!(row["exists"],false);
        assert_eq!(row["writable"],false);
        assert_eq!(describe(None,home).unwrap()["path"],"/fixture/home/Desktop");
        assert!(describe(Some("relative"),home).is_err());
    }
    #[test] fn home_list_hides_dotfiles_and_sorts_folders_first() {
        let root=env::temp_dir().join(format!("lumen-home-list-{}",std::process::id()));
        fs::create_dir_all(root.join("Zeta")).unwrap();
        fs::write(root.join("alpha"),b"hello").unwrap();
        fs::write(root.join(".private"),b"hidden").unwrap();
        let rows=home_list(&root);
        assert_eq!(rows.as_array().unwrap().len(),2);
        assert_eq!(rows[0]["name"],"Zeta");
        assert_eq!(rows[0]["directory"],true);
        assert_eq!(rows[1]["name"],"alpha");
        fs::remove_dir_all(root).unwrap();
    }
}
