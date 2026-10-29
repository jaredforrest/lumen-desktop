//! Transitional owned preview JSON-line command.
use lumen_file_surfaces::preview::PreviewSession;
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};
use std::io::{self, BufRead, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

type Output = Arc<Mutex<io::Stdout>>;
struct Worker {
    capability: String,
    operation: String,
    cancel: Arc<AtomicBool>,
}
static STOP: AtomicBool = AtomicBool::new(false);

extern "C" fn stop(_signal: i32) {
    STOP.store(true, Ordering::SeqCst);
}

fn install_stop_handler() {
    // SAFETY: the handler only stores to a lock-free atomic, and the action is
    // initialized before this process begins processing requests.
    unsafe {
        let mut action: libc::sigaction = std::mem::zeroed();
        action.sa_sigaction = stop as *const () as usize;
        action.sa_flags = 0;
        libc::sigemptyset(&mut action.sa_mask);
        libc::sigaction(libc::SIGTERM, &action, std::ptr::null_mut());
    }
}

fn emit(output: &Output, value: &Value) {
    if let Ok(mut output) = output.lock() {
        let _ = writeln!(output, "{value}");
        let _ = output.flush();
    }
}

fn action_worker(
    session: Arc<PreviewSession>,
    request: Value,
    workers: Arc<Mutex<HashMap<u64, Worker>>>,
    output: Output,
    cancel: Arc<AtomicBool>,
) {
    let id = request["id"].as_u64().unwrap();
    let capability = request["capability"].as_str().unwrap_or("");
    let operation = request["operation"].as_str().unwrap_or("");
    let result = session.resource_action(
        capability,
        operation,
        request["application"].as_str(),
        request["choice"].as_str(),
        &request["parameters"],
        &cancel,
    );
    let mut value = match result {
        Ok(value) => value,
        Err(error) => json!({"reason":error.reason,"error":error.message}),
    };
    value["id"] = json!(id);
    value["action"] = json!("resource-action");
    value["operation"] = json!(operation);
    value["capability"] = json!(capability);
    value["generation"] = json!(session.requests.generation());
    workers.lock().unwrap().remove(&id);
    emit(&output, &value);
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
            .map_or(available.len(), |index| index + 1);
        let ended = available[count - 1] == b'\n';
        if bytes.len() + count > 32_769 {
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

fn response(
    session: &PreviewSession,
    request: &Value,
    capabilities: &mut HashMap<u64, String>,
    workers: &Arc<Mutex<HashMap<u64, Worker>>>,
) -> Value {
    let id = request["id"].as_u64().unwrap();
    let action = request["action"].as_str().unwrap_or("");
    let mut value = match action {
        "preview" => {
            let result = (|| {
                let path = request["path"]
                    .as_str()
                    .ok_or("A local absolute preview path is required")?;
                let page = request.get("page").and_then(Value::as_u64).unwrap_or(1);
                session
                    .requests
                    .register(id, request["token"].as_str())
                    .map_err(|error| error.to_string())?;
                session
                    .request(
                        Path::new(path),
                        page,
                        &request["identity"],
                        request["token"].as_str(),
                        Some(id),
                    )
                    .map_err(|error| error.to_string())
            })();
            match result {
                Ok(value) => {
                    if let Some(key) = value["actionCapability"].as_str() {
                        capabilities.insert(id, key.to_owned());
                    }
                    value
                }
                Err(error) => {
                    let _ = session
                        .requests
                        .retire(&[id], None, Duration::from_secs(0), false);
                    json!({"reason":"provider_failure","error":error})
                }
            }
        }
        "retire-preview" => {
            let ids = request["requests"]
                .as_array()
                .filter(|ids| ids.len() <= 64)
                .and_then(|ids| ids.iter().map(Value::as_u64).collect::<Option<Vec<_>>>());
            match ids {
                Some(ids) => {
                    let keys = ids
                        .iter()
                        .filter_map(|id| capabilities.remove(id))
                        .collect::<Vec<_>>();
                    session.actions.retire(&keys);
                    match session.requests.retire(
                        &ids,
                        request["keepSourceToken"].as_str(),
                        Duration::from_secs(9),
                        true,
                    ) {
                        Ok(()) => json!({"retired":true}),
                        Err(error) => json!({"reason":error.reason,"error":error.message}),
                    }
                }
                None => json!({"reason":"invalid_request","error":"Invalid preview release"}),
            }
        }
        "cancel-action" => {
            let target = request["target"].as_u64();
            let mut accepted = false;
            if let Some(target) = target {
                if let Some(worker) = workers.lock().unwrap().get(&target) {
                    accepted = worker.capability == request["capability"].as_str().unwrap_or("")
                        && matches!(worker.operation.as_str(), "ocr" | "cutout" | "image-markup");
                    if accepted {
                        worker.cancel.store(true, Ordering::SeqCst);
                    }
                }
            }
            json!({"accepted":accepted})
        }
        _ => json!({"reason":"invalid_request","error":"Invalid preview action"}),
    };
    value["id"] = json!(id);
    value["action"] = json!(action);
    value["generation"] = json!(session.requests.generation());
    if action == "preview" {
        value["sourcePath"] = request["path"].clone();
    }
    value
}

fn main() {
    unsafe { libc::umask(0o077) };
    install_stop_handler();
    let Ok(session) = PreviewSession::new() else {
        return;
    };
    let session = Arc::new(session);
    let output: Output = Arc::new(Mutex::new(io::stdout()));
    emit(
        &output,
        &json!({"event":"ready","generation":session.requests.generation()}),
    );
    let mut input = io::stdin().lock();
    let mut capabilities = HashMap::new();
    let workers: Arc<Mutex<HashMap<u64, Worker>>> = Arc::new(Mutex::new(HashMap::new()));
    let mut worker_threads = Vec::new();
    let mut seen = HashSet::new();
    let mut highest = 0_u64;
    while !STOP.load(Ordering::SeqCst) {
        let Ok(Some(line)) = next_line(&mut input) else {
            break;
        };
        let mut request = Value::Null;
        let mut value = match line {
            Some(line) => match serde_json::from_slice::<Value>(&line) {
                Ok(parsed) => {
                    request = parsed;
                    Value::Null
                }
                Err(_) => json!({"error":"Invalid preview request"}),
            },
            None => json!({"error":"Invalid preview request"}),
        };
        if value.is_null() {
            let id = request["id"].as_u64();
            if !request.is_object()
                || id.is_none_or(|id| id == 0 || id > 9_007_199_254_740_991)
                || request["generation"] != session.requests.generation()
            {
                value = json!({"error":"Invalid preview request"});
            } else {
                let id = id.unwrap();
                if seen.contains(&id) || id <= highest.saturating_sub(128) {
                    value = json!({"id":id,"action":request["action"],"generation":session.requests.generation(),"reason":"invalid_request","error":"This request identity was retired"});
                } else {
                    highest = highest.max(id);
                    seen.retain(|old| *old > highest.saturating_sub(128));
                    seen.insert(id);
                    if request["action"] == "resource-action" {
                        let mut running = workers.lock().unwrap();
                        if running.is_empty() {
                            let cancel = Arc::new(AtomicBool::new(false));
                            running.insert(
                                id,
                                Worker {
                                    capability: request["capability"]
                                        .as_str()
                                        .unwrap_or("")
                                        .to_owned(),
                                    operation: request["operation"]
                                        .as_str()
                                        .unwrap_or("")
                                        .to_owned(),
                                    cancel: cancel.clone(),
                                },
                            );
                            let session = session.clone();
                            let workers = workers.clone();
                            let output = output.clone();
                            worker_threads.push(std::thread::spawn(move || {
                                action_worker(session, request, workers, output, cancel)
                            }));
                            continue;
                        }
                        value = json!({"id":id,"action":"resource-action","generation":session.requests.generation(),"capability":request["capability"],"operation":request["operation"],"reason":"busy","error":"Another preview action is still completing"});
                    } else {
                        value = response(&session, &request, &mut capabilities, &workers);
                    }
                }
            }
        }
        emit(&output, &value);
    }
    for worker in workers.lock().unwrap().values() {
        worker.cancel.store(true, Ordering::SeqCst);
    }
    for thread in worker_threads {
        let _ = thread.join();
    }
    let _ = session.requests.close();
}
