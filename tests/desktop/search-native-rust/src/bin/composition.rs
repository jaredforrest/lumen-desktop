//! Actual Wayland preedit/commit and physical key ownership in the owned launcher.
use lumen_search_native_fixture::{Result, Session, copy_tree, ensure, s, sha_file, wait};
use serde_json::{Value, json};
use std::{env, fs, io::Write, path::PathBuf};
fn process_start(pid: u32) -> Option<String> {
    fs::read_to_string(format!("/proc/{pid}/stat"))
        .ok()?
        .rsplit_once(") ")?
        .1
        .split_whitespace()
        .nth(19)
        .map(str::to_owned)
}

fn state(f: &Session, pid: u32) -> Result<Value> {
    let value: Value = serde_json::from_str(&f.call(
        "quickshell",
        &[
            "ipc",
            "--pid",
            &pid.to_string(),
            "call",
            "composition",
            "status",
        ],
    )?)?;
    fs::write(
        f.out.join("last-state.json"),
        serde_json::to_vec_pretty(&value)?,
    )?;
    Ok(value)
}
fn reset(f: &Session, pid: u32) -> Result<()> {
    f.call(
        "quickshell",
        &[
            "ipc",
            "--pid",
            &pid.to_string(),
            "call",
            "composition",
            "reset",
        ],
    )?;
    wait("focused empty actual editor", 10, || {
        let v = state(f, pid)?;
        Ok((v["focus"] == true && v["query"] == "" && v["selected"] == 0).then_some(()))
    })
}
fn send(f: &mut Session, child: usize, log: &str, serial: &mut u32, command: &str) -> Result<()> {
    *serial += 1;
    f.write(child, &format!("{serial} {command}\n"))?;
    wait("input protocol roundtrip acknowledgement", 5, || {
        Ok(fs::read_to_string(f.out.join(log))?
            .lines()
            .any(|line| line.starts_with(&format!("ack {serial}")))
            .then_some(()))
    })
}
fn key(f: &mut Session, keyboard: usize, serial: &mut u32, code: u32) -> Result<()> {
    for pressed in [1, 0] {
        send_keyboard(f, keyboard, serial, &format!("k {code} {pressed}"))?;
    }
    Ok(())
}
fn send_keyboard(f: &mut Session, child: usize, serial: &mut u32, command: &str) -> Result<()> {
    *serial += 1;
    let (op, rest) = command.split_once(' ').ok_or("keyboard command")?;
    let mut transcript = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(f.out.join("keyboard-commands.jsonl"))?;
    writeln!(
        transcript,
        "{}",
        json!({"serial":serial,"operation":op,"arguments":rest})
    )?;
    f.write(child, &format!("{op} {serial} {rest}\n"))?;
    wait("physical keyboard roundtrip", 5, || {
        Ok(fs::read_to_string(f.out.join("keyboard.log"))?
            .lines()
            .any(|line| line == format!("ack {serial}"))
            .then_some(()))
    })
}
fn run() -> Result<()> {
    let mut args = env::args_os().skip(1);
    let mut launcher = None;
    let mut sway = None;
    let mut keyboard = None;
    let mut output = None;
    let mut driver = None;
    let mut font = None;
    let mut effects = false;
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("--launcher") => launcher = Some(PathBuf::from(args.next().ok_or("launcher")?)),
            Some("--sway") => sway = Some(PathBuf::from(args.next().ok_or("sway")?)),
            Some("--keyboard") => keyboard = Some(PathBuf::from(args.next().ok_or("keyboard")?)),
            Some("--output") => output = Some(PathBuf::from(args.next().ok_or("output")?)),
            Some("--software-driver") => {
                driver = Some(fs::canonicalize(PathBuf::from(
                    args.next().ok_or("driver")?,
                ))?)
            }
            Some("--font-config") => {
                font = Some(fs::canonicalize(PathBuf::from(args.next().ok_or("font")?))?)
            }
            Some("--effects") => effects = true,
            _ => return Err("Unknown composition fixture option".into()),
        }
    }
    let launcher = fs::canonicalize(launcher.ok_or("--launcher")?)?;
    let sway = fs::canonicalize(sway.ok_or("--sway")?)?;
    let keyboard = fs::canonicalize(keyboard.ok_or("--keyboard")?)?;
    for path in [&launcher, &sway, &keyboard] {
        ensure(
            path.starts_with("/nix/store"),
            "Immutable executable required",
        )?;
    }
    let mut f = Session::new(&output.ok_or("--output")?, &launcher, &sway, "lumen-ime-")?;
    f.set(
        "PATH",
        format!(
            "{}:{}",
            env!("LUMEN_TEST_COMPOSITION_TOOLS"),
            f.env.get("PATH").ok_or("launcher PATH")?
        ),
    );
    f.set_many(&[
        ("QT_QPA_PLATFORM", "wayland"),
        ("QT_IM_MODULE", "wayland"),
        ("QT_QUICK_BACKEND", "software"),
        ("QT_QPA_PLATFORMTHEME", "generic"),
        ("QT_NO_XDG_DESKTOP_PORTAL", "1"),
        ("QT_ACCESSIBILITY", "0"),
        ("NO_AT_BRIDGE", "1"),
        ("WLR_BACKENDS", "headless"),
        ("WLR_RENDERER", if effects { "gles2" } else { "pixman" }),
        ("WLR_RENDERER_ALLOW_SOFTWARE", "1"),
        (
            "WLR_RENDERER_FORCE_SOFTWARE",
            if effects { "1" } else { "0" },
        ),
        ("WLR_LIBINPUT_NO_DEVICES", "1"),
        ("LUMEN_TEST_LAYOUT", "de"),
    ]);
    if let Some(driver) = driver {
        ensure(driver.starts_with("/nix/store"), "Immutable driver")?;
        f.set("LIBGL_DRIVERS_PATH", s(&driver.join("lib/dri")));
        f.set(
            "__EGL_VENDOR_LIBRARY_FILENAMES",
            s(&driver.join("share/glvnd/egl_vendor.d/50_mesa.json")),
        );
        f.set("MESA_LOADER_DRIVER_OVERRIDE", "llvmpipe");
    }
    if let Some(font) = font {
        f.set("FONTCONFIG_FILE", s(&font));
    }
    let assets = f.out.join("assets");
    copy_tree(
        &PathBuf::from(f.env.get("LUMEN_DESKTOP_ASSETS").ok_or("assets")?),
        &assets,
    )?;
    let palette: Value =
        serde_json::from_slice(&fs::read(assets.join("appearance/palette.json"))?)?;
    let appearance = PathBuf::from(f.env.get("XDG_STATE_HOME").ok_or("state home")?)
        .join("lumen-desktop/appearance.json");
    fs::create_dir_all(appearance.parent().ok_or("appearance parent")?)?;
    fs::write(
        appearance,
        json!({"font":palette["font"],"palette":palette["dark"],"preferences":{}}).to_string(),
    )?;
    // This instantiates only the real launcher with private synthetic services,
    // not the full shell or any hardware/account provider.
    fs::write(
        f.out.join("shell.qml"),
        include_str!("../../composition.qml"),
    )?;
    f.start_bus()?;
    let global = f.out.join("global-invocations");
    let config = f.out.join("sway.conf");
    fs::write(
        &config,
        format!(
            "output HEADLESS-1 resolution 1100x900\nseat seat0 fallback true\nxwayland disable\nfocus_follows_mouse no\nfor_window [all] floating enable\nbindsym Control+Mod1+q exec touch {}\n",
            global.display()
        ),
    )?;
    f.launch(
        &s(&sway),
        &["--unsupported-gpu", "-c", &s(&config)],
        "sway.log",
        false,
    )?;
    f.wait_sway(1)?;
    let keyboard_index = f.launch(&s(&keyboard), &[], "keyboard.log", true)?;
    wait("physical keyboard initialized", 10, || {
        Ok(fs::read_to_string(f.out.join("keyboard.log"))?
            .contains("ready")
            .then_some(()))
    })?;
    let ime = f.launch(
        env!("LUMEN_TEST_INPUT_METHOD"),
        &[],
        "input-method.log",
        true,
    )?;
    let shell = f.launch(
        "quickshell",
        &["-p", &s(&f.out.join("shell.qml"))],
        "shell.log",
        false,
    )?;
    let pid = f.child_pid(shell);
    wait("actual launcher IPC and editor constructed", 10, || {
        let v = state(&f, pid)?;
        Ok(v.get("query").is_some().then_some(()))
    })?;
    let shell_executable = fs::read_link(format!("/proc/{pid}/exe"))?;
    fs::write(
        f.out.join("shell-runtime-maps.txt"),
        fs::read(format!("/proc/{pid}/maps"))?,
    )?;
    reset(&f, pid)?;
    wait("real Wayland text input activated", 10, || {
        Ok(fs::read_to_string(f.out.join("input-method.log"))?
            .lines()
            .any(|line| line.starts_with("state ") && line.ends_with(" 1"))
            .then_some(()))
    })?;
    let mut serial = 0;
    let mut keyboard_serial = 0;
    let mut checks = Vec::new();
    send(&mut f, ime, "input-method.log", &mut serial, "p")?;
    wait("real preedit visible", 10, || {
        let v = state(&f, pid)?;
        Ok((v["composing"] == true && v["preedit"] == "にほん").then_some(()))
    })?;
    f.call(
        "quickshell",
        &[
            "ipc",
            "--pid",
            &pid.to_string(),
            "call",
            "composition",
            "showTip",
        ],
    )?;
    wait("actual tooltip visible beside composing editor", 10, || {
        Ok((state(&f, pid)?["tooltip"] == true).then_some(()))
    })?;
    let before = state(&f, pid)?;
    for code in [108, 103, 28, 1] {
        let presses = state(&f, pid)?["keys"]["presses"]
            .as_u64()
            .ok_or("actual key witness")?;
        key(&mut f, keyboard_index, &mut keyboard_serial, code)?;
        wait(
            "physical press reached actual editor before assertion",
            5,
            || {
                Ok((state(&f, pid)?["keys"]["presses"]
                    .as_u64()
                    .is_some_and(|n| n > presses))
                .then_some(()))
            },
        )?;
        let after = state(&f, pid)?;
        ensure(
            after["selected"] == before["selected"]
                && after["closed"] == before["closed"]
                && after["launches"] == before["launches"]
                && after["query"] == before["query"]
                && after["tooltip"] == true,
            format!("Preedit key {code} escaped editor: {after}"),
        )?;
    }
    checks.push(json!({"case":"actual preedit protects ungrabbed Enter Escape Up Down","state":state(&f,pid)?}));
    f.call(
        "quickshell",
        &[
            "ipc",
            "--pid",
            &pid.to_string(),
            "call",
            "composition",
            "hideTip",
        ],
    )?;
    send(&mut f, ime, "input-method.log", &mut serial, "c")?;
    wait("actual Unicode commit", 10, || {
        let v = state(&f, pid)?;
        Ok((v["composing"] == false && v["query"] == "café é 😀 日本語").then_some(()))
    })?;
    checks
        .push(json!({"case":"actual commit preserves Unicode codepoints","state":state(&f,pid)?}));
    reset(&f, pid)?;
    send(&mut f, ime, "input-method.log", &mut serial, "g")?;
    wait("grabbed preedit", 10, || {
        Ok((state(&f, pid)?["composing"] == true).then_some(()))
    })?;
    let candidate_before = state(&f, pid)?;
    for code in [108, 103] {
        key(&mut f, keyboard_index, &mut keyboard_serial, code)?;
        wait("actual candidate provider consumed arrow", 5, || {
            Ok(fs::read_to_string(f.out.join("input-method.log"))?
                .contains(&format!("candidate arrow {code}"))
                .then_some(()))
        })?;
        let after = state(&f, pid)?;
        ensure(
            after["selected"] == candidate_before["selected"]
                && after["closed"] == candidate_before["closed"]
                && after["launches"] == candidate_before["launches"]
                && after["keys"]["presses"] == candidate_before["keys"]["presses"],
            format!("Grabbed candidate arrow escaped provider: {after}"),
        )?;
    }
    key(&mut f, keyboard_index, &mut keyboard_serial, 28)?;
    wait("first Enter commits candidate only", 10, || {
        let v = state(&f, pid)?;
        Ok((v["query"] == "日本語"
            && v["composing"] == false
            && v["closed"] == 0
            && v["launches"] == 0)
            .then_some(()))
    })?;
    send(&mut f, ime, "input-method.log", &mut serial, "r")?;
    key(&mut f, keyboard_index, &mut keyboard_serial, 28)?;
    wait(
        "second Enter opens matching committed Japanese result once",
        10,
        || Ok((state(&f, pid)?["launches"] == 1).then_some(())),
    )?;
    checks.push(json!({"case":"first Enter commits candidate; second opens actual matching result once","state":state(&f,pid)?}));
    reset(&f, pid)?;
    key(&mut f, keyboard_index, &mut keyboard_serial, 108)?;
    ensure(
        state(&f, pid)?["selected"] == 1,
        "ordinary Down did not select second result",
    )?;
    key(&mut f, keyboard_index, &mut keyboard_serial, 28)?;
    wait("ordinary Enter launches once", 10, || {
        Ok((state(&f, pid)?["launches"] == 2).then_some(()))
    })?;
    checks.push(json!({"case":"candidate Enter owned; post-composition Down Enter unchanged","state":state(&f,pid)?}));
    reset(&f, pid)?;
    send(&mut f, ime, "input-method.log", &mut serial, "g")?;
    wait("second grabbed preedit", 10, || {
        Ok((state(&f, pid)?["composing"] == true).then_some(()))
    })?;
    f.call(
        "quickshell",
        &[
            "ipc",
            "--pid",
            &pid.to_string(),
            "call",
            "composition",
            "showTip",
        ],
    )?;
    wait(
        "actual tooltip visible during grabbed composition",
        10,
        || Ok((state(&f, pid)?["tooltip"] == true).then_some(())),
    )?;
    let closed = state(&f, pid)?["closed"].clone();
    key(&mut f, keyboard_index, &mut keyboard_serial, 1)?;
    wait("first Escape cancels only preedit", 10, || {
        let v = state(&f, pid)?;
        Ok(
            (v["composing"] == false && v["closed"] == closed && v["tooltip"] == true)
                .then_some(()),
        )
    })?;
    f.call(
        "quickshell",
        &[
            "ipc",
            "--pid",
            &pid.to_string(),
            "call",
            "composition",
            "hideTip",
        ],
    )?;
    send(&mut f, ime, "input-method.log", &mut serial, "r")?;
    key(&mut f, keyboard_index, &mut keyboard_serial, 1)?;
    ensure(
        state(&f, pid)?["closed"].as_u64() == closed.as_u64().map(|n| n + 1),
        "second Escape did not close overlay once",
    )?;
    checks.push(json!({"case":"candidate Escape then ordinary Escape","state":state(&f,pid)?}));
    reset(&f, pid)?;
    key(&mut f, keyboard_index, &mut keyboard_serial, 13)?;
    key(&mut f, keyboard_index, &mut keyboard_serial, 18)?;
    wait("German acute dead key plus e", 10, || {
        Ok((state(&f, pid)?["query"] == "é").then_some(()))
    })?;
    checks.push(
        json!({"case":"actual German dead acute plus e composes once","state":state(&f,pid)?}),
    );
    reset(&f, pid)?;
    send_keyboard(&mut f, keyboard_index, &mut keyboard_serial, "k 100 1")?;
    send_keyboard(&mut f, keyboard_index, &mut keyboard_serial, "m 128 0")?;
    key(&mut f, keyboard_index, &mut keyboard_serial, 16)?;
    send_keyboard(&mut f, keyboard_index, &mut keyboard_serial, "k 100 0")?;
    send_keyboard(&mut f, keyboard_index, &mut keyboard_serial, "m 0 0")?;
    wait("German AltGr q text", 10, || {
        Ok((state(&f, pid)?["query"] == "@").then_some(()))
    })?;
    ensure(!global.exists(), "AltGr invoked Ctrl+Alt global shortcut")?;
    let altgr = state(&f, pid)?;
    ensure(
        altgr["keys"]["text"] == "@"
            && altgr["keys"]["modifiers"]
                .as_u64()
                .is_some_and(|value| value & 0x0c000000 == 0),
        format!("AltGr translated text exposed Ctrl/Alt command modifiers: {altgr}"),
    )?;
    checks.push(json!({"case":"actual German Mod5 AltGr text and zero Ctrl+Alt action","state":state(&f,pid)?}));
    send_keyboard(&mut f, keyboard_index, &mut keyboard_serial, "m 12 0")?;
    key(&mut f, keyboard_index, &mut keyboard_serial, 16)?;
    send_keyboard(&mut f, keyboard_index, &mut keyboard_serial, "m 0 0")?;
    wait("global shortcut positive control", 10, || {
        Ok(global.exists().then_some(()))
    })?;
    checks
        .push(json!({"case":"actual Ctrl+Alt positive control triggers configured global action"}));
    f.call(
        "quickshell",
        &[
            "ipc",
            "--pid",
            &pid.to_string(),
            "call",
            "composition",
            "openForm",
        ],
    )?;
    wait("real production form editor focused", 10, || {
        let v = state(&f, pid)?;
        Ok((v["focus"] == true && v["query"] == "sftp://example.invalid/").then_some(()))
    })?;
    send(&mut f, ime, "input-method.log", &mut serial, "p")?;
    wait("real production form preedit", 10, || {
        Ok((state(&f, pid)?["composing"] == true).then_some(()))
    })?;
    let form_before = state(&f, pid)?;
    key(&mut f, keyboard_index, &mut keyboard_serial, 28)?;
    wait(
        "actual form editor received candidate confirmation",
        5,
        || {
            Ok((state(&f, pid)?["keys"]["presses"].as_u64()
                > form_before["keys"]["presses"].as_u64())
            .then_some(()))
        },
    )?;
    let form_after = state(&f, pid)?;
    ensure(
        form_after["networkSubmits"] == 0 && form_after["query"] == form_before["query"],
        format!("Candidate Enter submitted actual form: {form_after}"),
    )?;
    let presses = form_after["keys"]["presses"]
        .as_u64()
        .ok_or("form key witness")?;
    key(&mut f, keyboard_index, &mut keyboard_serial, 1)?;
    wait("actual popup editor received composing Escape", 5, || {
        Ok((state(&f, pid)?["keys"]["presses"]
            .as_u64()
            .is_some_and(|n| n > presses))
        .then_some(()))
    })?;
    let escaped = state(&f, pid)?;
    ensure(
        escaped["formVisible"] == true
            && escaped["focus"] == true
            && escaped["networkSubmits"] == 0,
        format!("Composing Escape closed actual modal form: {escaped}"),
    )?;
    send(&mut f, ime, "input-method.log", &mut serial, "c")?;
    let address = "sftp://example.invalid/café é 😀 日本語";
    wait("real form commit without accepted side effect", 10, || {
        let v = state(&f, pid)?;
        Ok(
            (v["composing"] == false && v["query"] == address && v["networkSubmits"] == 0)
                .then_some(()),
        )
    })?;
    key(&mut f, keyboard_index, &mut keyboard_serial, 28)?;
    wait(
        "next ordinary Enter submits actual form exactly once",
        10,
        || {
            let v = state(&f, pid)?;
            Ok((v["networkSubmits"] == 1 && v["submittedAddress"] == address).then_some(()))
        },
    )?;
    checks.push(json!({"case":"actual production TextField candidate Enter commit then ordinary accepted once","state":state(&f,pid)?}));
    let processes = (0..f.child_count())
        .map(|index| {
            let pid = f.child_pid(index);
            json!({"pid":pid,"start":process_start(pid)})
        })
        .collect::<Vec<_>>();
    let mut receipt = json!({"pass":true,"checks":checks,"launcher":launcher,"launcher_sha256":sha_file(&launcher)?,"quickshell":shell_executable,"quickshell_sha256":sha_file(&shell_executable)?,"sway":sway,"keyboard":keyboard,"input_method":env!("LUMEN_TEST_INPUT_METHOD"),"assets":lumen_search_native_fixture::sha_assets(&assets)?,"owned_processes":processes,"processes_before_reap":f.process_status()});
    let output = f.out.clone();
    drop(f);
    for process in &processes {
        let pid = process["pid"].as_u64().ok_or("pid")?;
        let now = process_start(u32::try_from(pid)?);
        ensure(
            now.is_none() || process["start"].as_str() != now.as_deref(),
            "Owned native child remained after reap",
        )?;
    }
    receipt["owned_children_reaped"] = json!(true);
    fs::write(
        output.join("result.json"),
        serde_json::to_vec_pretty(&receipt)?,
    )?;
    println!("PASS composition {}", checks.len());
    Ok(())
}
fn main() {
    if let Err(error) = run() {
        eprintln!("composition: {error}");
        std::process::exit(1);
    }
}
