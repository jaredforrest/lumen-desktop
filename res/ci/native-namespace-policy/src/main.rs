fn main() {
    let mut args = std::env::args().skip(1);
    let operation = args.next().unwrap_or_default();
    let output = match (args.next().as_deref(), args.next(), args.next()) {
        (Some("--output"), Some(path), None) => path,
        _ => {
            eprintln!("Usage: lumen-native-namespace-policy prepare|cleanup --output PATH");
            std::process::exit(2);
        }
    };
    if let Err(error) = lumen_native_namespace_policy::run(&operation, &output) {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
