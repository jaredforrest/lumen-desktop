use lumen_browser_workflows_fixture::boundary::{Options, run_outer, validate_inside};
use lumen_browser_workflows_fixture::native;
use std::env;
fn main() {
    let result = (|| -> lumen_browser_workflows_fixture::Result<i32> {
        let options = Options::parse(env::args().skip(1))?;
        if options.inside {
            validate_inside(&options.output)?;
            native::run(options)?;
            Ok(0)
        } else {
            run_outer(&options)
        }
    })();
    match result {
        Ok(code) => std::process::exit(code),
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1)
        }
    }
}
