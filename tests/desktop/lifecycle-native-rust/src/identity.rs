//! Actual QObject identity over separate observer connections and hidden views.
use crate::harness::{Fixture, Options, Result, read, require, save, wait, wait_true};
use serde_json::{Value, json};
use std::{collections::BTreeSet, env, fs, path::Path};

fn state(f: &Fixture, path: &Path) -> Result<Value> {
    Ok(serde_json::from_str(&f.call(
        "quickshell",
        &[
            "-p",
            path.to_str().ok_or("Observer path")?,
            "ipc",
            "call",
            "identityAudit",
            "status",
        ],
    )?)?)
}
fn expected(f: &Fixture, pid: u32) -> Result<BTreeSet<String>> {
    let mut all = Vec::new();
    Fixture::nodes(&f.tree()?, &mut all);
    all.into_iter()
        .filter(|node| node["pid"] == pid && node["shell"] == "xdg_shell")
        .map(|node| {
            node["foreign_toplevel_identifier"]
                .as_str()
                .filter(|id| !id.is_empty())
                .map(str::to_owned)
                .ok_or("Owned view missing ext identifier".into())
        })
        .collect()
}
fn observe(
    f: &Fixture,
    path: &Path,
    epoch: &str,
    ids: &BTreeSet<String>,
    label: &str,
) -> Result<Value> {
    let value = wait(label, || {
        let value = state(f, path)?;
        if value["epoch"] != epoch {
            return Ok(None);
        }
        let rows = value["windows"].as_array().ok_or("Observer windows")?;
        let observed = rows
            .iter()
            .filter_map(|row| row["identifier"].as_str())
            .map(str::to_owned)
            .collect::<BTreeSet<_>>();
        Ok(
            (rows.len() == ids.len() && observed == *ids && !observed.contains(""))
                .then_some(value),
        )
    })?;
    save(&f.options.output.join(format!("{label}.json")), &value)?;
    Ok(value)
}
fn start_observer(f: &mut Fixture, name: &str, epoch: &str) -> Result<(usize, std::path::PathBuf)> {
    let path = f.options.output.join(format!("{name}.qml"));
    fs::write(
        &path,
        include_str!("../identity-observer.qml").replace("__EPOCH__", epoch),
    )?;
    let index = f.launch(
        Path::new("quickshell"),
        &["-p", path.to_str().ok_or("QML path")?],
        &format!("{name}-{epoch}.log"),
    )?;
    Ok((index, path))
}
pub fn run(options: Options) -> Result<()> {
    let mut f = Fixture::new(options)?;
    let wire = fs::canonicalize(
        env::var_os("LUMEN_IDENTITY_WIRE").ok_or("Pinned LUMEN_IDENTITY_WIRE client required")?,
    )?;
    require(
        wire.starts_with("/nix/store") && wire.is_file(),
        "Immutable identity wire client required",
    )?;
    let directory = f.options.output.join("owned-client");
    fs::create_dir(&directory)?;
    let client = f.sibling("lifecycle-client")?;
    let owner = f.launch(
        &client,
        &[
            directory.to_str().ok_or("Client path")?,
            "org.example.IdentityFixture",
            "--duplicate-titles",
        ],
        "client.log",
    )?;
    let pid = f.processes[owner].0.id();
    let ids = wait("owned duplicate-title native views", || {
        let ids = expected(&f, pid)?;
        Ok((ids.len() == 2).then_some(ids))
    })?;
    let mut wire_clients = Vec::new();
    for (target_number, target) in ids.iter().enumerate() {
        for connection in 0..2 {
            let output = f
                .options
                .output
                .join(format!("wire-{target_number}-{connection}"));
            let index = f.launch(
                &wire,
                &[target, output.to_str().ok_or("Wire output")?],
                &format!("wire-{target_number}-{connection}.log"),
            )?;
            wait_true("real foreign resource ready", || {
                Ok(output.join("ready.json").is_file())
            })?;
            require(
                read(&output.join("ready.json"))?["identifier"] == *target,
                "Wire target identity changed",
            )?;
            wire_clients.push((index, target.clone(), output));
        }
    }
    let (first, first_path) = start_observer(&mut f, "observer-first", "first")?;
    observe(&f, &first_path, "first", &ids, "first-exact-set")?;
    let (second, second_path) = start_observer(&mut f, "observer-second", "independent")?;
    observe(
        &f,
        &second_path,
        "independent",
        &ids,
        "second-connection-exact-set",
    )?;
    let mut all = Vec::new();
    Fixture::nodes(&f.tree()?, &mut all);
    for node in all
        .iter()
        .filter(|node| node["pid"] == pid && node["shell"] == "xdg_shell")
    {
        f.call(
            "swaymsg",
            &[&format!("[con_id={}]", node["id"]), "move scratchpad"],
        )?;
    }
    require(
        expected(&f, pid)? == ids,
        "Hiding altered stable identifiers",
    )?;
    f.stop(first)?;
    let (restarted, restarted_path) = start_observer(&mut f, "observer-first", "restarted-hidden")?;
    observe(
        &f,
        &restarted_path,
        "restarted-hidden",
        &ids,
        "hidden-restart-exact-set",
    )?;
    observe(
        &f,
        &second_path,
        "independent",
        &ids,
        "independent-hidden-exact-set",
    )?;
    fs::write(directory.join("command"), "close second")?;
    let remaining = wait("owned closure", || {
        let remaining = expected(&f, pid)?;
        Ok((remaining.len() == 1 && remaining.is_subset(&ids)).then_some(remaining))
    })?;
    observe(
        &f,
        &restarted_path,
        "restarted-hidden",
        &remaining,
        "closure-first-exact-set",
    )?;
    observe(
        &f,
        &second_path,
        "independent",
        &remaining,
        "closure-second-exact-set",
    )?;
    for (_, target, output) in &wire_clients {
        if !remaining.contains(target) {
            wait_true("closed actual retained resource rejected", || {
                Ok(output.join("result.json").is_file())
            })?;
            require(
                read(&output.join("result.json"))?["inertUnavailable"] == true,
                "Closed actual resource retained identity",
            )?;
        }
    }
    f.stop(owner)?;
    wait_true("owned client fully retired", || {
        Ok(expected(&f, pid)?.is_empty())
    })?;
    observe(
        &f,
        &restarted_path,
        "restarted-hidden",
        &BTreeSet::new(),
        "retired-first-empty",
    )?;
    observe(
        &f,
        &second_path,
        "independent",
        &BTreeSet::new(),
        "retired-second-empty",
    )?;
    for (index, _, output) in &wire_clients {
        wait_true("all actual retained resources rejected", || {
            Ok(output.join("result.json").is_file())
        })?;
        let status = wait("bounded protocol helper exit", || {
            Ok(f.processes[*index].0.try_wait()?)
        })?;
        require(status.success(), "Actual protocol helper failed")?;
    }
    f.stop(restarted)?;
    f.stop(second)?;
    let renderer = fs::read_to_string(f.options.output.join("sway.log"))?;
    if f.options.effects {
        require(
            renderer.contains("llvmpipe"),
            "Effects renderer is not proven software",
        )?;
    }
    save(
        &f.options.output.join("result.json"),
        &json!({"passed":true,"ownedPid":pid,"originalIdentifiers":ids,"remainingAfterOneClose":remaining,"wireClient":wire,"wireConnections":wire_clients.len(),"checks":["actual QObject property exactly matches owned PID tree identifier set","same app and duplicate titles have distinct stable identifiers","independent observer connections map to the same ext identifier set","new observer binds while both views are hidden in scratchpad","ordinary closure retires exact QObject identities on both connections","owned process exit leaves both observers empty","four independent actual wire connections retain resources after Closed and receive unavailable empty identity"],"scope":"Actual Quickshell optional property and compositor live/inert resource membership; no title/appId join or mixed switcher routing claim","effectsSoftware":!f.options.effects || renderer.contains("llvmpipe")}),
    )
}
