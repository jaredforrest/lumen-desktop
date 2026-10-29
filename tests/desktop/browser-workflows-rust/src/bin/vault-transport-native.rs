//! Actual stalled TLS and body shutdown against a private loopback backend.
use lumen_browser_workflows_fixture::vault_https::{PrivateHttps, client_config, connect};
use lumen_browser_workflows_fixture::{Result, fail};
use serde_json::json;
use std::env;
use std::fs;
use std::io::{Read, Write};
use std::net::{Ipv4Addr, TcpListener, TcpStream};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};

struct Options {
    tls_tool: PathBuf,
    output: PathBuf,
    inside: bool,
}
fn arguments() -> Result<Options> {
    let mut tls_tool = None;
    let mut output = None;
    let mut inside = false;
    let mut args = env::args_os().skip(1);
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("--tls-tool") => {
                tls_tool = Some(PathBuf::from(args.next().ok_or("Missing TLS tool")?))
            }
            Some("--output") => output = Some(PathBuf::from(args.next().ok_or("Missing output")?)),
            Some("--inside") => inside = true,
            _ => return Err(fail("Unknown vault transport option")),
        }
    }
    let output = output.ok_or("Missing output")?;
    Ok(Options {
        tls_tool: tls_tool.ok_or("Missing TLS tool")?,
        output: if output.is_absolute() {
            output
        } else {
            env::current_dir()?.join(output)
        },
        inside,
    })
}
fn outer(mut o: Options) -> Result<()> {
    o.tls_tool = o.tls_tool.canonicalize()?;
    if !o.tls_tool.starts_with("/nix/store") {
        return Err(fail("Pinned TLS tool required"));
    }
    fs::create_dir(&o.output)?;
    fs::set_permissions(&o.output, fs::Permissions::from_mode(0o700))?;
    let source = env::current_dir()?.canonicalize()?;
    let exe = env::current_exe()?;
    let executable = if exe.starts_with(&source) {
        Path::new("/source").join(exe.strip_prefix(&source)?)
    } else {
        exe
    };
    let status = Command::new("lumen-application-isolation")
        .arg("run")
        .arg("--source")
        .arg(source)
        .arg("--output")
        .arg(&o.output)
        .arg("--")
        .arg(executable)
        .arg("--inside")
        .arg("--tls-tool")
        .arg(o.tls_tool)
        .arg("--output")
        .arg("/artifacts")
        .status()?;
    if !status.success() {
        return Err(fail(format!("Vault transport failed: {status}")));
    }
    Ok(())
}
fn certificate(o: &Options) -> Result<(PathBuf, PathBuf)> {
    let cert = PathBuf::from("/home/fixture/certificate.pem");
    let key = PathBuf::from("/home/fixture/key.pem");
    let output = Command::new(&o.tls_tool)
        .args([
            "req",
            "-x509",
            "-newkey",
            "rsa:2048",
            "-nodes",
            "-days",
            "1",
            "-subj",
            "/CN=Disposable transport",
            "-addext",
            "subjectAltName=IP:127.0.0.1",
            "-addext",
            "basicConstraints=CA:FALSE",
            "-keyout",
        ])
        .arg(&key)
        .arg("-out")
        .arg(&cert)
        .output()?;
    if !output.status.success() {
        return Err(fail("Disposable certificate generation failed"));
    }
    Ok((cert, key))
}
fn backend(stop: Arc<AtomicBool>) -> Result<thread::JoinHandle<()>> {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 8238))?;
    listener.set_nonblocking(true)?;
    Ok(thread::spawn(move || {
        while !stop.load(Ordering::SeqCst) {
            match listener.accept() {
                Ok((mut socket, _)) => {
                    let _ = socket.set_read_timeout(Some(Duration::from_secs(2)));
                    let mut request: Vec<u8> = Vec::new();
                    while !request.windows(4).any(|part| part == b"\r\n\r\n")
                        && request.len() < 65536
                    {
                        let mut chunk = [0; 4096];
                        match socket.read(&mut chunk) {
                            Ok(0) | Err(_) => break,
                            Ok(n) => request.extend(&chunk[..n]),
                        }
                    }
                    let _ = socket.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 19\r\nConnection: close\r\n\r\n{\"disposable\":true}");
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(10))
                }
                Err(_) => break,
            }
        }
    }))
}
fn verified_request(cert: &Path) -> Result<String> {
    let mut client = connect(client_config(cert)?)?;
    client.write_all(b"GET /positive HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n")?;
    let mut text = String::new();
    client.read_to_string(&mut text)?;
    if !text.contains("Strict-Transport-Security: max-age=31536000")
        || !text.contains("{\"disposable\":true}")
    {
        return Err(fail("Verified HTTPS or backend response missing"));
    }
    Ok(text)
}
fn inside(o: Options) -> Result<()> {
    if env::var("HOME")? != "/home/fixture"
        || Path::new("/.old-root").exists()
        || unsafe { libc::getuid() } == 0
    {
        return Err(fail("Private transport boundary missing"));
    }
    let (cert, key) = certificate(&o)?;
    let stop = Arc::new(AtomicBool::new(false));
    let backend = backend(Arc::clone(&stop))?;
    let mut records = Vec::new();
    let result = (|| -> Result<()> {
        let mut https = PrivateHttps::start(&cert, &key)?;
        let stalled = TcpStream::connect((Ipv4Addr::LOCALHOST, 8237))?;
        let _ = verified_request(&cert)?;
        let started = Instant::now();
        https.close()?;
        if started.elapsed() >= Duration::from_secs(4) || https.worker_count() != 0 {
            return Err(fail("Stalled TLS shutdown exceeded deadline"));
        }
        drop(stalled);
        records.push(
            json!({"case":"stalled TLS handshake","seconds":started.elapsed().as_secs_f64()}),
        );
        let mut https = PrivateHttps::start(&cert, &key)?;
        let mut client = connect(client_config(&cert)?)?;
        client.write_all(b"POST /body HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Length: 64\r\nConnection: close\r\n\r\nx")?;
        let started = Instant::now();
        let mut byte = [0];
        match client.read(&mut byte) {
            Ok(0) => {}
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::UnexpectedEof
                        | std::io::ErrorKind::ConnectionReset
                        | std::io::ErrorKind::BrokenPipe
                ) => {}
            _ => return Err(fail("Incomplete body remained open or returned a response")),
        }
        if started.elapsed() >= Duration::from_secs(3) {
            return Err(fail("Incomplete body did not expire"));
        }
        https.close()?;
        records.push(
            json!({"case":"incomplete body expires","seconds":started.elapsed().as_secs_f64()}),
        );
        let mut https = PrivateHttps::start(&cert, &key)?;
        let mut client = connect(client_config(&cert)?)?;
        client.write_all(b"POST /body HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Length: 64\r\nConnection: close\r\n\r\nx")?;
        let started = Instant::now();
        https.close()?;
        if started.elapsed() >= Duration::from_secs(4)
            || https.worker_count() != 0
            || https.connection_count() != 0
        {
            return Err(fail("Incomplete body shutdown exceeded deadline"));
        }
        records.push(
            json!({"case":"incomplete body interrupted","seconds":started.elapsed().as_secs_f64()}),
        );
        Ok(())
    })();
    stop.store(true, Ordering::SeqCst);
    backend
        .join()
        .map_err(|_| fail("Backend worker panicked"))?;
    result?;
    fs::write(
        o.output.join("result.json"),
        format!(
            "{}\n",
            serde_json::to_string_pretty(
                &json!({"passed":true,"receipts":records,"scope":"Private loopback backend and verified Rust TLS front end"})
            )?
        ),
    )?;
    Ok(())
}
fn main() {
    let result = arguments().and_then(|o| if o.inside { inside(o) } else { outer(o) });
    if let Err(error) = result {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
