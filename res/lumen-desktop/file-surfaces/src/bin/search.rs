use lumen_file_surfaces::preview::PreviewSession;
use lumen_file_surfaces::search::{Index, configured_roots};
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};
use std::env;
use std::io::{self, BufRead};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, mpsc};
use std::thread;
use std::time::{Duration, Instant};

enum Event {
    Request(Value),
    Invalid,
    Dirty(PathBuf),
    Closed,
}
fn next_line(input: &mut impl BufRead) -> io::Result<Option<Option<Vec<u8>>>> {
    let mut bytes = Vec::new();
    let mut oversized = false;
    loop {
        let available = input.fill_buf()?;
        if available.is_empty() {
            return if bytes.is_empty() && !oversized {
                Ok(None)
            } else {
                Ok(Some((!oversized).then_some(bytes)))
            };
        }
        let count = available
            .iter()
            .position(|byte| *byte == b'\n')
            .map_or(available.len(), |position| position + 1);
        let ended = available[count - 1] == b'\n';
        if bytes.len() + count > 65_537 {
            oversized = true;
        }
        if !oversized {
            bytes.extend_from_slice(&available[..count]);
        }
        input.consume(count);
        if ended {
            return Ok(Some((!oversized).then_some(bytes)));
        }
    }
}
fn start_scan(index: &Arc<Index>, force: bool) {
    if index.begin_scan(force) {
        let index = index.clone();
        thread::spawn(move || index.scan());
    }
}
fn dispatch(
    index: Arc<Index>,
    session: Arc<PreviewSession>,
    capabilities: Arc<Mutex<HashMap<u64, String>>>,
    request: Value,
    emit: Arc<dyn Fn(Value) + Send + Sync>,
) {
    let action = request["action"].as_str().unwrap_or("").to_owned();
    let id = request["id"].clone();
    let result = (|| -> lumen_file_surfaces::search::Result<Value> {
        match action.as_str() {
            "search" => {
                start_scan(&index, false);
                index.search(request["query"].as_str().unwrap_or(""))
            }
            "preview" => index.preview(
                &session,
                request["id"]
                    .as_u64()
                    .ok_or("Invalid preview request identity")?,
                request["path"].as_str().ok_or("Missing file path")?,
                request["token"].as_str().ok_or("Missing source token")?,
            ),
            "open" | "parent" => index.open(
                request["path"].as_str().ok_or("Missing file path")?,
                request["token"].as_str().ok_or("Missing source token")?,
                action == "parent",
            ),
            "browse" => {
                let selected = index.selected(
                    request["path"].as_str().ok_or("Missing file path")?,
                    request["token"].as_str().ok_or("Missing source token")?,
                )?;
                let target = if request["parent"].as_bool().unwrap_or(false) {
                    selected.parent().unwrap_or(&selected).to_path_buf()
                } else {
                    selected
                };
                if !target.is_dir() {
                    Err("The selected folder is unavailable".into())
                } else {
                    Ok(json!({"browse":target}))
                }
            }
            "refresh" => {
                start_scan(&index, true);
                Ok(json!({}))
            }
            _ => Err("Unknown search command".into()),
        }
    })();
    match result {
        Ok(mut value) => {
            if action == "preview" {
                if let (Some(request_id), Some(key)) =
                    (request["id"].as_u64(), value["actionCapability"].as_str())
                {
                    capabilities
                        .lock()
                        .unwrap()
                        .insert(request_id, key.to_owned());
                }
            }
            if let Some(object) = value.as_object_mut() {
                object.insert("id".into(), id);
                object.insert("action".into(), json!(action));
            }
            emit(value);
        }
        Err(_) => {
            if action == "preview" {
                if let Some(request_id) = request["id"].as_u64() {
                    let _ =
                        session
                            .requests
                            .retire(&[request_id], None, Duration::from_secs(0), false);
                }
            }
            emit(
                json!({"id":id,"action":action,"previewGeneration":session.requests.generation(),"reason":"provider_failure","error":"The file is unavailable, the query is incomplete, or its application could not be opened"}),
            );
        }
    }
}
fn run() -> lumen_file_surfaces::search::Result<()> {
    unsafe {
        libc::umask(0o077);
    }
    let output_lock = Arc::new(Mutex::new(()));
    let emit: Arc<dyn Fn(Value) + Send + Sync> = Arc::new(move |value| {
        let _guard = output_lock.lock().unwrap();
        println!("{value}");
    });
    let cache = env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env::var_os("HOME").unwrap_or_default()).join(".cache"))
        .join("lumen-desktop/search/index.sqlite");
    let (tx, rx) = mpsc::channel::<Event>();
    let (watch_tx, watch_rx) = mpsc::channel::<PathBuf>();
    let session = Arc::new(PreviewSession::new().map_err(|error| error.to_string())?);
    let mut index = Index::new(configured_roots()?, cache, watch_tx, emit.clone())?;
    index.use_source_generation(session.generation.clone());
    let index = Arc::new(index);
    let capabilities = Arc::new(Mutex::new(HashMap::<u64, String>::new()));
    let mut ready = index.status();
    ready["event"] = json!("ready");
    ready["previewGeneration"] = json!(session.requests.generation());
    emit(ready);
    start_scan(&index, true);
    let watch_events = tx.clone();
    thread::spawn(move || {
        for path in watch_rx {
            if watch_events.send(Event::Dirty(path)).is_err() {
                break;
            }
        }
    });
    let input_events = tx.clone();
    thread::spawn(move || {
        let mut input = io::stdin().lock();
        while let Ok(Some(line)) = next_line(&mut input) {
            let event = match line {
                Some(line) => match serde_json::from_slice::<Value>(&line) {
                    Ok(value) if value.is_object() => Event::Request(value),
                    _ => Event::Invalid,
                },
                None => Event::Invalid,
            };
            if input_events.send(event).is_err() {
                return;
            }
        }
        let _ = input_events.send(Event::Closed);
    });
    let mut dirty = HashSet::new();
    let mut due: Option<Instant> = None;
    let mut workers: Vec<(bool, thread::JoinHandle<()>)> = Vec::new();
    loop {
        match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(Event::Request(request)) => {
                let mut index_worker = 0;
                while index_worker < workers.len() {
                    if workers[index_worker].1.is_finished() {
                        let (_, worker) = workers.remove(index_worker);
                        let _ = worker.join();
                    } else {
                        index_worker += 1;
                    }
                }
                let preview_request = request["action"] == "preview";
                let active = workers
                    .iter()
                    .filter(|(preview, _)| *preview == preview_request)
                    .count();
                if request["action"] != "retire-preview"
                    && active >= if preview_request { 1 } else { 3 }
                {
                    emit(
                        json!({"id":request["id"],"action":request["action"],"reason":"busy","error":"The search provider is still completing requests"}),
                    );
                    continue;
                }
                if matches!(
                    request["action"].as_str(),
                    Some("preview" | "retire-preview")
                ) {
                    if request["previewGeneration"] != session.requests.generation() {
                        emit(json!({"error":"Invalid search request"}));
                        continue;
                    }
                    if request["action"] == "preview" {
                        let Some(id) = request["id"].as_u64() else {
                            emit(json!({"error":"Invalid search request"}));
                            continue;
                        };
                        if session
                            .requests
                            .register(id, request["token"].as_str())
                            .is_err()
                        {
                            emit(json!({"error":"Invalid search request"}));
                            continue;
                        }
                    } else {
                        let Some(ids) = request["requests"]
                            .as_array()
                            .filter(|ids| ids.len() <= 64)
                            .and_then(|ids| {
                                ids.iter().map(Value::as_u64).collect::<Option<Vec<_>>>()
                            })
                        else {
                            emit(json!({"error":"Invalid search request"}));
                            continue;
                        };
                        let keys = ids
                            .iter()
                            .filter_map(|id| capabilities.lock().unwrap().remove(id))
                            .collect::<Vec<_>>();
                        session.actions.retire(&keys);
                        let keep = request["keepSourceToken"].as_str();
                        let result =
                            session
                                .requests
                                .retire(&ids, keep, Duration::from_secs(9), true);
                        match result {
                            Ok(()) => emit(
                                json!({"id":request["id"],"action":"retire-preview","retired":true,"previewGeneration":session.requests.generation()}),
                            ),
                            Err(error) => emit(
                                json!({"id":request["id"],"action":"retire-preview","reason":error.reason,"error":error.message,"previewGeneration":session.requests.generation()}),
                            ),
                        };
                        continue;
                    }
                }
                let index = index.clone();
                let session = session.clone();
                let capabilities = capabilities.clone();
                let emit = emit.clone();
                workers.push((
                    preview_request,
                    thread::spawn(move || dispatch(index, session, capabilities, request, emit)),
                ));
            }
            Ok(Event::Invalid) => emit(json!({"error":"Invalid search request"})),
            Ok(Event::Dirty(directory)) => {
                dirty.insert(directory);
                if due.is_none() {
                    due = Some(Instant::now() + Duration::from_millis(350));
                }
            }
            Ok(Event::Closed) => break,
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
            Err(mpsc::RecvTimeoutError::Timeout) => {}
        }
        if due.is_some_and(|time| Instant::now() >= time) {
            if index.is_scanning() {
                due = Some(Instant::now() + Duration::from_millis(500));
            } else {
                due = None;
                let pending = std::mem::take(&mut dirty);
                let index = index.clone();
                thread::spawn(move || index.refresh_dirty(pending));
            }
        }
    }
    index.close();
    for (_, worker) in workers {
        let _ = worker.join();
    }
    let _ = session.requests.close();
    Ok(())
}
fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
