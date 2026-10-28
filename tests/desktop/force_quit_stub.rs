//! Private application-control peer for the Force Quit QML state machine.
use std::env;
use std::fs::OpenOptions;
use std::io::{self, Write};
use std::thread;
use std::time::Duration;

fn main() -> io::Result<()> {
    let args: Vec<String> = env::args().collect();
    let request = args.get(2).expect("request id");
    let action = args.get(3).expect("action");
    let value = args.get(4).expect("value");
    let mut log = OpenOptions::new().create(true).append(true).open(env::var_os("LUMEN_FORCE_QUIT_CALLS").expect("call log"))?;
    writeln!(log, "{action}\t{value}")?;
    if action == "prepare" {
        if value.contains("\"title\":\"slow\"") { thread::sleep(Duration::from_millis(200)); }
        let serial = request.parse::<u128>().expect("numeric request id");
        println!("{{\"requestId\":\"{request}\",\"token\":\"{serial:032x}\",\"windowCount\":1}}");
    } else {
        println!("{{\"requestId\":\"{request}\",\"terminated\":1,\"cancelled\":true}}");
    }
    Ok(())
}
