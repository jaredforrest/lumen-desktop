use crate::{Result, fail};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{
    Arc, Condvar, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::thread::{self, JoinHandle};
use std::time::Duration;

const PAGE: &[u8] = include_bytes!("../page.html");
const DOWNLOAD_CHUNK: &[u8] = b"Local disposable download.\n";
pub fn payload() -> Vec<u8> {
    DOWNLOAD_CHUNK.repeat(8192 * 8)
}
pub fn digest() -> String {
    format!("{:x}", Sha256::digest(payload()))
}
#[derive(Clone)]
pub struct Gate(Arc<(Mutex<bool>, Condvar)>);
impl Gate {
    pub fn open(&self) {
        let (lock, signal) = &*self.0;
        *lock.lock().unwrap() = true;
        signal.notify_all();
    }
    fn wait(&self, stopping: &AtomicBool) {
        let (lock, signal) = &*self.0;
        let mut open = lock.lock().unwrap();
        while !*open && !stopping.load(Ordering::SeqCst) {
            open = signal
                .wait_timeout(open, Duration::from_millis(50))
                .unwrap()
                .0;
        }
    }
}
pub struct FixtureServer {
    origin: String,
    fixed: Gate,
    cancel: Gate,
    requests: Arc<Mutex<Vec<Value>>>,
    stopping: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}
impl FixtureServer {
    pub fn start() -> Result<Self> {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        listener.set_nonblocking(true)?;
        let port = listener.local_addr()?.port();
        let origin = format!("http://127.0.0.1:{port}");
        let fixed = Gate(Arc::new((Mutex::new(false), Condvar::new())));
        let cancel = Gate(Arc::new((Mutex::new(false), Condvar::new())));
        let requests = Arc::new(Mutex::new(Vec::new()));
        let stopping = Arc::new(AtomicBool::new(false));
        let server_requests = requests.clone();
        let server_stopping = stopping.clone();
        let server_fixed = fixed.clone();
        let server_cancel = cancel.clone();
        let thread = thread::spawn(move || {
            let mut clients = Vec::new();
            while !server_stopping.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        let req = server_requests.clone();
                        let stop = server_stopping.clone();
                        let fixed = server_fixed.clone();
                        let cancel = server_cancel.clone();
                        clients.push(thread::spawn(move || {
                            let _ = handle(stream, port, req, stop, fixed, cancel);
                        }));
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(10))
                    }
                    Err(_) => break,
                }
            }
            for client in clients {
                let _ = client.join();
            }
        });
        Ok(Self {
            origin,
            fixed,
            cancel,
            requests,
            stopping,
            thread: Some(thread),
        })
    }
    pub fn origin(&self) -> &str {
        &self.origin
    }
    pub fn gate(&self, name: &str) -> Result<Gate> {
        match name {
            "fixed" => Ok(self.fixed.clone()),
            "cancel" => Ok(self.cancel.clone()),
            _ => Err(fail("unknown download gate")),
        }
    }
    pub fn snapshot(&self) -> Vec<Value> {
        self.requests.lock().unwrap().clone()
    }
    pub fn close(&mut self) {
        self.stopping.store(true, Ordering::SeqCst);
        self.fixed.open();
        self.cancel.open();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
impl Drop for FixtureServer {
    fn drop(&mut self) {
        self.close();
    }
}
fn respond(
    mut stream: TcpStream,
    status: &str,
    content_type: &str,
    body: &[u8],
    attachment: Option<&str>,
) -> Result<()> {
    let mut headers = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n",
        body.len()
    );
    if let Some(name) = attachment {
        headers.push_str(&format!(
            "Content-Disposition: attachment; filename=\"fixture-{name}.bin\"\r\n"
        ));
    }
    headers.push_str("\r\n");
    stream.write_all(headers.as_bytes())?;
    stream.write_all(body)?;
    Ok(())
}
fn handle(
    mut stream: TcpStream,
    port: u16,
    requests: Arc<Mutex<Vec<Value>>>,
    stopping: Arc<AtomicBool>,
    fixed: Gate,
    cancel: Gate,
) -> Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(5)))?;
    let mut request = Vec::new();
    let mut chunk = [0; 1024];
    while !request.windows(4).any(|v| v == b"\r\n\r\n") && request.len() < 16384 {
        let n = stream.read(&mut chunk)?;
        if n == 0 {
            break;
        }
        request.extend_from_slice(&chunk[..n]);
    }
    let input = String::from_utf8_lossy(&request);
    let mut lines = input.split("\r\n");
    let first = lines.next().unwrap_or("");
    let mut parts = first.split_whitespace();
    let method = parts.next().unwrap_or("");
    let uri = parts.next().unwrap_or("");
    let host = lines
        .find_map(|line| {
            line.split_once(':')
                .filter(|(name, _)| name.eq_ignore_ascii_case("Host"))
                .map(|(_, value)| value.trim())
        })
        .unwrap_or("");
    if method != "GET" || host != format!("127.0.0.1:{port}") {
        return respond(
            stream,
            "400 Bad Request",
            "text/plain",
            b"Synthetic loopback origin required",
            None,
        );
    }
    let path = uri.split(['?', '#']).next().unwrap_or("");
    if path == "/" || path == "/page" {
        return respond(stream, "200 OK", "text/html; charset=utf-8", PAGE, None);
    }
    let name = match path {
        "/download/fixed" => "fixed",
        "/download/cancel" => "cancel",
        _ => return respond(stream, "404 Not Found", "text/plain", b"Not found", None),
    };
    let content = payload();
    let index = {
        let mut entries = requests.lock().unwrap();
        entries.push(json!({"name":name,"sent":0,"complete":false,"disconnected":false}));
        entries.len() - 1
    };
    let header = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Disposition: attachment; filename=\"fixture-{name}.bin\"\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        content.len()
    );
    let write_result = (|| -> Result<()> {
        stream.write_all(header.as_bytes())?;
        stream.write_all(&content[..65536])?;
        stream.flush()?;
        requests.lock().unwrap()[index]["sent"] = json!(65536);
        (if name == "fixed" { fixed } else { cancel }).wait(&stopping);
        if stopping.load(Ordering::SeqCst) {
            return Ok(());
        }
        for chunk in content[65536..].chunks(65536) {
            stream.write_all(chunk)?;
            stream.flush()?;
            let mut entries = requests.lock().unwrap();
            let sent = entries[index]["sent"].as_u64().unwrap_or(0) + chunk.len() as u64;
            entries[index]["sent"] = json!(sent);
            drop(entries);
            if name == "cancel" {
                thread::sleep(Duration::from_millis(10));
            }
        }
        requests.lock().unwrap()[index]["complete"] = json!(true);
        Ok(())
    })();
    if write_result.is_err() {
        requests.lock().unwrap()[index]["disconnected"] = json!(true);
    }
    write_result
}
