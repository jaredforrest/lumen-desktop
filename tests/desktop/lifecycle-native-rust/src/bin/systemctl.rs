use std::env;
use std::fs::OpenOptions;
use std::io::Write;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let marker = env::var("LUMEN_TEST_ACTION_MARKER")?;
    let error = std::path::PathBuf::from(format!("{marker}.error"));
    let mut file = OpenOptions::new().create(true).append(true).open(marker)?;
    writeln!(
        file,
        "{}",
        env::args().skip(1).collect::<Vec<_>>().join(" ")
    )?;
    if let Ok(message)=std::fs::read_to_string(error) {eprintln!("{message}");std::process::exit(1);}
    Ok(())
}
