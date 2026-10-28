use lumen_file_jobs::trash::Trash;
use lumen_file_jobs::{JobResult, Jobs, default_journal, trash_items};
use serde_json::{Value, json};
use std::env;
use std::fs;
use std::io::{Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::thread;
use std::time::{Duration, Instant};

static STOP: AtomicBool = AtomicBool::new(false);
extern "C" fn signal_stop(_: libc::c_int) {
    STOP.store(true, Ordering::SeqCst);
}

fn endpoint() -> JobResult<PathBuf> {
    Ok(
        PathBuf::from(env::var_os("XDG_RUNTIME_DIR").ok_or("XDG_RUNTIME_DIR is missing")?)
            .join("lumen-file-operations.sock"),
    )
}

fn read_line(stream: &mut impl Read, max: usize) -> JobResult<Vec<u8>> {
    let mut line = Vec::new();
    let mut byte = [0_u8];
    while line.len() < max {
        let count = stream.read(&mut byte).map_err(|error| error.to_string())?;
        if count == 0 || byte[0] == b'\n' {
            return Ok(line);
        }
        line.push(byte[0]);
    }
    Err("File operation request is too large".into())
}

fn controller(slot: &Mutex<Option<Arc<Trash>>>) -> JobResult<Arc<Trash>> {
    let mut current = slot.lock().unwrap();
    if current.is_none() {
        *current = Some(Arc::new(Trash::new()?));
    }
    Ok(Arc::clone(current.as_ref().unwrap()))
}

fn request(jobs: &Arc<Jobs>, trash: &Mutex<Option<Arc<Trash>>>, input: &Value) -> JobResult<Value> {
    let action = input["action"].as_str().unwrap_or("");
    if action.starts_with("trash-") {
        let fields: &[&str] = match action {
            "trash-prepare" => &["mode", "items"],
            "trash-confirm" | "trash-cancel" => &["token"],
            "trash-stop" | "trash-status" => &[],
            "trash-inspect" => &["operationId"],
            "trash-capabilities" => &["location"],
            _ => return Err("Unsupported Trash request".into()),
        };
        let object = input.as_object().ok_or("Unsupported Trash request")?;
        if object
            .keys()
            .any(|key| key != "action" && !fields.contains(&key.as_str()))
        {
            return Err("Unsupported Trash request".into());
        }
        if action == "trash-capabilities" {
            let available = input["location"] == "trash:";
            return Ok(json!({"available":available,"fallback":false,
                "scope":"Only recognized items already in personal Trash",
                "reason":if available {""} else {"Permanent deletion is unavailable for this location"}}));
        }
        let trash = controller(trash)?;
        return match action {
            "trash-status" => trash.status(),
            "trash-inspect" => trash.inspect(&input["operationId"]),
            "trash-prepare" => trash.prepare(
                &input["mode"],
                input.get("items").filter(|value| !value.is_null()),
            ),
            "trash-cancel" => trash.cancel(&input["token"]),
            "trash-stop" => trash.stop(),
            "trash-confirm" => {
                let busy = jobs.state()["jobs"].as_array().is_some_and(|items| {
                    items.iter().any(|job| {
                        ["running", "cancelling"].contains(&job["state"].as_str().unwrap_or(""))
                    })
                });
                trash.confirm(&input["token"], busy)
            }
            _ => unreachable!(),
        };
    }
    match input["action"].as_str() {
        Some("job-start") => {
            if trash
                .lock()
                .unwrap()
                .as_ref()
                .is_some_and(|trash| trash.busy())
            {
                return Err("Wait for the current Trash deletion or stop it first".into());
            }
            jobs.start(input.clone())
        }
        Some("job-cancel") => Ok(jobs.cancel(input["job"].as_str().ok_or("Missing job ID")?)),
        Some("job-status") => Ok(jobs.state()),
        _ => Err("Unknown file job request".into()),
    }
}

fn serve() -> JobResult<()> {
    unsafe {
        libc::umask(0o077);
        libc::signal(
            libc::SIGTERM,
            signal_stop as *const () as libc::sighandler_t,
        );
        libc::signal(libc::SIGINT, signal_stop as *const () as libc::sighandler_t);
    }
    let jobs = Arc::new(Jobs::new(Some(default_journal()?)));
    let trash = Mutex::new(None);
    let path = endpoint()?;
    if path.exists() {
        fs::remove_file(&path).map_err(|error| error.to_string())?;
    }
    let listener = UnixListener::bind(&path).map_err(|error| error.to_string())?;
    listener
        .set_nonblocking(true)
        .map_err(|error| error.to_string())?;
    while !STOP.load(Ordering::SeqCst) {
        match listener.accept() {
            Ok((mut conn, _)) => {
                conn.set_read_timeout(Some(Duration::from_secs(3)))
                    .map_err(|error| error.to_string())?;
                let result = read_line(&mut conn, 1024 * 1024)
                    .and_then(|line| {
                        serde_json::from_slice::<Value>(&line).map_err(|error| error.to_string())
                    })
                    .and_then(|input| request(&jobs, &trash, &input));
                let reply = match result {
                    Ok(value) => value,
                    Err(error) => json!({"error":error}),
                };
                let _ = writeln!(conn, "{reply}");
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(50));
            }
            Err(error) => return Err(error.to_string()),
        }
    }
    jobs.cancel_all();
    if let Some(trash) = trash.lock().unwrap().as_ref() {
        let _ = trash.stop();
        trash.close();
    }
    let until = Instant::now() + Duration::from_secs(115);
    while Instant::now() < until
        && jobs.state()["jobs"].as_array().is_some_and(|items| {
            items
                .iter()
                .any(|job| ["running", "cancelling"].contains(&job["state"].as_str().unwrap_or("")))
        })
    {
        thread::sleep(Duration::from_millis(50));
    }
    drop(listener);
    let _ = fs::remove_file(path);
    Ok(())
}

