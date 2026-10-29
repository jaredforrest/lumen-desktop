use std::env;
fn main() {
    if env::args().any(|arg|arg=="subscribe") {println!("{{\"success\":true}}");}
    else {println!("{}",env::var("MOCK_SWAY_TREE").unwrap());}
}
