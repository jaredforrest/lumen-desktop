//! Bounded JSON-RPC standard-input client for an owned installed control bridge.
use crate::{Result, ensure, error};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::fs::File;
use std::io::{Read, Write};
use std::os::fd::AsRawFd;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

const METADATA_BUDGET: usize = 8 * 1024 * 1024;
const FRAME_BUDGET: usize = 2048;

pub struct Control {
    process: Child,
    buffer: Vec<u8>,
    pub serial: i64,
    pub frames: Vec<Value>,
}
impl Control {
    pub fn start(
        command: &[String],
        log: &Path,
        environment: Option<&BTreeMap<String, String>>,
    ) -> Result<Self> {
        let (program, args) = command.split_first().ok_or("Empty control command")?;
        let file = File::create(log)?;
        let mut child = Command::new(program);
        child
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(file);
        if let Some(environment) = environment {
            child.env_clear().envs(environment);
        }
        Ok(Self {
            process: child.spawn()?,
            buffer: Vec::new(),
            serial: 0,
            frames: Vec::new(),
        })
    }
    fn send(&mut self, request: &Value) -> Result<()> {
        let pipe = self.process.stdin.as_mut().ok_or("Control input closed")?;
        pipe.write_all(serde_json::to_string(request)?.as_bytes())?;
        pipe.write_all(b"\n")?;
        pipe.flush()?;
        Ok(())
    }
    fn accept_frame(&mut self, line: &[u8], method: &str) -> Result<Option<Value>> {
        let reply: Value = serde_json::from_slice(line)
            .map_err(|_| error("Control bridge emitted malformed JSON"))?;
        ensure(
            reply.is_object() && reply["jsonrpc"] == "2.0",
            "Control bridge emitted an invalid envelope",
        )?;
        self.frames.push(reply.clone());
        ensure(
            self.frames.len() <= FRAME_BUDGET,
            "Control metadata frame budget exceeded",
        )?;
        if reply["id"].as_i64() == Some(self.serial) {
            ensure(
                reply.get("error").is_none(),
                format!("Control request failed: {method}"),
            )?;
            return Ok(Some(reply.get("result").cloned().unwrap_or(Value::Null)));
        }
        Ok(None)
    }
    pub fn request(
        &mut self,
        method: &str,
        params: Option<Value>,
        timeout: Duration,
    ) -> Result<Value> {
        self.serial += 1;
        self.send(&json!({"jsonrpc":"2.0","id":self.serial,"method":method,"params":params.unwrap_or_else(||json!({}))}))?;
        let deadline = Instant::now() + timeout;
        let mut received = 0usize;
        loop {
            while let Some(index) = self.buffer.iter().position(|byte| *byte == b'\n') {
                let line = self.buffer.drain(..=index).collect::<Vec<_>>();
                if let Some(result) = self.accept_frame(&line[..line.len() - 1], method)? {
                    return Ok(result);
                }
            }
            if Instant::now() >= deadline {
                break;
            }
            let remaining = deadline
                .saturating_duration_since(Instant::now())
                .as_millis()
                .min(i32::MAX as u128) as i32;
            let fd = self
                .process
                .stdout
                .as_ref()
                .ok_or("Control output closed")?
                .as_raw_fd();
            let mut pollfd = libc::pollfd {
                fd,
                events: libc::POLLIN,
                revents: 0,
            };
            let ready = unsafe { libc::poll(&mut pollfd, 1, remaining) };
            if ready < 0 {
                return Err(std::io::Error::last_os_error().into());
            }
            if ready == 0 {
                break;
            }
            let mut chunk = [0u8; 65536];
            let size = self
                .process
                .stdout
                .as_mut()
                .ok_or("Control output closed")?
                .read(&mut chunk)?;
            ensure(
                size > 0,
                format!("Owned control bridge exited before acknowledging {method}"),
            )?;
            received += size;
            ensure(
                received <= METADATA_BUDGET,
                "Control request metadata budget exceeded",
            )?;
            self.buffer.extend_from_slice(&chunk[..size]);
            ensure(
                self.buffer.len() <= METADATA_BUDGET,
                "Control response exceeded fixture metadata budget",
            )?;
        }
        Err(error(format!("Owned control request timed out: {method}")))
    }
    pub fn initialize(&mut self) -> Result<Value> {
        let result=self.request("initialize",Some(json!({"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"Fixture","version":"1"}})),Duration::from_secs(10))?;
        self.send(&json!({"jsonrpc":"2.0","method":"notifications/initialized"}))?;
        Ok(result)
    }
    pub fn tool(&mut self, name: &str, arguments: Option<Value>) -> Result<Value> {
        let result = self.request(
            "tools/call",
            Some(json!({"name":name,"arguments":arguments.unwrap_or_else(||json!({}))})),
            Duration::from_secs(10),
        )?;
        ensure(
            result.is_object() && result["isError"] != true,
            format!("Owned application tool failed: {name}"),
        )?;
        let text = result["content"]
            .as_array()
            .map(|parts| {
                parts
                    .iter()
                    .filter(|part| part["type"] == "text")
                    .filter_map(|part| part["text"].as_str())
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .unwrap_or_default();
        Ok(serde_json::from_str(&text).unwrap_or_else(|_| Value::String(text)))
    }
    pub fn command(&mut self, app: &str, name: &str, params: Option<Value>) -> Result<Value> {
        let tool = match app {
            "writer" => "execute",
            "sheet" => "execute_command",
            "slides" => "run_command",
            _ => return Err(error("Unknown office provider")),
        };
        self.tool(
            tool,
            Some(json!({"command":name,"params":params.unwrap_or_else(||json!({}))})),
        )
    }
    pub fn close(&mut self) -> Result<()> {
        if self.process.try_wait()?.is_none() {
            self.process.kill()?;
        }
        self.process.wait()?;
        self.process.stdin.take();
        self.process.stdout.take();
        Ok(())
    }
}
impl Drop for Control {
    fn drop(&mut self) {
        let _ = self.close();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unrelated_frames_cannot_ack_request() {
        let root = tempfile::tempdir().unwrap();
        let mut control = Control::start(
            &[
                std::env::current_exe().unwrap().display().to_string(),
                "--help".into(),
            ],
            &root.path().join("stderr"),
            None,
        )
        .unwrap();
        control.serial = 1;
        assert_eq!(
            control
                .accept_frame(br#"{"jsonrpc":"2.0","method":"progress"}"#, "one")
                .unwrap(),
            None
        );
        assert_eq!(
            control
                .accept_frame(br#"{"jsonrpc":"2.0","id":11,"result":"unrelated"}"#, "one")
                .unwrap(),
            None
        );
        assert_eq!(
            control
                .accept_frame(
                    br#"{"jsonrpc":"2.0","id":1,"result":{"answer":"one"}}"#,
                    "one"
                )
                .unwrap(),
            Some(json!({"answer":"one"}))
        );
        assert_eq!(control.frames.len(), 3);
    }
    #[test]
    fn error_reply_fails_without_retry() {
        let root = tempfile::tempdir().unwrap();
        let mut control = Control::start(
            &[
                std::env::current_exe().unwrap().display().to_string(),
                "--help".into(),
            ],
            &root.path().join("stderr"),
            None,
        )
        .unwrap();
        control.serial = 1;
        assert!(
            control
                .accept_frame(
                    br#"{"jsonrpc":"2.0","id":1,"error":{"code":-1}}"#,
                    "mutation"
                )
                .is_err()
        );
        assert_eq!(control.serial, 1);
    }
}
