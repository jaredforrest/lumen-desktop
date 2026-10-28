//! Validated Focus profiles and local civil-time schedules.
use serde_json::{Value, json};
use std::collections::HashSet;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};
use crate::Result;

fn wall_time() -> f64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0.0, |duration| duration.as_secs_f64())
}
fn boot_time() -> f64 {
    let mut stamp = libc::timespec { tv_sec:0, tv_nsec:0 };
    unsafe { libc::clock_gettime(libc::CLOCK_BOOTTIME, &mut stamp); }
    stamp.tv_sec as f64 + stamp.tv_nsec as f64 / 1e9
}
fn default_profiles() -> Vec<Value> {
    ["quiet", "work", "personal"].into_iter().map(|id| json!({"id":id,
        "name":match id { "quiet" => "Do Not Disturb", "work" => "Work", _ => "Personal" },
        "allowedApps":[],"allowCritical":false,"schedules":[]})).collect()
}
fn profile(value: &Value) -> Result<Value> {
    let object = value.as_object().ok_or("Invalid Focus profile")?;
    if object.keys().any(|key| !["id","name","allowedApps","allowCritical","schedules"].contains(&key.as_str())) {
        return Err("Invalid Focus profile".into());
    }
    let id = value["id"].as_str().ok_or("Invalid Focus identity")?;
    if id.is_empty() || id.len()>64 || !id.bytes().all(|byte| byte.is_ascii_alphanumeric() || b"-_".contains(&byte)) {
        return Err("Invalid Focus identity".into());
    }
    let name = value["name"].as_str().ok_or("Invalid Focus name")?.trim();
    if name.is_empty() || name.chars().count()>64 { return Err("Invalid Focus name".into()); }
    let apps = value.get("allowedApps").unwrap_or(&Value::Null);
    let apps = if apps.is_null() && value.get("allowedApps").is_none() { vec![] }
        else { apps.as_array().cloned().ok_or("Invalid Focus application exceptions")? };
    if apps.len()>100 || apps.iter().any(|app| app.as_str().is_none_or(|text| text.is_empty() || text.chars().count()>256)) {
        return Err("Invalid Focus application exceptions".into());
    }
    let mut unique = Vec::new();
    for app in apps { if !unique.contains(&app) { unique.push(app); } }
    let critical = match value.get("allowCritical") { None => false,
        Some(item) => item.as_bool().ok_or("Invalid critical notification permission")? };
    let schedules = match value.get("schedules") { None => vec![],
        Some(item) => item.as_array().cloned().ok_or("Invalid Focus schedules")? };
    if schedules.len()>16 { return Err("Invalid Focus schedules".into()); }
    for schedule in &schedules {
        let object = schedule.as_object().ok_or("Invalid Focus schedule")?;
        if object.len()!=3 || !["days","start","end"].iter().all(|key| object.contains_key(*key)) {
            return Err("Invalid Focus schedule".into());
        }
        let days = schedule["days"].as_array().ok_or("Invalid schedule weekdays")?;
        if days.is_empty() || days.len()>7 { return Err("Invalid schedule weekdays".into()); }
        let mut seen = HashSet::new();
        for day in days {
            let value = day.as_u64().filter(|value| *value<=6).ok_or("Invalid schedule weekdays")?;
            if !seen.insert(value) { return Err("Invalid schedule weekdays".into()); }
        }
        let start = schedule["start"].as_u64().filter(|value| *value<1440).ok_or("Invalid schedule time")?;
        let end = schedule["end"].as_u64().filter(|value| *value<1440).ok_or("Invalid schedule time")?;
        if start==end { return Err("Invalid schedule time".into()); }
    }
    Ok(json!({"id":id,"name":name,"allowedApps":unique,"allowCritical":critical,"schedules":schedules}))
}
fn local_day_minute() -> (u64,u64) {
    let mut now: libc::time_t = 0;
    let mut local: libc::tm = unsafe { std::mem::zeroed() };
    unsafe { libc::time(&mut now); libc::localtime_r(&now, &mut local); }
    (((local.tm_wday+6)%7) as u64, (local.tm_hour*60+local.tm_min) as u64)
}
fn scheduled(item: &Value, day: u64, minute: u64) -> bool {
    item["schedules"].as_array().is_some_and(|rules| rules.iter().any(|rule| {
        let start = rule["start"].as_u64().unwrap_or(0);
        let end = rule["end"].as_u64().unwrap_or(0);
        let days = rule["days"].as_array();
        let on = |day: u64| days.is_some_and(|days| days.iter().any(|value| value.as_u64()==Some(day)));
        if start<end { on(day) && start<=minute && minute<end }
        else { (on(day) && minute>=start) || (on((day+6)%7) && minute<end) }
    }))
}
pub struct Focus { pub path: PathBuf, pub profiles: Vec<Value>, pub manual: Option<String>,
    pub until: f64, pub boot_deadline: f64 }
