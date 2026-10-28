//! Exact window and process authority at the private Sway command boundary.
use serde_json::{Value, json};
use std::env;
use std::fs;
use std::io::Write;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};
use std::process::{self, Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

fn swaymsg() -> ! {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.iter().any(|item| item == "get_version") {
        println!("{}", json!({"native_paired_layouts":true}));
    } else if args.iter().any(|item| item == "get_tree") {
        print!("{}", fs::read_to_string(env::var("PAIR_TREE").unwrap()).unwrap());
    } else {
        let mut log = fs::OpenOptions::new().create(true).append(true).open(env::var("PAIR_LOG").unwrap()).unwrap();
        writeln!(log, "{}", args.last().unwrap()).unwrap();
        println!("{}", json!([{"success":true}]));
    }
    process::exit(0)
}
struct Fixture { root: PathBuf, path: String, tree: Value }
impl Fixture {
    fn new() -> Self {
        let nonce = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let root = env::temp_dir().join(format!("lumen-window-pair-{}-{nonce}", process::id()));
        fs::create_dir(&root).unwrap();
        symlink(env::current_exe().unwrap(), root.join("swaymsg")).unwrap();
        let path = format!("{}:{}", root.display(), env::var("PATH").unwrap());
        let tree = json!({"nodes":[
            {"id":10,"pid":100,"app_id":"a","name":"First","type":"con","lumen_pair":{"eligible":true}},
            {"id":20,"pid":200,"app_id":"b","name":"Second","type":"con","lumen_pair":{"eligible":true}}
        ]});
        Self { root, path, tree }
    }
    fn call(&self, request: Value) -> Value {
        fs::write(self.root.join("tree.json"), self.tree.to_string()).unwrap();
        let mut child = Command::new(env!("CARGO_BIN_EXE_lumen-window-pair"))
            .env("PATH", &self.path).env("PAIR_TREE", self.root.join("tree.json"))
            .env("PAIR_LOG", self.root.join("commands"))
            .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap();
        writeln!(child.stdin.take().unwrap(), "{request}").unwrap();
        let output = child.wait_with_output().unwrap();
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        serde_json::from_slice(&output.stdout).unwrap()
    }
    fn commands(&self) -> Vec<String> {
        fs::read_to_string(self.root.join("commands")).unwrap_or_default().lines().map(str::to_owned).collect()
    }
}
impl Drop for Fixture { fn drop(&mut self) { let _ = fs::remove_dir_all(&self.root); } }

fn main() {
    if Path::new(&env::args().next().unwrap()).file_name().is_some_and(|name| name == "swaymsg") { swaymsg(); }
    let fixture = Fixture::new();
    assert_eq!(fixture.call(json!({"action":"create","id":10,"pid":100,"partner":20,"partnerPid":200,"side":"right"})), json!({"ok":true}));
    assert_eq!(fixture.commands(), ["lumen_pair create 20 10"]);

    let mut fixture = Fixture::new();
    fixture.tree["nodes"][0]["app_id"] = Value::Null;
    fixture.tree["nodes"][0]["window_properties"] = json!({"class":"LegacyEditor"});
    assert_eq!(fixture.call(json!({"action":"status","id":10}))["source"]["app"], "LegacyEditor");
    fixture.call(json!({"action":"create","id":10,"pid":100,"partner":20,"partnerPid":200,"side":"left"}));
    assert_eq!(fixture.commands(), ["lumen_pair create 10 20"]);

    let mut fixture = Fixture::new();
    fixture.tree["nodes"].as_array_mut().unwrap().remove(0);
    assert!(fixture.call(json!({"action":"leave","id":10,"pid":100}))["error"].as_str().unwrap().contains("captured window"));
    assert!(fixture.commands().is_empty());
    fixture.tree["nodes"].as_array_mut().unwrap().insert(0,
        json!({"id":10,"pid":100,"app_id":"a","type":"con","lumen_pair":{"eligible":true}}));
    for request in [json!({"action":"swap","id":10,"pid":101}),
        json!({"action":"replace","id":10,"pid":100,"partner":20,"partnerPid":201})] {
        assert!(fixture.call(request).get("error").is_some());
    }
    assert!(fixture.commands().is_empty());

    for id in [json!(true), json!(0), json!(-1), json!("10; exit"), json!(1_u64<<53)] {
        assert!(fixture.call(json!({"action":"leave","id":id,"pid":100})).get("error").is_some());
    }
    for ratio in [json!(null), json!(true), json!(0), json!(1), json!("0.5; exit")] {
        assert!(fixture.call(json!({"action":"ratio","id":10,"pid":100,"ratio":ratio})).get("error").is_some());
    }
    assert!(fixture.commands().is_empty());

    fixture.tree["nodes"][1]["lumen_pair"]["id"] = json!(30);
    fixture.tree["nodes"].as_array_mut().unwrap().push(json!({"id":40,"pid":400,"app_id":"org.quickshell",
        "name":"Paired Windows","type":"con","lumen_pair":{"eligible":true}}));
    assert_eq!(fixture.call(json!({"action":"status","id":10}))["candidates"], json!([]));
}
