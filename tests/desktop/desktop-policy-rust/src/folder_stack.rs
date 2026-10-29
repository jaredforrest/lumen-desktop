//! Live Folder Stack actions, captured file identity, and owned directory handoff.
use super::*;

fn state(ctx: &Context) -> Result<Value> { ctx.ui() }
fn ipc(ctx: &Context, method: &str, args: &[&str]) -> Result<String> { ctx.ipc(method, args) }
fn point(ctx: &Context, name: &str) -> Result<Value> { Ok(serde_json::from_str(&ipc(ctx, "point", &[name])?)?) }
fn records(path: &Path) -> Result<Vec<Value>> {
    if !path.exists() { return Ok(Vec::new()); }
    let mut result = Vec::new();
    for line in fs::read_to_string(path)?.lines() {
        result.extend(serde_json::from_str::<Vec<Value>>(line)?);
    }
    Ok(result)
}
fn show(ctx: &Context, folder: &Path, presentation: &str) -> Result<Value> {
    let folder = path(folder);
    ipc(ctx, "presentFolder", &[&folder, presentation])?;
    wait("Folder Stack listing", || {
        let row = state(ctx)?;
        Ok((row["visible"] == true && row["listing"] == false && row["folder"] == folder && !row["facts"].is_null()).then_some(row))
    })
}
fn focus(ctx: &Context, method: &str, name: &str) -> Result<()> {
    let args = if method == "focusControl" { vec![name] } else { vec![] };
    need(ipc(ctx, method, &args)? == "true", format!("Missing stack focus {name}"))?;
    wait("Folder Stack focused control", || Ok((point(ctx, name)?["active"] == true).then_some(())))
}
fn center(row: &Value) -> Result<(f64, f64)> {
    Ok((row["x"].as_f64().ok_or("Missing point x")? + row["width"].as_f64().ok_or("Missing point width")? / 2.0,
        row["y"].as_f64().ok_or("Missing point y")? + row["height"].as_f64().ok_or("Missing point height")? / 2.0))
}
fn press(ctx: &mut Context, name: &str) -> Result<()> {
    let row = point(ctx, name)?;
    need(row["width"].as_f64().is_some_and(|v| v > 0.0) && row["height"].as_f64().is_some_and(|v| v > 0.0), "Empty stack target")?;
    ctx.move_pointer(center(&row)?)?;
    wait("Folder Stack hover", || Ok((point(ctx, name)?["hovered"] == true).then_some(())))?;
    ctx.press()?;
    wait("Folder Stack press", || Ok((point(ctx, name)?["down"] == true).then_some(())))
}
fn instrument(ctx: &Context) -> Result<()> {
    let target = ctx.root.0.join("assets/shell/FolderStack.qml");
    let original = fs::read_to_string(&target)?;
    let anchor = "    id:root";
    let gate = "onStarted:{received=false;write(JSON.stringify(request)+\"\\n\");}";
    need(original.matches(anchor).count() == 1 && original.matches(gate).count() == 2,
        "Folder Stack instrumentation anchors changed")?;
    let aliases = "\n    property alias fixtureListing:worker\n    property alias fixtureAction:actionWorker\n    property bool fixtureHoldListing:false";
    let modified = original.replacen(anchor, &(anchor.to_owned() + aliases), 1).replacen(gate,
        "onStarted:{received=false;if(!root.fixtureHoldListing)write(JSON.stringify(request)+\"\\n\");}", 1);
    fs::write(&target, &modified)?;
    need(modified.replacen(aliases, "", 1).replacen(
        "onStarted:{received=false;if(!root.fixtureHoldListing)write(JSON.stringify(request)+\"\\n\");}", gate, 1) == original,
        "Folder Stack fixture instrumentation did not round trip")?;
    save(&ctx.opt.output.join("instrumentation.json"), &json!({"originalSha256":format!("{:x}", Sha256::digest(original.as_bytes())),
        "modifiedSha256":format!("{:x}", Sha256::digest(modified.as_bytes())),"roundtrip":true,
        "scope":"Read-only Process aliases and initial listing stdin gate; production provider and action path unchanged"}))
}