fn client() -> JobResult<Value> {
    let mut line = read_line(&mut std::io::stdin(), 1024 * 1024)?;
    line.push(b'\n');
    let mut conn = UnixStream::connect(endpoint()?).map_err(|error| error.to_string())?;
    conn.set_read_timeout(Some(Duration::from_secs(5)))
        .map_err(|error| error.to_string())?;
    conn.set_write_timeout(Some(Duration::from_secs(5)))
        .map_err(|error| error.to_string())?;
    conn.write_all(&line).map_err(|error| error.to_string())?;
    let line = read_line(&mut conn, 4 * 1024 * 1024)?;
    serde_json::from_slice(&line).map_err(|error| error.to_string())
}

fn trash_client_reply(line: &[u8]) -> Value {
    let parsed = serde_json::from_slice::<Value>(line);
    let id = parsed
        .as_ref()
        .ok()
        .and_then(|value| value.get("id"))
        .cloned()
        .unwrap_or(Value::Null);
    let result = (|| -> JobResult<Value> {
        let mut request = parsed.map_err(|error| error.to_string())?;
        let object = request
            .as_object_mut()
            .ok_or("Invalid Trash request identity")?;
        let number = object
            .get("id")
            .and_then(Value::as_u64)
            .ok_or("Invalid Trash request identity")?;
        if !(1..=9_007_199_254_740_991).contains(&number) {
            return Err("Invalid Trash request identity".into());
        }
        let action = object
            .get("action")
            .and_then(Value::as_str)
            .filter(|action| action.starts_with("trash-"))
            .ok_or("Unsupported Trash client request")?
            .to_owned();
        object.remove("id");
        let mut conn = UnixStream::connect(endpoint()?).map_err(|error| error.to_string())?;
        conn.set_read_timeout(Some(Duration::from_secs(5)))
            .map_err(|error| error.to_string())?;
        conn.set_write_timeout(Some(Duration::from_secs(5)))
            .map_err(|error| error.to_string())?;
        writeln!(conn, "{request}").map_err(|error| error.to_string())?;
        let response = read_line(&mut conn, 4 * 1024 * 1024)?;
        let mut value =
            serde_json::from_slice::<Value>(&response).map_err(|error| error.to_string())?;
        let reply = value
            .as_object_mut()
            .ok_or("Invalid Trash service response")?;
        if let Some(error) = reply.get("error").and_then(Value::as_str) {
            return Err(error.to_owned());
        }
        reply.insert("id".into(), json!(number));
        reply.insert("action".into(), json!(action));
        Ok(value)
    })();
    result.unwrap_or_else(
        |error| json!({"id":id,"error":error.chars().take(1024).collect::<String>()}),
    )
}

fn trash_client() -> JobResult<()> {
    let mut stdin = std::io::stdin().lock();
    loop {
        let line = match read_line(&mut stdin, 1024 * 1024 - 1) {
            Ok(line) => line,
            Err(error) if error == "File operation request is too large" => {
                println!(
                    "{}",
                    json!({"id":null,"error":"Trash request exceeds its declared limit"})
                );
                return Ok(());
            }
            Err(error) => return Err(error),
        };
        if line.is_empty() {
            return Ok(());
        }
        println!("{}", trash_client_reply(&line));
        std::io::stdout()
            .flush()
            .map_err(|error| error.to_string())?;
    }
}

fn main() {
    let mode = env::args().nth(1).unwrap_or_else(|| "request".into());
    let result = match mode.as_str() {
        "serve" => serve().map(|_| None),
        "request" => client().map(Some),
        "trash-client" => trash_client().map(|_| None),
        "trash-items" => trash_items().map(|items| Some(json!(items))),
        _ => Err("Unknown file job command".into()),
    };
    match result {
        Ok(Some(value)) => println!("{value}"),
        Ok(None) => (),
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    }
}