impl Focus {
    pub fn new(path: PathBuf) -> Result<Self> {
        let mut focus = Self { path, profiles:default_profiles(), manual:None, until:0.0, boot_deadline:0.0 };
        let raw = match fs::read(&focus.path) { Ok(raw) => raw,
            Err(error) if error.kind()==std::io::ErrorKind::NotFound => return Ok(focus),
            Err(error) => return Err(error.to_string()) };
        let result = (|| -> Result<()> {
            let value: Value = serde_json::from_slice(&raw).map_err(|e| e.to_string())?;
            if value["version"]!=1 { return Err("Invalid Focus state".into()); }
            let rows = value["profiles"].as_array().ok_or("Invalid Focus profiles")?;
            if !(1..=20).contains(&rows.len()) { return Err("Invalid Focus profiles".into()); }
            let profiles = rows.iter().map(profile).collect::<Result<Vec<_>>>()?;
            let mut ids = HashSet::new();
            if profiles.iter().any(|item| !ids.insert(item["id"].as_str().unwrap_or(""))) {
                return Err("Invalid Focus profiles".into());
            }
            let manual = match value.get("manual") { Some(Value::Null) | None => None,
                Some(Value::String(text)) if text.is_empty() || ids.contains(text.as_str()) => Some(text.clone()),
                _ => return Err("Invalid active Focus".into()) };
            let until = value.get("until").and_then(Value::as_f64).unwrap_or(0.0);
            if !until.is_finite() || until<0.0 { return Err("Invalid Focus expiry".into()); }
            focus.profiles=profiles;focus.manual=manual;focus.until=until;
            if until>0.0 {
                let remaining=(until-wall_time()).min(86400.0);
                if remaining<=0.0 { focus.manual=None; focus.until=0.0; }
                else { focus.boot_deadline=boot_time()+remaining; }
            }
            Ok(())
        })();
        if result.is_err() {
            let stamp=SystemTime::now().duration_since(UNIX_EPOCH).map_err(|e| e.to_string())?.as_nanos();
            fs::rename(&focus.path,focus.path.with_file_name(format!("focus-invalid-{stamp}.json")))
                .map_err(|e| e.to_string())?;
        }
        Ok(focus)
    }
    pub fn save(&self) -> Result<()> {
        let pending=self.path.with_extension("tmp");
        let mut file=OpenOptions::new().write(true).create(true).truncate(true).mode(0o600)
            .open(&pending).map_err(|e| e.to_string())?;
        file.write_all(json!({"version":1,"profiles":self.profiles,"manual":self.manual,"until":self.until})
            .to_string().as_bytes()).map_err(|e| e.to_string())?;
        file.sync_all().map_err(|e| e.to_string())?;
        fs::rename(pending,&self.path).map_err(|e| e.to_string())
    }
    fn selected(&self) -> Option<&Value> {
        if self.until>0.0 && (wall_time()>=self.until || boot_time()>=self.boot_deadline) { return None; }
        if let Some(manual)=&self.manual {
            return self.profiles.iter().find(|item| item["id"]==*manual);
        }
        let (day,minute)=local_day_minute();
        self.profiles.iter().find(|item| scheduled(item,day,minute))
    }
    pub fn state(&self) -> Value {
        let active=self.selected();
        json!({"profiles":self.profiles,"active":active.and_then(|item| item["id"].as_str()).unwrap_or(""),
            "name":active.and_then(|item| item["name"].as_str()).unwrap_or(""),
            "until":self.until,"scheduled":self.manual.is_none()})
    }
    pub fn active(&self) -> bool { self.selected().is_some() }
    pub fn allows(&self, app: &str, urgency: u8) -> bool {
        self.selected().is_none_or(|item| item["allowedApps"].as_array().is_some_and(|apps| apps.iter().any(|value| value==app))
            || urgency==2 && item["allowCritical"]==true)
    }
    pub fn expire(&mut self) -> Result<bool> {
        if self.until>0.0 && (wall_time()>=self.until || boot_time()>=self.boot_deadline) {
            self.manual=None;self.until=0.0;self.boot_deadline=0.0;self.save()?;return Ok(true);
        }
        Ok(false)
    }
    pub fn command(&mut self, data: &Value) -> Result<()> {
        match data["operation"].as_str().unwrap_or("activate") {
            "profile" => {
                let item=profile(&data["profile"])?;
                if let Some(index)=self.profiles.iter().position(|row| row["id"]==item["id"]) {
                    self.profiles[index]=item;
                } else if self.profiles.len()<20 { self.profiles.push(item); }
                else { return Err("Too many Focus profiles".into()); }
            }
            "activate" => {
                let manual = match data.get("profile") {
                    None => Some(String::new()), Some(Value::Null) => None,
                    Some(Value::String(value)) if value.is_empty() || self.profiles.iter().any(|item| item["id"]==*value) => Some(value.clone()),
                    _ => return Err("Unknown Focus".into()),
                };
                let seconds=data.get("seconds").map_or(Some(0), Value::as_u64)
                    .filter(|value| *value<=86400).ok_or("Invalid Focus duration")?;
                self.manual=manual;
                self.until=if seconds>0 { wall_time()+seconds as f64 } else {0.0};
                self.boot_deadline=if seconds>0 { boot_time()+seconds as f64 } else {0.0};
            }
            _ => return Err("Unknown Focus operation".into()),
        }
        self.save()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn focus_validates_profiles_and_overnight_schedules() {
        assert!(profile(&json!({"id":"work","name":"Work","schedules":[{"days":[0],"start":1380,"end":60}]})).is_ok());
        assert!(scheduled(&json!({"schedules":[{"days":[0],"start":1380,"end":60}]}),1,30));
        assert!(!scheduled(&json!({"schedules":[{"days":[0],"start":1380,"end":60}]}),2,30));
        assert!(profile(&json!({"id":"bad space","name":"Bad"})).is_err());
        for invalid in [json!({"id":"work","name":"Work","allowCritical":"yes"}),
            json!({"id":"work","name":"Work","allowedApps":[""]}),
            json!({"id":"work","name":"Work","schedules":[{"days":[true],"start":1,"end":2}]}),
            json!({"id":"work","name":"Work","schedules":[{"days":[1],"start":2,"end":2}]})] {
            assert!(profile(&invalid).is_err());
        }
    }
    #[test]
    fn focus_persists_activation_and_exceptions() {
        let directory=std::env::temp_dir().join(format!("lumen-focus-{}",std::process::id()));
        let _=fs::remove_dir_all(&directory);fs::create_dir(&directory).unwrap();
        let path=directory.join("focus.json");
        let mut focus=Focus::new(path.clone()).unwrap();
        assert!(focus.allows("chat",1));
        focus.command(&json!({"profile":"work"})).unwrap();
        assert!(!focus.allows("chat",1));
        focus.command(&json!({"operation":"profile","profile":{"id":"work","name":"Work",
            "allowedApps":["calendar"],"allowCritical":true,"schedules":[]}})).unwrap();
        assert!(focus.allows("calendar",1));assert!(focus.allows("chat",2));
        assert!(!focus.allows("chat",1));
        let restored=Focus::new(path.clone()).unwrap();
        assert_eq!(restored.state()["active"],"work");
        focus.command(&json!({"profile":"work","seconds":3600})).unwrap();
        focus.boot_deadline=boot_time()-1.0;
        assert!(focus.expire().unwrap());
        assert!(focus.state()["scheduled"]==true);
        for invalid in [json!(true),json!(-1),json!(86401),json!(1.5)] {
            assert!(focus.command(&json!({"profile":"work","seconds":invalid})).is_err());
        }
        fs::remove_dir_all(directory).unwrap();
    }
}
