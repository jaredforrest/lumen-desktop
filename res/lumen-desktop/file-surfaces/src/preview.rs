//! Owned preview session shared by Files, Search, and Quick Look.
use crate::captured::{Captured, Facts, Unavailable};
use crate::line_service::LineService;
use crate::requests::PreviewRequests;
use crate::snapshots::Snapshots;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::fs::{self, Metadata};
use std::io::{Read, Write};
use std::os::fd::AsRawFd;
use std::os::unix::fs::MetadataExt;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

pub type Identity = Box<dyn Fn(&Path, &Metadata) -> Result<String, String> + Send + Sync>;
pub type Validate = Box<dyn Fn(&Path, &str) -> Result<(), String> + Send + Sync>;
pub type Resources = Snapshots<Identity, Validate>;

fn unavailable(reason: &'static str, message: &'static str) -> Unavailable {
    Unavailable { reason, message }
}

pub fn token(generation: &str, path: &Path, info: &Metadata) -> Result<String, String> {
    let path = serde_json::to_string(&path.to_string_lossy().as_ref())
        .map_err(|error| error.to_string())?;
    let facts = format!(
        "[{path}, {}, {}, {}, {}, {}]",
        info.dev(),
        info.ino(),
        info.ctime() * 1_000_000_000 + info.ctime_nsec(),
        info.mtime() * 1_000_000_000 + info.mtime_nsec(),
        info.len()
    );
    Ok(format!(
        "{generation}:{}",
        hex::encode(Sha256::digest(facts.as_bytes()))
    ))
}

pub fn file_uri(path: &Path) -> String {
    use std::os::unix::ffi::OsStrExt;
    let mut uri = String::from("file://");
    for byte in path.as_os_str().as_bytes() {
        if byte.is_ascii_alphanumeric() || b"/-._~".contains(byte) {
            uri.push(*byte as char);
        } else {
            uri.push_str(&format!("%{byte:02X}"));
        }
    }
    uri
}

pub fn local_uri(value: &str) -> Result<PathBuf, String> {
    use std::os::unix::ffi::OsStringExt;
    let encoded = value
        .strip_prefix("file:///")
        .ok_or("Unsupported preview resource")?;
    let encoded = format!("/{encoded}");
    let mut bytes = Vec::with_capacity(encoded.len());
    let mut input = encoded.as_bytes().iter().copied();
    while let Some(byte) = input.next() {
        if byte == b'%' {
            let high = input.next().and_then(|part| (part as char).to_digit(16));
            let low = input.next().and_then(|part| (part as char).to_digit(16));
            let (Some(high), Some(low)) = (high, low) else {
                return Err("Unsupported preview resource".into());
            };
            bytes.push(((high << 4) | low) as u8);
        } else {
            bytes.push(byte);
        }
    }
    if bytes.contains(&0) {
        return Err("Unsupported preview resource".into());
    }
    Ok(PathBuf::from(std::ffi::OsString::from_vec(bytes)))
}

pub struct PreviewSession {
    pub generation: String,
    pub actions: Captured,
    pub resources: Arc<Resources>,
    pub requests: PreviewRequests<Identity, Validate>,
    tags: Mutex<LineService>,
}

impl PreviewSession {
    pub fn new() -> Result<Self, Unavailable> {
        let mut random = [0_u8; 16];
        getrandom::fill(&mut random).map_err(|_| {
            unavailable(
                "provider_failure",
                "Preview provider identity is unavailable",
            )
        })?;
        let generation = hex::encode(random);
        let source = generation.clone();
        let identity: Identity = Box::new(move |path, info| token(&source, path, info));
        let source = generation.clone();
        let validate: Validate = Box::new(move |path, expected| {
            if !path.is_absolute()
                || fs::symlink_metadata(path)
                    .map_err(|error| error.to_string())?
                    .file_type()
                    .is_symlink()
            {
                return Err("The selected preview changed".into());
            }
            let info = fs::symlink_metadata(path).map_err(|error| error.to_string())?;
            if token(&source, path, &info)? != expected {
                return Err("The selected preview changed".into());
            }
            Ok(())
        });
        let resources = Arc::new(Snapshots::new(identity, validate)?);
        let requests = PreviewRequests::new(resources.clone())?;
        Ok(Self {
            generation,
            actions: Captured::new()?,
            resources,
            requests,
            tags: Mutex::new(LineService::new(
                std::env::var("LUMEN_FILE_TAGS_BIN").unwrap_or_else(|_| "lumen-file-tags".into()),
            )),
        })
    }

