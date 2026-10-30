use super::*;
use regex::Regex;
use serde_json::Map;

fn audit(ctx: &Context, shell: u32, method: &str, args: &[&str]) -> Result<String> {
    let pid = shell.to_string();
    let mut all = vec!["ipc", "--pid", pid.as_str(), "call", "switchAudit", method];
    all.extend(args);
    ctx.named("quickshell", &all)
}
fn status(ctx: &Context, shell: u32) -> Result<Value> {
    Ok(serde_json::from_str(&audit(ctx, shell, "status", &[])?)?)
}
fn focused(ctx: &Context, identities: &BTreeMap<String, Value>) -> Result<Option<String>> {
    let Some(node) = ctx.windows()?.into_iter().find(|w| w["focused"] == true) else {
        return Ok(None);
    };
    let title = node["name"].as_str().ok_or("Focused title missing")?;
    if let Some(expected) = identities.get(title) {
        ensure(
            *expected == json!([node["id"], node["pid"]]),
            format!("Wrong native identity for {title}"),
        )?;
    }
    Ok(Some(title.into()))
}
fn focus(ctx: &Context, shell: u32, title: &str) -> Result<()> {
    ctx.focus(title)?;
    wait(&format!("shell focus {title}"), || {
        Ok((status(ctx, shell)?["active"] == title).then_some(()))
    })?;
    let target=ctx.find(title)?.ok_or("Focused native window missing")?["id"].clone();
    wait(&format!("broker source focus {title}"),||{
        let state=status(ctx,shell)?;
        Ok((state["nativeObservation"]["source"]["kind"]=="native" && state["nativeObservation"]["source"]["conId"]==target).then_some(()))
    })
}
fn native_selection(state:&Value,identities:&BTreeMap<String,Value>,title:&str)->Result<()>{
    ensure(state["picker"]["nativeCapture"]["conId"]==identities[title][0],format!("Preview did not capture exact native {title}: {state}"))
}
fn app(
    ctx: &mut Context,
    shell: u32,
    identities: &mut BTreeMap<String, Value>,
    title: &str,
    app_id: &str,
) -> Result<()> {
    let args = [
        format!("--app-id={app_id}"),
        format!("--title={title}"),
        "sleep".into(),
        "600".into(),
    ];
    let arg_refs = args.iter().map(String::as_str).collect::<Vec<_>>();
    let proc = ctx.launch(Path::new("foot"), &arg_refs, &format!("{title}.log"))?;
    wait(&format!("window {title}"), || ctx.find(title))?;
    wait(&format!("shell window {title}"), || {
        Ok(status(ctx, shell)?["windows"]
            .as_array()
            .filter(|rows| rows.iter().any(|w| w["title"] == title))
            .map(|_| ()))
    })?;
    let node = ctx.find(title)?.ok_or("New native window missing")?;
    ensure(
        node["pid"] == ctx.procs[proc].0.id(),
        format!("Wrong PID for {title}"),
    )?;
    identities.insert(title.into(), json!([node["id"], node["pid"]]));
    Ok(())
}
fn held(ctx: &mut Context, shell: u32, reverse: bool, repeat: usize) -> Result<(usize, i64, u64)> {
    let commits = status(ctx, shell)?["picker"]["commits"]
        .as_i64()
        .ok_or("Commit count missing")?;
    let events = fs::metadata(ctx.opt.output.join("native-events.jsonl"))?.len();
    let mut keys = vec![
        "-M".to_owned(),
        "logo".into(),
        "-P".into(),
        "Super_L".into(),
    ];
    if reverse {
        keys.extend(["-M".into(), "shift".into()]);
    }
    for _ in 0..repeat {
        keys.extend(["-k".into(), "Tab".into(), "-s".into(), "200".into()]);
    }
    keys.extend(["-s".into(), "1600".into()]);
    if reverse {
        keys.extend(["-m".into(), "shift".into()]);
    }
    keys.extend(["-p".into(), "Super_L".into(), "-m".into(), "logo".into()]);
    let refs = keys.iter().map(String::as_str).collect::<Vec<_>>();
    let tool = ctx.opt.wtype.clone();
    let index = ctx.launch(&tool, &refs, &format!("keys-{}.log", ctx.procs.len()))?;
    Ok((index, commits, events))
}
fn preview(
    ctx: &Context,
    shell: u32,
    identities: &BTreeMap<String, Value>,
    commits: i64,
    offset: u64,
) -> Result<Value> {
    let state = wait("switcher preview", || {
        let v = status(ctx, shell)?;
        Ok((v["picker"]["visible"] == true).then_some(v))
    })?;
    ensure(
        matches!(focused(ctx, identities)?.as_deref(), None | Some("A1")),
        "Preview changed native document focus",
    )?;
    ensure(state["picker"]["commits"] == commits, "Preview committed")?;
    pause(150);
    let bytes = fs::read(ctx.opt.output.join("native-events.jsonl"))?;
    for line in bytes
        .get(offset as usize..)
        .unwrap_or_default()
        .split(|b| *b == b'\n')
        .filter(|l| !l.is_empty())
    {
        let event: Value = serde_json::from_slice(line)?;
        if event["change"] == "focus" {
            ensure(
                event["container"]["id"] == identities["A1"][0],
                "Preview focused another native window",
            )?;
        }
    }
    Ok(state)
}
fn release(ctx: &mut Context, index: usize) -> Result<()> {
    let status = ctx.procs[index].0.wait()?;
    ensure(
        status.success(),
        format!("Physical key sequence exited {status}"),
    )
}
fn kill_window(ctx: &Context, title: &str) -> Result<()> {
    let node = ctx
        .find(title)?
        .ok_or_else(|| fail(format!("Missing target {title}")))?;
    ctx.named("swaymsg", &[&format!("[con_id={}]", node["id"]), "kill"])?;
    wait(&format!("{title} removed"), || {
        Ok(ctx.find(title)?.is_none().then_some(()))
    })
}
pub fn run(ctx: &mut Context) -> Result<()> {
    let out = ctx.opt.output.clone();
    for (name, value) in [
        ("QT_QPA_PLATFORM", "wayland"),
        ("QT_QPA_PLATFORMTHEME", "generic"),
        ("QT_QUICK_BACKEND", "software"),
        ("QT_ACCESSIBILITY", "0"),
        ("NO_AT_BRIDGE", "1"),
        ("GTK_A11Y", "none"),
        ("WLR_BACKENDS", "headless"),
        (
            "WLR_RENDERER",
            if ctx.opt.effects { "gles2" } else { "pixman" },
        ),
        (
            "WLR_RENDERER_FORCE_SOFTWARE",
            if ctx.opt.effects { "1" } else { "0" },
        ),
        ("WLR_RENDERER_ALLOW_SOFTWARE", "1"),
        ("LIBGL_ALWAYS_SOFTWARE", "1"),
        ("WLR_LIBINPUT_NO_DEVICES", "1"),
        ("GLOBAL_MENU_ENABLED", "0"),
        ("LUMEN_FILE_SERVICE", "0"),
        ("LUMEN_LAUNCH_SCOPES", "0"),
    ] {
        ctx.vars.insert(name.into(), value.into());
    }
    for name in ["WAYLAND_DISPLAY", "SWAYSOCK", "DISPLAY"] {
        ctx.vars.remove(name);
    }
    let assets = out.join("assets");
    copy_tree(
        Path::new(
            ctx.vars
                .get("LUMEN_DESKTOP_ASSETS")
                .ok_or("Installed assets missing")?,
        ),
        &assets,
    )?;
    let source = assets.join("shell");
    ctx.named(
        "lumen-fixture-providers",
        &["isolate", &path(&source), &path(&out)],
    )?;
    let qml = source.join("shell.qml");
    let original = fs::read_to_string(&qml)?;
    let needle = "        window.activate();";
    ensure(
        original.matches(needle).count() == 1,
        "Installed activation seam changed",
    )?;
    let mut modified = original.replacen(
        needle,
        "        if(!switchAuditSuppressActivation)window.activate();",
        1,
    );
    let end = modified.rfind('}').ok_or("Shell root missing")?;
    modified.insert_str(end,"\n property bool switchAuditSuppressActivation:false\n IpcHandler {target:\"switchAudit\"\n function suppressActivation(value:bool):void{root.switchAuditSuppressActivation=value;}\n function focusCaptured(title:string):void{const target=root.windows.find(w=>w.title===title);if(target)root.focusWindow(target);}\n function status():string{return JSON.stringify({picker:JSON.parse(applicationSwitcher.status()),active:root.active?root.active.title:\"\",nativeObservation:root.nativeWindowObservation,error:root.applicationCommandError,windows:root.windows.map(w=>({title:w.title,app:w.appId,minimized:w.minimized})),creation:root.windowCreationOrder.map(w=>w.title)});}\n }\n");
    fs::write(&qml, modified)?;
    let pidfile = out.join("service.pid");
    let locator = out.join("systemctl-private");
    fs::write(
        &locator,
        format!(
            "#!{}\ndef --wrapped main [...args: string] {{ exec cat {} }}\n",
            which("nu", &ctx.vars)?.display(),
            pidfile.display()
        ),
    )?;
    fs::set_permissions(&locator, fs::Permissions::from_mode(0o700))?;
    let action_path = ctx.opt.action.as_ref().ok_or("Installed action required")?;
    let action_text = fs::read_to_string(action_path)?;
    let re = Regex::new(r"/nix/store/[^\s]+/bin/lumen-shell-ipc")?;
    let ipc_path = re
        .find(&action_text)
        .ok_or("Installed router path missing")?
        .as_str();
    let original_ipc = fs::read_to_string(ipc_path)?;
    ensure(
        original_ipc.matches("--systemctl ").count() == 1,
        "Installed router seam changed",
    )?;
    let systemctl = Regex::new(r"--systemctl (/nix/store/[^ ]+/bin/systemctl)")?
        .captures(&original_ipc)
        .ok_or("Pinned systemctl missing")?[1]
        .to_owned();
    let modified_ipc = original_ipc.replacen(&systemctl, &path(&locator), 1);
    ensure(
        modified_ipc.replacen(&path(&locator), &systemctl, 1) == original_ipc,
        "Router transformation not reversible",
    )?;
    let ipc = out.join("shell-ipc");
    fs::write(&ipc, &modified_ipc)?;
    fs::set_permissions(&ipc, fs::Permissions::from_mode(0o700))?;
    let action = out.join("desktop-action");
    let modified_action = action_text.replace(ipc_path, &path(&ipc));
    ensure(
        modified_action.replace(&path(&ipc), ipc_path) == action_text,
        "Action transformation not reversible",
    )?;
    fs::write(&action, &modified_action)?;
    fs::set_permissions(&action, fs::Permissions::from_mode(0o700))?;
    save(
        &out.join("routing-manifest.json"),
        &json!({"action":action_path,"originalRouter":ipc_path,"systemctl":systemctl,
        "privateLocator":locator,"originalSha256":hash(original_ipc.as_bytes()),"modifiedSha256":hash(modified_ipc.as_bytes()),"actionDispatchPreserved":true}),
    )?;
    let binds = [
        ("Mod4+Tab", "app-switcher"),
        ("Mod4+Shift+Tab", "app-switcher-reverse"),
        ("--release Super_L", "app-switcher-commit"),
        ("--release Super_R", "app-switcher-commit"),
        ("Mod4+grave", "window-next"),
        ("Mod4+Shift+grave", "window-previous"),
    ];
    let mut config="output HEADLESS-1 resolution 1280x900\nseat seat0 fallback true\nxwayland disable\nfocus_follows_mouse no\nfor_window [all] floating enable\n".to_owned();
    for (key, command) in binds {
        config.push_str(&format!(
            "bindsym {key} exec {} {command}\n",
            action.display()
        ));
    }
    let swayconf = out.join("sway.conf");
    fs::write(&swayconf, config)?;
    let sway = ctx.opt.sway.clone();
    ctx.launch(
        &sway,
        &["--unsupported-gpu", "-c", &path(&swayconf)],
        "sway.log",
    )?;
    let runtime = ctx.root.0.clone();
    ctx.vars.insert(
        "SWAYSOCK".into(),
        path(&wait("compositor socket", || {
            first(&runtime, "sway-ipc", ".lock")
        })?),
    );
    ctx.vars.insert(
        "WAYLAND_DISPLAY".into(),
        wait("Wayland display", || {
            Ok(first(&runtime, "wayland-", ".lock")?
                .map(|p| p.file_name().unwrap().to_string_lossy().into_owned()))
        })?,
    );
    ctx.launch(
        Path::new("swaymsg"),
        &["-m", "-r", "-t", "subscribe", "[\"window\"]"],
        "native-events.jsonl",
    )?;
    ctx.launch(
        &ctx.opt.wtype.clone(),
        &["-s", "60000"],
        "keyboard-seat.log",
    )?;
    let shell_index = ctx.launch(
        Path::new("quickshell"),
        &["-p", &path(&source)],
        "shell.log",
    )?;
    let shell = ctx.procs[shell_index].0.id();
    fs::write(&pidfile, shell.to_string())?;
    let mut identities: BTreeMap<String, Value> = BTreeMap::new();
    let mut results = Map::new();
    wait("full shell", || {
        Ok((status(ctx, shell)?["picker"]["phase"] == "closed").then_some(()))
    })?;
    for (title, id) in [
        ("A1", "lumen-fixture-a"),
        ("A2", "lumen-fixture-a"),
        ("A3", "lumen-fixture-a"),
        ("B", "lumen-fixture-b"),
        ("C", "lumen-fixture-c"),
    ] {
        app(ctx, shell, &mut identities, title, id)?;
    }
    for title in ["C", "B", "A1"] {
        focus(ctx, shell, title)?;
    }
    wait("initial native observation",||{
        let s=status(ctx,shell)?;
        let observation=&s["nativeObservation"];
        Ok((observation["source"]["conId"]==identities["A1"][0] && observation["captures"].as_array().is_some_and(|r|r.len()==5)).then_some(()))
    })?;
    let (proc, commits, offset) = held(ctx, shell, false, 1)?;
    let previewed = preview(ctx, shell, &identities, commits, offset)?;
    native_selection(&previewed,&identities,"B")?;
    ensure(
        previewed["picker"]["key"] == "lumen-fixture-b",
        "Forward preview wrong app",
    )?;
    release(ctx, proc)?;
    let mut trace=vec![previewed];
    let forward=wait("forward release B", || {
        let state=status(ctx,shell)?;
        if trace.last()!=Some(&state){if trace.len()==16{trace.remove(0);}trace.push(state);}
        Ok((focused(ctx, &identities)?.as_deref() == Some("B")).then_some(()))
    });
    save(&out.join("forward-release-trace.json"),&json!(trace))?;
    forward?;
    results.insert("forward".into(), status(ctx, shell)?);
    for title in ["C", "B", "A1"] {
        focus(ctx, shell, title)?;
    }
    let (proc, commits, offset) = held(ctx, shell, true, 1)?;
    let previewed = preview(ctx, shell, &identities, commits, offset)?;
    native_selection(&previewed,&identities,"C")?;
    ensure(
        previewed["picker"]["key"] == "lumen-fixture-c",
        "Reverse preview wrong app",
    )?;
    release(ctx, proc)?;
    wait("reverse release C", || {
        Ok((focused(ctx, &identities)?.as_deref() == Some("C")).then_some(()))
    })?;
    results.insert("reverse".into(), status(ctx, shell)?);
    for title in ["C", "B", "A1"] {
        focus(ctx, shell, title)?;
    }
    let (proc, commits, offset) = held(ctx, shell, false, 1)?;
    native_selection(&preview(ctx, shell, &identities, commits, offset)?,&identities,"B")?;
    ctx.keys(&["-k", "Escape"])?;
    release(ctx, proc)?;
    wait("cancelled picker", || {
        Ok((status(ctx, shell)?["picker"]["phase"] == "cancelled").then_some(()))
    })?;
    ensure(
        focused(ctx, &identities)?.as_deref() == Some("A1"),
        "Escape changed native focus",
    )?;
    results.insert("cancel".into(), status(ctx, shell)?);
    focus(ctx, shell, "A1")?;
    for target in ["A2", "A3", "A1"] {
        ctx.keys(&["-M", "logo", "-k", "grave", "-m", "logo"])?;
        wait(&format!("same-app next {target}"), || {
            Ok((focused(ctx, &identities)?.as_deref() == Some(target)).then_some(()))
        })?;
    }
    ctx.keys(&[
        "-M", "logo", "-M", "shift", "-k", "grave", "-m", "shift", "-m", "logo",
    ])?;
    wait("same-app previous", || {
        Ok((focused(ctx, &identities)?.as_deref() == Some("A3")).then_some(()))
    })?;
    results.insert("cycle".into(), status(ctx, shell)?);
    audit(ctx, shell, "suppressActivation", &["true"])?;
    focus(ctx, shell, "A1")?;
    audit(ctx, shell, "focusCaptured", &["A2"])?;
    focus(ctx, shell, "C")?;
    pause(300);
    ensure(
        focused(ctx, &identities)?.as_deref() == Some("C"),
        "Delayed fallback overrode newer focus",
    )?;
    results.insert("fallbackCancelled".into(), status(ctx, shell)?);
    audit(ctx, shell, "focusCaptured", &["A2"])?;
    wait("legitimate fallback A2", || {
        Ok((focused(ctx, &identities)?.as_deref() == Some("A2")).then_some(()))
    })?;
    results.insert("fallbackLegitimate".into(), status(ctx, shell)?);
    audit(ctx, shell, "focusCaptured", &["A3"])?;
    kill_window(ctx, "A3")?;
    focus(ctx, shell, "C")?;
    pause(300);
    ensure(
        focused(ctx, &identities)?.as_deref() == Some("C"),
        "Destroyed target stole focus",
    )?;
    results.insert("fallbackDestroyed".into(), status(ctx, shell)?);
    audit(ctx, shell, "focusCaptured", &["A2"])?;
    kill_window(ctx, "A2")?;
    app(
        ctx,
        shell,
        &mut identities,
        "A2replacement",
        "lumen-fixture-a",
    )?;
    focus(ctx, shell, "C")?;
    pause(300);
    ensure(
        focused(ctx, &identities)?.as_deref() == Some("C"),
        "Replacement target stole focus",
    )?;
    results.insert("fallbackReplacement".into(), status(ctx, shell)?);
    audit(ctx, shell, "suppressActivation", &["false"])?;
    let node = ctx.find("A2replacement")?.ok_or("Replacement missing")?;
    ctx.named(
        "swaymsg",
        &[
            &format!("[con_id={}]", node["id"]),
            "lumen_window",
            "minimize",
        ],
    )?;
    wait("replacement minimized", || {
        Ok((ctx
            .find("A2replacement")?
            .is_some_and(|n| n["lumen_state"]["minimised"] == true))
        .then_some(()))
    })?;
    focus(ctx, shell, "A1")?;
    ctx.keys(&["-M", "logo", "-k", "grave", "-m", "logo"])?;
    wait("cycle restores minimized", || {
        Ok((focused(ctx, &identities)?.as_deref() == Some("A2replacement")).then_some(()))
    })?;
    ensure(
        ctx.find("A2replacement")?
            .is_some_and(|n| n["lumen_state"]["minimised"] == false),
        "Cycle left window minimized",
    )?;
    results.insert("minimizedCycle".into(), status(ctx, shell)?);
    for title in ["C", "B", "A1"] {
        focus(ctx, shell, title)?;
    }
    let (proc, commits, offset) = held(ctx, shell, false, 1)?;
    native_selection(&preview(ctx, shell, &identities, commits, offset)?,&identities,"B")?;
    kill_window(ctx, "B")?;
    release(ctx, proc)?;
    wait("removed app cancels", || {
        Ok((status(ctx, shell)?["picker"]["phase"] == "cancelled").then_some(()))
    })?;
    wait("removed app restores A1", || {
        Ok((focused(ctx, &identities)?.as_deref() == Some("A1")).then_some(()))
    })?;
    results.insert("destroyed".into(), status(ctx, shell)?);
    app(ctx, shell, &mut identities, "B2", "lumen-fixture-b")?;
    for title in ["C", "B2", "A1"] {
        focus(ctx, shell, title)?;
    }
    let (proc, commits, offset) = held(ctx, shell, false, 1)?;
    native_selection(&preview(ctx, shell, &identities, commits, offset)?,&identities,"B2")?;
    kill_window(ctx, "B2")?;
    app(ctx, shell, &mut identities, "B3", "lumen-fixture-b")?;
    // Mapping B3 can focus it independently of the switcher. Restore the
    // captured source beneath the overlay so a stale replacement dispatch
    // cannot masquerade as successful refusal.
    ctx.focus("A1")?;
    release(ctx, proc)?;
    wait("same-app replacement refused", || {
        let state=status(ctx,shell)?;
        Ok((state["picker"]["phase"]=="cancelled" && state["picker"]["commits"]==commits && focused(ctx, &identities)?.as_deref() == Some("A1")).then_some(()))
    })?;
    results.insert("replacementRefused".into(), status(ctx, shell)?);
    let node = ctx.find("B3")?.ok_or("B3 missing")?;
    ctx.named(
        "swaymsg",
        &[
            &format!("[con_id={}]", node["id"]),
            "lumen_window",
            "minimize",
        ],
    )?;
    wait("B3 minimized", || {
        Ok(ctx
            .find("B3")?
            .filter(|n| n["lumen_state"]["minimised"] == true)
            .map(|_| ()))
    })?;
    for title in ["C", "A1"] {
        focus(ctx, shell, title)?;
    }
    let (proc, commits, offset) = held(ctx, shell, false, 1)?;
    let previewed = preview(ctx, shell, &identities, commits, offset)?;
    ensure(
        previewed["picker"]["key"] == "lumen-fixture-c",
        "Minimized preview start wrong",
    )?;
    ctx.keys(&["-k", "Tab"])?;
    wait("minimized app selection", || {
        Ok((status(ctx, shell)?["picker"]["key"] == "lumen-fixture-b").then_some(()))
    })?;
    native_selection(&status(ctx,shell)?,&identities,"B3")?;
    ensure(
        status(ctx, shell)?["picker"]["commits"] == commits,
        "Minimized preview committed early",
    )?;
    release(ctx, proc)?;
    wait("restore minimized B3", || {
        Ok((focused(ctx, &identities)?.as_deref() == Some("B3")).then_some(()))
    })?;
    ensure(
        ctx.find("B3")?
            .is_some_and(|n| n["lumen_state"]["minimised"] == false),
        "Minimized app stayed minimized",
    )?;
    results.insert("minimizedApp".into(), status(ctx, shell)?);
    results.insert("nativeIdentities".into(), serde_json::to_value(identities)?);
    save(&out.join("result.json"), &Value::Object(results))?;
    println!("PASS installed native switcher and same-app cycling");
    Ok(())
}
