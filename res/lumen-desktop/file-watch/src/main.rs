//! Nonrecursive directory subscriptions for Files and Search.
use notify::{RecommendedWatcher, RecursiveMode, Watcher, recommended_watcher};
use serde_json::{Value, json};
use std::collections::HashSet;
use std::io::{BufRead, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

type Output = Arc<Mutex<BufWriter<std::io::Stdout>>>;

fn emit(output: &Output, value: &Value) {
    if let Ok(mut writer) = output.lock() {
        let _ = writeln!(writer, "{value}");
        let _ = writer.flush();
    }
}

fn directories_for(paths: &[PathBuf], watched: &HashSet<PathBuf>) -> HashSet<PathBuf> {
    paths.iter().filter_map(|path| {
        if watched.contains(path) { Some(path.clone()) }
        else { path.parent().filter(|parent| watched.contains(*parent)).map(Path::to_path_buf) }
    }).collect()
}

fn main() {
    let output: Output = Arc::new(Mutex::new(BufWriter::new(std::io::stdout())));
    let watched = Arc::new(Mutex::new(HashSet::<PathBuf>::new()));
    let callback_output = output.clone();
    let callback_watched = watched.clone();
    let mut watcher: RecommendedWatcher = match recommended_watcher(move |event: notify::Result<notify::Event>| {
        if let Ok(event) = event {
            if event.kind.is_access() { return; }
            if let Ok(paths) = callback_watched.lock() {
                for directory in directories_for(&event.paths, &paths) {
                    emit(&callback_output, &json!({"event":"changed","path":directory}));
                }
            }
        }
    }) {
        Ok(watcher) => watcher,
        Err(error) => { emit(&output,&json!({"error":error.to_string()})); return; }
    };
    for line in std::io::stdin().lock().lines() {
        let Ok(line) = line else { break };
        let Ok(request) = serde_json::from_str::<Value>(&line) else { continue };
        if request["action"] == "close" { break; }
        if request["action"] != "set" { continue; }
        let Some(paths) = request["paths"].as_array() else { continue };
        if paths.len()>8192 { emit(&output,&json!({"event":"ready","generation":request["generation"],"registered":[],"failed":paths})); continue; }
        let requested: HashSet<PathBuf> = paths.iter().filter_map(|value| value.as_str().map(PathBuf::from)).collect();
        let current = watched.lock().unwrap().clone();
        for path in current.difference(&requested) { let _ = watcher.unwatch(path); }
        let mut registered: HashSet<PathBuf> = current.intersection(&requested).cloned().collect();
        let mut failed = HashSet::<PathBuf>::new();
        for path in requested.difference(&current) {
            if watcher.watch(path, RecursiveMode::NonRecursive).is_ok() { registered.insert(path.clone()); }
            else { failed.insert(path.clone()); }
        }
        *watched.lock().unwrap() = registered.clone();
        emit(&output,&json!({"event":"ready","generation":request["generation"],
            "registered":registered,"failed":failed}));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn maps_child_events_to_armed_parent_only() {
        let watched=HashSet::from([PathBuf::from("/tmp/armed")]);
        let paths=[PathBuf::from("/tmp/armed/child"),PathBuf::from("/tmp/other/child")];
        assert_eq!(directories_for(&paths,&watched),watched);
    }
}
