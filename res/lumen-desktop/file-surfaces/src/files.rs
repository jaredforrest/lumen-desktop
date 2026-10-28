//! Files listing and preference requests shared by the live QML surfaces.
use crate::captured::Facts;
use serde_json::{Value, json};
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::os::fd::AsRawFd;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Component, Path, PathBuf};
use unicode_casefold::UnicodeCaseFold;

const LIMIT: usize = 5_000;
const TAG_IDS: &[&str] = &["red", "orange", "yellow", "green", "blue", "purple", "grey"];

fn fold(value: &str) -> String {
    value.case_fold().collect()
}

pub fn uri(path: &Path) -> String {
    let mut result = String::from("file://");
    for byte in path.as_os_str().as_bytes() {
        if byte.is_ascii_alphanumeric() || b"/-._~".contains(byte) {
            result.push(*byte as char);
        } else {
            result.push_str(&format!("%{byte:02X}"));
        }
    }
    result
}

fn home() -> Result<PathBuf, String> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or("HOME is missing".into())
}

pub fn local_path(value: &str) -> Result<PathBuf, String> {
    if value.is_empty() || value.contains('\0') {
        return Err("A local path is required".into());
    }
    let expanded = if value == "~" {
        home()?
    } else if let Some(suffix) = value.strip_prefix("~/") {
        home()?.join(suffix)
    } else {
        PathBuf::from(value)
    };
    let absolute = if expanded.is_absolute() {
        expanded
    } else {
        std::env::current_dir()
            .map_err(|error| error.to_string())?
            .join(expanded)
    };
    let mut result = PathBuf::from("/");
    for part in absolute.components() {
        match part {
            Component::Normal(piece) => result.push(piece),
            Component::ParentDir => {
                result.pop();
            }
            Component::RootDir => result = PathBuf::from("/"),
            Component::CurDir | Component::Prefix(_) => (),
        }
    }
    Ok(result)
}

pub fn local_name(value: &str) -> Result<&str, String> {
    if value.is_empty() || value == "." || value == ".." || value.contains(['/', '\0']) {
        Err("Enter a name without a slash".into())
    } else {
        Ok(value)
    }
}

fn writable(path: &Path) -> bool {
    use std::ffi::CString;
    CString::new(path.as_os_str().as_bytes())
        .is_ok_and(|value| unsafe { libc::access(value.as_ptr(), libc::W_OK) } == 0)
}

fn kind(name: &str, directory: bool) -> String {
    if directory {
        "Folder".into()
    } else {
        mime_guess::from_path(name)
            .first_raw()
            .unwrap_or("Document")
            .into()
    }
}

fn breadcrumbs(folder: &Path) -> Vec<Value> {
    folder.ancestors().collect::<Vec<_>>().into_iter().rev().map(|path| {
        json!({"name":path.file_name().map(|name| name.to_string_lossy().to_string()).unwrap_or("/".into()),"path":path})
    }).collect()
}

