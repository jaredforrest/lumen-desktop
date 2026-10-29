//! Bounded loopback HTTPS front end for a disposable compatibility server.
use crate::{Result, fail};
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use rustls::{
    ClientConfig, ClientConnection, RootCertStore, ServerConfig, ServerConnection, StreamOwned,
};
use std::fs::File;
use std::io::{BufReader, Read, Write};
use std::net::{IpAddr, Ipv4Addr, Shutdown, SocketAddr, TcpListener, TcpStream};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

const MAX_BODY: usize = 4 * 1024 * 1024;
const MAX_RESPONSE: usize = 8 * 1024 * 1024;
const MAX_HEADERS: usize = 64 * 1024;
const READ_TIMEOUT: Duration = Duration::from_secs(2);
const CLOSE_TIMEOUT: Duration = Duration::from_secs(3);

fn certificate(path: &Path) -> Result<Vec<CertificateDer<'static>>> {
    Ok(
        rustls_pemfile::certs(&mut BufReader::new(File::open(path)?))
            .collect::<std::io::Result<Vec<_>>>()?,
    )
}
fn key(path: &Path) -> Result<PrivateKeyDer<'static>> {
    rustls_pemfile::private_key(&mut BufReader::new(File::open(path)?))?
        .ok_or_else(|| fail("Disposable TLS key missing"))
}
fn config(cert: &Path, key_path: &Path) -> Result<Arc<ServerConfig>> {
    let provider = rustls::crypto::ring::default_provider();
    let config = ServerConfig::builder_with_provider(provider.into())
        .with_safe_default_protocol_versions()?
        .with_no_client_auth()
        .with_single_cert(certificate(cert)?, key(key_path)?)?;
    Ok(Arc::new(config))
}
pub fn client_config(cert: &Path) -> Result<Arc<ClientConfig>> {
    let mut roots = RootCertStore::empty();
    for cert in certificate(cert)? {
        roots.add(cert)?;
    }
    let provider = rustls::crypto::ring::default_provider();
    Ok(Arc::new(
        ClientConfig::builder_with_provider(provider.into())
            .with_safe_default_protocol_versions()?
            .with_root_certificates(roots)
            .with_no_client_auth(),
    ))
}
pub fn connect(config: Arc<ClientConfig>) -> Result<StreamOwned<ClientConnection, TcpStream>> {
    let socket = TcpStream::connect_timeout(
        &SocketAddr::from(([127, 0, 0, 1], 8237)),
        Duration::from_secs(2),
    )?;
    socket.set_read_timeout(Some(Duration::from_secs(5)))?;
    socket.set_write_timeout(Some(Duration::from_secs(5)))?;
    let name = rustls::pki_types::ServerName::IpAddress(IpAddr::V4(Ipv4Addr::LOCALHOST).into());
    Ok(StreamOwned::new(
        ClientConnection::new(config, name)?,
        socket,
    ))
}
fn read_request(stream: &mut StreamOwned<ServerConnection, TcpStream>) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    let end = loop {
        if let Some(index) = bytes.windows(4).position(|part| part == b"\r\n\r\n") {
            break index + 4;
        }
        if bytes.len() >= MAX_HEADERS {
            return Err(fail("Disposable HTTP header budget exceeded"));
        }
        let mut chunk = [0; 4096];
        let n = stream.read(&mut chunk)?;
        if n == 0 {
            return Err(fail("Disposable HTTP peer closed"));
        }
        bytes.extend_from_slice(&chunk[..n]);
    };
    let headers = std::str::from_utf8(&bytes[..end])?;
    let mut lines = headers.split("\r\n");
    let request = lines.next().ok_or("Request line missing")?;
    let parts = request.split_whitespace().collect::<Vec<_>>();
    if parts.len() != 3
        || !parts[1].starts_with('/')
        || parts[1].starts_with("//")
        || !parts[2].starts_with("HTTP/1.")
    {
        return Err(fail("Invalid disposable HTTP request"));
    }
    let mut length = 0;
    let mut has_length = false;
    let mut forwarded = format!("{request}\r\n");
    for line in lines {
        if line.is_empty() {
            continue;
        }
        let (name, value) = line.split_once(':').ok_or("Malformed disposable header")?;
        let name_lower = name.trim().to_ascii_lowercase();
        if name_lower == "transfer-encoding" {
            return Err(fail("Chunked fixture request refused"));
        }
        if name_lower == "content-length" {
            if has_length {
                return Err(fail("Duplicate request length"));
            }
            length = value.trim().parse::<usize>()?;
            has_length = true;
            if length > MAX_BODY {
                return Err(fail("Disposable request body budget exceeded"));
            }
        }
        if ![
            "connection",
            "keep-alive",
            "proxy-authenticate",
            "proxy-authorization",
            "te",
            "trailer",
            "upgrade",
        ]
        .contains(&name_lower.as_str())
        {
            forwarded.push_str(line);
            forwarded.push_str("\r\n");
        }
    }
    forwarded.push_str("Connection: close\r\n\r\n");
    let mut request = forwarded.into_bytes();
    let mut body = bytes[end..].to_vec();
    if body.len() > length {
        body.truncate(length);
    }
    while body.len() < length {
        let mut chunk = [0; 8192];
        let remaining = 8192_usize.min(length - body.len());
        let n = stream.read(&mut chunk[..remaining])?;
        if n == 0 {
            return Err(fail("Incomplete disposable HTTP request body"));
        }
        body.extend_from_slice(&chunk[..n]);
    }
    request.extend(body);
    Ok(request)
}
fn forward(request: &[u8]) -> Result<Vec<u8>> {
    let mut backend = TcpStream::connect_timeout(
        &SocketAddr::from(([127, 0, 0, 1], 8238)),
        Duration::from_secs(2),
    )?;
    backend.set_read_timeout(Some(Duration::from_secs(10)))?;
    backend.set_write_timeout(Some(Duration::from_secs(5)))?;
    backend.write_all(request)?;
    let mut response = Vec::new();
    loop {
        let mut chunk = [0; 8192];
        let n = backend.read(&mut chunk)?;
        if n == 0 {
            break;
        }
        response.extend_from_slice(&chunk[..n]);
        if response.len() > MAX_RESPONSE {
            return Err(fail("Disposable response budget exceeded"));
        }
    }
    let status_end = response
        .windows(2)
        .position(|part| part == b"\r\n")
        .ok_or("Backend status missing")?
        + 2;
    let mut public = Vec::with_capacity(response.len() + 96);
    public.extend_from_slice(&response[..status_end]);
    public
        .extend_from_slice(b"Strict-Transport-Security: max-age=31536000\r\nConnection: close\r\n");
    public.extend_from_slice(&response[status_end..]);
    Ok(public)
}
fn worker(socket: TcpStream, config: Arc<ServerConfig>) -> Result<()> {
    socket.set_read_timeout(Some(READ_TIMEOUT))?;
    socket.set_write_timeout(Some(Duration::from_secs(5)))?;
    let mut stream = StreamOwned::new(ServerConnection::new(config)?, socket);
    let request = read_request(&mut stream)?;
    let response = match forward(&request) {
        Ok(bytes) => bytes,
        Err(_) => {
            b"HTTP/1.1 502 Bad Gateway\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec()
        }
    };
    stream.write_all(&response)?;
    stream.flush()?;
    stream.conn.send_close_notify();
    stream.flush()?;
    Ok(())
}
pub struct PrivateHttps {
    stop: Arc<AtomicBool>,
    listener: Option<JoinHandle<()>>,
    workers: Arc<Mutex<Vec<JoinHandle<()>>>>,
    connections: Arc<Mutex<Vec<TcpStream>>>,
}
impl PrivateHttps {
    pub fn start(cert: &Path, key_path: &Path) -> Result<Self> {
        let config = config(cert, key_path)?;
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 8237))?;
        listener.set_nonblocking(true)?;
        let stop = Arc::new(AtomicBool::new(false));
        let workers = Arc::new(Mutex::new(Vec::new()));
        let connections = Arc::new(Mutex::new(Vec::new()));
        let listener_handle = {
            let stop = Arc::clone(&stop);
            let workers = Arc::clone(&workers);
            let connections = Arc::clone(&connections);
            thread::spawn(move || {
                while !stop.load(Ordering::SeqCst) {
                    match listener.accept() {
                        Ok((socket, _)) => {
                            if stop.load(Ordering::SeqCst) {
                                break;
                            }
                            let Ok(copy) = socket.try_clone() else {
                                continue;
                            };
                            connections.lock().unwrap().push(copy);
                            let config = Arc::clone(&config);
                            let handle = thread::spawn(move || {
                                let closing = socket.try_clone().ok();
                                let _ = worker(socket, config);
                                if let Some(socket) = closing {
                                    let _ = socket.shutdown(Shutdown::Both);
                                }
                            });
                            workers.lock().unwrap().push(handle);
                        }
                        Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                            thread::sleep(Duration::from_millis(10))
                        }
                        Err(_) => break,
                    }
                }
            })
        };
        Ok(Self {
            stop,
            listener: Some(listener_handle),
            workers,
            connections,
        })
    }
    pub fn close(&mut self) -> Result<()> {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(handle) = self.listener.take() {
            handle.join().map_err(|_| fail("HTTPS listener panicked"))?;
        }
        for socket in self.connections.lock().unwrap().drain(..) {
            let _ = socket.shutdown(Shutdown::Both);
        }
        let deadline = Instant::now() + CLOSE_TIMEOUT;
        while self
            .workers
            .lock()
            .unwrap()
            .iter()
            .any(|worker| !worker.is_finished())
        {
            if Instant::now() >= deadline {
                return Err(fail("Disposable HTTPS workers did not stop"));
            }
            thread::sleep(Duration::from_millis(10));
        }
        for worker in self.workers.lock().unwrap().drain(..) {
            worker.join().map_err(|_| fail("HTTPS worker panicked"))?;
        }
        Ok(())
    }
    pub fn worker_count(&self) -> usize {
        self.workers.lock().unwrap().len()
    }
    pub fn connection_count(&self) -> usize {
        self.connections.lock().unwrap().len()
    }
}
