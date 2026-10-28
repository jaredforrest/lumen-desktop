//! Bounded NUL framed CDP diagnostics over inherited process pipes.
use crate::{Result, fail};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::fs::File;
use std::io::{Read, Write};
use std::os::fd::{AsRawFd, FromRawFd};
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

pub const MAX_FRAME_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_QUEUED_EVENTS: usize = 2000;
pub const MAX_EVENT_BYTES: usize = 16 * 1024 * 1024;

fn pipe() -> Result<(File, File)> {
    let mut fds = [0; 2];
    if unsafe { libc::pipe2(fds.as_mut_ptr(), libc::O_CLOEXEC) } < 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    Ok((unsafe { File::from_raw_fd(fds[0]) }, unsafe {
        File::from_raw_fd(fds[1])
    }))
}
fn nonblocking(file: &File) -> Result<()> {
    let flags = unsafe { libc::fcntl(file.as_raw_fd(), libc::F_GETFL) };
    if flags < 0
        || unsafe { libc::fcntl(file.as_raw_fd(), libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0
    {
        return Err(std::io::Error::last_os_error().into());
    }
    Ok(())
}
fn ready(fd: i32, flags: i16, deadline: Instant) -> Result<()> {
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(fail("Diagnostic pipe deadline reached"));
        }
        let mut poll = libc::pollfd {
            fd,
            events: flags,
            revents: 0,
        };
        let result = unsafe {
            libc::poll(
                &mut poll,
                1,
                remaining.as_millis().min(i32::MAX as u128) as i32,
            )
        };
        if result > 0 {
            return Ok(());
        }
        if result == 0 {
            return Err(fail("Diagnostic pipe deadline reached"));
        }
        let error = std::io::Error::last_os_error();
        if error.kind() != std::io::ErrorKind::Interrupted {
            return Err(error.into());
        }
    }
}
pub struct Frames {
    read: File,
    write: File,
    sequence: u64,
    buffer: Vec<u8>,
    pub events: Vec<Value>,
    event_bytes: usize,
}
impl Frames {
    pub fn new(read: File, write: File) -> Self {
        Self {
            read,
            write,
            sequence: 0,
            buffer: Vec::new(),
            events: Vec::new(),
            event_bytes: 0,
        }
    }
    fn send(&mut self, value: &Value, deadline: Instant) -> Result<()> {
        let mut payload = serde_json::to_vec(value)?;
        payload.push(0);
        if payload.len() > MAX_FRAME_BYTES {
            return Err(fail("Diagnostic request exceeds frame budget"));
        }
        while !payload.is_empty() {
            ready(self.write.as_raw_fd(), libc::POLLOUT, deadline)?;
            match self.write.write(&payload) {
                Ok(0) => return Err(fail("Diagnostic command pipe closed")),
                Ok(n) => {
                    payload.drain(..n);
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
                Err(e) => return Err(e.into()),
            }
        }
        Ok(())
    }
    fn receive(&mut self, deadline: Instant) -> Result<Value> {
        loop {
            if let Some(i) = self.buffer.iter().position(|v| *v == 0) {
                let frame: Vec<_> = self.buffer.drain(..=i).collect();
                let value: Value = serde_json::from_slice(&frame[..frame.len() - 1])?;
                if !value.is_object() {
                    return Err(fail("Diagnostic response is not an object"));
                }
                return Ok(value);
            }
            ready(self.read.as_raw_fd(), libc::POLLIN, deadline)?;
            let mut chunk = [0; 65536];
            match self.read.read(&mut chunk) {
                Ok(0) => return Err(fail("Application diagnostic pipe closed")),
                Ok(n) => self.buffer.extend_from_slice(&chunk[..n]),
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
                Err(e) => return Err(e.into()),
            };
            if self.buffer.len() > MAX_FRAME_BYTES {
                return Err(fail("Diagnostic response exceeds frame budget"));
            }
        }
    }
    pub fn request(
        &mut self,
        method: &str,
        params: Value,
        session: Option<&str>,
        timeout: Duration,
    ) -> Result<Value> {
        self.sequence += 1;
        let id = self.sequence;
        let deadline = Instant::now() + timeout;
        let mut value = json!({"id":id,"method":method,"params":params});
        if let Some(session) = session {
            value["sessionId"] = json!(session);
        }
        self.send(&value, deadline)?;
        loop {
            let response = self.receive(deadline)?;
            if response.get("id").is_none() {
                self.event_bytes += serde_json::to_vec(&response)?.len();
                if self.events.len() >= MAX_QUEUED_EVENTS || self.event_bytes > MAX_EVENT_BYTES {
                    return Err(fail("Diagnostic event queue exceeds its budget"));
                }
                self.events.push(response);
                continue;
            }
            if response["id"] != id {
                return Err(fail("Unexpected diagnostic response identity"));
            }
            if !response["error"].is_null() {
                return Err(fail(response["error"].to_string()));
            }
            return Ok(response.get("result").cloned().unwrap_or(json!({})));
        }
    }
}
pub struct PipeTransport {
    pub frames: Frames,
    pub process: Child,
}
impl PipeTransport {
    pub fn start(
        command: &[String],
        environment: &BTreeMap<String, String>,
        log: &Path,
    ) -> Result<Self> {
        if command.is_empty() {
            return Err(fail("Missing browser command"));
        }
        let (incoming, write) = pipe()?;
        let (read, outgoing) = pipe()?;
        let child_input = incoming.as_raw_fd();
        let child_output = outgoing.as_raw_fd();
        let logfile = File::create(log)?;
        let mut cmd = Command::new("lumen-browser-pipe-child");
        cmd.arg(child_input.to_string())
            .arg(child_output.to_string())
            .args(command)
            .env_clear()
            .envs(environment)
            .stdout(Stdio::from(logfile.try_clone()?))
            .stderr(Stdio::from(logfile));
        unsafe {
            cmd.pre_exec(move || {
                for fd in [child_input, child_output] {
                    if libc::fcntl(fd, libc::F_SETFD, 0) < 0 {
                        return Err(std::io::Error::last_os_error());
                    }
                }
                libc::setsid();
                Ok(())
            });
        }
        let process = cmd.spawn()?;
        drop(incoming);
        drop(outgoing);
        nonblocking(&read)?;
        nonblocking(&write)?;
        Ok(Self {
            frames: Frames::new(read, write),
            process,
        })
    }
    pub fn request(
        &mut self,
        method: &str,
        params: Value,
        session: Option<&str>,
        timeout: Duration,
    ) -> Result<Value> {
        self.frames.request(method, params, session, timeout)
    }
    pub fn exited(&mut self) -> Result<bool> {
        Ok(self.process.try_wait()?.is_some())
    }
    pub fn close(mut self) -> Result<()> {
        drop(self.frames);
        if self.process.try_wait()?.is_none() {
            let end = Instant::now() + Duration::from_secs(3);
            while self.process.try_wait()?.is_none() && Instant::now() < end {
                std::thread::sleep(Duration::from_millis(20));
            }
            if self.process.try_wait()?.is_none() {
                unsafe {
                    libc::killpg(self.process.id() as i32, libc::SIGTERM);
                }
                let term_end = Instant::now() + Duration::from_secs(3);
                while self.process.try_wait()?.is_none() && Instant::now() < term_end {
                    std::thread::sleep(Duration::from_millis(20));
                }
                if self.process.try_wait()?.is_none() {
                    unsafe {
                        libc::killpg(self.process.id() as i32, libc::SIGKILL);
                    }
                }
            }
        }
        let _ = self.process.wait();
        Ok(())
    }
}