/// Lists a local folder with the same bounds and metadata fields as the QML protocol.
/// `inspect` is the typed Rust colour-tag service, injected for process fixtures.
pub fn listing<I, T>(data: &Value, inspect: I, trash_items: T) -> Result<Value, String>
where
    I: Fn(&Value) -> Result<Value, String>,
    T: Fn() -> Result<Value, String>,
{
    if data["path"] == "trash:" {
        return Ok(
            json!({"path":"trash:","parent":home()?,"rows":trash_items()?,"breadcrumbs":[],"writable":false,"limited":false,"unreadable":0}),
        );
    }
    let default = home()?;
    let folder = local_path(
        data["path"]
            .as_str()
            .unwrap_or_else(|| default.to_str().unwrap_or("/")),
    )?;
    let raw_query = fold(data["query"].as_str().unwrap_or(""));
    let tag_terms: Vec<&str> = raw_query
        .split_whitespace()
        .filter_map(|part| part.strip_prefix("tag:"))
        .collect();
    if tag_terms.iter().any(|tag| !TAG_IDS.contains(tag)) {
        return Err(
            "Unknown colour tag; use red, orange, yellow, green, blue, purple or grey".into(),
        );
    }
    let query = raw_query
        .split_whitespace()
        .filter(|part| !part.starts_with("tag:"))
        .collect::<Vec<_>>()
        .join(" ");
    let mut rows = Vec::new();
    let mut unreadable = 0;
    let mut scanned = 0;
    let mut limited = false;
    let entries = fs::read_dir(&folder).map_err(|error| error.to_string())?;
    for item in entries {
        scanned += 1;
        if scanned > 100_000 {
            limited = true;
            break;
        }
        let Ok(item) = item else {
            unreadable += 1;
            continue;
        };
        let name = item.file_name().to_string_lossy().to_string();
        if data["hidden"] != true && name.starts_with('.') {
            continue;
        }
        if !query.is_empty() && !fold(&name).contains(&query) {
            continue;
        }
        let path = item.path();
        let info = match fs::symlink_metadata(&path) {
            Ok(info) => info,
            Err(_) => {
                unreadable += 1;
                continue;
            }
        };
        let facts = Facts::read(&path).map_err(|error| error.to_string())?;
        let metadata = inspect(
            &json!({"action":"inspect","path":path,"mode":info.mode(),"device":info.dev(),"inode":info.ino(),"nlink":info.nlink()}),
        )?;
        let state = &metadata["linkState"];
        let directory = state["directory"] == true;
        let tags = metadata["tags"].as_array().cloned().unwrap_or_default();
        if tag_terms
            .iter()
            .any(|tag| !tags.iter().any(|item| item["id"].as_str() == Some(*tag)))
        {
            continue;
        }
        let file_kind = kind(&name, directory);
        rows.push(json!({
            "actionFacts":facts.wire(),"identity":metadata["identity"],"renameFollow":metadata["renameFollow"],
            "tags":tags,"tagsSupported":metadata["supported"],"tagsError":metadata["error"],
            "name":name,"path":path,"uri":uri(&path),"directory":directory,
            "link":state["link"],"linkStatus":state["linkStatus"],"linkError":state["linkError"],
            "size":info.len(),"mtime":info.mtime() as f64 + info.mtime_nsec() as f64 / 1e9,"kind":file_kind,
        }));
        if rows.len() >= LIMIT {
            limited = true;
            break;
        }
    }
    let sort = data["sort"].as_str().unwrap_or("name");
    if !["name", "modified", "size", "kind"].contains(&sort) {
        return Err("Unknown sort order".into());
    }
    if !data["foldersFirst"].is_null() && !data["foldersFirst"].is_boolean() {
        return Err("Folders-first must be a boolean".into());
    }
    let grouping = data["groupBy"].as_str().unwrap_or("none");
    if !["none", "kind"].contains(&grouping) {
        return Err("Unknown grouping".into());
    }
    rows.sort_by_key(|row| fold(row["name"].as_str().unwrap_or("")));
    rows.sort_by(|a, b| {
        let order = match sort {
            "modified" => a["mtime"]
                .as_f64()
                .unwrap_or(0.0)
                .total_cmp(&b["mtime"].as_f64().unwrap_or(0.0)),
            "size" => a["size"].as_u64().cmp(&b["size"].as_u64()),
            "kind" => a["kind"].as_str().cmp(&b["kind"].as_str()).then_with(|| {
                fold(a["name"].as_str().unwrap_or("")).cmp(&fold(b["name"].as_str().unwrap_or("")))
            }),
            _ => {
                fold(a["name"].as_str().unwrap_or("")).cmp(&fold(b["name"].as_str().unwrap_or("")))
            }
        };
        if data["descending"] == true {
            order.reverse()
        } else {
            order
        }
    });
    let folders_first = data["foldersFirst"] != false;
    if folders_first {
        rows.sort_by_key(|row| row["directory"] != true);
    }
    for row in &mut rows {
        let kind = row["kind"].as_str().unwrap_or("");
        row["group"] = json!(if row["directory"] == true {
            "Folders"
        } else if kind.starts_with("image/") {
            "Images"
        } else if kind.starts_with("audio/") {
            "Audio"
        } else if kind.starts_with("video/") {
            "Video"
        } else {
            "Documents"
        });
    }
    if grouping == "kind" {
        rows.sort_by_key(|row| {
            (
                if row["directory"] == true && folders_first {
                    0
                } else {
                    1
                },
                row["group"].as_str().unwrap_or("").to_owned(),
            )
        });
    }
    Ok(
        json!({"path":folder,"parent":folder.parent().unwrap_or(&folder),"rows":rows,
        "limited":limited,"unreadable":unreadable,"writable":writable(&folder),"breadcrumbs":breadcrumbs(&folder)}),
    )
}

