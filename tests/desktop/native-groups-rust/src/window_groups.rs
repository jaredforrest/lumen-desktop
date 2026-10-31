use super::*;

pub fn setup(opt: Options) -> Result<Context> {
    fs::create_dir(&opt.output)?;
    let root = PrivateRoot::new()?;
    let runtime = root.0.join("runtime");
    fs::create_dir(&runtime)?;
    fs::set_permissions(&runtime, fs::Permissions::from_mode(0o700))?;
    let mut vars: BTreeMap<String, String> = env::vars().collect();
    vars.insert("HOME".into(), text(&root.0));
    vars.insert("XDG_RUNTIME_DIR".into(), text(&runtime));
    for name in [
        "XDG_CONFIG_HOME",
        "XDG_STATE_HOME",
        "XDG_CACHE_HOME",
        "XDG_DATA_HOME",
    ] {
        let dir = root.0.join(name);
        fs::create_dir(&dir)?;
        vars.insert(name.into(), text(&dir));
    }
    for name in [
        "SWAYSOCK",
        "WAYLAND_DISPLAY",
        "DISPLAY",
        "DBUS_SESSION_BUS_ADDRESS",
        "AT_SPI_BUS_ADDRESS",
    ] {
        vars.remove(name);
    }
    for (name, value) in [
        ("WLR_BACKENDS", "headless"),
        ("WLR_HEADLESS_OUTPUTS", "2"),
        ("WLR_RENDERER", if opt.effects { "gles2" } else { "pixman" }),
        (
            "WLR_RENDERER_FORCE_SOFTWARE",
            if opt.effects { "1" } else { "0" },
        ),
        ("WLR_RENDERER_ALLOW_SOFTWARE", "1"),
        ("LIBGL_ALWAYS_SOFTWARE", "1"),
        ("WLR_LIBINPUT_NO_DEVICES", "1"),
    ] {
        vars.insert(name.into(), value.into());
    }
    vars.insert(
        "DBUS_SYSTEM_BUS_ADDRESS".into(),
        format!("unix:path={}", runtime.join("no-system-bus").display()),
    );
    let swaymsg = opt
        .sway
        .parent()
        .ok_or("Compositor bin directory missing")?
        .join("swaymsg");
    let qml = root.0.join("unused.qml");
    let mut ctx = Context {
        opt,
        root,
        vars,
        swaymsg,
        qml,
        procs: Vec::new(),
        captured: Vec::new(),
        shell_pid: 0,
        pointer: None,
        held: false,
        position: (0., 0.),
    };
    let config = ctx.root.0.join("bus.conf");
    fs::write(
        &config,
        format!(
            "<busconfig><type>session</type><listen>unix:tmpdir={}</listen><policy context=\"default\"><allow own=\"*\"/><allow send_destination=\"*\"/><allow receive_sender=\"*\"/></policy></busconfig>",
            runtime.display()
        ),
    )?;
    let log = File::create(ctx.opt.output.join("bus.log"))?;
    let mut bus = Command::new("dbus-daemon")
        .args([
            "--nofork",
            "--print-address=1",
            &format!("--config-file={}", config.display()),
        ])
        .env_clear()
        .envs(&ctx.vars)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::from(log))
        .process_group(0)
        .spawn()?;
    let address = BufReader::new(bus.stdout.take().ok_or("Private bus stdout missing")?)
        .lines()
        .next()
        .ok_or("Private bus address missing")??;
    ensure(address.starts_with("unix:"), "Private bus address invalid")?;
    ctx.procs.push(Managed(bus));
    ctx.vars.insert("DBUS_SESSION_BUS_ADDRESS".into(), address);
    let config = ctx.root.0.join("sway.conf");
    fs::write(
        &config,
        "output HEADLESS-1 resolution 800x600\nxwayland disable\nseat seat0 fallback true\n",
    )?;
    let sway = ctx.opt.sway.clone();
    ctx.launch(
        &sway,
        &["--unsupported-gpu", "-c", &text(&config)],
        "sway.log",
        false,
    )?;
    ctx.vars.insert(
        "SWAYSOCK".into(),
        text(&wait("private compositor", || {
            first(&runtime, "sway-ipc", ".lock")
        })?),
    );
    ctx.vars.insert(
        "WAYLAND_DISPLAY".into(),
        wait("private Wayland display", || {
            Ok(first(&runtime, "wayland-", ".lock")?
                .map(|p| p.file_name().unwrap().to_string_lossy().into_owned()))
        })?,
    );
    Ok(ctx)
}
fn state(ctx: &Context) -> Result<Value> {
    ctx.groups()
}
fn group(ctx: &Context, n: usize) -> Result<Value> {
    ctx.group(n)
}
fn act(
    ctx: &Context,
    action: &str,
    tail: &str,
    success: bool,
    revision: Option<Value>,
) -> Result<()> {
    let rev = revision.unwrap_or(state(ctx)?["revision"].clone());
    let args = if tail.is_empty() {
        String::new()
    } else {
        format!(" {tail}")
    };
    ctx.sway(
        &format!("lumen_window groups {rev} {action}{args}"),
        success,
    )
}
fn placement(ctx: &Context, n: usize) -> Result<Value> {
    let row = ctx.window(n)?.ok_or("Native window missing")?;
    fn workspace(node: &Value, id: &Value, current: Option<String>) -> Option<String> {
        let current = if node["type"] == "workspace" {
            node["name"].as_str().map(str::to_owned)
        } else {
            current
        };
        if node["id"] == *id {
            return current;
        }
        for key in ["nodes", "floating_nodes"] {
            if let Some(children) = node[key].as_array() {
                for child in children {
                    if let Some(found) = workspace(child, id, current.clone()) {
                        return Some(found);
                    }
                }
            }
        }
        None
    }
    let tree = ctx.native("get_tree")?;
    Ok(
        json!({"workspace":workspace(&tree,&row["id"],None),"rect":row["rect"],"floating":row.get("floating")}),
    )
}
fn keyboard_recipient(ctx: &Context, target: usize) -> Result<()> {
    let log = ctx.opt.output.join("client.log");
    let before = fs::read_to_string(&log)?.len();
    ctx.key(&["x"])?;
    wait("keyboard recipient", || {
        let source = fs::read_to_string(&log)?;
        let keys = source
            .get(before..)
            .unwrap_or("")
            .lines()
            .filter(|line| line.starts_with("key ") && line.ends_with(" 1"))
            .collect::<Vec<_>>();
        if keys.is_empty() {
            return Ok(None);
        }
        ensure(
            keys.iter()
                .all(|line| line.split_whitespace().nth(1) == Some(&target.to_string()[..])),
            format!("Wrong keyboard recipient: {keys:?}"),
        )?;
        Ok(Some(()))
    })
}
fn group_hidden(group: &Value) -> Result<bool> {
    Ok(group["members"]
        .as_array()
        .ok_or("Members")?
        .iter()
        .all(|m| truth(&m["group_hidden"])))
}
fn no_group_hidden(row: &Value) -> Value {
    let mut value = row["lumen_state"].clone();
    if let Some(map) = value.as_object_mut() {
        map.remove("group_hidden");
    }
    value
}
pub fn run(ctx: &mut Context) -> Result<()> {
    ensure(
        truth(&ctx.native("get_version")?["native_window_groups"]),
        "Native group capability missing",
    )?;
    ensure(
        !truth(&state(ctx)?["enabled"]),
        "Groups enabled before fixture input",
    )?;
    ctx.launch(Path::new("wtype"), &["-s", "60000"], "keyboard.log", false)?;
    let client = ctx.opt.client.clone();
    let client_index = ctx.launch(&client, &[], "client.log", true)?;
    wait("four relationship windows", || {
        let tree = ctx.native("get_tree")?;
        let mut all = Vec::new();
        nodes(&tree, &mut all);
        Ok((all
            .iter()
            .filter(|row| {
                row["name"]
                    .as_str()
                    .is_some_and(|name| name.starts_with("Relationship "))
            })
            .count()
            == 4)
            .then_some(()))
    })?;
    ctx.sway("[app_id=\"org.example.*\"] floating enable", true)?;
    ctx.captured = (0..4)
        .map(|n| {
            ctx.window(n)?
                .ok_or_else(|| fail(format!("Missing Relationship {n}")))
        })
        .collect::<Result<Vec<_>>>()?;
    act(ctx, "on", "", true, None)?;
    wait("three native groups", || {
        Ok((state(ctx)?["groups"]
            .as_array()
            .is_some_and(|g| g.len() == 3))
        .then_some(()))
    })?;
    ensure(
        group(ctx, 0)?["id"] == group(ctx, 2)?["id"]
            && group(ctx, 0)?["id"] != group(ctx, 1)?["id"],
        "Native parent/group ownership incorrect",
    )?;
    let mut checks = vec!["Native parent follows owner; independent roots receive distinct groups"];
    let stale = state(ctx)?["revision"].clone();
    let g = group(ctx, 0)?["id"].clone();
    act(ctx, "select", &g.to_string(), true, None)?;
    act(
        ctx,
        "add",
        &format!("{g} {} {}", ctx.captured[1]["id"], ctx.captured[1]["pid"]),
        true,
        None,
    )?;
    ensure(
        group(ctx, 0)?["id"] == group(ctx, 1)?["id"]
            && group(ctx, 0)?["members"]
                .as_array()
                .is_some_and(|m| m.len() == 3),
        "Native add failed",
    )?;
    act(ctx, "off", "", false, Some(stale))?;
    ensure(truth(&state(ctx)?["enabled"]), "Stale off changed state")?;
    let pid = integer(&ctx.captured[3]["pid"])?;
    for wrong in [pid + 1, pid + 4_294_967_296] {
        act(
            ctx,
            "add",
            &format!("{g} {} {wrong}", ctx.captured[3]["id"]),
            false,
            None,
        )?;
    }
    act(
        ctx,
        "remove",
        &format!("{g} {} {}", ctx.captured[1]["id"], ctx.captured[1]["pid"]),
        true,
        None,
    )?;
    ensure(
        group(ctx, 1)?["id"] != group(ctx, 0)?["id"]
            && truth(&group(ctx, 1)?["selected"])
            && group_hidden(&group(ctx, 0)?)?,
        "Native remove/selection incorrect",
    )?;
    ctx.sway(&format!("[con_id={}] focus", ctx.captured[0]["id"]), true)?;
    wait("native group focus", || {
        Ok(truth(&group(ctx, 0)?["selected"]).then_some(()))
    })?;
    ensure(
        group(ctx, 0)?["members"]
            .as_array()
            .ok_or("Members")?
            .iter()
            .all(|m| !truth(&m["group_hidden"])),
        "Focused group hidden",
    )?;
    keyboard_recipient(ctx, 0)?;
    for (n, x) in [(0, 400), (1, 40), (2, 400), (3, 400)] {
        ctx.sway(
            &format!("[con_id={}] move position {x} 40", ctx.captured[n]["id"]),
            true,
        )?;
    }
    let before = (0..4)
        .map(|n| placement(ctx, n))
        .collect::<Result<Vec<_>>>()?;
    ctx.pointer_start()?;
    ctx.move_pointer(100., 100.)?;
    ctx.press()?;
    ctx.release()?;
    pause(80);
    ensure(
        !truth(&group(ctx, 1)?["selected"]),
        "Hidden group received pointer click",
    )?;
    ctx.sway(&format!("[con_id={}] focus", ctx.captured[0]["id"]), true)?;
    keyboard_recipient(ctx, 0)?;
    ctx.sway(&format!("[con_id={}] focus", ctx.captured[1]["id"]), true)?;
    wait("second group focus", || {
        Ok(truth(&group(ctx, 1)?["selected"]).then_some(()))
    })?;
    keyboard_recipient(ctx, 1)?;
    ctx.write(client_index, "modal\n")?;
    wait("actual modal child", || {
        Ok(
            truth(&ctx.window(2)?.ok_or("Modal missing")?["lumen_relationships"]["modal"])
                .then_some(()),
        )
    })?;
    ctx.sway(&format!("[con_id={}] focus", ctx.captured[0]["id"]), true)?;
    keyboard_recipient(ctx, 2)?;
    ensure(
        truth(&group(ctx, 0)?["selected"]) && group(ctx, 2)?["id"] == group(ctx, 0)?["id"],
        "Modal group routing incorrect",
    )?;
    ctx.write(client_index, "unmodal\n")?;
    wait("unmodal child", || {
        Ok(
            (!truth(&ctx.window(2)?.ok_or("Child missing")?["lumen_relationships"]["modal"]))
                .then_some(()),
        )
    })?;
    checks.push("Atomic add/remove/select, stale revision/PID rejection; real keyboard recipients and hidden-window pointer exclusion");
    act(ctx, "select", &group(ctx, 1)?["id"].to_string(), true, None)?;
    let owner_group = group(ctx, 0)?["id"].clone();
    ctx.sway(
        &format!("[con_id={}] lumen_window minimize", ctx.captured[0]["id"]),
        true,
    )?;
    ensure(
        group(ctx, 2)?["id"] == owner_group && group_hidden(&group(ctx, 2)?)?,
        "Minimised owner changed modal group",
    )?;
    ctx.sway(
        &format!("[con_id={}] scratchpad show", ctx.captured[0]["id"]),
        true,
    )?;
    ensure(
        group(ctx, 0)?["id"] == owner_group,
        "Scratchpad changed group ownership",
    )?;
    ctx.sway(
        &format!("[con_id={}] lumen_window minimize", ctx.captured[1]["id"]),
        true,
    )?;
    ctx.sway(
        &format!("[con_id={}] lumen_window hide", ctx.captured[3]["id"]),
        true,
    )?;
    let hidden = [
        no_group_hidden(&ctx.window(1)?.ok_or("Window 1 missing")?),
        no_group_hidden(&ctx.window(3)?.ok_or("Window 3 missing")?),
    ];
    act(ctx, "off", "", true, None)?;
    ensure(!truth(&state(ctx)?["enabled"]), "Group mode stayed enabled")?;
    for (index, n) in [1, 3].into_iter().enumerate() {
        let after = ctx.window(n)?.ok_or("Window missing")?;
        save(
            &ctx.opt.output.join(format!("group-off-independent-state-{n}.json")),
            &json!({"before":hidden[index],"after":no_group_hidden(&after),"nativeAfter":after,"groups":state(ctx)?}),
        )?;
        ensure(
            no_group_hidden(&after) == hidden[index],
            "Independent hide state changed",
        )?;
    }
    for n in [0, 2] {
        ensure(
            placement(ctx, n)? == before[n],
            format!("Native placement changed for {n}"),
        )?;
    }
    ctx.sway(
        &format!("[con_id={}] scratchpad show", ctx.captured[1]["id"]),
        true,
    )?;
    ctx.sway(
        &format!("[con_id={}] lumen_window unhide", ctx.captured[3]["id"]),
        true,
    )?;
    checks.push("Disabled mode preserves ordinary geometry and independent minimized/application-hidden reasons");
    act(ctx, "on", "", true, None)?;
    ctx.sway("lumen_window show-desktop-on", true)?;
    ensure(
        state(ctx)?["groups"]
            .as_array()
            .ok_or("Groups")?
            .iter()
            .all(|g| truth(&g["suspended"])),
        "Show Desktop failed to suspend groups",
    )?;
    act(ctx, "off", "", true, None)?;
    ctx.sway("lumen_window show-desktop-off", true)?;
    for n in 0..4 {
        ensure(
            ctx.window(n)?.ok_or("Window missing")?["pid"] == ctx.captured[n]["pid"],
            "Document PID changed",
        )?;
    }
    checks.push(
        "Show Desktop suspension and disable/restore retain all exact live document identities",
    );
    act(ctx, "on", "", true, None)?;
    ctx.sway(
        &format!("[con_id={}] sticky enable", ctx.captured[3]["id"]),
        true,
    )?;
    ensure(
        state(ctx)?["groups"]
            .as_array()
            .ok_or("Groups")?
            .iter()
            .all(|g| {
                g["members"]
                    .as_array()
                    .is_some_and(|m| m.iter().all(|member| member["id"] != ctx.captured[3]["id"]))
            })
            && !truth(
                &ctx.window(3)?.ok_or("Sticky window missing")?["lumen_state"]["group_hidden"],
            ),
        "Sticky window remained grouped",
    )?;
    ctx.sway(
        &format!("[con_id={}] sticky disable", ctx.captured[3]["id"]),
        true,
    )?;
    wait("unsticky group", || {
        Ok(state(ctx)?["groups"]
            .as_array()
            .filter(|groups| {
                groups.iter().any(|g| {
                    g["members"].as_array().is_some_and(|m| {
                        m.iter().any(|member| member["id"] == ctx.captured[3]["id"])
                    })
                })
            })
            .map(|_| ()))
    })?;
    checks.push("Sticky windows stay outside grouping; removing sticky status creates a fresh ordinary group");
    ctx.sway(
        &format!("[con_id={}] fullscreen enable", ctx.captured[0]["id"]),
        true,
    )?;
    ensure(
        truth(&group(ctx, 0)?["suspended"]),
        "Fullscreen group not suspended",
    )?;
    act(
        ctx,
        "select",
        &group(ctx, 0)?["id"].to_string(),
        false,
        None,
    )?;
    ctx.sway(
        &format!("[con_id={}] fullscreen disable", ctx.captured[0]["id"]),
        true,
    )?;
    ctx.sway(
        &format!(
            "lumen_pair create {} {}",
            ctx.captured[1]["id"], ctx.captured[3]["id"]
        ),
        true,
    )?;
    ensure(
        truth(&group(ctx, 1)?["suspended"]),
        "Paired group not suspended",
    )?;
    act(
        ctx,
        "select",
        &group(ctx, 1)?["id"].to_string(),
        false,
        None,
    )?;
    ctx.sway(&format!("lumen_pair leave {}", ctx.captured[1]["id"]), true)?;
    let locker = ctx.opt.locker.clone();
    let lock_index = ctx.launch(&locker, &[], "locker.log", true)?;
    wait("native session lock", || {
        Ok(truth(&state(ctx)?["locked"]).then_some(()))
    })?;
    act(ctx, "off", "", false, None)?;
    ensure(
        truth(&state(ctx)?["enabled"]),
        "Lock allowed group mutation",
    )?;
    ctx.write(lock_index, "u\n")?;
    wait("locker exit", || {
        Ok(ctx.procs[lock_index]
            .0
            .try_wait()?
            .filter(|s| s.success())
            .map(|_| ()))
    })?;
    wait("native unlock", || {
        Ok((!truth(&state(ctx)?["locked"])).then_some(()))
    })?;
    ensure(truth(&state(ctx)?["enabled"]), "Unlock disabled grouping")?;
    checks.push("Actual fullscreen/pair contexts suspend grouping; actual session lock rejects mutations until unlock");
    ctx.sway(
        &format!(
            "[con_id={}] move container to workspace 2",
            ctx.captured[0]["id"]
        ),
        true,
    )?;
    wait("native workspace split", || {
        Ok((group(ctx, 0)?["workspace"] != group(ctx, 1)?["workspace"]).then_some(()))
    })?;
    let g0 = group(ctx, 0)?;
    act(
        ctx,
        "add",
        &format!(
            "{} {} {}",
            g0["id"], ctx.captured[3]["id"], ctx.captured[3]["pid"]
        ),
        false,
        None,
    )?;
    ctx.sway("move workspace to output HEADLESS-2", true)?;
    let retained = group(ctx, 0)?["id"].clone();
    ctx.sway("output HEADLESS-2 disable", true)?;
    wait("retained group after output removal", || {
        Ok(state(ctx)?["groups"]
            .as_array()
            .filter(|g| g.iter().any(|row| row["id"] == retained))
            .map(|_| ()))
    })?;
    ensure(
        ctx.window(0)?.ok_or("Window 0 missing")?["pid"] == ctx.captured[0]["pid"],
        "Output removal replaced document",
    )?;
    ctx.write(client_index, "close-second\n")?;
    wait("destroyed second window", || {
        Ok(ctx.window(1)?.is_none().then_some(()))
    })?;
    ensure(
        state(ctx)?["groups"]
            .as_array()
            .ok_or("Groups")?
            .iter()
            .all(|g| {
                g["members"]
                    .as_array()
                    .is_some_and(|m| m.iter().all(|member| member["id"] != ctx.captured[1]["id"]))
            }),
        "Destroyed member remained",
    )?;
    let incoming = ctx.launch(
        Path::new("foot"),
        &["--app-id=org.example.GroupIncoming", "sleep", "60"],
        "incoming.log",
        false,
    )?;
    let new_window = wait("incoming native window", || {
        let tree = ctx.native("get_tree")?;
        let mut all = Vec::new();
        nodes(&tree, &mut all);
        Ok(all
            .into_iter()
            .find(|row| row["app_id"] == "org.example.GroupIncoming")
            .cloned())
    })?;
    wait("incoming selected group", || {
        Ok(state(ctx)?["groups"]
            .as_array()
            .and_then(|g| {
                g.iter()
                    .find(|group| {
                        group["members"].as_array().is_some_and(|m| {
                            m.iter().any(|member| member["id"] == new_window["id"])
                        })
                    })
                    .filter(|group| truth(&group["selected"]))
            })
            .map(|_| ()))
    })?;
    unsafe {
        libc::killpg(ctx.procs[incoming].0.id() as i32, libc::SIGTERM);
    }
    wait("incoming process exit", || {
        Ok(ctx.procs[incoming].0.try_wait()?.map(|_| ()))
    })?;
    wait("incoming group retirement", || {
        Ok(state(ctx)?["groups"]
            .as_array()
            .filter(|g| {
                g.iter().all(|group| {
                    group["members"]
                        .as_array()
                        .is_some_and(|m| m.iter().all(|member| member["id"] != new_window["id"]))
                })
            })
            .map(|_| ()))
    })?;
    checks.push("Incoming mapped window receives selected independent group; actual process exit removes membership");
    checks.push("Native workspace movement and destruction reconcile exact membership");
    act(ctx, "off", "", true, None)?;
    unsafe {
        libc::killpg(ctx.procs[1].0.id() as i32, libc::SIGTERM);
    }
    let status = wait("clean compositor shutdown", || {
        Ok(ctx.procs[1].0.try_wait()?)
    })?;
    ensure(status.success(), format!("Compositor exited {status}"))?;
    ctx.result(&checks)?;
    println!("PASS native window groups");
    Ok(())
}