    pub fn identity(&self, path: &Path, info: &Metadata) -> Result<String, Unavailable> {
        token(&self.generation, path, info)
            .map_err(|_| unavailable("provider_failure", "Preview identity is unavailable"))
    }

    fn tag_identity(&self, info: &Metadata, expected: &Value) -> Result<Value, Unavailable> {
        let response = self
            .tags
            .lock()
            .unwrap()
            .tag(&json!({
                "action":"identity","device":info.dev(),"inode":info.ino(),"expected":expected,
            }))
            .map_err(|_| unavailable("source_changed", "The selected preview changed"))?;
        if response["matches"] != true {
            return Err(unavailable(
                "source_changed",
                "The selected preview changed",
            ));
        }
        Ok(response["wire"].clone())
    }

    fn shared_preview(&self, path: &Path, page: u64) -> Result<Value, Unavailable> {
        let helper =
            std::env::var("LUMEN_FILE_PREVIEW_BIN").unwrap_or_else(|_| "lumen-file-preview".into());
        let output = Command::new("timeout")
            .arg("12s")
            .arg(helper)
            .arg(path)
            .arg(page.to_string())
            .output()
            .map_err(|_| unavailable("provider_failure", "The preview provider is unavailable"))?;
        if !output.status.success() || output.stdout.len() > 8 * 1024 * 1024 {
            return Err(unavailable(
                "provider_failure",
                "The preview provider failed",
            ));
        }
        let result: Value = serde_json::from_slice(&output.stdout).map_err(|_| {
            unavailable(
                "provider_failure",
                "The preview provider returned invalid data",
            )
        })?;
        if result.get("error").is_some() {
            return Err(unavailable(
                "provider_failure",
                "The preview provider failed",
            ));
        }
        Ok(result)
    }

    pub fn request(
        &self,
        path: &Path,
        page: u64,
        expected: &Value,
        selected_token: Option<&str>,
        request_id: Option<u64>,
    ) -> Result<Value, Unavailable> {
        if !path.is_absolute() || page == 0 || page > 100_000 {
            return Err(unavailable(
                "invalid_request",
                "A local absolute preview path and valid page are required",
            ));
        }
        let info = fs::symlink_metadata(path)
            .map_err(|_| unavailable("source_changed", "The selected preview changed"))?;
        let source_identity = self.tag_identity(&info, expected)?;
        let observed = self.identity(path, &info)?;
        if selected_token.is_some_and(|token| token != observed) {
            return Err(unavailable(
                "source_changed",
                "The selected preview changed",
            ));
        }
        let facts = Facts::read(path)?;
        let capability = self.actions.capture(path, Some(facts), None)?;
        if info.is_file() && info.len() > 64 * 1024 * 1024 {
            return Ok(
                json!({"reason":"policy_limit","error":"This preview supports files up to 64 MiB","actionCapability":capability}),
            );
        }
        let result = (|| {
            let mut result;
            if info.is_file() {
                let lease = if let Some(id) = request_id {
                    Some(self.requests.work(id)?)
                } else {
                    None
                };
                let active = lease.as_ref().map(|(key, _)| key.as_str());
                let resource = self.resources.capture(path, &observed, active)?;
                if let Some(id) = request_id {
                    self.requests.retain_source(id, &observed)?;
                }
                result = self.shared_preview(&resource, page)?;
                if let Some(uri) = result["uri"]
                    .as_str()
                    .filter(|uri| *uri != file_uri(&resource))
                {
                    let rendered = local_uri(uri).map_err(|_| {
                        unavailable("invalid_request", "Unsupported preview resource")
                    })?;
                    let rendered_info = fs::symlink_metadata(&rendered).map_err(|_| {
                        unavailable("source_changed", "The preview resource changed")
                    })?;
                    let retained = self.resources.capture(
                        &rendered,
                        &self.identity(&rendered, &rendered_info)?,
                        active,
                    )?;
                    result["uri"] = json!(file_uri(&retained));
                }
                if Facts::read(path)? != facts {
                    return Err(unavailable(
                        "source_changed",
                        "The selected preview changed",
                    ));
                }
                self.actions.bind_resource(
                    &capability,
                    resource.clone(),
                    Some(observed.clone()),
                )?;
                result["actionCapability"] = json!(capability);
                result["path"] = json!(path);
                result["name"] = json!(path.file_name().unwrap_or_default().to_string_lossy());
                result["title"] = result["name"].clone();
                result["size"] = json!(info.len());
                result["mtime"] = json!(info.mtime() as f64 + info.mtime_nsec() as f64 / 1e9);
                result["mime"] = json!(mime_guess::from_path(path).first_raw().unwrap_or(""));
                result["sourceFacts"] = json!({"device":info.dev().to_string(),"inode":info.ino().to_string(),"size":info.len(),"mode":info.mode(),"mtimeNs":info.mtime()*1_000_000_000+info.mtime_nsec(),"ctimeNs":info.ctime()*1_000_000_000+info.ctime_nsec()});
                result["sourceToken"] = json!(observed);
                result["sourceIdentity"] = source_identity;
                result["resourcePath"] = json!(resource);
                result["requestedPage"] = json!(page);
                if let Some(id) = request_id {
                    let paths = result["uri"]
                        .as_str()
                        .map(local_uri)
                        .transpose()
                        .map_err(|_| {
                            unavailable("invalid_request", "Unsupported preview resource")
                        })?
                        .into_iter()
                        .collect::<Vec<_>>();
                    self.requests.finish(id, &paths)?;
                }
            } else if info.is_dir() {
                result = self.shared_preview(path, page)?;
                if Facts::read(path)? != facts {
                    return Err(unavailable(
                        "source_changed",
                        "The selected preview changed",
                    ));
                }
                result["actionCapability"] = json!(capability);
                result["sourceToken"] = json!(observed);
                result["sourceIdentity"] = source_identity;
                result["requestedPage"] = json!(page);
            } else {
                return Err(unavailable(
                    "unsupported",
                    "This file type cannot be previewed safely",
                ));
            }
            Ok(result)
        })();
        if result.is_err() {
            self.actions.retire(&[capability]);
        }
        result
    }