/// Strict persistent Files preferences, serialized with a per-file lock.
pub fn preferences(data: &Value, state_root: &Path) -> Result<Value, String> {
    let file = state_root.join("lumen-desktop/session/files.json");
    let parent = file.parent().ok_or("Invalid preference path")?;
    fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    let parent_info = fs::symlink_metadata(parent).map_err(|error| error.to_string())?;
    if !parent_info.is_dir()
        || parent_info.file_type().is_symlink()
        || parent_info.uid() != unsafe { libc::geteuid() }
    {
        return Err("Files preference directory is unavailable".into());
    }
    fs::set_permissions(parent, fs::Permissions::from_mode(0o700))
        .map_err(|error| error.to_string())?;
    let lock = OpenOptions::new()
        .write(true)
        .create(true)
        .mode(0o600)
        .open(file.with_extension("lock"))
        .map_err(|error| error.to_string())?;
    if unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX) } != 0 {
        return Err("Files preferences are busy".into());
    }
    let mut bytes = Vec::new();
    let mut prefs = if OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(&file)
        .and_then(|reader| reader.take(1_048_577).read_to_end(&mut bytes).map(|_| ()))
        .is_ok()
        && bytes.len() <= 1_048_576
    {
        serde_json::from_slice::<Value>(&bytes)
            .ok()
            .filter(Value::is_object)
            .unwrap_or(json!({}))
    } else {
        json!({})
    };
    if data["action"] == "save-preferences" {
        let incoming = data["preferences"]
            .as_object()
            .ok_or("Unknown file preference")?;
        for (key, value) in incoming {
            let valid = match key.as_str() {
                "view" => {
                    ["grid", "list", "columns", "gallery"].contains(&value.as_str().unwrap_or(""))
                }
                "sort" => {
                    ["name", "modified", "size", "kind"].contains(&value.as_str().unwrap_or(""))
                }
                "descending" | "hidden" | "foldersFirst" => value.is_boolean(),
                "groupBy" => ["none", "kind"].contains(&value.as_str().unwrap_or("")),
                "sidebarWidth" => value
                    .as_i64()
                    .is_some_and(|number| (120..=360).contains(&number)),
                "collapsedSidebarGroups" => value.as_array().is_some_and(|groups| {
                    groups.iter().all(|group| {
                        ["favourites", "locations"].contains(&group.as_str().unwrap_or(""))
                    }) && groups.len()
                        == groups
                            .iter()
                            .collect::<std::collections::HashSet<_>>()
                            .len()
                }),
                "favorites" => {
                    value.is_null()
                        || value.as_array().is_some_and(|items| {
                            items.len() <= 32
                                && items.iter().all(valid_favorite)
                                && items
                                    .iter()
                                    .map(|item| item["path"].as_str())
                                    .collect::<std::collections::HashSet<_>>()
                                    .len()
                                    == items.len()
                        })
                }
                _ => return Err("Unknown file preference".into()),
            };
            if !valid {
                return Err(format!("Invalid file preference: {key}"));
            }
        }
        for (key, value) in incoming {
            prefs[key] = value.clone();
        }
        let mut temp =
            tempfile::NamedTempFile::new_in(parent).map_err(|error| error.to_string())?;
        serde_json::to_writer(&mut temp, &prefs).map_err(|error| error.to_string())?;
        temp.flush()
            .and_then(|_| temp.as_file().sync_all())
            .map_err(|error| error.to_string())?;
        temp.persist(&file).map_err(|error| error.to_string())?;
    }
    Ok(json!({"preferences":prefs}))
}

fn valid_favorite(value: &Value) -> bool {
    let Some(item) = value.as_object() else {
        return false;
    };
    if item.len() != 2 || !item.contains_key("name") || !item.contains_key("path") {
        return false;
    }
    let Some(name) = item["name"].as_str() else {
        return false;
    };
    let Some(path) = item["path"].as_str() else {
        return false;
    };
    !name.trim().is_empty()
        && name.chars().count() <= 64
        && !name.chars().any(char::is_control)
        && path.starts_with('/')
        && path.chars().count() <= 4096
        && !path.chars().any(char::is_control)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;

    #[test]
    fn listing_keeps_folders_first_and_exact_action_facts() {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir(root.path().join("Alpha")).unwrap();
        fs::write(root.path().join("beta.txt"), "private").unwrap();
        fs::write(root.path().join(".hidden"), "hidden").unwrap();
        symlink(root.path().join("beta.txt"), root.path().join("link")).unwrap();
        let value = listing(&json!({"path":root.path(),"action":"list"}), |request| {
            let path = Path::new(request["path"].as_str().unwrap());
            let info = fs::symlink_metadata(path).unwrap();
            Ok(json!({"identity":"fixture","renameFollow":"none","tags":[],"supported":true,"error":"",
                "linkState":{"directory":info.is_dir(),"link":info.file_type().is_symlink(),"linkStatus":"ok","linkError":""}}))
        }, || Ok(json!([]))).unwrap();
        let rows = value["rows"].as_array().unwrap();
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0]["name"], "Alpha");
        assert_eq!(rows[1]["name"], "beta.txt");
        assert_eq!(rows[1]["actionFacts"].as_array().unwrap().len(), 6);
        assert_eq!(rows[2]["link"], true);
        assert!(rows.iter().all(|row| row["name"] != ".hidden"));
    }

    #[test]
    fn preferences_reject_invalid_destination_without_changing_saved_state() {
        let root = tempfile::tempdir().unwrap();
        preferences(
            &json!({"action":"save-preferences","preferences":{"view":"grid"}}),
            root.path(),
        )
        .unwrap();
        assert!(preferences(&json!({"action":"save-preferences","preferences":{"favorites":[{"name":"Bad","path":"relative"}]}}), root.path()).is_err());
        assert_eq!(
            preferences(&json!({"action":"preferences"}), root.path()).unwrap()["preferences"]["view"],
            "grid"
        );
    }
}
