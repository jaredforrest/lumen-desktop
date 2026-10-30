use lumen_shortcut_settings::runtime;
fn main() {
    let mut args = std::env::args().skip(1);
    let first = args.next();
    let result = (|| -> Result<serde_json::Value, String> {
        let first = first.ok_or("Shortcut generation required")?;
        let check = first == "--check";
        let path = if check {
            args.next().ok_or("Shortcut generation required")?
        } else {
            first
        };
        if args.next().is_some() {
            return Err("Unexpected witness argument".into());
        }
        let source = std::path::Path::new(&path);
        if check {
            runtime::confirmed(source)
        } else {
            runtime::publish(source)
        }
    })();
    match result {
        Ok(epoch) => println!("{}", serde_json::json!({"confirmed":true,"epoch":epoch})),
        Err(error) => {
            eprintln!("Shortcut runtime remains unconfirmed: {error}");
            std::process::exit(1);
        }
    }
}
