//! Actual shortcut edit transport with immutable module-generated configurations.
use lumen_files_share_menu_fixture::*;
use serde_json::{Value, json};
use std::{
    env, fs,
    io::{Read, Write},
    os::unix::{fs::symlink, net::UnixListener},
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
    for (field, name) in [
        ("registry", "shortcuts.json"),
        ("editor", "shortcut-editor.json"),
    ] {
        let next = config.join(format!(".{name}.next"));
        fs::write(
            &next,
            fs::read(bundle[field].as_str().ok_or("Generation path")?)?,
        )?;
        fs::rename(next, config.join(name))?;
    }
    Ok(())
}
fn compositor(root: &Path) -> Result<PathBuf> {
    let socket = root.join("runtime/disposable-compositor.sock");
    let listener = UnixListener::bind(&socket)?;
    let root = root.to_owned();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else {
                break;
            };
            let _ = stream.set_read_timeout(Some(std::time::Duration::from_millis(500)));
            let mut header = [0u8; 14];
            if stream.read_exact(&mut header).is_err() {
                continue;
            }
            let generation = fs::read_to_string(root.join("live-generation")).unwrap_or_default();
            let witness = load(root.join("manifest.json"))
                .ok()
                .and_then(|v| v["witness"].as_str().map(str::to_owned))
                .unwrap_or_default();
            let mut value = json!({"config":format!("exec_always --no-startup-id {witness} {generation}\n"),"lumen_config_valid":true});
            match fs::read_to_string(root.join("runtime-mode"))
                .unwrap_or_default()
                .as_str()
            {
                "old-provider" => {
                    value.as_object_mut().unwrap().remove("lumen_config_valid");
                }
                "invalid-config" => value["lumen_config_valid"] = json!(false),
                _ => {}
            }
            let bytes = value.to_string().into_bytes();
            header[..6].copy_from_slice(b"i3-ipc");
            header[6..10].copy_from_slice(&(bytes.len() as u32).to_le_bytes());
            header[10..].copy_from_slice(&9u32.to_le_bytes());
            let _ = stream.write_all(&header);
            let _ = stream.write_all(&bytes);
        }
    });
    Ok(socket)
}
fn activate(
    bundle: &Value,
    root: &Path,
    manifest: &Value,
    values: &std::collections::BTreeMap<String, String>,
) -> Result<()> {
    let bundle = load(bundle.as_str().ok_or("Bundle")?)?;
    let source = bundle["registry"].as_str().ok_or("Registry")?;
    fs::write(root.join("live-generation"), source)?;
    let output = Command::new(manifest["witness"].as_str().ok_or("Witness")?)
        .arg(source)
        .env_clear()
        .envs(values)
        .output()?;
    require(
        output.status.success(),
        &format!(
            "Actual runtime witness failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ),
    )
}
fn provider() -> Result<()> {
    let root = PathBuf::from(env::var("SHORTCUT_FIXTURE_ROOT")?);
    let manifest = load(env::var("SHORTCUT_FIXTURE_MANIFEST")?)?;
    let args = env::args().skip(1).collect::<Vec<_>>();
    if args == ["read"] {
        if fs::read_to_string(root.join("mode"))? == "flood-read" {
            println!("{}", "x".repeat(600 * 1024));
            return Ok(());
        }
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
    if mode == "publish" || mode == "malformed-mutation" {
        let state = manifest["states"]
            .as_array()
            .ok_or("States")?
            .iter()
            .find(|state| state["settings"] == saved)
            .ok_or("No actual module generation for request")?;
        publish(&state["bundle"], &root.join("config/lumen-desktop"))?;
        activate(&state["bundle"], &root, &manifest, &env::vars().collect())?;
    }
    if mode == "flood-mutation" {
        println!("{}", "x".repeat(600 * 1024));
        return Ok(());
    }
    if mode == "malformed-mutation" {
        println!("partial deployment response");
        return Ok(());
    }
    println!("{saved}");
    Ok(())
}
fn invoke(
    helper: &str,
    request: &Value,
    values: &std::collections::BTreeMap<String, String>,
) -> Result<Value> {
    // timeout owns and reaps the helper; wait_with_output drains pipes while it
    // runs, so the observer cannot itself deadlock on a complete catalog reply.
    let mut child = Command::new("timeout")
        .args(["--kill-after=1s", "8s", helper])
        .env_clear()
        .envs(values)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    writeln!(child.stdin.take().ok_or("Input")?, "{request}")?;
    let out = child.wait_with_output()?;
    require(
        !matches!(out.status.code(), Some(124 | 137)),
        "Edit helper exceeded finite acceptance budget; killed and reaped",
    )?;
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
    store(root.join("manifest.json"), &manifest)?;
    let socket = compositor(root)?;
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
    values.insert("SWAYSOCK".into(), socket.display().to_string());
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
    activate(&initial["bundle"], root, &manifest, &values)?;
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
    let security = invoke(
        helper,
        &request(
            "set",
            json!("Super+Ctrl+F12"),
            invoke(helper, &json!({"action":"status"}), &values)?["generation"].clone(),
        ),
        &values,
    )?;
    require(
        security["exitSuccess"] == false && !root.join("calls.jsonl").exists(),
        "Security reservation was silently overridden",
    )?;
    evidence.push(json!({"case":"security reservation","response":security}));
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
    for mode in ["flood-read", "flood-mutation", "malformed-mutation"] {
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
            "Uncertain transport claimed successful application",
        )?;
        let calls = fs::read_to_string(root.join("calls.jsonl")).unwrap_or_default();
        require(
            calls.lines().count() == usize::from(mode != "flood-read"),
            "Uncertain mutation triggered blind rollback",
        )?;
        require(
            load(root.join("settings.json"))?
                == if mode == "flood-read" {
                    initial["settings"].clone()
                } else {
                    manifest["states"][1]["settings"].clone()
                },
            "Uncertain transport lost authoritative outcome",
        )?;
        let expected_bundle = if mode == "malformed-mutation" {
            &new_bundle
        } else {
            &default_bundle
        };
        require(
            fs::read(config.join("shortcuts.json"))?
                == fs::read(expected_bundle["registry"].as_str().unwrap())?,
            "Uncertain transport overwrote published generation",
        )?;
        evidence.push(json!({"case":mode,"response":response,"calls":calls}));
        store(root.join("settings.json"), &initial["settings"])?;
        publish(&initial["bundle"], &config)?;
        activate(&initial["bundle"], root, &manifest, &values)?;
    }
    fs::write(config.join("shortcuts.json"), "{broken")?;
    let malformed = invoke(helper, &json!({"action":"status"}), &values)?;
    require(
        malformed["available"] == false && malformed["editable"] == false,
        "Malformed map did not fail closed",
    )?;
    evidence.push(json!({"case":"malformed map","response":malformed}));
    publish(&initial["bundle"], &config)?;
    activate(&initial["bundle"], root, &manifest, &values)?;
    for (name, registry_index, catalog_index) in [
        ("disabled catalog with old map", 0, 2),
        ("restored catalog with disabled map", 2, 0),
    ] {
        let registry_bundle = load(
            manifest["states"][registry_index]["bundle"]
                .as_str()
                .unwrap(),
        )?;
        let catalog_bundle = load(
            manifest["states"][catalog_index]["bundle"]
                .as_str()
                .unwrap(),
        )?;
        fs::write(
            config.join("shortcuts.json"),
            fs::read(registry_bundle["registry"].as_str().unwrap())?,
        )?;
        fs::write(
            config.join("shortcut-editor.json"),
            fs::read(catalog_bundle["editor"].as_str().unwrap())?,
        )?;
        let response = invoke(helper, &json!({"action":"status"}), &values)?;
        require(
            response["available"] == false && response["editable"] == false,
            "Skewed generation pair was accepted",
        )?;
        evidence.push(json!({"case":name,"response":response}));
    }
    publish(&initial["bundle"], &config)?;
    for mode in ["old-provider", "invalid-config"] {
        fs::write(root.join("runtime-mode"), mode)?;
        let response = invoke(helper, &json!({"action":"status"}), &values)?;
        require(
            response["runtimeConfirmed"] == false && response["editable"] == false,
            "Unvalidated compositor was advertised as active",
        )?;
        evidence.push(json!({"case":mode,"response":response}));
    }
    fs::write(root.join("runtime-mode"), "")?;
    let record = root.join("runtime/lumen-desktop/shortcuts-runtime.json");
    let mut stale = load(&record)?;
    stale["epoch"]["start"] = json!("another-process-epoch");
    store(&record, &stale)?;
    let response = invoke(helper, &json!({"action":"status"}), &values)?;
    require(
        response["runtimeConfirmed"] == false && response["editable"] == false,
        "Stale manager epoch was accepted",
    )?;
    evidence.push(json!({"case":"stale process epoch","response":response}));
    activate(&initial["bundle"], root, &manifest, &values)?;
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(&record, fs::Permissions::from_mode(0o644))?;
    let response = invoke(helper, &json!({"action":"status"}), &values)?;
    require(
        response["runtimeConfirmed"] == false,
        "Public runtime record accepted",
    )?;
    evidence.push(json!({"case":"unsafe witness permissions","response":response}));
    fs::set_permissions(&record, fs::Permissions::from_mode(0o600))?;
    for linked in [false, true] {
        let backup = record.with_extension("original");
        fs::rename(&record, &backup)?;
        if linked {
            std::os::unix::fs::symlink(&backup, &record)?;
        } else {
            fs::hard_link(&backup, &record)?;
        }
        let response = invoke(helper, &json!({"action":"status"}), &values)?;
        require(
            response["runtimeConfirmed"] == false && response["editable"] == false,
            "Linked runtime record accepted",
        )?;
        evidence.push(json!({"case":if linked {"symlink witness"} else {"multiply linked witness"},"response":response}));
        fs::remove_file(&record)?;
        fs::rename(&backup, &record)?;
        activate(&initial["bundle"], root, &manifest, &values)?;
    }
    // Descriptor-level broken-file cases must return without waiting for a FIFO
    // peer or allocating the complete oversized declaration.
    for name in ["shortcut-editor.json", "shortcuts.json"] {
        for fifo in [true, false] {
            let path = config.join(name);
            fs::remove_file(&path)?;
            if fifo {
                use std::os::unix::ffi::OsStrExt;
                let c = std::ffi::CString::new(path.as_os_str().as_bytes())?;
                require(
                    unsafe { libc::mkfifo(c.as_ptr(), 0o600) } == 0,
                    "FIFO setup",
                )?;
            } else {
                fs::write(&path, vec![b' '; 700 * 1024])?;
            }
            let response = invoke(helper, &json!({"action":"status"}), &values)?;
            require(
                response["available"] == false && response["editable"] == false,
                "Broken declaration accepted",
            )?;
            evidence.push(json!({"case":format!("{name} {}", if fifo {"FIFO"} else {"oversize"}),"response":response}));
            publish(&initial["bundle"], &config)?;
        }
    }
    for (field, name) in [
        ("editor", "shortcut-editor.json"),
        ("registry", "shortcuts.json"),
    ] {
        let path = config.join(name);
        fs::remove_file(&path)?;
        symlink(
            default_bundle[field]
                .as_str()
                .ok_or("Immutable declaration")?,
            &path,
        )?;
    }
    let response = invoke(helper, &json!({"action":"status"}), &values)?;
    require(
        response["runtimeConfirmed"] == true && response["editable"] == true,
        "Declarative store symlink was rejected",
    )?;
    evidence.push(json!({"case":"declarative immutable symlink targets","response":response}));
    publish(&initial["bundle"], &config)?;
    let backup = record.with_extension("safe");
    fs::rename(&record, &backup)?;
    use std::os::unix::ffi::OsStrExt;
    let c = std::ffi::CString::new(record.as_os_str().as_bytes())?;
    require(
        unsafe { libc::mkfifo(c.as_ptr(), 0o600) } == 0,
        "Witness FIFO setup",
    )?;
    let response = invoke(helper, &json!({"action":"status"}), &values)?;
    require(
        response["runtimeConfirmed"] == false && response["editable"] == false,
        "FIFO witness accepted",
    )?;
    evidence.push(json!({"case":"FIFO witness","response":response}));
    fs::remove_file(&record)?;
    fs::rename(backup, &record)?;
    let transaction = root.join("runtime/lumen-shortcut-settings");
    let lock = transaction.join("transaction.lock");
    use std::os::fd::AsRawFd;
    let held = fs::OpenOptions::new().read(true).write(true).open(&lock)?;
    require(
        unsafe { libc::flock(held.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0,
        "Fixture lock contention setup",
    )?;
    let response = invoke(helper, &json!({"action":"status"}), &values)?;
    require(
        response["exitSuccess"] == false
            && response["editable"] == false
            && response["error"]
                .as_str()
                .is_some_and(|s| s.contains("in progress")),
        "Contended status did not fail promptly",
    )?;
    evidence.push(json!({"case":"nonblocking transaction contention","response":response}));
    drop(held);
    for directory in [false, true] {
        let path = if directory { &transaction } else { &lock };
        fs::set_permissions(
            path,
            fs::Permissions::from_mode(if directory { 0o777 } else { 0o666 }),
        )?;
        let response = invoke(helper, &json!({"action":"status"}), &values)?;
        require(
            response["exitSuccess"] == false && response["editable"] == false,
            "Unsafe transaction metadata accepted",
        )?;
        evidence.push(json!({"case":if directory {"unsafe transaction directory"} else {"unsafe transaction lock"},"response":response}));
        fs::set_permissions(
            path,
            fs::Permissions::from_mode(if directory { 0o700 } else { 0o600 }),
        )?;
    }
    for directory in [false, true] {
        let path = if directory { &transaction } else { &lock };
        let original = path.with_extension("original");
        fs::rename(path, &original)?;
        symlink(&original, path)?;
        let response = invoke(helper, &json!({"action":"status"}), &values)?;
        require(
            response["exitSuccess"] == false && response["editable"] == false,
            "Linked transaction path accepted",
        )?;
        evidence.push(json!({"case":if directory {"linked transaction directory"} else {"linked transaction lock"},"response":response}));
        fs::remove_file(path)?;
        fs::rename(original, path)?;
    }
    store(
        root.join("process-result.json"),
        &json!({"passed":true,"cases":evidence,"groups":evidence.len(),"scope":"Real edit helper, actual validator and immutable module configurations; disposable deployment provider"}),
    )?;
    fs::write(root.join("mode"), "publish")?;
    let assets = root.join("assets");
    copy_tree(Path::new("/source/res/lumen-desktop"), &assets)?;
    let palette = load(assets.join("appearance/palette.json"))?;
    let appearance = root.join("appearance.json");
    store(
        &appearance,
        &json!({"font":palette["font"],"palette":palette["dark"],"preferences":{"textScale":1}}),
    )?;
    values.insert(
        "LUMEN_APPEARANCE_STATE".into(),
        appearance.display().to_string(),
    );
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
        if editable {
            activate(&initial["bundle"], root, &manifest, &values)?;
        }
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
