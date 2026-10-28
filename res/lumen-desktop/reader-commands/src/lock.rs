//! Native compositor lock facts, fail closed across IPC changes and reconnects.
use serde_json::{Value, json};
use std::env;
use std::fs;
use std::io::{Read, Write};
use std::os::fd::AsRawFd;
use std::os::unix::fs::{FileTypeExt, MetadataExt};
use std::os::unix::net::UnixStream;
use std::time::Duration;
use crate::{Result, monotonic};

const MAGIC: &[u8;6] = b"i3-ipc";
#[derive(Clone, PartialEq, Eq)]
struct Token { pid:i32, uid:u32, gid:u32, dev:u64, ino:u64 }
fn connect(path: &str) -> Result<(UnixStream, Token)> {
    let before=fs::metadata(path).map_err(|e| e.to_string())?;
    if !before.file_type().is_socket() { return Err("Not an IPC socket".into()); }
    let stream=UnixStream::connect(path).map_err(|e| e.to_string())?;
    stream.set_read_timeout(Some(Duration::from_millis(200))).map_err(|e| e.to_string())?;
    stream.set_write_timeout(Some(Duration::from_millis(200))).map_err(|e| e.to_string())?;
    let mut peer=libc::ucred {pid:0,uid:0,gid:0};
    let mut size=std::mem::size_of::<libc::ucred>() as libc::socklen_t;
    if unsafe { libc::getsockopt(stream.as_raw_fd(),libc::SOL_SOCKET,libc::SO_PEERCRED,
        (&raw mut peer).cast(),&raw mut size) }!=0 || size as usize!=std::mem::size_of::<libc::ucred>() {
        return Err("Cannot identify IPC owner".into());
    }
    let after=fs::metadata(path).map_err(|e| e.to_string())?;
    if before.dev()!=after.dev() || before.ino()!=after.ino() || peer.uid!=unsafe{libc::geteuid()} {
        return Err("Changed or foreign IPC socket".into());
    }
    Ok((stream,Token {pid:peer.pid,uid:peer.uid,gid:peer.gid,dev:before.dev(),ino:before.ino()}))
}
fn frame(stream: &mut UnixStream, kind:u32, body:&Value) -> Result<Value> {
    let payload=body.to_string();
    let mut header=[0u8;14];header[..6].copy_from_slice(MAGIC);
    header[6..10].copy_from_slice(&(payload.len() as u32).to_le_bytes());
    header[10..14].copy_from_slice(&kind.to_le_bytes());
    stream.write_all(&header).map_err(|e| e.to_string())?;
    stream.write_all(payload.as_bytes()).map_err(|e| e.to_string())?;
    stream.read_exact(&mut header).map_err(|e| e.to_string())?;
    let length=u32::from_le_bytes(header[6..10].try_into().unwrap()) as usize;
    if &header[..6]!=MAGIC || length>65536 || u32::from_le_bytes(header[10..14].try_into().unwrap())!=kind {
        return Err("Invalid IPC frame".into());
    }
    let mut payload=vec![0u8;length];stream.read_exact(&mut payload).map_err(|e| e.to_string())?;
    serde_json::from_slice(&payload).map_err(|e| e.to_string())
}
pub struct LockAuthority { monitor:Option<UnixStream>, token:Option<Token>, last_token:Option<Token>,
    revision:i64, buffer:Vec<u8>, retry:f64, facts:Value }
impl LockAuthority {
    pub fn new() -> Self {
        Self { monitor:None,token:None,last_token:None,revision:-1,buffer:Vec::new(),retry:0.0,
            facts:json!({"known":false,"locked":true}) }
    }
    fn close(&mut self) {
        self.monitor=None;self.token=None;self.buffer.clear();
        self.facts=json!({"known":false,"locked":true});
    }
    fn ensure_monitor(&mut self, path:&str) -> Result<()> {
        if self.monitor.is_some() { return Ok(()); }
        if monotonic()<self.retry { return Err("Lock authority retry pending".into()); }
        self.retry=monotonic()+1.0;
        let (mut monitor,token)=connect(path)?;
        if frame(&mut monitor,2,&json!(["tick","shutdown"]))?["success"]!=true {
            return Err("Subscription rejected".into());
        }
        monitor.set_nonblocking(true).map_err(|e| e.to_string())?;
        if self.last_token.as_ref()!=Some(&token) { self.revision=-1; }
        self.last_token=Some(token.clone());self.token=Some(token);self.monitor=Some(monitor);
        Ok(())
    }
    fn drain_events(&mut self) -> Result<()> {
        let monitor=self.monitor.as_mut().ok_or("No lock monitor")?;
        let mut chunk=[0u8;65536];
        loop {
            match monitor.read(&mut chunk) {
                Ok(0) => return Err("Closed lock authority".into()),
                Ok(count) => self.buffer.extend_from_slice(&chunk[..count]),
                Err(error) if error.kind()==std::io::ErrorKind::WouldBlock => break,
                Err(error) => return Err(error.to_string()),
            }
            if self.buffer.len()>131072 { return Err("Invalid authority event".into()); }
        }
        while self.buffer.len()>=14 {
            let length=u32::from_le_bytes(self.buffer[6..10].try_into().unwrap()) as usize;
            let kind=u32::from_le_bytes(self.buffer[10..14].try_into().unwrap());
            if &self.buffer[..6]!=MAGIC || length>65536 || kind==0x80000006 {
                return Err("Invalid authority event".into());
            }
            if self.buffer.len()<14+length { break; }
            self.buffer.drain(..14+length);
        }
        Ok(())
    }
    fn query(&mut self,path:&str) -> Result<()> {
        self.ensure_monitor(path)?;
        self.drain_events()?;
        let (mut stream, token)=connect(path)?;
        if self.token.as_ref()!=Some(&token) { return Err("Changed compositor".into()); }
        let response=frame(&mut stream,7,&json!(""))?;
        self.drain_events()?;
        let value=&response["lumen_session_lock"];
        let revision=value["revision"].as_i64().ok_or("Unknown lock authority")?;
        if value["version"]!=1 || value["available"]!=true || !value["locked"].is_boolean()
            || revision<0 || revision<self.revision { return Err("Unknown lock authority".into()); }
        self.revision=revision;
        self.facts=json!({"known":true,"locked":value["locked"]});
        Ok(())
    }
    pub fn refresh(&mut self) -> bool {
        let before=self.facts.clone();
        let result=env::var("SWAYSOCK").map_err(|e| e.to_string()).and_then(|path| self.query(&path));
        if result.is_err() { self.close(); }
        before!=self.facts
    }
    pub fn snapshot(&mut self) -> Value { self.refresh();self.facts.clone() }
    pub fn signature(&self) -> Option<String> {
        let token=self.token.as_ref()?;
        if self.facts["known"]!=true { return None; }
        Some(format!("{}:{}:{}:{}:{}:{}",token.pid,token.uid,token.gid,token.dev,token.ino,self.revision))
    }
}
