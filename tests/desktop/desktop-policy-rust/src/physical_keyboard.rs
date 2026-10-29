//! Private virtual keyboard protocol fixture used by native input scenarios.
use super::*;

pub struct Keyboard {
    child: Child,
    input: std::process::ChildStdin,
    output: BufReader<std::process::ChildStdout>,
    serial: u32,
}
impl Keyboard {
    pub fn start(ctx: &Context, log: &str) -> Result<Self> {
        let tool = ctx.opt.keyboard.as_ref().ok_or("Physical keyboard tool required")?;
        let stderr = File::create(ctx.opt.output.join(log))?;
        let mut child = Command::new(tool).env_clear().envs(&ctx.vars).process_group(0)
            .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(stderr).spawn()?;
        let input = child.stdin.take().ok_or("Keyboard input missing")?;
        let mut output = BufReader::new(child.stdout.take().ok_or("Keyboard output missing")?);
        let mut line = String::new(); output.read_line(&mut line)?;
        need(line.trim()=="ready",format!("Physical keyboard unavailable: {line:?}"))?;
        Ok(Self { child, input, output, serial:0 })
    }
    fn event(&mut self, op: char, value: u32, pressed: u32) -> Result<()> {
        self.serial+=1;
        writeln!(self.input,"{op} {} {value} {pressed}",self.serial)?;
        self.input.flush()?;
        let mut ack=String::new(); self.output.read_line(&mut ack)?;
        need(ack.trim()==format!("ack {}",self.serial),format!("Keyboard event not acknowledged: {ack:?}"))
    }
    pub fn key(&mut self, name: &str) -> Result<()> {
        let code=match name {
            "Escape"=>1,"Tab"=>15,"Return"=>28,"space"=>57,"Right"=>106,"Left"=>105,
            "Up"=>103,"Down"=>108,"Home"=>102,"End"=>107,"PageUp"=>104,"PageDown"=>109,
            _=>return Err(fail(format!("Unsupported physical key {name}")))
        };
        self.event('k',code,1)?;self.event('k',code,0)
    }
    pub fn shift_tab(&mut self) -> Result<()> {
        self.event('m',1,0)?;
        self.key("Tab")?;
        self.event('m',0,0)
    }
    pub fn logo_number(&mut self, number: u8) -> Result<()> {
        need(number == 1 || number == 2, "Only workspaces 1 and 2 are supported")?;
        self.event('m',64,0)?;
        self.event('k',number as u32 + 1,1)?;
        self.event('k',number as u32 + 1,0)?;
        self.event('m',0,0)
    }
    pub fn text(&mut self, value: &str) -> Result<()> {
        for letter in value.chars() {
            let lower=letter.to_ascii_lowercase();
            let code=if letter==' ' {57} else if letter=='-' {12} else {
                [("qwertyuiop",16),("asdfghjkl",30),("zxcvbnm",44)].into_iter()
                    .find_map(|(row,start)|row.find(lower).map(|i|start+i as u32))
                    .ok_or_else(||fail(format!("Unsupported physical text {letter}")))?
            };
            if letter.is_ascii_uppercase() { self.event('m',1,0)?; }
            self.event('k',code,1)?;self.event('k',code,0)?;
            if letter.is_ascii_uppercase() { self.event('m',0,0)?; }
        }
        Ok(())
    }
}
impl Drop for Keyboard {
    fn drop(&mut self) {
        let _=self.child.kill();let _=self.child.wait();
    }
}
