//! Actual shortcut edit transport with immutable module-generated configurations.
use lumen_files_share_menu_fixture::*;
use serde_json::{Value, json};
use std::{
    env, fs,
    io::Write,
    os::unix::fs::symlink,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};
fn load(path: impl AsRef<Path>) -> Result<Value> {
    Ok(serde_json::from_slice(&fs::read(path)?)?)
}
fn store(path: impl AsRef<Path>, value: &Value) -> Result<()> {
    fs::write(path, serde_json::to_vec_pretty(value)?)?;
    Ok(())
}
fn publish(bundle: &Value, config: &Path) -> Result<()> {
    let bundle = load(bundle.as_str().ok_or("Bundle path")?)?;
    fs::create_dir_all(config)?;
    fs::copy(
        bundle["registry"].as_str().ok_or("Registry")?,
        config.join("shortcuts.json"),
    )?;
    fs::copy(
        bundle["editor"].as_str().ok_or("Editor")?,
        config.join("shortcut-editor.json"),
    )?;
    Ok(())
}
fn provider() -> Result<()> {
    let root = PathBuf::from(env::var("SHORTCUT_FIXTURE_ROOT")?);
    let manifest = load(env::var("SHORTCUT_FIXTURE_MANIFEST")?)?;
    let args = env::args().skip(1).collect::<Vec<_>>();
    if args == ["read"] {
        println!("{}", load(root.join("settings.json"))?);
        return Ok(());
    }
    require(
        args.len() == 4 && args[0] == "compare-set" && args[1] == "shortcuts",
        "Unexpected settings invocation",
    )?;
    let expected: Value = serde_json::from_str(&args[2])?;
    if load(root.join("settings.json"))?["shortcuts"] != expected {
        println!("{}", json!({"error":"Concurrent settings change"}));
        std::process::exit(1);
    }
    let value: Value = serde_json::from_str(&args[3])?;
    let calls = root.join("calls.jsonl");
    let count = fs::read_to_string(&calls)
        .unwrap_or_default()
        .lines()
        .count();
    writeln!(
        fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&calls)?,
        "{}",
        json!({"section":"shortcuts","value":value})
    )?;
    let mode = fs::read_to_string(root.join("mode"))?;
    if mode == "reject" || (mode == "rollback-fail" && count > 0) {
        println!("{}", json!({"error":"Disposable deployment refused"}));
        std::process::exit(1);
    }
    let saved = json!({"shortcuts":value});
    store(root.join("settings.json"), &saved)?;
    if mode == "publish" {
        let state = manifest["states"]
            .as_array()
            .ok_or("States")?
            .iter()
            .find(|state| state["settings"] == saved)
            .ok_or("No actual module generation for request")?;
        publish(&state["bundle"], &root.join("config/lumen-desktop"))?;
    }
    println!("{saved}");
    Ok(())
}
fn invoke(
    helper: &str,
    request: &Value,
    values: &std::collections::BTreeMap<String, String>,
) -> Result<Value> {
    let mut child = Command::new(helper)
        .env_clear()
        .envs(values)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    writeln!(child.stdin.take().ok_or("Input")?, "{request}")?;
    let out = child.wait_with_output()?;
    let mut value: Value = serde_json::from_slice(&out.stdout)
        .map_err(|e| format!("Helper JSON: {e}; {}", String::from_utf8_lossy(&out.stderr)))?;
    value["exitSuccess"] = json!(out.status.success());
    Ok(value)
}
fn run(options: &Options) -> Result<()> {
    let manifest_path = immutable(
        options
            .window_source
            .as_ref()
            .ok_or("Immutable module fixture manifest required")?,
    )?;
    let manifest = load(&manifest_path)?;
    let helper = manifest["helper"].as_str().ok_or("Helper")?;
    immutable(Path::new(helper))?;
    let validator = manifest["validator"].as_str().ok_or("Validator")?;
    immutable(Path::new(validator))?;
    let root = &options.output;
    fs::create_dir_all(root.join("bin"))?;
    fs::create_dir_all(root.join("runtime"))?;
    symlink(env::current_exe()?, root.join("bin/lumen-nix-settings"))?;
    symlink(validator, root.join("bin/lumen-shortcuts"))?;
    let mut values = private_environment()?;
    let installed = launcher_environment(&options.launcher)?;
    for key in ["QML_IMPORT_PATH", "QT_PLUGIN_PATH", "LUMEN_DESKTOP_ASSETS"] {
        if let Some(value) = installed.get(key) {
            values.insert(key.into(), value.clone());
        }
    }
    values.insert(
        "FONTCONFIG_FILE".into(),
        options.fontconfig.display().to_string(),
    );
    values.insert(
        "XDG_RUNTIME_DIR".into(),
        root.join("runtime").display().to_string(),
    );
    values.insert(
        "XDG_CONFIG_HOME".into(),
        root.join("config").display().to_string(),
    );
    values.insert(
        "PATH".into(),
        format!(
            "{}:{}",
            root.join("bin").display(),
            installed.get("PATH").ok_or("Installed PATH")?
        ),
    );
    values.insert("SHORTCUT_FIXTURE_ROOT".into(), root.display().to_string());
    values.insert(
        "SHORTCUT_FIXTURE_MANIFEST".into(),
        manifest_path.display().to_string(),
    );
    let config = root.join("config/lumen-desktop");
    let initial = &manifest["states"][0];
    publish(&manifest["readOnly"], &config)?;
    store(root.join("settings.json"), &initial["settings"])?;
    fs::write(root.join("mode"), "publish")?;
    let read_only = invoke(helper, &json!({"action":"status"}), &values)?;
    require(
        read_only["editable"] == false,
        "Default editor must be read-only",
    )?;
    let refused = invoke(
        helper,
        &json!({"action":"set","id":"window.fullscreen.toggle","chord":"Mod4+Control+x","generation":read_only["generation"]}),
        &values,
    )?;
    require(
        refused["exitSuccess"] == false && !root.join("calls.jsonl").exists(),
        "Read-only edit reached settings",
    )?;
    publish(&initial["bundle"], &config)?;
    let mut evidence = vec![json!({"case":"read-only opt-in","response":refused})];
    let request = |action: &str, chord: Value, generation: Value| json!({"action":action,"id":"window.fullscreen.toggle","chord":chord,"generation":generation});
    for (name, req) in [
        (
            "stale generation",
            request("set", json!("Mod4+Control+x"), json!("stale")),
        ),
        (
            "protected pair",
            json!({"action":"disable","id":"default.1","generation":invoke(helper,&json!({"action":"status"}),&values)?["generation"]}),
        ),
        (
            "alias collision",
            request(
                "set",
                json!("Super+Ctrl+Shift+F"),
                invoke(helper, &json!({"action":"status"}), &values)?["generation"].clone(),
            ),
        ),
    ] {
        let response = invoke(helper, &req, &values)?;
        require(
            response["exitSuccess"] == false && !root.join("calls.jsonl").exists(),
            "Invalid edit mutated settings",
        )?;
        evidence.push(json!({"case":name,"response":response}));
    }
    for (action, chord, index) in [
        ("set", json!("Mod4+Control+x"), 1),
        ("disable", Value::Null, 2),
        ("restore", Value::Null, 0),
    ] {
        let current = invoke(helper, &json!({"action":"status"}), &values)?;
        let response = invoke(
            helper,
            &request(action, chord, current["generation"].clone()),
            &values,
        )?;
        require(
            response["exitSuccess"] == true
                && response["message"]
                    .as_str()
                    .is_some_and(|m| m.contains("confirmed")),
            "Actual effective map did not confirm edit",
        )?;
        require(
            load(root.join("settings.json"))? == manifest["states"][index]["settings"],
            "Wrong authoritative section",
        )?;
        let expected = load(
            manifest["states"][index]["bundle"]
                .as_str()
                .ok_or("Bundle")?,
        )?;
        require(
            fs::read(config.join("shortcuts.json"))?
                == fs::read(expected["registry"].as_str().ok_or("Registry")?)?,
            "Effective module generation mismatch",
        )?;
        evidence.push(json!({"case":action,"response":response}));
    }
    let default_bundle = load(initial["bundle"].as_str().unwrap())?;
    let new_bundle = load(manifest["states"][1]["bundle"].as_str().unwrap())?;
    require(
        default_bundle["dispatch"]["Mod4+Control+f"] == new_bundle["dispatch"]["Mod4+Control+x"],
        "Rebinding changed captured command identity",
    )?;
    for mode in ["reject", "no-confirm", "rollback-fail"] {
        fs::write(root.join("mode"), mode)?;
        let _ = fs::remove_file(root.join("calls.jsonl"));
        let current = invoke(helper, &json!({"action":"status"}), &values)?;
        let response = invoke(
            helper,
            &request(
                "set",
                json!("Mod4+Control+x"),
                current["generation"].clone(),
            ),
            &values,
        )?;
        require(
            response["exitSuccess"] == false && response["message"].is_null(),
            "Failed deployment claimed apply",
        )?;
        let calls = fs::read_to_string(root.join("calls.jsonl"))?;
        require(
            calls.lines().count() == if mode == "reject" { 1 } else { 2 },
            "Rollback invocation count changed",
        )?;
        if mode != "rollback-fail" {
            require(
                load(root.join("settings.json"))? == initial["settings"],
                "Previous settings not retained/restored",
            )?;
        } else {
            require(
                response["error"]
                    .as_str()
                    .is_some_and(|m| m.contains("rollback failed")),
                "Rollback uncertainty hidden",
            )?;
        }
        require(
            fs::read(config.join("shortcuts.json"))?
                == fs::read(default_bundle["registry"].as_str().unwrap())?,
            "Failed deployment changed effective map",
        )?;
        evidence.push(json!({"case":mode,"response":response,"calls":calls}));
        store(root.join("settings.json"), &initial["settings"])?;
    }
    fs::write(config.join("shortcuts.json"), "{broken")?;
    let malformed = invoke(helper, &json!({"action":"status"}), &values)?;
    require(
        malformed["available"] == false && malformed["editable"] == false,
        "Malformed map did not fail closed",
    )?;
    evidence.push(json!({"case":"malformed map","response":malformed}));
    publish(&initial["bundle"], &config)?;
    fs::write(root.join("mode"), "publish")?;
    let assets = root.join("assets");
    copy_tree(Path::new("/source/res/lumen-desktop"), &assets)?;
    let state = assets.join("shell/ShortcutEditorState.qml");
    let mut seams = vec![];
    replace_once(
        &state,
        "command:[\"lumen-shortcut-settings\"]",
        &format!("command:[{}]", serde_json::to_string(helper)?),
        "Actual immutable raw helper; only executable resolution substitutes the pinned session wrapper",
        &mut seams,
    )?;
    store(root.join("qml-seams.json"), &json!(seams))?;
    for (name, width, editable) in [
        ("read-only", 320, false),
        ("compact", 320, true),
        ("regular", 520, true),
    ] {
        publish(
            if editable {
                &initial["bundle"]
            } else {
                &manifest["readOnly"]
            },
            &config,
        )?;
        store(root.join("settings.json"), &initial["settings"])?;
        let qml = root.join(format!("{name}.qml"));
        let screenshot = root.join(format!("{name}.png"));
        fs::write(
            &qml,
            include_str!("../../shortcut_editor_fixture.qml")
                .replace("WIDTH", &width.to_string())
                .replace("EDITABLE", if editable { "true" } else { "false" })
                .replace("SCREENSHOT", &screenshot.display().to_string()),
        )?;
        let result = Command::new("timeout")
            .args(["20s", "quickshell", "-p"])
            .arg(qml)
            .env_clear()
            .envs(&values)
            .output()?;
        let log = String::from_utf8_lossy(&result.stdout).to_string()
            + &String::from_utf8_lossy(&result.stderr);
        fs::write(root.join(format!("{name}.log")), &log)?;
        require(
            result.status.success()
                && log.contains(if editable {
                    "SHORTCUT_EDITOR_ACTUAL_PASS"
                } else {
                    "SHORTCUT_EDITOR_READONLY_PASS"
                }),
            &format!("Actual shortcut QML failed: {log}"),
        )?;
    }
    store(
        root.join("result.json"),
        &json!({"passed":true,"groups":evidence.len(),"cases":evidence,"moduleManifest":manifest_path,"scope":"Real Rust edit process and validator; real immutable Nix module generations; disposable settings/deployment transport only, no host activation"}),
    )?;
    Ok(())
}
fn main() -> Result<()> {
    if env::args().next().as_deref().is_some_and(|name| {
        Path::new(name)
            .file_name()
            .is_some_and(|n| n == "lumen-nix-settings")
    }) {
        return provider();
    }
    let options = options()?;
    if !options.inside {
        return outer(&options);
    }
    if options.run {
        run(&options)
    } else {
        inner_with_timeout(&options, 60)
    }
}
