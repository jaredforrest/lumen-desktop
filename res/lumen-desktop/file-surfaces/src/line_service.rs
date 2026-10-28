//! Bounded JSON-line bridge to existing Rust file-tag and rename services.
use serde_json::Value;
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

const MAX_RESPONSE: usize = 8 * 1024 * 1024;

struct Running {
    child: Child,
    input: ChildStdin,
    output: BufReader<ChildStdout>,
}

pub struct LineService {
    command: String,
    running: Option<Running>,
}

impl LineService {
    pub fn new(command: impl Into<String>) -> Self {
        Self {
            command: command.into(),
            running: None,
        }
    }

    fn start(&mut self) -> Result<(), String> {
        if self
            .running
            .as_mut()
            .is_some_and(|running| running.child.try_wait().ok().flatten().is_some())
        {
            self.stop();
        }
        if self.running.is_some() {
            return Ok(());
        }
        let mut child = Command::new(&self.command)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|_| "The file service is unavailable".to_owned())?;
        let input = child
            .stdin
            .take()
            .ok_or("The file service is unavailable")?;
        let output = BufReader::new(
            child
                .stdout
                .take()
                .ok_or("The file service is unavailable")?,
        );
        self.running = Some(Running {
            child,
            input,
            output,
        });
        Ok(())
    }

    fn response(output: &mut BufReader<ChildStdout>) -> Result<Value, String> {
        let mut bytes = Vec::new();
        loop {
            let available = output.fill_buf().map_err(|_| "The file service stopped")?;
            if available.is_empty() {
                return Err("The file service stopped".into());
            }
            let count = available
                .iter()
                .position(|byte| *byte == b'\n')
                .map_or(available.len(), |index| index + 1);
            if bytes.len() + count > MAX_RESPONSE {
                return Err("The file service response is too large".into());
            }
            let ended = available[count - 1] == b'\n';
            bytes.extend_from_slice(&available[..count]);
            output.consume(count);
            if ended {
                break;
            }
        }
        serde_json::from_slice(&bytes).map_err(|_| "The file service returned invalid JSON".into())
    }

    pub fn request(&mut self, value: &Value) -> Result<Value, String> {
        self.start()?;
        let running = self.running.as_mut().unwrap();
        let result = (|| {
            serde_json::to_writer(&mut running.input, value)
                .map_err(|_| "The file service stopped")?;
            running
                .input
                .write_all(b"\n")
                .and_then(|_| running.input.flush())
                .map_err(|_| "The file service stopped")?;
            Self::response(&mut running.output)
        })();
        if result.is_err() {
            self.stop();
        }
        result
    }

    pub fn tag(&mut self, value: &Value) -> Result<Value, String> {
        let result = self.request(value)?;
        if let Some(error) = result["error"].as_str() {
            if result.get("tags").is_none() && result.get("text").is_none() {
                return Err(error.to_owned());
            }
        }
        Ok(result)
    }

    fn stop(&mut self) {
        let Some(mut running) = self.running.take() else {
            return;
        };
        drop(running.input);
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            match running.child.try_wait() {
                Ok(Some(_)) | Err(_) => break,
                Ok(None) if Instant::now() >= deadline => {
                    let _ = running.child.kill();
                    let _ = running.child.wait();
                    break;
                }
                Ok(None) => thread::sleep(Duration::from_millis(10)),
            }
        }
    }
}

impl Drop for LineService {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn persistent_line_exchange_preserves_lossless_request() {
        let mut client = LineService::new("cat");
        for value in [
            json!({"action":"first","path":"/tmp/a #"}),
            json!({"action":"second","number":18446744073709551615_u64}),
        ] {
            assert_eq!(client.request(&value).unwrap(), value);
        }
    }
}
