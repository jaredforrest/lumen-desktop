//! Native lock shielding owns input even while projection metadata keeps logical scope.
use super::*;
use std::io::Write;
use std::path::{Path, PathBuf};
pub(super) fn pinned(f: &Fixture, name: &str) -> Result<PathBuf> {
    let path = std::env::split_paths(std::ffi::OsStr::new(f.vars.get("PATH").ok_or("PATH")?))
        .map(|p| p.join(name))
        .find(|p| p.is_file())
        .ok_or("Pinned input/lock helper missing")?
        .canonicalize()?;
    require(
        path.starts_with("/nix/store"),
        "Immutable input/lock helper required",
    )?;
    Ok(path)
}
pub(super) fn pointer(
    f: &mut Fixture,
    process: usize,
    log: &Path,
    serial: u32,
    command: &str,
) -> Result<()> {
    let transcript = format!("{command}a {serial}\n");
    save(
        &f.options
            .output
            .join(format!("lock-pointer-command-{serial}.json")),
        &json!({"command":transcript,"before":f.tree()?}),
    )?;
    let input = f.processes[process]
        .0
        .stdin
        .as_mut()
        .ok_or("Pointer stdin")?;
    input.write_all(transcript.as_bytes())?;
    input.flush()?;
    wait_true(
        "actual pointer processing roundtrip acknowledgement",
        || {
            let bytes = fs::read(log)?;
            require(bytes.len() < 4096, "Pointer acknowledgement log bound")?;
            Ok(std::str::from_utf8(&bytes)?
                .lines()
                .any(|line| line == format!("ack {serial}")))
        },
    )
}
fn locked(f: &Fixture) -> Result<bool> {
    let version: Value = serde_json::from_str(&f.call("swaymsg", &["-t", "get_version", "-r"])?)?;
    require(
        version["lumen_session_lock"]["available"] == true,
        "Native lock state unavailable",
    )?;
    version["lumen_session_lock"]["locked"]
        .as_bool()
        .ok_or("Invalid native lock state".into())
}
pub(super) fn run(f: &mut Fixture, id: u64, folder: &Path) -> Result<()> {
    let driver = pinned(f, "wlrctl")?;
    let log = f.options.output.join("lock-pointer.log");
    let process =
        f.launch_input_args(&driver, &["pointer", "move", "0", "0"], "lock-pointer.log")?;
    pointer(f, process, &log, 1, "m 0 0\n")?;
    request(f, "seat seat0 cursor set 0 0", None)?;
    let node = view(f, id)?;
    let point = (
        node["rect"]["x"].as_i64().ok_or("Pointer x")?
            + node["window_rect"]["x"].as_i64().ok_or("Client x")?
            + node["window_rect"]["width"]
                .as_i64()
                .ok_or("Client width")?
                / 2,
        node["rect"]["y"].as_i64().ok_or("Pointer y")?
            + node["window_rect"]["y"].as_i64().ok_or("Client y")?
            + node["window_rect"]["height"]
                .as_i64()
                .ok_or("Client height")?
                / 2,
    );
    pointer(
        f,
        process,
        &log,
        2,
        &format!("m {} {}\np\nr\n", point.0, point.1),
    )?;
    wait_true("real pre-lock projected GTK input", || {
        Ok(clicks(folder)? == 1)
    })?;
    let ordinary_folder = f.options.output.join("lock-source");
    fs::create_dir(&ordinary_folder)?;
    let client = f.sibling("lifecycle-client")?;
    let source_process = f.launch(
        &client,
        &[
            ordinary_folder.to_str().ok_or("Lock source path")?,
            "org.example.MruB",
        ],
        "lock-source.log",
    )?;
    let source_pid = f.processes[source_process].0.id();
    let source = wait("ordinary current-Space lock source", || {
        Ok(nodes(f)?
            .iter()
            .find(|n| n["pid"] == source_pid)
            .and_then(|n| n["id"].as_u64()))
    })?;
    request(f, &format!("[con_id={source}] focus"), None)?;
    require(
        focused(f)? == source && current_workspace(f)?["name"] == "2",
        "Lock source must differ from projected hit target on current Space",
    )?;
    save(
        &f.options.output.join("before-lock-source.json"),
        &json!({"source":source,"target":id,"tree":f.tree()?}),
    )?;
    let locker = pinned(f, "lumen-test-session-lock")?;
    let lock_process = f.launch_input(&locker, "actual-lock.log")?;
    wait_true("actual compositor protocol lock acknowledged", || {
        Ok(
            fs::read_to_string(f.options.output.join("actual-lock.log"))?
                .lines()
                .any(|s| s == "locked")
                && locked(f)?,
        )
    })?;
    let before = f.tree()?;
    let seats_before: Value =
        serde_json::from_str(&f.call("swaymsg", &["-t", "get_seats", "-r"])?)?;
    let target = view(f, id)?;
    request(
        f,
        &operation(&target, false)?,
        Some("Projection unavailable while session locked"),
    )?;
    require(
        before == f.tree()?,
        "Locked exact off command mutated native state",
    )?;
    let source_before = view(f, source)?;
    let on_before = f.tree()?;
    request(
        f,
        &operation(&source_before, true)?,
        Some("Projection unavailable while session locked"),
    )?;
    require(
        on_before == f.tree()?,
        "Locked exact on command mutated native state",
    )?;
    let pending = nodes(f)?
        .iter()
        .find(|n| n["app_id"] == APP && n["id"] != id)
        .and_then(|n| n["id"].as_u64())
        .ok_or("Pending assignment sibling")?;
    require(
        view(f, pending)?["lumen_all_desktops"]["enabled"] == false,
        "Locked pending witness must begin disabled",
    )?;
    f.call(
        "lumen-spaces",
        &["assign", &format!("wayland:{APP}"), "all"],
    )?;
    let watcher = f.launch(
        Path::new("lumen-spaces"),
        &["watch"],
        "locked-assignment-watch.log",
    )?;
    wait_true("locked watcher ready without command rejection", || {
        Ok(
            fs::read_to_string(f.options.output.join("locked-assignment-watch.log"))?
                .contains("\"ready\":true"),
        )
    })?;
    require(
        f.processes[watcher].alive()?
            && view(f, pending)?["lumen_all_desktops"]["enabled"] == false,
        "Locked assignment mutated native presentation or killed watcher",
    )?;
    pointer(f, process, &log, 3, "m 0 0\np\nr\n")?;
    let after = f.tree()?;
    let seats_after: Value = serde_json::from_str(&f.call("swaymsg", &["-t", "get_seats", "-r"])?)?;
    let capture = Command::new("grim")
        .arg(f.options.output.join("native-lock-shield.png"))
        .env_clear()
        .envs(&f.vars)
        .output()?;
    save(
        &f.options.output.join("locked-projection-input.json"),
        &json!({"before":before,"after":after,"seatsBefore":seats_before,"seatsAfter":seats_after,"clicks":clicks(folder)?,"locked":locked(f)?,"captureStatus":capture.status.code(),"captureStderr":String::from_utf8_lossy(&capture.stderr)}),
    )?;
    require(
        locked(f)? && clicks(folder)? == 1,
        "Projected client received physical input while native lock active",
    )?;
    let native = nodes(f)?;
    require(
        seats_after.as_array().is_some_and(|seats| {
            seats.iter().all(|seat| {
                seat["focus"] == 0
                    || native
                        .iter()
                        .find(|n| n["id"] == seat["focus"])
                        .is_some_and(|n| !n["foreign_toplevel_identifier"].is_string())
            })
        }),
        "Locked projected hit activated a managed application view",
    )?;
    let input = f.processes[lock_process]
        .0
        .stdin
        .as_mut()
        .ok_or("Lock stdin")?;
    input.write_all(b"u\n")?;
    input.flush()?;
    wait_true("actual protocol unlock", || Ok(!locked(f)?))?;
    wait_true(
        "fresh unlock observation reconciles deferred native assignment",
        || Ok(view(f, pending)?["lumen_all_desktops"]["enabled"] == true),
    )?;
    require(
        f.processes[watcher].alive()?,
        "Assignment watcher exited across real lock/unlock",
    )?;
    require(
        focused(f)? == source,
        "Native unlock failed to restore the distinct source focus",
    )?;
    pointer(f, process, &log, 4, "m 0 0\np\nr\n")?;
    wait_true(
        "projected client input restored without delayed locked press",
        || Ok(clicks(folder)? == 2),
    )?;
    request(f, &format!("[con_id={source}] focus"), None)?;
    let surface_locker = pinned(f, "projection-lock-client")?;
    let surface_process = f.launch_input(&surface_locker, "actual-surface-lock.log")?;
    wait_true("actual lock surfaces configured", || {
        Ok(
            fs::read_to_string(f.options.output.join("actual-surface-lock.log"))?
                .lines()
                .filter_map(|line| serde_json::from_str::<Value>(line).ok())
                .any(|row| row["ready"] == true)
                && locked(f)?,
        )
    })?;
    pointer(f, process, &log, 5, "m 0 0\np\nr\n")?;
    wait_true("actual allowed lock surface pointer recipient", || {
        let bytes = fs::read(f.options.output.join("actual-surface-lock.log"))?;
        require(bytes.len() < 65536, "Lock recipient log bound")?;
        let rows = std::str::from_utf8(&bytes)?
            .lines()
            .map(serde_json::from_str::<Value>)
            .collect::<std::result::Result<Vec<_>, _>>()?;
        save(
            &f.options.output.join("allowed-lock-surface-input.json"),
            &json!({"transcript":rows,"gtkPresses":clicks(folder)?,"tree":f.tree()?}),
        )?;
        Ok(rows
            .iter()
            .filter(|row| row["pointer"] == true && row["pressed"] == true && row["button"] == 272)
            .count()
            == 1
            && clicks(folder)? == 2)
    })?;
    let screenshot = Command::new("grim")
        .arg(f.options.output.join("native-lock-surface.png"))
        .env_clear()
        .envs(&f.vars)
        .output()?;
    require(
        screenshot.status.success(),
        "Actual lock surface screenshot failed",
    )?;
    let input = f.processes[surface_process]
        .0
        .stdin
        .as_mut()
        .ok_or("Surface lock stdin")?;
    input.write_all(b"u\n")?;
    input.flush()?;
    wait_true("actual lock surfaces unlock", || Ok(!locked(f)?))?;
    require(
        focused(f)? == source,
        "Allowed lock surface input changed original return focus",
    )?;
    pointer(f, process, &log, 6, "m 0 0\np\nr\n")?;
    wait_true("actual projection input resumes after lock surface", || {
        Ok(clicks(folder)? == 3 && focused(f)? == id)
    })?;
    require(
        f.processes[watcher].alive()?,
        "Watcher failed across lock surface protocol",
    )?;
    require(
        view(f, id)?["lumen_all_desktops"]["enabled"] == true
            && current_workspace(f)?["name"] == "2",
        "Lock changed retained projection assignment or effective desktop",
    )?;
    save(
        &f.options.output.join("result.json"),
        &json!({"passed":true,"checks":8,"driver":driver,"locker":locker,"final":f.tree()?,"scope":"Real lock protocol acknowledgement and native lock fact; real pointer roundtrip barriers; projected GTK gets pre/post unlock presses, no locked or delayed press; native focus shielding preserved. Metadata remains logical placement rather than claiming lock visibility."}),
    )?;
    Ok(())
}
