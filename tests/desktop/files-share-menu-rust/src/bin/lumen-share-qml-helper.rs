//! Disposable registry seam: records intent and reports synthetic outcomes only.
use serde_json::{Value, json};
use std::{fs, time::Duration};
fn main() {
    let args: Vec<_> = std::env::args().skip(1).collect();
    match args.as_slice() {
        [a] if a == "list" => println!(
            "{}",
            json!({"schema":1,"destinations":[{"id":"fixture","label":"Synthetic available destination with a deliberately long presentation label","state":"ready","detail":"Select this synthetic endpoint to exercise explicit consent and inspectable results without sending user files or contacting any external service.","fingerprint":"a".repeat(64)},{"id":"offline","label":"Synthetic offline destination","state":"offline","detail":"Open the provider application, then refresh destinations.","fingerprint":"b".repeat(64)}]})
        ),
        [a, data] if a == "invoke" => {
            let path = std::env::var_os("SHARE_FIXTURE_REQUEST_LOG").unwrap();
            let mut rows: Vec<Value> = fs::read(&path)
                .map(|b| serde_json::from_slice(&b).unwrap())
                .unwrap_or_default();
            let request: Value = serde_json::from_str(data).unwrap();
            rows.push(request.clone());
            fs::write(path, serde_json::to_vec(&rows).unwrap()).unwrap();
            std::thread::sleep(Duration::from_millis(150));
            let state = ["offline", "cancelled", "error", "handoff"][rows.len() - 1];
            let mut result = json!({"state":state,"detail":format!("Synthetic {state} outcome. Open the provider application or refresh destinations.")});
            if state == "handoff" {
                result["command"] = json!([
                    "/nix/store/synthetic-no-execution-provider",
                    "send",
                    request["files"][0]["path"]
                ]);
            }
            println!("{result}")
        }
        _ => std::process::exit(2),
    }
}