pub fn run(ctx: &mut Context) -> Result<()> {
    for (key, value) in [
        ("LUMEN_LAUNCH_SCOPES", "0"), ("XDG_CURRENT_DESKTOP", "GNOME"),
        ("LUMEN_NATIVE_WINDOW_ACTIONS", "0"), ("LUMEN_NATIVE_WINDOW_CONTROLS", "0"),
    ] { ctx.vars.insert(key.into(), value.into()); }
    ctx.vars.insert("DBUS_SYSTEM_BUS_ADDRESS".into(), format!("unix:path={}",ctx.root.0.join("absent-system").display()));
    ctx.vars.insert("PULSE_SERVER".into(), format!("unix:{}",ctx.root.0.join("absent-audio").display()));
    if let Some(client) = &ctx.opt.client {
        ctx.vars.insert("LUMEN_FILES_BIN".into(), path(client));
    }
    ctx.vars.insert("XDG_DATA_DIRS".into(), ctx.vars["XDG_DATA_HOME"].clone());
    ctx.checked("output HEADLESS-1 resolution 1280x900 position 0 0")?;
    ctx.checked("output HEADLESS-2 disable")?;
    let palette = load(&ctx.repo.join("res/lumen-desktop/appearance/palette.json"))?;
    save(&ctx.root.0.join("appearance.json"), &json!({"font":palette["font"],"palette":palette["dark"],
        "preferences":{"reduceMotion":true}}))?;
    instrument(ctx)?;
    let home = ctx.root.0.join("HOME");
    let docs = home.join("Mixed"); fs::create_dir(&docs)?;
    let alpha = docs.join("Alpha # ü.txt"); fs::write(&alpha, "captured alpha")?;
    let beta = docs.join("Beta.txt"); fs::write(&beta, "captured beta")?;
    let child = docs.join("Child"); fs::create_dir(&child)?; fs::write(child.join("inside.txt"), "owned child")?;
    let link = docs.join("Link.txt"); symlink(&alpha, &link)?;
    fs::write(docs.join("picture.png"), b"only synthetic icon metadata")?;
    let empty = home.join("Empty"); fs::create_dir(&empty)?;
    let huge = home.join("Huge"); fs::create_dir(&huge)?;
    for index in 0..5005 { fs::write(huge.join(format!("item-{index:05}.txt")), index.to_string())?; }
    let apps = ctx.root.0.join("XDG_DATA_HOME/applications"); fs::create_dir_all(&apps)?;
    let opened = ctx.opt.output.join("opened.jsonl");
    let observer = std::env::current_exe()?.parent().ok_or("Fixture binary directory missing")?.join("dock_offer_observer");
    fs::write(apps.join("lumen-stack-observer.desktop"), format!("[Desktop Entry]\nType=Application\nName=Stack Observer\nExec={} {} %U\nMimeType=text/plain;\nTerminal=false\n", observer.display(), opened.display()))?;
    fs::write(ctx.root.0.join("XDG_CONFIG_HOME/mimeapps.list"), "[Default Applications]\ntext/plain=lumen-stack-observer.desktop;\n")?;
    let qml = path(&ctx.qml);
    ctx.launch("quickshell", &["-p", &qml], "folder-stack-shell.log", false)?;
    ctx.launch("wtype", &["-s", "120000"], "folder-stack-keyboard.log", false)?;
    ctx.pointer_start()?;
    wait("Folder Stack IPC", || Ok((state(ctx)?["generation"].as_i64().is_some()).then_some(())))?;
    let mut checks = Vec::new();
    let row = show(ctx, &empty, "list")?;
    need(row["rows"].as_array().is_some_and(|v| v.is_empty()) && row["error"] == "", "Empty stack state incorrect")?;
    focus(ctx, "focusFooter", "@footer")?;
    ctx.key(&["-k", "space"])?;
    wait("empty stack opens Files", || { let row = state(ctx)?;
        Ok((row["filesVisible"] == true && row["filesBusy"] == false && row["filesPath"] == path(&empty)
            && row["filesRows"].as_array().is_some_and(|v| v.is_empty())).then_some(())) })?;
    checks.push("Empty stack keyboard footer opens Files with captured directory facts");
    ipc(ctx, "closeFiles", &[])?;
    let row = show(ctx, &docs, "list")?;
    let names = row["rows"].as_array().ok_or("No mixed rows")?.iter().filter_map(|v| v["name"].as_str()).collect::<BTreeSet<_>>();
    let expected = fs::read_dir(&docs)?.filter_map(|entry| entry.ok().and_then(|v| v.file_name().into_string().ok())).collect::<BTreeSet<_>>();
    need(names == expected.iter().map(String::as_str).collect(), "Mixed stack rows differ from source")?;
    need(row["rows"].as_array().is_some_and(|rows| rows.iter().all(|r| r["facts"].as_array().is_some_and(|v| v.len() == 6 && v.iter().all(Value::is_string)))),
        "Mixed stack lacks captured source facts")?;
    checks.push("Mixed stack publishes lossless captured source versions and list presentation");
    focus(ctx, "focusControl", "@sort")?; ctx.key(&["-k", "Down"])?;
    wait("keyboard stack sort", || { let row = state(ctx)?; Ok((row["sort"] == "modified" && row["listing"] == false).then_some(())) })?;
    need(state(ctx)?["persisted"].as_array().and_then(|v| v.last()).is_some_and(|v| v["path"] == path(&docs)), "Sort persisted wrong folder")?;
    checks.push("Keyboard sort persists only captured folder invocation");
    focus(ctx, "focusControl", "@presentation")?; ctx.key(&["-k", "Down"])?;
    wait("keyboard stack grid", || Ok((state(ctx)?["presentation"] == "grid").then_some(())))?;
    ctx.key(&["-k", "Up"])?;
    wait("keyboard stack list", || Ok((state(ctx)?["presentation"] == "list").then_some(())))?;
    checks.push("Keyboard presentation switches list/grid without changing folder ownership");
    let before = state(ctx)?["persisted"].as_array().ok_or("Missing persisted options")?.len();
    press(ctx, "@sort")?; ctx.release()?;
    wait("stack sort popup", || Ok((state(ctx)?["sortPopup"] == true).then_some(())))?;
    show(ctx, &empty, "list")?;
    need(state(ctx)?["sortPopup"] == false && state(ctx)?["persisted"].as_array().is_some_and(|v| v.len() == before), "Sort popup persisted across folders")?;
    show(ctx, &docs, "list")?;
    checks.push("Sort popup retires on another folder invocation without persisting to new folder");
    press(ctx, &path(&alpha))?; ctx.release()?;
    wait("private file handler", || Ok((records(&opened)?.len() == 1 && state(ctx)?["visible"] == false).then_some(())))?;
    need(records(&opened)?[0]["path"] == path(&alpha) && records(&opened)?[0]["sha256"] == hash(&alpha)?, "Original file dispatch changed")?;
    checks.push("File click reaches private default handler with exact path and original bytes");
    show(ctx, &docs, "list")?;
    press(ctx, &path(&alpha))?;
    let old = ctx.root.0.join("old-alpha"); fs::rename(&alpha, &old)?; fs::write(&alpha, "replacement alpha")?;
    ctx.release()?;
    wait("replaced file rejected", || { let row = state(ctx)?; Ok((row["error"].as_str().is_some_and(|v| !v.is_empty()) && row["busy"] == false).then_some(())) })?;
    need(records(&opened)?.len() == 1 && state(ctx)?["visible"] == true, "Replacement dispatched to handler")?;
    checks.push("Filesystem replacement during held press refuses external dispatch");
    show(ctx, &docs, "list")?; press(ctx, &path(&alpha))?; ipc(ctx, "reload", &[])?;
    wait("stack reload", || Ok((state(ctx)?["listing"] == false && !state(ctx)?["facts"].is_null()).then_some(())))?;
    ctx.release()?; wait("reload action settled", || Ok((state(ctx)?["busy"] == false).then_some(())))?;
    need(records(&opened)?.len() == 1, "Reload transferred held pointer action")?;
    checks.push("Reload while pointer held cannot transfer activation to refreshed row");
    show(ctx, &docs, "list")?; press(ctx, &path(&alpha))?; ctx.key(&["-k", "Down"])?; ctx.release()?;
    wait("held file dispatch", || Ok((records(&opened)?.len() == 2 && state(ctx)?["busy"] == false).then_some(())))?;
    need(records(&opened)?[1]["path"] == path(&alpha) && records(&opened)?[1]["sha256"] == hash(&alpha)?, "Held file identity changed")?;
    checks.push("Pointer press owns file despite keyboard selection move before release");
    show(ctx, &docs, "list")?; press(ctx, &path(&link))?; ctx.release()?;
    wait("symlink refused", || Ok((state(ctx)?["error"].as_str().is_some_and(|v| !v.is_empty()) && state(ctx)?["busy"] == false).then_some(())))?;
    need(records(&opened)?.len() == 2 && fs::symlink_metadata(&link)?.file_type().is_symlink(), "Symlink action dereferenced")?;
    checks.push("Symbolic-link stack action is unsupported without dereference");
    let row = show(ctx, &docs, "list")?;
    ipc(ctx, "hold", &["true"])?; press(ctx, &path(&child))?; ctx.release()?;
    wait("captured child held", || Ok((state(ctx)?["held"].is_object()).then_some(())))?;
    let expected_facts = row["rows"].as_array().ok_or("Missing child rows")?.iter().find(|r| r["path"] == path(&child)).ok_or("Missing child row")?["facts"].clone();
    need(state(ctx)?["held"]["facts"] == expected_facts, "Captured child facts changed")?;
    let retired = ctx.root.0.join("retired-child"); fs::rename(&child, &retired)?; fs::create_dir(&child)?; fs::write(child.join("replacement.txt"), "replacement")?;
    let previous = state(ctx)?["filesPath"].clone(); let history = state(ctx)?["filesHistory"].clone();
    ipc(ctx, "releaseDirectory", &[])?;
    wait("replaced directory rejected", || { let row = state(ctx)?;
        Ok((row["filesVisible"] == true && row["filesBusy"] == false && row["filesError"].as_str().is_some_and(|v| !v.is_empty())).then_some(())) })?;
    need(state(ctx)?["filesPath"] == previous && state(ctx)?["filesHistory"] == history
        && !state(ctx)?["filesRows"].as_array().is_some_and(|v| v.contains(&json!("replacement.txt"))), "Stale directory committed Files state")?;
    checks.push("Delayed owned directory handoff refuses replaced target");
    ipc(ctx, "closeFiles", &[])?; ipc(ctx, "hold", &["false"])?; show(ctx, &docs, "list")?;
    press(ctx, &path(&child))?; ctx.release()?;
    wait("fresh child directory", || { let row = state(ctx)?;
        Ok((row["filesVisible"] == true && row["filesBusy"] == false && row["filesPath"] == path(&child)).then_some(())) })?;
    need(state(ctx)?["filesRows"] == json!(["replacement.txt"]), "Fresh child listing wrong")?;
    checks.push("Fresh directory selection opens Files through anchored listing");
    ipc(ctx, "closeFiles", &[])?; let row = show(ctx, &huge, "grid")?;
    need(row["grid"] == true && row["limited"] == true && row["rowCount"] == 5000 && row["total"] == 5000, "Huge grid limit wrong")?;
    let footer = point(ctx, "@footer")?;
    need(footer["y"].as_f64().is_some_and(|v| v >= 0.0 && v + footer["height"].as_f64().unwrap_or(1000.0) <= 900.0), "Huge stack footer out of output")?;
    ctx.move_pointer(center(&point(ctx, "@grid")?)?)?;
    ctx.pointer_command("w 5")?;
    wait("huge stack scroll", || Ok((state(ctx)?["scrollY"].as_f64().is_some_and(|v| v > 0.0)).then_some(())))?;
    ctx.picture("huge-stack.png")?;
    checks.push("Large grid is limited, scrollable, and has reachable folder action");
    ipc(ctx, "holdListing", &["true"])?;
    ipc(ctx, "presentFolder", &[&path(&huge), "list"])?;
    wait("listing child held", || Ok((state(ctx)?["listing"] == true).then_some(())))?;
    need(ipc(ctx, "killListing", &[])? == "true", "Could not kill held listing child")?;
    wait("listing child death", || { let row = state(ctx)?;
        Ok((row["error"].as_str().is_some_and(|v| !v.is_empty()) && row["listing"] == false).then_some(())) })?;
    need(state(ctx)?["rows"].as_array().is_some_and(|v| v.is_empty()) && state(ctx)?["facts"].is_null(), "Killed listing retained rows")?;
    ipc(ctx, "holdListing", &["false"])?; ipc(ctx, "reload", &[])?;
    wait("explicit listing recovery", || { let row = state(ctx)?;
        Ok((row["listing"] == false && row["rowCount"] == 5000 && !row["facts"].is_null()).then_some(())) })?;
    checks.push("Listing child death reports unavailable, then explicit reload restores rows");
    let log = fs::read_to_string(ctx.opt.output.join("folder-stack-shell.log"))?;
    need(!["ReferenceError", "TypeError", "binding loop"].iter().any(|marker| log.contains(marker)), format!("Folder Stack QML error: {log}"))?;
    save(&ctx.opt.output.join("result.json"), &json!({"passed":true,"checks":checks,"dispatches":records(&opened)?,
        "liveSocketUnchanged":ctx.original_socket == env::var("SWAYSOCK").ok()}))?;
    println!("PASS Folder Stack native: {} checks", checks.len());
    Ok(())
}
