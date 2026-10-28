//! Fixed private application-control peer for the native Force Quit surface.
use serde_json::json;
use std::env;
use std::fs::OpenOptions;
use std::io::{self, Write};

fn main() -> io::Result<()> {
    let args = env::args().collect::<Vec<_>>();
    let request = args
        .get(2)
        .ok_or_else(|| io::Error::other("Missing request id"))?;
    let action = args
        .get(3)
        .ok_or_else(|| io::Error::other("Missing action"))?;
    let log = env::var_os("LUMEN_FORCE_QUIT_CALLS")
        .ok_or_else(|| io::Error::other("Missing private call log"))?;
    let mut output = OpenOptions::new().create(true).append(true).open(log)?;
    writeln!(output, "{}", json!({"request":request,"action":action}))?;
    println!(
        "{}",
        json!({"requestId":request,"token":"00000000000000000000000000000000","windowCount":2,"cancelled":true})
    );
    Ok(())
}
