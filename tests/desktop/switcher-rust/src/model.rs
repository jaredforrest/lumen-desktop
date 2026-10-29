use super::*;

fn ipc(ctx: &Context, fixture: &Path, method: &str) -> Result<String> {
    ctx.named(
        "quickshell",
        &["-p", &path(fixture), "ipc", "call", "fixture", method],
    )
}
fn state(ctx: &Context, fixture: &Path) -> Result<Value> {
    Ok(serde_json::from_str(&ipc(ctx, fixture, "status")?)?)
}
fn focused_app(ctx: &Context) -> Result<Option<String>> {
    Ok(ctx
        .windows()?
        .into_iter()
        .find(|w| w["focused"] == true)
        .and_then(|w| w["app_id"].as_str().map(str::to_owned)))
}
pub fn run(ctx: &mut Context) -> Result<()> {
    let out = ctx.opt.output.clone();
    for (name, value) in [
        ("WLR_BACKENDS", "headless"),
        ("WLR_RENDERER", "pixman"),
        ("WLR_LIBINPUT_NO_DEVICES", "1"),
        ("QT_QPA_PLATFORM", "wayland"),
        ("QT_QUICK_BACKEND", "software"),
        ("GDK_BACKEND", "wayland"),
        ("QT_QPA_PLATFORMTHEME", ""),
        ("QT_ACCESSIBILITY", "0"),
        ("QT_LINUX_ACCESSIBILITY_ALWAYS_ON", "0"),
        ("NO_AT_BRIDGE", "1"),
        ("GTK_A11Y", "none"),
    ] {
        ctx.vars.insert(name.into(), value.into());
    }
    let source = env::current_dir()?.join("res/lumen-desktop/shell");
    let mut fixture = out.join("fixture");
    fs::create_dir(&fixture)?;
    for name in ["AppSwitcher.qml", "AppSwitcherModel.js"] {
        fs::copy(source.join(name), fixture.join(name))?;
    }
    fs::write(
        fixture.join("qmldir"),
        "singleton Theme 1.0 Theme.qml\nAppSwitcher 1.0 AppSwitcher.qml\nAppIcon 1.0 AppIcon.qml\n",
    )?;
    fs::write(
        fixture.join("Theme.qml"),
        "pragma Singleton\nimport QtQuick\nQtObject {property string font:\"Sans\";property string outline:\"#999999\";property var colors:({background:\"#eeeeee\",outline:\"#999999\",surface:\"#ffffff\",accent:\"#0088ff\",text:\"#000000\"})}",
    )?;
    fs::write(
        fixture.join("AppIcon.qml"),
        "import QtQuick\nRectangle {property var iconSource;color:\"#ffffff\"}",
    )?;
    fs::write(fixture.join("shell.qml"), include_str!("../model.qml"))?;
    let service_pid = out.join("service.pid");
    let service_control = out.join("systemctl-fixture");
    fs::write(
        &service_control,
        format!(
            "#!{}\ndef --wrapped main [...args: string] {{ exec cat {} }}\n",
            which("nu", &ctx.vars)?.display(),
            service_pid.display()
        ),
    )?;
    fs::set_permissions(&service_control, fs::Permissions::from_mode(0o700))?;
    let ipc_bin = if let Some(program) = ctx.vars.get("LUMEN_SHELL_IPC_BIN") {
        PathBuf::from(program)
    } else {
        which("lumen-shell-ipc", &ctx.vars)?
    };
    let quickshell = which("quickshell", &ctx.vars)?;
    let binding = format!(
        "{} --systemctl {} --quickshell {} -- call fixture step",
        ipc_bin.display(),
        service_control.display(),
        quickshell.display()
    );
    let config = out.join("sway.conf");
    fs::write(
        &config,
        format!(
            "output HEADLESS-1 resolution 1000x700\nseat seat0 fallback true\nbindsym Mod4+Tab exec {binding} next\nbindsym --release Super_L exec {binding} commit\nbindsym --release Super_R exec {binding} commit\n"
        ),
    )?;
    let sway = ctx.opt.sway.clone();
    ctx.launch(
        &sway,
        &["--unsupported-gpu", "-c", &path(&config)],
        "sway.log",
    )?;
    let runtime = ctx.root.0.clone();
    ctx.vars.insert(
        "SWAYSOCK".into(),
        path(&wait("compositor socket", || {
            first(&runtime, "sway-ipc.", ".lock")
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
        Path::new("foot"),
        &["--app-id=a", "--title=A", "sleep", "600"],
        "app-a.log",
    )?;
    ctx.launch(
        Path::new("foot"),
        &["--app-id=b", "--title=B", "sleep", "600"],
        "app-b.log",
    )?;
    wait("two application windows", || {
        Ok((ctx.windows()?.len() == 2).then_some(()))
    })?;
    ctx.named("swaymsg", &["[app_id=\"a\"] focus"])?;
    ensure(
        focused_app(ctx)?.as_deref() == Some("a"),
        "Initial app focus incorrect",
    )?;
    let shell = ctx.launch(&quickshell, &["-p", &path(&fixture)], "shell.log")?;
    fs::write(&service_pid, ctx.procs[shell].0.id().to_string())?;
    wait("switcher closed", || {
        Ok((state(ctx, &fixture)?["picker"]["phase"] == "closed").then_some(()))
    })?;
    ctx.keys(&[
        "-M", "logo", "-P", "Super_L", "-k", "Tab", "-s", "350", "-p", "Super_L", "-m", "logo",
    ])?;
    let first = wait("first activation", || {
        let v = state(ctx, &fixture)?;
        Ok((v["activated"] == 1).then_some(v))
    })?;
    ensure(
        first["app"] == "b" && first["picker"]["phase"] == "committed",
        "First selection incorrect",
    )?;
    wait("first native focus", || {
        Ok((focused_app(ctx)?.as_deref() == Some("b")).then_some(()))
    })?;
    ctx.keys(&[
        "-M", "logo", "-P", "Super_L", "-k", "Tab", "-s", "250", "-k", "Tab", "-s", "250", "-p",
        "Super_L", "-m", "logo",
    ])?;
    let second = wait("second activation", || {
        let v = state(ctx, &fixture)?;
        Ok((v["activated"] == 2).then_some(v))
    })?;
    ensure(second["app"] == "a", "Repeated Tab selected wrong app")?;
    wait("second native focus", || {
        Ok((focused_app(ctx)?.as_deref() == Some("a")).then_some(()))
    })?;
    ctx.keys(&[
        "-M", "logo", "-P", "Super_L", "-k", "Tab", "-s", "250", "-k", "Escape", "-s", "100", "-p",
        "Super_L", "-m", "logo",
    ])?;
    let cancelled = wait("Escape cancel", || {
        let v = state(ctx, &fixture)?;
        Ok((v["picker"]["phase"] == "cancelled").then_some(v))
    })?;
    ensure(cancelled["activated"] == 2, "Escape activated preview")?;
    wait("cancelled native focus", || {
        Ok((focused_app(ctx)?.as_deref() == Some("a")).then_some(()))
    })?;
    ipc(ctx, &fixture, "raceEpoch")?;
    let epoch_cancelled = wait("epoch race cancellation", || {
        let v = state(ctx, &fixture)?;
        Ok((v["picker"]["phase"] == "cancelled").then_some(v))
    })?;
    ensure(
        epoch_cancelled["activated"] == 2,
        "Epoch race activated preview",
    )?;
    ipc(ctx, &fixture, "raceRemovedApp")?;
    let removed_cancelled = wait("removed app race cancellation", || {
        let v = state(ctx, &fixture)?;
        Ok((v["picker"]["phase"] == "cancelled").then_some(v))
    })?;
    ensure(
        removed_cancelled["activated"] == 2,
        "Removed app activated preview",
    )?;
    ctx.procs[shell].stop()?;
    let rebuilt = out.join("rebuilt-assets");
    copy_tree(&fixture, &rebuilt)?;
    fixture = rebuilt;
    let rebuilt_shell = ctx.launch(&quickshell, &["-p", &path(&fixture)], "rebuilt-shell.log")?;
    fs::write(&service_pid, ctx.procs[rebuilt_shell].0.id().to_string())?;
    wait("rebuilt switcher closed", || {
        Ok((state(ctx, &fixture)?["picker"]["phase"] == "closed").then_some(()))
    })?;
    ctx.keys(&[
        "-M", "logo", "-P", "Super_L", "-k", "Tab", "-s", "350", "-p", "Super_L", "-m", "logo",
    ])?;
    let after_rebuild = wait("rebuilt switcher activation", || {
        let v = state(ctx, &fixture)?;
        Ok((v["activated"] == 1).then_some(v))
    })?;
    ensure(
        after_rebuild["app"] == "b" && after_rebuild["picker"]["phase"] == "committed",
        "Rebuilt service routing incorrect",
    )?;
    save(
        &out.join("result.json"),
        &json!({"first":first,"second":second,"cancelled":cancelled,"epoch_cancelled":epoch_cancelled,"removed_cancelled":removed_cancelled,"after_rebuild":after_rebuild}),
    )?;
    println!(
        "PASS real Super release, app grouping, repeat and cancellation without preview activation"
    );
    Ok(())
}