    pub fn resource_action(
        &self,
        capability: &str,
        operation: &str,
        application: Option<&str>,
        choice: Option<&str>,
        parameters: &Value,
        cancel: &AtomicBool,
    ) -> Result<Value, Unavailable> {
        let selected = self.actions.selected(capability)?;
        let lease = self.resources.begin()?;
        let result = (|| {
            if let Some(token) = selected.resource_token.as_deref() {
                self.resources.retain(&lease, token)?;
            }
            let _pin = self.resources.pin(&lease)?;
            let mut result = match operation {
                "applications" => self.actions.applications(capability)?,
                "open" | "open-with" | "set-default" => {
                    self.actions
                        .dispatch(capability, operation, application, choice)?
                }
                "ocr" | "cutout" => {
                    let resource = selected.resource.as_deref().ok_or_else(|| {
                        unavailable(
                            "unsupported",
                            "This preview does not support that content action",
                        )
                    })?;
                    if !fs::symlink_metadata(resource).is_ok_and(|info| info.is_file()) {
                        return Err(unavailable(
                            "source_changed",
                            "The retained preview is unavailable",
                        ));
                    }
                    let mut result = run_content(resource, operation, parameters, cancel)?;
                    self.actions.selected(capability)?;
                    if result.get("error").is_some() && result.get("reason").is_none() {
                        result["reason"] = json!("provider_failure");
                    }
                    if operation == "cutout" {
                        if let Some(subject) = result["subject"].as_str() {
                            result["subjectFacts"] = json!(Facts::read(Path::new(subject))?.wire());
                        }
                    }
                    result
                }
                _ => return Err(unavailable("invalid_request", "Unsupported preview action")),
            };
            if !result.is_object() {
                result = json!({"reason":"provider_failure","error":"The provider returned an invalid result"});
            }
            Ok(result)
        })();
        self.resources.release(&lease);
        result
    }
}

