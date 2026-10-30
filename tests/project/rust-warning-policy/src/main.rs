#[cfg(not(feature = "preserved"))]
compile_error!("The owned builder lost an existing compiler flag");

fn main() {
    #[cfg(feature = "warning")]
    let policy_fixture_unused = 42;
    println!(
        "owned Rust warning policy fixture {}",
        owned_warning_dependency::value()
    );
}
