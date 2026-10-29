use crate::Result;
use rusqlite::{Connection, OpenFlags, types::ValueRef};
use serde_json::{Value, json};
use std::path::Path;
use std::time::Duration;

pub const IN_PROGRESS: i64 = 0;
pub const COMPLETE: i64 = 1;
pub const CANCELLED: i64 = 2;
pub const INTERRUPTED: i64 = 4;
fn open(profile: &Path) -> Result<Option<Connection>> {
    let path = profile.join("Default/History");
    if !path.exists() {
        return Ok(None);
    }
    let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    conn.busy_timeout(Duration::from_millis(100))?;
    Ok(Some(conn))
}
fn fsdecode(value: ValueRef<'_>) -> String {
    match value {
        ValueRef::Blob(bytes) | ValueRef::Text(bytes) => {
            String::from_utf8_lossy(bytes).into_owned()
        }
        _ => String::new(),
    }
}
fn locked_or_rows<T>(work: impl FnOnce() -> Result<Vec<T>>) -> Result<Vec<T>> {
    match work() {
        Err(error) if error.to_string().to_lowercase().contains("locked") => Ok(Vec::new()),
        other => other,
    }
}
pub fn download_rows(profile: &Path) -> Result<Vec<Value>> {
    locked_or_rows(|| {
        let Some(conn) = open(profile)? else {
            return Ok(Vec::new());
        };
        let mut query=conn.prepare("SELECT id,guid,current_path,target_path,received_bytes,total_bytes,state,interrupt_reason FROM downloads ORDER BY id")?;
        let rows=query.query_map([],|row|Ok(json!({"id":row.get::<_,i64>(0)?,"guid":row.get::<_,String>(1)?,"current_path":fsdecode(row.get_ref(2)?),"target_path":fsdecode(row.get_ref(3)?),"received_bytes":row.get::<_,i64>(4)?,"total_bytes":row.get::<_,i64>(5)?,"state":row.get::<_,i64>(6)?,"interrupt_reason":row.get::<_,i64>(7)?})))?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    })
}
pub fn history_urls(profile: &Path) -> Result<Vec<String>> {
    locked_or_rows(|| {
        let Some(conn) = open(profile)? else {
            return Ok(Vec::new());
        };
        let mut query = conn.prepare("SELECT url FROM urls ORDER BY id DESC LIMIT 4096")?;
        let rows = query.query_map([], |row| row.get(0))?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    })
}
