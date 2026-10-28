//! Immutable provider boundary, bounded status and exact literal dispatch.
use serde_json::{Value, json};
use std::env;
use std::fs;
use std::path::Path;
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

fn invoke(args: &[&str], provider: &str, mode: &str, log: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_lumen-nearby-sharing"))
        .args(args)
        .env("LUMEN_NEARBY_SHARING_PROVIDER", provider)
        .env("NEARBY_FIXTURE_MODE", mode)
        .env("NEARBY_FIXTURE_LOG", log)
        .output()
        .unwrap()
}

fn status(output: &Output) -> Value { serde_json::from_slice(&output.stdout).unwrap() }

#[test]
fn disabled_untrusted_and_provider_boundaries() {
    let fixture = env::var("LUMEN_NEARBY_TEST_PROVIDER").unwrap();
    let unique = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let root = std::env::temp_dir().join(format!("lumen-nearby-test-{}-{unique}", std::process::id()));
    fs::create_dir(&root).unwrap();
    let log = root.join("args");
    assert_eq!(status(&invoke(&["status"], "", "good", &log)), json!({
        "available":false,"active":false,"canSend":false,"detail":"Not configured"
    }));
    assert_eq!(status(&invoke(&["status"], "/tmp/provider", "good", &log))["detail"], "Unavailable");
    assert!(!invoke(&["open"], "", "good", &log).status.success());
    assert!(!invoke(&["send", "/tmp/file"], "", "good", &log).status.success());

    let good = invoke(&["status"], &fixture, "good", &log);
    assert!(good.status.success());
    assert_eq!(status(&good), json!({"available":true,"active":false,"canSend":true,"detail":"On"}));
    for mode in ["malformed", "diagnostic", "overflow", "no-send", "timeout"] {
        let result = invoke(&["status"], &fixture, mode, &log);
        assert!(result.status.success(), "{mode}");
        assert_eq!(status(&result)["detail"], if mode == "no-send" { "On" } else { "Unavailable" }, "{mode}");
    }

    assert!(invoke(&["open"], &fixture, "good", &log).status.success());
    assert_eq!(fs::read_to_string(&log).unwrap(), "open\n");
    let paths = ["/tmp/a file", "/tmp/$(not-a-command)", "/tmp/é"];
    assert!(invoke(&["send", paths[0], paths[1], paths[2]], &fixture, "good", &log).status.success());
    assert_eq!(fs::read_to_string(&log).unwrap(), format!("send\n{}\n{}\n{}\n", paths[0], paths[1], paths[2]));
    fs::remove_file(&log).unwrap();
    assert!(!invoke(&["send", paths[0], paths[1], paths[2]], &fixture, "no-send", &log).status.success());
    assert!(!log.exists());
    assert!(!invoke(&["send", "relative"], &fixture, "good", &log).status.success());
    fs::remove_dir_all(root).unwrap();
}
