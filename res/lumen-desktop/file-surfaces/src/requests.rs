//! Provider-scoped preview request retirement and retained source ownership.
use crate::captured::Unavailable;
use crate::snapshots::{Pin, Snapshots};
use std::collections::{HashMap, HashSet};
use std::fs::Metadata;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

fn invalid(message: &'static str) -> Unavailable {
    Unavailable {
        reason: "invalid_request",
        message,
    }
}

struct Job {
    lease: String,
    source: Option<String>,
    retired: bool,
}

struct Source {
    lease: String,
    owners: HashSet<u64>,
}

struct State {
    jobs: HashMap<u64, Job>,
    sources: HashMap<String, Source>,
    reserved: HashSet<String>,
    last_id: u64,
}

pub struct PreviewRequests<I, V>
where
    I: Fn(&Path, &Metadata) -> Result<String, String> + Send + Sync,
    V: Fn(&Path, &str) -> Result<(), String> + Send + Sync,
{
    resources: Arc<Snapshots<I, V>>,
    generation: String,
    state: Mutex<State>,
}

impl<I, V> PreviewRequests<I, V>
where
    I: Fn(&Path, &Metadata) -> Result<String, String> + Send + Sync,
    V: Fn(&Path, &str) -> Result<(), String> + Send + Sync,
{
    pub fn new(resources: Arc<Snapshots<I, V>>) -> Result<Self, Unavailable> {
        let mut bytes = [0_u8; 24];
        getrandom::fill(&mut bytes)
            .map_err(|_| invalid("Private preview provider identity is unavailable"))?;
        Ok(Self {
            resources,
            generation: hex::encode(bytes),
            state: Mutex::new(State {
                jobs: HashMap::new(),
                sources: HashMap::new(),
                reserved: HashSet::new(),
                last_id: 0,
            }),
        })
    }

    pub fn generation(&self) -> &str {
        &self.generation
    }

    fn drop_source(state: &mut State, resources: &Snapshots<I, V>, key: &str) {
        if state
            .sources
            .get(key)
            .is_some_and(|source| source.owners.is_empty())
            && !state.reserved.contains(key)
        {
            if let Some(source) = state.sources.remove(key) {
                resources.release(&source.lease);
            }
        }
    }

    pub fn register(&self, request_id: u64, token: Option<&str>) -> Result<(), Unavailable> {
        let mut state = self.state.lock().unwrap();
        if request_id <= state.last_id {
            return Err(invalid("Invalid preview request identity"));
        }
        if state.jobs.len() >= 64 {
            return Err(invalid("Too many outstanding preview requests"));
        }
        state.last_id = request_id;
        let previous: Vec<String> = state.reserved.drain().collect();
        for old in previous {
            if Some(old.as_str()) != token {
                Self::drop_source(&mut state, &self.resources, &old);
            }
        }
        let source = token
            .filter(|key| state.sources.contains_key(*key))
            .map(ToOwned::to_owned);
        let lease = self.resources.begin()?;
        if let Some(key) = &source {
            state
                .sources
                .get_mut(key)
                .unwrap()
                .owners
                .insert(request_id);
        }
        state.jobs.insert(
            request_id,
            Job {
                lease,
                source,
                retired: false,
            },
        );
        Ok(())
    }

    pub fn work(&self, request_id: u64) -> Result<(String, Pin<'_, I, V>), Unavailable> {
        let lease = {
            let state = self.state.lock().unwrap();
            let job = state
                .jobs
                .get(&request_id)
                .filter(|job| !job.retired)
                .ok_or_else(|| invalid("The preview request was retired"))?;
            job.lease.clone()
        };
        let pin = self.resources.pin(&lease)?;
        Ok((lease, pin))
    }

    pub fn retain_source(&self, request_id: u64, key: &str) -> Result<(), Unavailable> {
        let mut state = self.state.lock().unwrap();
        let job = state
            .jobs
            .get(&request_id)
            .filter(|job| !job.retired)
            .ok_or_else(|| invalid("The preview request was retired"))?;
        if job.source.as_ref().is_some_and(|old| old != key) {
            return Err(invalid("Preview source identity changed"));
        }
        if !state.sources.contains_key(key) {
            let lease = self.resources.begin()?;
            if let Err(error) = self.resources.retain(&lease, key) {
                self.resources.release(&lease);
                return Err(error);
            }
            state.sources.insert(
                key.to_owned(),
                Source {
                    lease,
                    owners: HashSet::new(),
                },
            );
        }
        state
            .sources
            .get_mut(key)
            .unwrap()
            .owners
            .insert(request_id);
        state.jobs.get_mut(&request_id).unwrap().source = Some(key.to_owned());
        Ok(())
    }

    pub fn finish(&self, request_id: u64, published: &[PathBuf]) -> Result<(), Unavailable> {
        let state = self.state.lock().unwrap();
        let job = state
            .jobs
            .get(&request_id)
            .filter(|job| !job.retired)
            .ok_or_else(|| invalid("The preview request was retired"))?;
        self.resources.keep(&job.lease, published)
    }

    pub fn retire(
        &self,
        request_ids: &[u64],
        keep_source: Option<&str>,
        timeout: Duration,
        wait: bool,
    ) -> Result<(), Unavailable> {
        let mut leases = Vec::new();
        {
            let mut state = self.state.lock().unwrap();
            for request_id in request_ids {
                let Some(job) = state.jobs.get_mut(request_id) else {
                    continue;
                };
                leases.push((*request_id, job.lease.clone()));
                if job.retired {
                    continue;
                }
                job.retired = true;
                let source_key = job.source.clone();
                let lease = job.lease.clone();
                if let Some(key) = source_key {
                    if let Some(source) = state.sources.get_mut(&key) {
                        source.owners.remove(request_id);
                    }
                    if Some(key.as_str()) == keep_source {
                        state.reserved.insert(key.clone());
                    } else {
                        state.reserved.remove(&key);
                    }
                    Self::drop_source(&mut state, &self.resources, &key);
                }
                self.resources.release(&lease);
            }
        }
        if !wait {
            return Ok(());
        }
        let deadline = Instant::now() + timeout;
        for (request_id, lease) in leases {
            if !self
                .resources
                .wait_released(&lease, deadline.saturating_duration_since(Instant::now()))
            {
                return Err(invalid(
                    "Preview retirement is still waiting for its worker",
                ));
            }
            let mut state = self.state.lock().unwrap();
            if state
                .jobs
                .get(&request_id)
                .is_some_and(|job| job.lease == lease)
            {
                state.jobs.remove(&request_id);
            }
        }
        Ok(())
    }

    pub fn close(&self) -> Result<(), Unavailable> {
        self.resources.close()?;
        let mut state = self.state.lock().unwrap();
        state.jobs.clear();
        state.sources.clear();
        state.reserved.clear();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::os::unix::fs::{MetadataExt, PermissionsExt};

    fn identity(_: &Path, info: &Metadata) -> Result<String, String> {
        Ok(format!("{}:{}:{}", info.dev(), info.ino(), info.len()))
    }
    fn validate(path: &Path, expected: &str) -> Result<(), String> {
        if identity(
            path,
            &fs::metadata(path).map_err(|error| error.to_string())?,
        )? == expected
        {
            Ok(())
        } else {
            Err("changed".into())
        }
    }

    #[test]
    fn source_handover_keeps_only_next_request_source() {
        let runtime = tempfile::tempdir().unwrap();
        fs::set_permissions(runtime.path(), fs::Permissions::from_mode(0o700)).unwrap();
        let source = runtime.path().join("document.pdf");
        let first = runtime.path().join("first.png");
        let second = runtime.path().join("second.png");
        for (path, text) in [(&source, "source"), (&first, "first"), (&second, "second")] {
            fs::write(path, text).unwrap();
        }
        let resources = Arc::new(
            Snapshots::new_with_runtime(identity, validate, Some(runtime.path().to_owned()))
                .unwrap(),
        );
        let requests = PreviewRequests::new(resources.clone()).unwrap();
        let source_key = identity(&source, &fs::metadata(&source).unwrap()).unwrap();
        requests.register(1, Some(&source_key)).unwrap();
        let retained;
        let raster;
        {
            let (lease, _pin) = requests.work(1).unwrap();
            retained = resources
                .capture(&source, &source_key, Some(&lease))
                .unwrap();
            requests.retain_source(1, &source_key).unwrap();
            let key = identity(&first, &fs::metadata(&first).unwrap()).unwrap();
            raster = resources.capture(&first, &key, Some(&lease)).unwrap();
            requests.finish(1, std::slice::from_ref(&raster)).unwrap();
        }
        requests
            .retire(&[1], Some(&source_key), Duration::from_secs(1), true)
            .unwrap();
        assert!(retained.exists());
        assert!(!raster.exists());
        requests.register(2, Some(&source_key)).unwrap();
        let next;
        {
            let (lease, _pin) = requests.work(2).unwrap();
            assert_eq!(
                resources
                    .capture(&source, &source_key, Some(&lease))
                    .unwrap(),
                retained
            );
            requests.retain_source(2, &source_key).unwrap();
            let key = identity(&second, &fs::metadata(&second).unwrap()).unwrap();
            next = resources.capture(&second, &key, Some(&lease)).unwrap();
            requests.finish(2, std::slice::from_ref(&next)).unwrap();
        }
        requests
            .retire(&[2], None, Duration::from_secs(1), true)
            .unwrap();
        assert!(!retained.exists());
        assert!(!next.exists());
    }
}
