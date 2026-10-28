//! Per-display viewer control socket. The compositor/input bridge is added separately.
use serde_json::json;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::os::fd::AsRawFd;
use std::os::unix::fs::{FileTypeExt, MetadataExt, PermissionsExt};
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::time::Duration;

unsafe extern "C" { fn flock(fd: i32, operation: i32) -> i32; }
const LOCK_EX: i32 = 2;
const LOCK_NB: i32 = 4;

pub struct Server {
    pub listener: UnixListener,
    pub path: PathBuf,
    _lock: File,
}

fn runtime_dir() -> Result<PathBuf, String> {
    let path=PathBuf::from(std::env::var_os("XDG_RUNTIME_DIR").ok_or("XDG_RUNTIME_DIR is unset")?);
    let meta=fs::metadata(&path).map_err(|error|error.to_string())?;
    if !meta.is_dir() || meta.uid()!=unsafe { getuid() } {return Err("Invalid runtime directory owner".into());}
    Ok(path)
}
unsafe extern "C" { fn getuid() -> u32; }

fn display_name() -> String {
    std::env::var("WAYLAND_DISPLAY").unwrap_or_else(|_|"wayland".into()).chars()
        .map(|ch|if ch.is_ascii_alphanumeric() || ch=='_' || ch=='-' {ch} else {'_'})
        .collect()
}

fn instance() -> &'static str {"lumen"}

pub fn path() -> Result<PathBuf,String> {Ok(runtime_dir()?.join(format!("{}-character-viewer-{}.sock",instance(),display_name())))}

pub fn request(command:&str) -> Result<String,String> {
    let mut client=UnixStream::connect(path()?).map_err(|error|error.to_string())?;
    client.set_read_timeout(Some(Duration::from_secs(1))).map_err(|error|error.to_string())?;
    client.set_write_timeout(Some(Duration::from_secs(1))).map_err(|error|error.to_string())?;
    client.write_all(command.as_bytes()).map_err(|error|error.to_string())?;
    let mut buffer=[0u8;1024];
    let count=client.read(&mut buffer).map_err(|error|error.to_string())?;
    if count==0 {return Err("The viewer gave no response".into());}
    String::from_utf8(buffer[..count].to_vec()).map_err(|error|error.to_string())
}

pub fn start() -> Result<Server,String> {
    let root=runtime_dir()?;
    let lock_path=root.join(format!("{}-character-input.lock",instance()));
    let lock=OpenOptions::new().write(true).create(true).truncate(false).open(lock_path)
        .map_err(|error|error.to_string())?;
    if unsafe { flock(lock.as_raw_fd(),LOCK_EX|LOCK_NB) }!=0 {return Err("Viewer daemon already running".into());}
    let path=path()?;
    if let Ok(meta)=fs::symlink_metadata(&path) {
        if !meta.file_type().is_socket() || meta.uid()!=unsafe {getuid()} {return Err("Unsafe viewer socket path".into());}
        fs::remove_file(&path).map_err(|error|error.to_string())?;
    }
    let listener=UnixListener::bind(&path).map_err(|error|error.to_string())?;
    fs::set_permissions(&path,fs::Permissions::from_mode(0o600)).map_err(|error|error.to_string())?;
    listener.set_nonblocking(true).map_err(|error|error.to_string())?;
    Ok(Server{listener,path,_lock:lock})
}

pub fn standalone_lock()->Result<Option<File>,String> {
    let path=runtime_dir()?.join(format!("{}-character-viewer.lock",instance()));
    let lock=OpenOptions::new().write(true).create(true).mode(0o600).open(path)
        .map_err(|error|error.to_string())?;
    if unsafe {flock(lock.as_raw_fd(),LOCK_EX|LOCK_NB)}!=0 {return Ok(None);}
    Ok(Some(lock))
}

pub fn receive(mut client:UnixStream, mut show:impl FnMut()->bool,
    available:bool,active:bool,visible:bool,native:bool) {
    let _=client.set_read_timeout(Some(Duration::from_millis(200)));
    let _=client.set_write_timeout(Some(Duration::from_millis(200)));
    let mut buffer=[0u8;32];
    if let Ok(count)=client.read(&mut buffer) {
        match &buffer[..count] {
            b"status" => {let _=client.write_all(json!({"running":true,"available":available,"active":active,
                "visible":visible,"mode":if native {"caret"} else {"window"}}).to_string().as_bytes());}
            b"show" => {if show() {let _=client.write_all(b"ok");}}
            _ => {}
        }
    }
}

pub fn remove_socket(path:&Path) {let _=fs::remove_file(path);}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn status_reply_is_json_and_does_not_show() {
        let (mut client,server)=UnixStream::pair().unwrap();
        client.write_all(b"status").unwrap();
        receive(server,||panic!("status must not show a window"),true,true,true,true);
        let mut reply=String::new();client.read_to_string(&mut reply).unwrap();
        assert_eq!(serde_json::from_str::<serde_json::Value>(&reply).unwrap()["visible"],true);
    }
    #[test]
    fn show_acknowledges_one_dispatch() {
        let (mut client,server)=UnixStream::pair().unwrap();
        client.write_all(b"show").unwrap();
        let mut calls=0;
        receive(server,||{calls+=1;true},false,false,false,false);
        let mut reply=String::new();client.read_to_string(&mut reply).unwrap();
        assert_eq!(reply,"ok");
        assert_eq!(calls,1);
    }
}
