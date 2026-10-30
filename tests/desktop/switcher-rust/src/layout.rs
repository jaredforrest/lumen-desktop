//! Actual source/layout changes with Settings open and a controlled held gesture.
use super::*;
fn row(id: &str, action: &str, chord: &str, key: &str, modifiers: &[&str]) -> Value {
    json!({"id":id,"action":action,"chord":chord,"scope":"global","appId":"","mode":"default","owner":"desktop","enabled":true,"physical":{"key":key,"modifiers":modifiers,"trigger":"press","device":"*","locked":false,"inhibited":false}})
}
pub fn registry(ctx: &Context, rebound: bool) -> Result<()> {
    let switch = if rebound {
        row(
            "fixture.switch",
            "shell.app_switcher",
            "Mod4+F6",
            "f6",
            &["Mod4"],
        )
    } else {
        row(
            "fixture.switch",
            "shell.app_switcher",
            "Mod4+Tab",
            "tab",
            &["Mod4"],
        )
    };
    let plus = row(
        "fixture.plus",
        "window.fill",
        "Control+plus",
        "plus",
        &["Control"],
    );
    let rows = vec![switch, plus];
    let hints=rows.iter().map(|r|json!({"action":r["action"],"scope":"global","appId":"","bindings":[{"id":r["id"],"chord":r["chord"],"physical":r["physical"]}]})).collect::<Vec<_>>();
    let file = Path::new(
        ctx.vars
            .get("XDG_CONFIG_HOME")
            .ok_or("Config root missing")?,
    )
    .join("lumen-desktop/shortcuts.json");
    fs::create_dir_all(file.parent().ok_or("Config parent missing")?)?;
    save(
        &file,
        &json!({"version":1,"valid":true,"bindings":rows,"hints":hints}),
    )
}
fn audit(ctx: &Context, shell: u32, method: &str) -> Result<Value> {
    let output = ctx.named(
        "quickshell",
        &[
            "ipc",
            "--pid",
            &shell.to_string(),
            "call",
            "layoutAudit",
            method,
        ],
    )?;
    Ok(serde_json::from_str(&output)?)
}
pub fn run(
    ctx: &mut Context,
    shell: u32,
    identities: &BTreeMap<String, Value>,
    config: &Path,
    action: &Path,
) -> Result<()> {
    // wtype's idle-only virtual device has no meaningful layout. Select an
    // actual fixed-keymap protocol keyboard before asserting layout authority.
    ctx.vars.insert(
        "LUMEN_TEST_KEYMAP".into(),
        env!("LUMEN_LAYOUT_KEYMAP_US").into(),
    );
    let _initial_us_keyboard = held_keyboard::Held::select_layout(ctx)?;
    fs::write(
        ctx.opt.output.join("layout-helper-startup.json"),
        ctx.named("lumen-shortcut-layout", &[])?,
    )?;
    fs::write(
        ctx.opt.output.join("layout-provider-startup.json"),
        ctx.named("swaymsg", &["-r", "-t", "get_seats"])?,
    )?;
    if ctx.opt.effects {
        let log = fs::read_to_string(ctx.opt.output.join("sway.log"))?;
        let renderer = log
            .lines()
            .find(|line| line.contains("GL renderer:"))
            .ok_or("Effects provider renderer identity was not logged")?;
        ensure(
            renderer.contains("llvmpipe") || renderer.contains("softpipe"),
            format!("Effects provider did not prove software rendering: {renderer}"),
        )?;
        fs::write(
            ctx.opt.output.join("renderer-proof.txt"),
            format!("{renderer}\n"),
        )?;
    }
    let before = wait("native layout observation", || {
        let s = audit(ctx, shell, "status")?;
        save(&ctx.opt.output.join("layout-audit-startup.json"), &s)?;
        Ok((s["available"] == true && s["mode"] == "default").then_some(s))
    })?;
    ensure(
        before["rows"].as_array().is_some_and(|r| {
            r.iter()
                .any(|r| r["id"] == "fixture.plus" && r["collision"] == true)
        }),
        format!("US physical collision missing: {before}"),
    )?;
    ctx.named(
        "quickshell",
        &[
            "ipc",
            "--pid",
            &shell.to_string(),
            "call",
            "layoutAudit",
            "open",
        ],
    )?;
    let painted_us = wait("painted layout Settings", || {
        let s = audit(ctx, shell, "status")?;
        Ok((s["settings"] == true
            && s["keycapsVisible"] == true
            && s["painted"]
                .to_string()
                .contains("Physical shortcut collision"))
        .then_some(s))
    })?;
    ctx.named(
        "grim",
        &[
            "-o",
            "HEADLESS-1",
            &path(&ctx.opt.output.join("layout-settings-us.png")),
        ],
    )?;
    let commits = painted_us["picker"]["commits"].clone();
    ctx.vars.insert(
        "LUMEN_TEST_KEYMAP".into(),
        env!("LUMEN_LAYOUT_KEYMAP_DE").into(),
    );
    let german_keyboard = held_keyboard::Held::select_layout(ctx)?;
    let german = wait("German Settings label and collision recalculation", || {
        let s = audit(ctx, shell, "status")?;
        Ok((s["generation"] != before["generation"]
            && s["rows"].as_array().is_some_and(|r| {
                r.iter()
                    .any(|r| r["id"] == "fixture.plus" && r["collision"] == false)
            })
            && !s["painted"]
                .to_string()
                .contains("Physical shortcut collision"))
        .then_some(s))
    })?;
    ensure(
        german["keycapsVisible"] == true,
        "Updated physical labels are outside the actual Settings viewport",
    )?;
    ensure(
        before["keycaps"] != german["keycaps"],
        "Layout change retained stale physical keycaps",
    )?;
    ensure(
        painted_us["painted"]
            .to_string()
            .contains(before["keycaps"].as_str().ok_or("US keycaps unavailable")?),
        "Actual Settings did not paint US keycaps",
    )?;
    ensure(
        german["painted"].to_string().contains(
            german["keycaps"]
                .as_str()
                .ok_or("German keycaps unavailable")?,
        ),
        "Actual Settings did not paint changed German keycaps",
    )?;
    ensure(
        german["picker"]["commits"] == commits,
        "Layout change invoked switcher action",
    )?;
    ensure(
        !ctx.opt.output.join("unexpected-shortcut-action").exists(),
        "Layout change invoked bound command",
    )?;
    save(
        &ctx.opt.output.join("layout-settings-before.json"),
        &painted_us,
    )?;
    ctx.named(
        "grim",
        &[
            "-o",
            "HEADLESS-1",
            &path(&ctx.opt.output.join("layout-settings-de.png")),
        ],
    )?;
    save(&ctx.opt.output.join("layout-settings-after.json"), &german)?;
    ctx.named(
        "quickshell",
        &[
            "ipc",
            "--pid",
            &shell.to_string(),
            "call",
            "layoutAudit",
            "close",
        ],
    )?;
    ctx.vars.insert(
        "LUMEN_TEST_KEYMAP".into(),
        env!("LUMEN_LAYOUT_KEYMAP_US").into(),
    );
    let _us_keyboard = held_keyboard::Held::select_layout(ctx)?;
    drop(german_keyboard);
    wait("US keyboard restored before held-rebind", || {
        let s = audit(ctx, shell, "status")?;
        Ok((s["keycaps"] == before["keycaps"]).then_some(()))
    })?;
    super::installed::focus(ctx, shell, "A1")?;
    let (mut held, commits, offset) = super::installed::held(ctx, shell, false, 1)?;
    let preview = super::installed::preview(ctx, shell, identities, commits, offset)?;
    super::installed::native_selection(&preview, identities, "B")?;
    let mut text = fs::read_to_string(config)?;
    let old = format!("bindsym Mod4+Tab exec {} app-switcher", action.display());
    ensure(
        text.matches(&old).count() == 1,
        "Authoritative held-rebind seam changed",
    )?;
    text = text.replace(
        &old,
        &format!("bindsym Mod4+F6 exec {} app-switcher", action.display()),
    );
    fs::write(config, text)?;
    registry(ctx, true)?;
    ctx.named("swaymsg", &["reload"])?;
    let cancelled = wait("held old generation cancellation", || {
        let s = audit(ctx, shell, "status")?;
        Ok((s["picker"]["visible"] == false
            && s["picker"]["phase"] == "cancelled"
            && s["switchHint"].as_str().is_some_and(|v| v.contains("F6")))
        .then_some(s))
    })?;
    held.assert_held()?;
    ensure(
        cancelled["picker"]["commits"] == commits,
        "Held rebind committed old gesture",
    )?;
    held.release()?;
    ensure(
        audit(ctx, shell, "status")?["picker"]["commits"] == commits,
        "Old modifier release committed after cancellation",
    )?;
    let mut next = held_keyboard::Held::start(ctx, false, 0)?;
    next.key(64)?; // Linux KEY_F6; the helper adds the protocol's XKB offset.
    let preview = super::installed::preview(ctx, shell, identities, commits, offset)?;
    super::installed::native_selection(&preview, identities, "B")?;
    next.release()?;
    let final_state = wait("new authoritative rebind commits once", || {
        let s = audit(ctx, shell, "status")?;
        Ok((s["picker"]["commits"] == commits + 1 && s["active"] == "B").then_some(s))
    })?;
    save(
        &ctx.opt.output.join("held-layout-rebind.json"),
        &json!({"cancelled":cancelled,"final":final_state}),
    )?;
    ensure(
        !ctx.opt.output.join("unexpected-shortcut-action").exists(),
        "Layout/rebind unexpectedly invoked a configured command",
    )?;
    save(
        &ctx.opt.output.join("result.json"),
        &json!({"passed":true,"groups":3,"layoutLabelsAndCollision":true,"settingsPainted":true,"heldGenerationCancelled":true,"newMapCommittedOnce":true}),
    )?;
    println!(
        "PASS native keyboard layout, painted Settings collision recalculation and held rebind"
    );
    Ok(())
}
