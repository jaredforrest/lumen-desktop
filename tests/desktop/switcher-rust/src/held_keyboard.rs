//! Acknowledged physical input with explicit release, never a timed key hold.
use super::*;
use std::io::Write;
use std::os::fd::AsRawFd;
use std::process::{ChildStdin, ChildStdout};
use std::sync::atomic::{AtomicUsize, Ordering};

fn line(fd:i32,budget:Duration)->Result<String>{
    let deadline=Instant::now()+budget;
    let mut bytes=Vec::new();
    while bytes.len()<128 {
        let remaining=deadline.saturating_duration_since(Instant::now());
        ensure(!remaining.is_zero(),"Physical keyboard acknowledgement timed out")?;
        let mut entry=libc::pollfd{fd,events:libc::POLLIN,revents:0};
        let ready=unsafe{libc::poll(&mut entry,1,remaining.as_millis().max(1).min(i32::MAX as u128) as i32)};
        if ready<0 {
            let error=std::io::Error::last_os_error();
            if error.kind()==std::io::ErrorKind::Interrupted{continue;}
            return Err(error.into());
        }
        if ready==0{continue;}
        let mut byte=0_u8;
        let read=unsafe{libc::read(fd,(&raw mut byte).cast(),1)};
        if read<0 && std::io::Error::last_os_error().kind()==std::io::ErrorKind::Interrupted{continue;}
        ensure(read==1,"Physical keyboard acknowledgement disconnected")?;
        if byte==b'\n'{return Ok(String::from_utf8(bytes)?);}
        bytes.push(byte);
    }
    Err(fail("Physical keyboard acknowledgement exceeds limit"))
}

pub struct Held {
    process:Managed,input:ChildStdin,output:ChildStdout,serial:u32,
    reverse:bool,held:bool,trace:Vec<String>,record:PathBuf,
}
impl Held {
    pub fn start(ctx:&Context,reverse:bool,repeat:usize)->Result<Self>{
        static SERIAL:AtomicUsize=AtomicUsize::new(0);
        let id=SERIAL.fetch_add(1,Ordering::Relaxed);
        let tool=ctx.opt.keyboard.as_ref().ok_or("Installed switcher requires --keyboard")?;
        let log=File::create(ctx.opt.output.join(format!("held-keyboard-{id}.log")))?;
        let child=Command::new(tool).env_clear().envs(&ctx.vars).process_group(0)
            .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::from(log)).spawn()?;
        let mut process=Managed(child);
        let input=process.0.stdin.take().ok_or("Physical keyboard stdin missing")?;
        let output=process.0.stdout.take().ok_or("Physical keyboard stdout missing")?;
        ensure(line(output.as_raw_fd(),Duration::from_secs(2))?=="ready","Physical keyboard not ready")?;
        let mut result=Self{process,input,output,serial:0,reverse,held:true,trace:vec!["ready".into()],record:ctx.opt.output.join(format!("held-keyboard-{id}.json"))};
        result.event('m',64|if reverse{1}else{0},0)?;
        result.event('k',125,1)?;
        for _ in 0..repeat{result.key(15)?;}
        Ok(result)
    }
    fn event(&mut self,op:char,value:u32,pressed:u32)->Result<()>{
        ensure(self.serial<128,"Physical keyboard event budget exhausted")?;
        self.serial+=1;
        writeln!(self.input,"{op} {} {value} {pressed}",self.serial)?;self.input.flush()?;
        let ack=line(self.output.as_raw_fd(),Duration::from_secs(2))?;
        ensure(ack==format!("ack {}",self.serial),format!("Unexpected physical keyboard acknowledgement {ack:?}"))?;
        self.trace.push(format!("{op} {} {value} {pressed}: {ack}",self.serial));
        save(&self.record,&json!({"held":self.held,"events":self.trace}))
    }
    pub fn key(&mut self,code:u32)->Result<()>{self.event('k',code,1)?;self.event('k',code,0)}
    pub fn assert_held(&mut self)->Result<()>{
        ensure(self.held && self.process.0.try_wait()?.is_none(),"Physical modifier owner exited or released before fixture was ready")?;
        // Roundtrip the still-held modifier mask after target/source setup.
        self.event('m',64|if self.reverse{1}else{0},0)
    }
    pub fn release(mut self)->Result<()>{
        self.assert_held()?;
        if self.reverse{self.event('m',64,0)?;}
        self.event('k',125,0)?;
        self.held=false;
        self.event('m',0,0)?;
        Ok(())
    }
}

#[cfg(test)]mod tests{
    use super::*;
    use std::os::unix::net::UnixStream;
    #[test]fn acknowledgements_are_framed_bounded_and_do_not_wait_forever(){
        let (reader,mut writer)=UnixStream::pair().unwrap();
        writer.write_all(b"ack 1\n").unwrap();
        assert_eq!(line(reader.as_raw_fd(),Duration::from_millis(50)).unwrap(),"ack 1");
        writer.write_all(&[b'x';128]).unwrap();
        assert!(line(reader.as_raw_fd(),Duration::from_millis(50)).unwrap_err().to_string().contains("exceeds limit"));
        writer.write_all(b"partial").unwrap();
        let started=Instant::now();
        assert!(line(reader.as_raw_fd(),Duration::from_millis(20)).unwrap_err().to_string().contains("timed out"));
        assert!(started.elapsed()<Duration::from_secs(1));
    }
}
