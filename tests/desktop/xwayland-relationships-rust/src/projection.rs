//! Actual managed X11 presentation; shares the private XRes/keyboard harness.
use super::*;

fn save(name: &str, value: &Value) -> Result<()> {
    fs::write(
        Path::new("/artifacts").join(name),
        serde_json::to_vec_pretty(value)?,
    )?;
    Ok(())
}
fn decimal(value: &Value, positive: bool) -> bool {
    value.as_str().is_some_and(|s| {
        s.len() <= 20
            && s.parse::<u64>()
                .is_ok_and(|n| (!positive || n > 0) && n.to_string() == s)
    })
}
fn metadata(node: &Value) -> Result<Value> {
    let p = &node["lumen_all_desktops"];
    require(
        p.as_object().is_some_and(|o| o.len() == 10)
            && [
                "version",
                "enabled",
                "supported",
                "projected",
                "reason",
                "revision",
                "origin_workspace",
                "origin_output",
                "effective_workspace",
                "effective_output",
            ]
            .iter()
            .all(|key| p.get(*key).is_some())
            && p["version"] == 1
            && ["enabled", "supported", "projected"]
                .iter()
                .all(|key| p[*key].is_boolean())
            && (p["reason"].is_null() || p["reason"].as_str().is_some_and(|s| s.len() <= 256))
            && decimal(&p["revision"], false),
        format!("Invalid projection metadata: {p}"),
    )?;
    for key in [
        "origin_workspace",
        "origin_output",
        "effective_workspace",
        "effective_output",
    ] {
        let place = &p[key];
        require(
            place.is_null()
                || (place.as_object().is_some_and(|o| o.len() == 2)
                    && decimal(&place["id"], true)
                    && place["name"].as_str().is_some_and(|s| {
                        !s.is_empty() && s.len() <= 4096 && !s.chars().any(char::is_control)
                    })),
            "Invalid native projection placement",
        )?;
    }
    if p["supported"] == true {
        require(
            [
                "origin_workspace",
                "origin_output",
                "effective_workspace",
                "effective_output",
            ]
            .iter()
            .all(|key| p[*key].is_object())
                && p["origin_output"] == p["effective_output"]
                && p["projected"] == (p["origin_workspace"] != p["effective_workspace"]),
            "Inconsistent native projection scope",
        )?;
    }
    Ok(p.clone())
}
fn state(
    f: &mut Fixture,
    who: usize,
    index: usize,
    enabled: bool,
    supported: bool,
) -> Result<Value> {
    let result = wait("actual X11 projection state", || {
        let node = f.mapped(who, index)?;
        let p = metadata(&node)?;
        Ok((p["enabled"] == enabled && p["supported"] == supported).then_some(node))
    });
    if result.is_err() {
        save("projection-failure-tree.json", &f.tree()?)?;
        save(
            "projection-failure-state.json",
            &json!({"client":who,"slot":index,"enabled":enabled,"supported":supported}),
        )?;
    }
    result
}
fn request(node: &Value, enabled: bool) -> Result<String> {
    let p = metadata(node)?;
    let foreign = node["foreign_toplevel_identifier"]
        .as_str()
        .ok_or("Missing X11 foreign identity")?;
    require(
        foreign.len() == 32
            && foreign
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
        "Invalid X11 foreign identity",
    )?;
    Ok(format!(
        "lumen_projection {} {foreign} {} {} {} {} {}",
        i(node, "id")?,
        i(node, "pid")?,
        p["origin_workspace"]["id"]
            .as_str()
            .ok_or("Missing origin workspace")?,
        p["origin_output"]["id"]
            .as_str()
            .ok_or("Missing origin output")?,
        p["revision"]
            .as_str()
            .ok_or("Missing projection revision")?,
        if enabled { "on" } else { "off" }
    ))
}
fn apply(f: &Fixture, node: &Value, enabled: bool) -> Result<()> {
    let response = f.ipc(&request(node, enabled)?)?;
    require(
        response
            .as_array()
            .is_some_and(|rows| rows.len() == 1 && rows[0]["success"] == true),
        "Native projection did not acknowledge exact command",
    )
}
fn refuse(f: &Fixture, command: &str, expected: &str, label: &str) -> Result<()> {
    let before = f.tree()?;
    let (status, raw) = f.call(&["-r", command])?;
    let receipt: Value = serde_json::from_str(&raw)?;
    let after = f.tree()?;
    save(
        &format!("{label}.json"),
        &json!({"command":command,"status":status.code(),"receipt":receipt,"before":before,"after":after}),
    )?;
    require(
        status.code() == Some(2)
            && receipt.as_array().is_some_and(|rows| {
                rows.len() == 1
                    && rows[0]["success"] == false
                    && rows[0]["parse_error"] == true
                    && rows[0]["error"] == expected
            }),
        "Wrong native refusal receipt",
    )?;
    require(
        before == after,
        "Rejected native projection mutated the full tree",
    )
}
fn current(f: &Fixture) -> Result<String> {
    let (status, raw) = f.call(&["-r", "-t", "get_workspaces"])?;
    require(status.success(), "Current workspace query failed")?;
    let rows: Value = serde_json::from_str(&raw)?;
    rows.as_array()
        .and_then(|rows| rows.iter().find(|row| row["focused"] == true))
        .and_then(|row| row["name"].as_str())
        .map(str::to_owned)
        .ok_or_else(|| fail("No focused workspace"))
}
fn identity(node: &Value) -> Value {
    json!({"id":node["id"],"pid":node["pid"],"xid":node["window"],"foreign":node["foreign_toplevel_identifier"],"rect":node["rect"]})
}
fn client_fullscreen(f: &mut Fixture, value: bool) -> Result<()> {
    let before = f.clients[0].state()?["windows"][0]["stateUpdates"]
        .as_u64()
        .ok_or("Missing actual property sequence")?;
    f.command(0, "fullscreen", 0, i64::from(value))?;
    wait("actual WM fullscreen property response", || {
        let state = f.clients[0].state()?;
        Ok((state["windows"][0]["stateUpdates"]
            .as_u64()
            .is_some_and(|n| n > before)
            && state["windows"][0]["fullscreen"] == value)
            .then_some(()))
    })
}
fn tiled_groups(f: &mut Fixture) -> Result<()> {
    f.ipc("workspace 4")?;
    let tiled = f.map(0, 2)?;
    f.ipc(&format!("[con_id={}] floating disable", i(&tiled, "id")?))?;
    f.map(0, 3)?;
    let groups = f.tree()?["window_groups"].clone();
    f.ipc(&format!("lumen_window groups {} on", groups["revision"]))?;
    let groups = f.tree()?["window_groups"].clone();
    save(
        "projection-group-precondition.json",
        &json!({"groups":groups,"tiled":f.mapped(0,2)?,"peer":f.mapped(0,3)?,"tree":f.tree()?}),
    )?;
    let group = groups["groups"]
        .as_array()
        .and_then(|rows| {
            rows.iter().find(|group| {
                group["members"]
                    .as_array()
                    .is_some_and(|members| members.iter().any(|member| member["id"] == tiled["id"]))
            })
        })
        .and_then(|group| group["id"].as_u64())
        .ok_or("Tiled X11 native group missing")?;
    let peer = f.mapped(0, 3)?;
    f.ipc(&format!(
        "lumen_window groups {} add {group} {} {}",
        groups["revision"],
        i(&peer, "id")?,
        i(&peer, "pid")?
    ))?;
    let groups = f.tree()?["window_groups"].clone();
    f.ipc(&format!(
        "lumen_window groups {} select {group}",
        groups["revision"]
    ))?;
    let groups = f.tree()?["window_groups"].clone();
    f.ipc(&format!(
        "lumen_window groups {} mode one_at_a_time",
        groups["revision"]
    ))?;
    f.focus(0, 3, None)?;
    f.type_only(0, 3, &[(0, 2), (0, 0), (0, 1), (1, 0)])?;
    let tiled = state(f, 0, 2, false, true)?;
    let peer = state(f, 0, 3, false, true)?;
    require(
        matches!(tiled["floating"].as_str(), Some("user_off" | "auto_off"))
            && tiled["lumen_state"]["group_hidden"] == true
            && peer["lumen_state"]["group_hidden"] == false,
        "Actual tiled/one-at-a-time precondition missing",
    )?;
    let before_groups = f.tree()?["window_groups"].clone();
    let before = json!([
        identity(&tiled),
        identity(&peer),
        tiled["floating"],
        tiled["lumen_state"],
        peer["lumen_state"]
    ]);
    apply(f, &tiled, true)?;
    apply(f, &peer, true)?;
    let a = state(f, 0, 2, true, true)?;
    let b = state(f, 0, 3, true, true)?;
    require(
        f.tree()?["window_groups"] == before_groups
            && json!([
                identity(&a),
                identity(&b),
                a["floating"],
                a["lumen_state"],
                b["lumen_state"]
            ]) == before,
        "X11 enable changed group masks, member geometry or tiled structure",
    )?;
    let origin = metadata(&a)?["origin_workspace"].clone();
    f.ipc("workspace 5")?;
    wait("actual grouped X11 effective placement", || {
        let a = state(f, 0, 2, true, true)?;
        let b = state(f, 0, 3, true, true)?;
        Ok((metadata(&a)?["effective_workspace"]["name"] == "5"
            && metadata(&b)?["effective_workspace"]["name"] == "5")
            .then_some(()))
    })?;
    let a = state(f, 0, 2, true, true)?;
    let b = state(f, 0, 3, true, true)?;
    require(
        metadata(&a)?["origin_workspace"] == origin
            && metadata(&b)?["origin_workspace"] == origin
            && a["lumen_state"]["group_hidden"] == true
            && b["lumen_state"]["group_hidden"] == false
            && identity(&a) == identity(&tiled)
            && identity(&b) == identity(&peer),
        "Projection changed native group origin, frame or independent one-at-a-time mask",
    )?;
    f.focus(0, 3, None)?;
    require(current(f)? == "5", "Grouped X11 focus jumped origin")?;
    f.type_only(0, 3, &[(0, 2), (0, 0), (0, 1), (1, 0)])?;
    let groups = f.tree()?["window_groups"].clone();
    f.ipc(&format!(
        "lumen_window groups {} mode all_together",
        groups["revision"]
    ))?;
    wait("all-together X11 masks", || {
        Ok((f.mapped(0, 2)?["lumen_state"]["group_hidden"] == false
            && f.mapped(0, 3)?["lumen_state"]["group_hidden"] == false)
            .then_some(()))
    })?;
    f.focus(0, 2, None)?;
    require(
        current(f)? == "5",
        "Tiled projected X11 activation jumped origin",
    )?;
    f.type_only(0, 2, &[(0, 3), (0, 0), (0, 1), (1, 0)])?;
    let a = state(f, 0, 2, true, true)?;
    let b = state(f, 0, 3, true, true)?;
    save(
        "projection-tiled-group.json",
        &json!({"beforeGroups":before_groups,"beforeTiled":tiled,"beforePeer":peer,"afterTiled":a,"afterPeer":b,"groups":f.tree()?["window_groups"]}),
    )?;
    apply(f, &a, false)?;
    // Clearing the focused member can change the effective workspace and the
    // remaining member's revision. Each separate actuation owns a fresh witness.
    let remaining = state(f, 0, 3, true, true)?;
    save(
        "projection-group-clear-recapture.json",
        &json!({"beforeFirstClear":b,"afterFirstClear":remaining}),
    )?;
    apply(f, &remaining, false)?;
    let groups = f.tree()?["window_groups"].clone();
    f.ipc(&format!("lumen_window groups {} off", groups["revision"]))?;
    f.checks.push("Managed tiled X11 projection retains exact tiled/native group structure and origin; one-at-a-time versus all-together masks remain independent, with real current-desktop key recipients in both modes");
    Ok(())
}
pub(super) fn run(f: &mut Fixture, options: &Options) -> Result<()> {
    let (status, raw) = f.call(&["-r", "-t", "get_version"])?;
    require(status.success(), "Missing actual version receipt")?;
    let version: Value = serde_json::from_str(&raw)?;
    save("projection-version.json", &version)?;
    let native = match version.pointer("/lumen_capabilities/all_desktops_projection") {
        None => false,
        Some(v) if v.as_u64() == Some(1) => true,
        _ => return Err(fail("Unsupported projection capability")),
    };
    f.focus(0, 0, None)?;
    f.type_only(0, 0, &[(1, 0)])?;
    let first = f.mapped(0, 0)?;
    if options.projection_groups {
        require(
            native && !options.without_xres,
            "Group-only causal case requires actual native projection and XRes",
        )?;
        tiled_groups(f)?;
        return Ok(());
    }
    let old_role = native
        && first["pid"].as_u64() == Some(u64::from(f.clients[0].child.id()))
        && first["lumen_all_desktops"]["enabled"] == false
        && first["lumen_all_desktops"]["supported"] == false
        && first["lumen_all_desktops"]["reason"] == "native-view-required";
    if !native || old_role {
        if old_role {
            metadata(&first)?;
            require(
                f.clients[0].state()?["xres"] == true,
                "Older role compatibility requires actual XRes ownership",
            )?;
        } else {
            require(
                first.get("lumen_all_desktops").is_none(),
                "Absent provider capability with unexplained native metadata",
            )?;
        }
        save(
            "projection-legacy-role.json",
            &json!({"capabilityPresent":native,"olderRole":old_role,"native":first}),
        )?;
        f.ipc(&format!("[con_id={}] sticky enable", i(&first, "id")?))?;
        f.ipc("workspace 2")?;
        f.focus(0, 0, None)?;
        f.type_only(0, 0, &[(1, 0)])?;
        let after = f.mapped(0, 0)?;
        require(
            after["sticky"] == true
                && after["id"] == first["id"]
                && after["pid"] == first["pid"]
                && after["window"] == first["window"],
            "Legacy sticky compatibility changed identity",
        )?;
        f.ipc(&format!("[con_id={}] sticky disable", i(&first, "id")?))?;
        f.checks.push("Genuine legacy provider (absent capability or exact known-PID older native-view-required role) retains floating sticky movement and exact physical key recipient; raw version and role metadata remain recorded");
        return Ok(());
    }
    if options.without_xres {
        let node = state(f, 0, 0, false, false)?;
        require(
            node["pid"] == 0 && f.clients[0].state()?["xres"] == false,
            "Unknown XRes PID became trusted",
        )?;
        refuse(
            f,
            &request(&node, true)?,
            "Projection unavailable",
            "unknown-xres-refusal",
        )?;
        f.type_only(0, 0, &[(1, 0)])?;
        f.checks.push("Actual XRes-disabled native provider refuses projection without mutating topology while ordinary X11 focus and key input remain usable");
        return Ok(());
    }
    require(
        first["pid"].as_u64() == Some(u64::from(f.clients[0].child.id()))
            && f.clients[0].state()?["xres"] == true,
        "Native X11 projection did not use actual XRes owner",
    )?;
    let first = state(f, 0, 0, false, true)?;
    let origin = metadata(&first)?;
    let retained = identity(&first);
    apply(f, &first, true)?;
    state(f, 0, 0, true, true)?;
    let second = f.mapped(1, 0)?;
    f.ipc(&format!(
        "[con_id={}] move container to workspace 2",
        i(&second, "id")?
    ))?;
    f.ipc("workspace 2")?;
    let projected = wait("X11 effective workspace two", || {
        let node = state(f, 0, 0, true, true)?;
        let p = metadata(&node)?;
        Ok((p["effective_workspace"]["name"] == "2" && p["projected"] == true).then_some(node))
    })?;
    require(
        identity(&projected) == retained
            && metadata(&projected)?["origin_workspace"] == origin["origin_workspace"],
        "X11 projection rewrote structural identity/frame/origin",
    )?;
    require(
        f.location(0, 0)?.as_deref() == origin["origin_workspace"]["name"].as_str(),
        "X11 structural workspace changed",
    )?;
    f.focus(0, 0, None)?;
    require(current(f)? == "2", "Projected X11 focus jumped to origin")?;
    f.type_only(0, 0, &[(1, 0)])?;
    save("projection-effective.json", &projected)?;
    f.checks.push("Managed X11 XRes PID, XID and foreign identity retain structural frame/origin while effective workspace changes; actual focus and physical input stay on current desktop");
    client_fullscreen(f, true)?;
    let suspended = state(f, 0, 0, true, false)?;
    require(
        suspended["fullscreen_mode"] == 1
            && current(f)?
                == origin["origin_workspace"]["name"]
                    .as_str()
                    .ok_or("Missing origin")?,
        "Foreground X11 fullscreen did not activate retained origin and suspend projection",
    )?;
    f.type_only(0, 0, &[(1, 0)])?;
    client_fullscreen(f, false)?;
    state(f, 0, 0, true, true)?;
    f.ipc("workspace 2")?;
    f.focus(1, 0, None)?;
    f.type_only(1, 0, &[(0, 0)])?;
    let before = f.tree()?;
    let property_before = f.clients[0].state()?["windows"][0]["stateUpdates"]
        .as_u64()
        .ok_or("Missing property sequence")?;
    f.command(0, "fullscreen", 0, 1)?;
    wait("acknowledged background fullscreen refusal", || {
        let state = f.clients[0].state()?;
        Ok((state["windows"][0]["stateUpdates"]
            .as_u64()
            .is_some_and(|n| n > property_before)
            && state["windows"][0]["fullscreen"] == false)
            .then_some(()))
    })?;
    let after = f.tree()?;
    save(
        "projection-background-fullscreen.json",
        &json!({"before":before,"after":after,"client":f.clients[0].state()?}),
    )?;
    require(
        before == after && current(f)? == "2" && f.focused(1, 0)?,
        "Background X11 fullscreen refusal mutated topology or focus",
    )?;
    f.type_only(1, 0, &[(0, 0)])?;
    f.checks.push("Real acknowledged EWMH fullscreen enters retained origin only for focused projected X11; suspension keeps usable input, exit restores eligibility, background request reasserts false property with full tree unchanged");
    let parent = f.xid(0, 0)?;
    f.command(0, "parent", 1, parent)?;
    f.map(0, 1)?;
    f.relation_wait("actual X11 owner", |r| r["parent"] == first["id"], 0, 1)?;
    let utility = wait("published owned X11 projection observation", || {
        let node = state(f, 0, 1, false, false)?;
        Ok((metadata(&node)?["revision"] != "0").then_some(node))
    })?;
    require(
        metadata(&utility)?["reason"] == "owned-family-pending",
        "Owned child must await its own explicit projection intent",
    )?;
    let pending_owner = state(f, 0, 0, true, false)?;
    require(
        metadata(&pending_owner)?["reason"] == "owned-family-pending",
        "Incomplete native family did not suspend owner",
    )?;
    let utility_identity = identity(&utility);
    save(
        "projection-owned-pending.json",
        &json!({"owner":pending_owner,"child":utility}),
    )?;
    apply(f, &utility, true)?;
    let enabled_utility = state(f, 0, 1, true, true)?;
    let enabled_owner = state(f, 0, 0, true, true)?;
    require(
        identity(&enabled_utility) == utility_identity
            && metadata(&enabled_owner)?["origin_workspace"] == origin["origin_workspace"]
            && metadata(&enabled_utility)?["origin_workspace"] == origin["origin_workspace"],
        "Explicit family completion changed source identity, frame or structural origin",
    )?;
    save(
        "projection-owned-enabled.json",
        &json!({"owner":enabled_owner,"child":enabled_utility}),
    )?;
    let parent_before_modal = metadata(&state(f, 0, 0, true, true)?)?;
    f.command(0, "modal", 1, 1)?;
    f.relation_wait("actual X11 modal", |r| r["modal"] == true, 0, 1)?;
    let suspended_parent = wait("observed parent modal suspension revision", || {
        let parent = state(f, 0, 0, true, false)?;
        let p = metadata(&parent)?;
        Ok((p["revision"] != parent_before_modal["revision"]
            && p["reason"] == "modal-owner-suspended")
            .then_some(p))
    })?;
    save(
        "projection-modal-readiness.json",
        &json!({"before":parent_before_modal,"suspended":suspended_parent}),
    )?;
    let utility = state(f, 0, 1, true, false)?;
    refuse(
        f,
        &request(&utility, true)?,
        "Projection unavailable",
        "modal-x11-refusal",
    )?;
    f.focus(0, 0, Some((0, 1)))?;
    f.type_only(0, 1, &[(0, 0), (1, 0)])?;
    state(f, 0, 1, true, false)?;
    f.command(0, "modal", 1, 0)?;
    f.relation_wait("actual X11 unmodal", |r| r["modal"] == false, 0, 1)?;
    state(f, 0, 0, true, true)?;
    state(f, 0, 1, true, true)?;
    f.command(0, "parent", 1, 0)?;
    f.relation_wait(
        "actual X11 independent child",
        |r| r["parent"].is_null(),
        0,
        1,
    )?;
    state(f, 0, 1, true, true)?;
    f.checks.push("Known owned family waits for child's own explicit native intent without inheriting flags; modal suspension retains both flags, refuses ON without topology mutation and permits real global modal activation; unmodal/unparent retain independent intent");
    let fresh = state(f, 0, 0, true, true)?;
    require(
        metadata(&fresh)?["revision"] != "18446744073709551615",
        "Stale revision negative has no distinct witness",
    )?;
    let mut stale = fresh.clone();
    stale["lumen_all_desktops"]["revision"] = json!("18446744073709551615");
    refuse(
        f,
        &request(&stale, false)?,
        "Projection witness unavailable",
        "stale-x11-clear",
    )?;
    apply(f, &fresh, false)?;
    let cleared = state(f, 0, 0, false, true)?;
    state(f, 0, 1, true, true)?;
    require(
        metadata(&cleared)?["origin_workspace"] == origin["origin_workspace"]
            && cleared["pid"] == first["pid"]
            && cleared["window"] == first["window"],
        "Checked clear lost origin or identity",
    )?;
    f.focus(0, 0, None)?;
    f.type_only(0, 0, &[(1, 0), (0, 1)])?;
    f.checks.push("Stale X11 revision clear is refused with full tree unchanged; fresh exact clear preserves live identity and ordinary origin focus/key routing");
    tiled_groups(f)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn decimal_identity_is_bounded_canonical_and_positive_where_required() {
        assert!(decimal(&json!("0"), false));
        assert!(!decimal(&json!("0"), true));
        assert!(decimal(&json!("18446744073709551615"), true));
        for bad in [
            json!("01"),
            json!(1),
            json!("18446744073709551616"),
            json!("-1"),
            json!(""),
        ] {
            assert!(!decimal(&bad, false));
        }
    }
}
