use image::{Rgb, RgbImage};
use serde_json::{Value, json};
use std::env;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::fs::symlink;
use std::path::Path;
use std::process::{self, Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

fn stub(name: &str) {
    let root = Path::new(&env::var("WALLPAPER_FIXTURE_ROOT").unwrap()).to_owned();
    if name == "lumen-nix-settings" {
        let mut args = env::args().skip(1);
        let action = args.next().unwrap();
        let path = root.join("settings.json");
        let mut saved: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        if action == "read" {
            println!("{saved}");
        } else {
            assert_eq!(action, "set");
            assert_eq!(args.next().as_deref(), Some("wallpaper"));
            if root.join("settings.deny").exists() {
                eprintln!("Fixture persistence refused");
                process::exit(1);
            }
            saved["wallpaper"] = serde_json::from_str(&args.next().unwrap()).unwrap();
            fs::write(root.join("settings.new"), saved.to_string()).unwrap();
            fs::rename(root.join("settings.new"), &path).unwrap();
            println!("{saved}");
        }
    } else {
        let args: Vec<_> = env::args().skip(1).collect();
        if args.iter().any(|arg| arg == "get_outputs") {
            print!("{}", fs::read_to_string(root.join("outputs.json")).unwrap());
        } else {
            let mut log = OpenOptions::new().append(true).create(true).open(root.join("actions.log")).unwrap();
            writeln!(log, "{}", args.join(" ")).unwrap();
            println!("{}", json!([{"success":true}]));
        }
    }
}

fn invoke(root: &Path, request: Value) -> Value {
    let mut child = Command::new(env!("CARGO_BIN_EXE_lumen-wallpaper-settings"))
        .env("PATH", format!("{}:{}", root.display(), env::var("PATH").unwrap()))
        .env("WALLPAPER_FIXTURE_ROOT", root)
        .env("XDG_RUNTIME_DIR", root)
        .env("LUMEN_WALLPAPER_FALLBACK", root.join("red ; image.png"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    writeln!(child.stdin.take().unwrap(), "{request}").unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    serde_json::from_slice(&output.stdout).unwrap()
}

fn output(name: &str, serial: &str, scale: f64) -> Value {
    json!({"name":name,"make":"Fixture","model":"Panel","serial":serial,"active":true,"scale":scale})
}

fn main() {
    let invoked = env::args().next().unwrap();
    let name = Path::new(&invoked).file_name().unwrap().to_str().unwrap();
    if name == "lumen-nix-settings" || name == "swaymsg" {
        stub(name);
        return;
    }
    let unique = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let root = env::temp_dir().join(format!("lumen-wallpaper-test-{}-{unique}", process::id()));
    fs::create_dir(&root).unwrap();
    let self_binary = env::current_exe().unwrap();
    symlink(&self_binary, root.join("lumen-nix-settings")).unwrap();
    symlink(&self_binary, root.join("swaymsg")).unwrap();
    fs::write(root.join("settings.json"), r#"{"inputs":{}}"#).unwrap();
    let red = root.join("red ; image.png");
    let blue = root.join("blue.jpg");
    RgbImage::from_pixel(100, 80, Rgb([255, 0, 0])).save(&red).unwrap();
    RgbImage::from_pixel(80, 120, Rgb([0, 0, 255])).save(&blue).unwrap();
    let outputs = root.join("outputs.json");
    fs::write(&outputs, json!([output("DP-1", "123", 1.0), output("DP-2", "456", 2.0)]).to_string()).unwrap();
    let red_text = red.to_str().unwrap();
    let blue_text = blue.to_str().unwrap();
    assert_eq!(invoke(&root, json!({"action":"set","scope":"default","image":red}))["available"], true);
    let identity = "display:[\"Fixture\",\"Panel\",\"456\"]";
    assert_eq!(invoke(&root, json!({"action":"set","scope":"display","identity":identity,"image":blue}))["available"], true);
    let state = invoke(&root, json!({"action":"status"}));
    assert_eq!(state["outputs"][0]["image"], red_text);
    assert_eq!(state["outputs"][1]["image"], blue_text);
    assert_eq!(state["outputs"][0]["metadata"]["format"], "PNG");
    assert_eq!(state["outputs"][1]["metadata"]["format"], "JPEG");
    let browse = invoke(&root, json!({"action":"browse","path":red}));
    assert_eq!(browse["selected"], red_text);
    assert!(browse["entries"].as_array().unwrap().iter().any(|row| row["path"] == blue_text));
    assert!(invoke(&root, json!({"action":"browse","path":"relative"})).get("error").is_some());
    fs::write(&outputs, json!([output("DP-1", "123", 1.0)]).to_string()).unwrap();
    assert_eq!(invoke(&root, json!({"action":"status"}))["disconnected"].as_array().unwrap().len(), 1);
    assert!(invoke(&root, json!({"action":"set","scope":"display","identity":"connector:missing","image":red}))["error"].as_str().unwrap().contains("disconnected"));
    fs::write(&outputs, json!([output("DP-1", "123", 1.0), output("renamed", "456", 1.5)]).to_string()).unwrap();
    assert_eq!(invoke(&root, json!({"action":"status"}))["outputs"][1]["image"], blue_text);
    fs::remove_file(&blue).unwrap();
    assert_eq!(invoke(&root, json!({"action":"status"}))["outputs"][1]["fallback"], true);
    assert_eq!(invoke(&root, json!({"action":"set","scope":"display","identity":identity,"image":null}))["available"], true);
    assert_eq!(invoke(&root, json!({"action":"status"}))["outputs"][1]["image"], red_text);
    fs::write(root.join("settings.deny"), "").unwrap();
    let before = fs::read_to_string(root.join("settings.json")).unwrap();
    assert!(invoke(&root, json!({"action":"set","scope":"default","image":null})).get("error").is_some());
    assert_eq!(fs::read_to_string(root.join("settings.json")).unwrap(), before);
    for path in ["https://example.invalid/x.png", "relative.png", "/bad\nname"] {
        assert!(invoke(&root, json!({"action":"set","scope":"default","image":path})).get("error").is_some());
    }
    assert!(!fs::read_to_string(root.join("actions.log")).unwrap().is_empty());
    fs::remove_dir_all(root).unwrap();
}
