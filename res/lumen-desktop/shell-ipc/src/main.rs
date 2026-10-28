//! Route a single IPC action to the current desktop shell service PID.
use std::{env,fs,io::Read,os::unix::fs::MetadataExt,process::{Command,Stdio},sync::mpsc,thread,time::{Duration,Instant}};
type Result<T>=std::result::Result<T,String>;

struct Args {systemctl:String,quickshell:String,action:Vec<String>}
fn args()->Result<Args>{
    let mut inputs=env::args().skip(1).peekable();
    let mut systemctl="systemctl".to_owned();let mut quickshell="quickshell".to_owned();let mut action=Vec::new();
    while let Some(value)=inputs.next(){
        match value.as_str(){
            "--systemctl"=>systemctl=inputs.next().ok_or("Missing systemctl path")?,
            "--quickshell"=>quickshell=inputs.next().ok_or("Missing quickshell path")?,
            "--"=>{action.extend(inputs);break;},
            _=>{action.push(value);action.extend(inputs);break;},
        }
    }
    if action.is_empty(){return Err("an IPC command is required".into());}
    Ok(Args{systemctl,quickshell,action})
}
fn command(program:&str,args:&[&str],timeout:u64,capture:bool)->Result<(i32,String)>{
    let mut child=Command::new(program).args(args).stdin(Stdio::null())
        .stdout(if capture{Stdio::piped()}else{Stdio::inherit()})
        .stderr(if capture{Stdio::piped()}else{Stdio::inherit()})
        .spawn().map_err(|e|e.to_string())?;
    let out=child.stdout.take().map(|mut pipe|{
        let (sender,receiver)=mpsc::channel();
        thread::spawn(move||{let mut bytes=Vec::new();let _=sender.send(pipe.read_to_end(&mut bytes).map(|_|bytes));});
        receiver
    });
    let err=child.stderr.take().map(|mut pipe|{
        let (sender,receiver)=mpsc::channel();
        thread::spawn(move||{let mut bytes=Vec::new();let _=sender.send(pipe.read_to_end(&mut bytes).map(|_|bytes));});
        receiver
    });
    let deadline=Instant::now()+Duration::from_secs(timeout);
    while Instant::now()<deadline{
        if let Some(status)=child.try_wait().map_err(|e|e.to_string())?{
            let stdout=out.map(|reader|reader.recv_timeout(deadline.saturating_duration_since(Instant::now()))
                .map_err(|_|"Output reader timed out")?.map_err(|e|e.to_string())).transpose()?.unwrap_or_default();
            let stderr=err.map(|reader|reader.recv_timeout(deadline.saturating_duration_since(Instant::now()))
                .map_err(|_|"Error reader timed out")?.map_err(|e|e.to_string())).transpose()?.unwrap_or_default();
            if !status.success() && capture {
                return Err(String::from_utf8_lossy(&stderr).trim().to_owned());
            }
            return Ok((status.code().unwrap_or(1),String::from_utf8_lossy(&stdout).into_owned()));
        }
        thread::sleep(Duration::from_millis(10));
    }
    let _=child.kill();let _=child.wait();
    Err(format!("{program} timed out"))
}
fn parse_pid(value:&str)->Result<u32>{
    let value=value.trim();
    if value.is_empty() || !value.bytes().all(|byte|byte.is_ascii_digit()){
        return Err("Desktop shell service is not running".into());
    }
    let pid=value.parse::<u32>().map_err(|_|"Desktop shell service is not running")?;
    if pid<2{return Err("Desktop shell service is not running".into());}
    Ok(pid)
}
fn route(args:Args)->Result<i32>{
    let (_,value)=command(&args.systemctl,&["--user","show","lumen-desktop-shell.service","--property=MainPID","--value"],5,true)?;
    let pid=parse_pid(&value)?;
    let info=fs::metadata(format!("/proc/{pid}")).map_err(|e|e.to_string())?;
    if info.uid()!=unsafe{libc::getuid()}{return Err("Desktop shell service has an unexpected owner".into());}
    let pid=pid.to_string();
    let mut call=vec!["ipc","--pid",pid.as_str()];
    call.extend(args.action.iter().map(String::as_str));
    command(&args.quickshell,&call,30,false).map(|(code,_)|code)
}
fn main(){
    match args().and_then(route){
        Ok(code)=>std::process::exit(code),
        Err(error)=>{eprintln!("Desktop action unavailable: {error}");std::process::exit(1);}
    }
}
#[cfg(test)]mod tests{
    use super::*;
    #[test]fn invalid_service_pids_never_dispatch(){
        for value in ["0","","123\n456","-1","１２３"]{assert!(parse_pid(value).is_err());}
        assert_eq!(parse_pid(" 42\n").unwrap(),42);
    }
}
