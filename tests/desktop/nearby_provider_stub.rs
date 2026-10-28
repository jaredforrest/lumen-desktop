//! Process tripwire for the unconfigured sharing state fixture.
fn main() {
    std::fs::write(std::env::var("LUMEN_PROVIDER_MARKER").unwrap(), b"unexpected provider launch").unwrap();
}
