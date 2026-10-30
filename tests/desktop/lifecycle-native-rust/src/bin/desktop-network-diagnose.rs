//! Unrelated network seam for disposable lifecycle sessions; never probes hosts.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    if std::env::args().skip(1).collect::<Vec<_>>() != ["--cached"] {
        return Err("Lifecycle network fixture supports cached status only".into());
    }
    println!(
        "{{\"status\":\"unknown\",\"reason\":\"fixture-unavailable\",\"message\":\"Network monitoring is unavailable in this disposable fixture.\"}}"
    );
    Ok(())
}
