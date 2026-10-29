//! Read-only display discovery for shared control presentation fixtures.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    if std::env::args().skip(1).collect::<Vec<_>>() != ["displays"] {
        return Err("Only private display discovery is allowed".into());
    }
    println!("{{\"displays\":[{{\"bus\":\"private\",\"name\":\"Private synthetic display with an intentionally long descriptive connection name\",\"level\":0.5,\"maximum\":100}}]}}");
    Ok(())
}