fn run_content(
    resource: &Path,
    operation: &str,
    parameters: &Value,
    cancel: &AtomicBool,
) -> Result<Value, Unavailable> {
    let mut request = json!({"action":operation,"path":resource});
    if operation == "ocr" {
        request["page"] = parameters.get("page").cloned().unwrap_or(json!(1));
        if let Some(seconds) = parameters.get("seconds") {
            request["seconds"] = seconds.clone();
        }
    }
    let helper =
        std::env::var("LUMEN_CONTEXT_TOOLS").unwrap_or_else(|_| "lumen-context-tools".into());
    let mut child = Command::new(helper)
        .process_group(0)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|_| unavailable("provider_failure", "The content provider is unavailable"))?;
    let input = child
        .stdin
        .take()
        .ok_or_else(|| unavailable("provider_failure", "The content provider is unavailable"))?;
    let payload = format!("{request}\n");
    let writer = thread::spawn(move || {
        let mut input = input;
        input.write_all(payload.as_bytes())
    });
    let mut output = child
        .stdout
        .take()
        .ok_or_else(|| unavailable("provider_failure", "The content provider is unavailable"))?;
    // A helper may leave descendants with its stdout pipe open. Read without a
    // blocking join so an exited leader cannot strand the preview worker.
    let flags = unsafe { libc::fcntl(output.as_raw_fd(), libc::F_GETFL) };
    if flags < 0
        || unsafe { libc::fcntl(output.as_raw_fd(), libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0
    {
        let _ = child.kill();
        let _ = child.wait();
        return Err(unavailable(
            "provider_failure",
            "The content provider failed",
        ));
    }
    let reader_stop = Arc::new(AtomicBool::new(false));
    let reader_done = reader_stop.clone();
    let reader = thread::spawn(move || {
        let mut bytes = Vec::new();
        let mut block = [0_u8; 8192];
        loop {
            match output.read(&mut block) {
                Ok(0) => break,
                Ok(count) => {
                    bytes.extend_from_slice(&block[..count]);
                    if bytes.len() > 8 * 1024 * 1024 {
                        return Err(());
                    }
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    if reader_done.load(Ordering::SeqCst) {
                        break;
                    }
                    thread::sleep(Duration::from_millis(20));
                }
                Err(_) => return Err(()),
            }
        }
        Ok(bytes)
    });
    let deadline = Instant::now() + Duration::from_secs(75);
    let status = loop {
        if cancel.load(Ordering::SeqCst) {
            // try_wait has not reaped this child, so its process-group ID cannot be reused.
            unsafe { libc::killpg(child.id() as i32, libc::SIGKILL) };
            let _ = child.wait();
            reader_stop.store(true, Ordering::SeqCst);
            let _ = writer.join();
            let _ = reader.join();
            return Err(unavailable("canceled", "Content action canceled"));
        }
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() >= deadline => {
                unsafe { libc::killpg(child.id() as i32, libc::SIGKILL) };
                let _ = child.wait();
                reader_stop.store(true, Ordering::SeqCst);
                let _ = writer.join();
                let _ = reader.join();
                return Err(unavailable(
                    "provider_timeout",
                    "The content action timed out",
                ));
            }
            Ok(None) => thread::sleep(Duration::from_millis(20)),
            Err(_) => {
                return Err(unavailable(
                    "provider_failure",
                    "The content provider failed",
                ));
            }
        }
    };
    reader_stop.store(true, Ordering::SeqCst);
    let _ = writer.join();
    let bytes = reader
        .join()
        .map_err(|_| unavailable("provider_failure", "The content provider failed"))?
        .map_err(|_| unavailable("provider_failure", "The content provider failed"))?;
    if !status.success() || bytes.len() > 8 * 1024 * 1024 {
        return Err(unavailable(
            "provider_failure",
            "The content provider failed",
        ));
    }
    let value: Value = serde_json::from_slice(&bytes).map_err(|_| {
        unavailable(
            "provider_failure",
            "The content provider returned an invalid result",
        )
    })?;
    if !value.is_object() {
        return Err(unavailable(
            "provider_failure",
            "The content provider returned an invalid result",
        ));
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_uri_accepts_only_local_absolute_resources() {
        assert_eq!(
            local_uri("file:///tmp/a%20b").unwrap(),
            PathBuf::from("/tmp/a b")
        );
        for invalid in [
            "https://host/file",
            "file://host/path",
            "file:///tmp/%ZZ",
            "file:///tmp/%00",
        ] {
            assert!(local_uri(invalid).is_err());
        }
    }
}
