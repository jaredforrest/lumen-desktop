//! No hardware writes in the private live appearance fixture.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    if std::env::args().skip(1).collect::<Vec<_>>() != ["displays"] {
        return Err("Only read-only display discovery is allowed".into());
    }
    println!("{{\"displays\":[]}}");
    Ok(())
}
