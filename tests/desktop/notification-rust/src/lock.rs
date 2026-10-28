use super::*;

fn redacted(fixture: &Fixture) -> Result<Value> {
    let value = fixture.snapshot()?;
    require(
        value["previewsVisible"] == false,
        "Locked previews remain visible",
    )?;
    for row in value["rows"]
        .as_array()
        .ok_or("Missing notification rows")?
    {
        require(
            row["image"].as_str().unwrap_or("").is_empty()
                && row["icon"].as_str().unwrap_or("").is_empty()
                && row["actions"].as_array().is_some_and(Vec::is_empty),
            "Locked notification retained media or actions",
        )?;
    }
    let body = value.to_string();
    require(
        !["Private subject", "Private body", "Private action"]
            .iter()
            .any(|text| body.contains(text)),
        "Locked notification leaked content",
    )?;
    Ok(value)
}
fn frames(path: &Path) -> Result<Vec<Value>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    Ok(BufReader::new(File::open(path)?)
        .lines()
        .filter_map(|line| line.ok().and_then(|text| serde_json::from_str(&text).ok()))
        .collect())
}
fn state_frame(fixture: &Fixture, values: &[Value]) -> Result<Value> {
    let tool = fixture
        .options
        .frame_tool
        .as_ref()
        .ok_or("Missing frame tool")?;
    Ok(serde_json::from_str(&call_input(
        &path(tool),
        &[],
        &fixture.vars,
        serde_json::to_string(values)?.as_bytes(),
    )?)?)
}
fn lock(fixture: &mut Fixture) -> Result<usize> {
    let client = path(
        fixture
            .options
            .lock_client
            .as_ref()
            .ok_or("Missing lock client")?,
    );
    let id = fixture.start(
        &format!("protocol-lock-{}.log", fixture.children.0.len()),
        &client,
        &[],
        true,
    )?;
    wait("native lock", || {
        Ok((fixture.snapshot()?["lock"] == json!({"known":true,"locked":true})).then_some(()))
    })?;
    Ok(id)
}
fn unlock(fixture: &mut Fixture, index: usize) -> Result<()> {
    fixture.children.0[index]
        .stdin
        .as_mut()
        .ok_or("Lock client stdin closed")?
        .write_all(b"u\n")?;
    fixture.children.0[index]
        .stdin
        .as_mut()
        .ok_or("Lock client stdin closed")?
        .flush()?;
    require(
        fixture.children.0[index].wait()?.success(),
        "Lock client failed",
    )?;
    wait("native unlock", || {
        Ok((fixture.snapshot()?["lock"] == json!({"known":true,"locked":false})).then_some(()))
    })?;
    Ok(())
}
fn ui(fixture: &Fixture) -> Result<Value> {
    fixture.ui("notificationAudit")
}
fn sensitive(value: &Value) -> bool {
    let text = value.to_string();
    ["Private subject", "Private body", "Private action"]
        .iter()
        .any(|marker| text.contains(marker))
}
pub fn run(fixture: &mut Fixture) -> Result<()> {
    let icon = fixture.private.join("Private-icon.png");
    fs::copy(fixture.assets.join("shell/wallpaper.png"), &icon)?;
    let icon_url = format!("file://{}", icon.display());
    let _first = fixture.notify("one", &icon_url, true)?;
    let old_frame = fixture.snapshot()?;
    require(
        old_frame.to_string().contains("Private body one"),
        "Initial body absent",
    )?;
    let palette = read(&fixture.assets.join("appearance/palette.json"))?;
    let appearance = fixture
        .private
        .join("XDG_STATE_HOME/lumen-desktop/appearance.json");
    fs::create_dir_all(appearance.parent().ok_or("Invalid appearance path")?)?;
    save(
        &appearance,
        &json!({"font":palette["font"],"palette":palette["light"],"preferences":{}}),
    )?;
    fixture
        .vars
        .insert("LUMEN_APPEARANCE_STATE".into(), path(&appearance));
    let qml = path(&fixture.qml);
    let _ui = fixture.start("ui.log", "quickshell", &["-p", &qml], false)?;
    wait("notification UI", || {
        let value = ui(fixture)?;
        Ok((value["available"] == true
            && value["accessible"]
                .as_str()
                .is_some_and(|s| s.contains("Private subject one")))
        .then_some(()))
    })?;
    let helper = fixture
        .vars
        .get("LUMEN_NOTIFICATION_HELPER")
        .ok_or("Missing helper")?
        .clone();
    let _watcher = fixture.start("transport.log", &helper, &["watch"], true)?;
    let held = lock(fixture)?;
    redacted(fixture)?;
    let transport = fixture.options.output.join("transport.log");
    wait("redacted lock transport", || {
        let rows = frames(&transport)?;
        if rows.is_empty() {
            return Ok(None);
        };
        Ok((state_frame(fixture, &rows)?["lock"]["locked"] == true).then_some(()))
    })?;
    let locked_position = frames(&transport)?
        .iter()
        .position(|row| row["lock"] == json!({"known":true,"locked":true}))
        .ok_or("Missing locked frame")?;
    wait("redacted UI", || {
        Ok((!sensitive(&ui(fixture)?)).then_some(()))
    })?;
    fixture.ipc("notificationAudit", "inject", &[&old_frame.to_string()])?;
    require(!sensitive(&ui(fixture)?), "Stale frame disclosed content")?;
    fixture.notify("two", &icon_url, true)?;
    redacted(fixture)?;
    wait("two redacted UI rows", || {
        Ok((ui(fixture)?["rows"]
            .as_array()
            .is_some_and(|v| v.len() == 2))
        .then_some(()))
    })?;
    require(!sensitive(&ui(fixture)?), "Locked UI disclosed body")?;
    wait("two transport rows", || {
        let rows = frames(&transport)?;
        if rows.is_empty() {
            return Ok(None);
        };
        Ok(state_frame(fixture, &rows)?["rows"]
            .as_array()
            .is_some_and(|v| v.len() == 2)
            .then_some(()))
    })?;
    require(
        frames(&transport)?[locked_position..]
            .iter()
            .all(|v| !sensitive(v)),
        "Locked transport leaked content",
    )?;
    let retained = fixture.private.join("history/history.json");
    let text = fs::read_to_string(&retained)?;
    require(
        text.contains("Private body one") && text.contains("Private body two"),
        "Retained history lost original content",
    )?;
    require(
        fs::metadata(&retained)?.permissions().mode() & 0o777 == 0o600
            && fs::metadata(retained.parent().ok_or("History parent")?)?
                .permissions()
                .mode()
                & 0o777
                == 0o700,
        "History permissions changed",
    )?;
    unlock(fixture, held)?;
    wait("unlocked UI", || {
        Ok(ui(fixture)?["accessible"]
            .as_str()
            .is_some_and(|s| s.contains("Private subject two"))
            .then_some(()))
    })?;
    let auth = fixture.private.join("assets/auth");
    fs::set_permissions(&auth, fs::Permissions::from_mode(0o700))?;
    let auth_qml = auth.join("lock.qml");
    fs::set_permissions(&auth_qml, fs::Permissions::from_mode(0o600))?;
    symlink(
        fixture.private.join("assets/shell/wallpaper.png"),
        auth.join("wallpaper.png"),
    )?;
    let text = fs::read_to_string(&auth_qml)?;
    let position = text
        .rfind('}')
        .ok_or("Production lock QML missing root end")?;
    fs::write(
        &auth_qml,
        format!(
            "{}\n IpcHandler {{target:\"lockFixture\";function unlock():void{{sessionLock.locked=false;Qt.quit();}}}}\n}}{}",
            &text[..position],
            &text[position + 1..]
        ),
    )?;
    let auth_text = path(&auth_qml);
    let locker = fixture.start(
        "production-lock.log",
        "quickshell",
        &["-p", &auth_text],
        false,
    )?;
    wait("production lock", || {
        Ok((fixture.snapshot()?["lock"]["locked"] == true).then_some(()))
    })?;
    redacted(fixture)?;
    call(
        "quickshell",
        &["-p", &auth_text, "ipc", "call", "lockFixture", "unlock"],
        &fixture.vars,
    )?;
    require(
        fixture.children.0[locker].wait()?.success(),
        "Production lock failed",
    )?;
    wait("production unlock", || {
        Ok((fixture.snapshot()?["lock"]["locked"] == false).then_some(()))
    })?;
    let fallback = fixture.start("fallback-lock.log", "swaylock", &["-c", "080b0f"], false)?;
    wait("fallback lock", || {
        Ok((fixture.snapshot()?["lock"]["locked"] == true).then_some(()))
    })?;
    redacted(fixture)?;
    fixture.children.signal(fallback, libc::SIGKILL);
    fixture.children.0[fallback].wait()?;
    require(
        fixture.snapshot()?["lock"]["locked"] == true,
        "Fallback client death unlocked",
    )?;
    redacted(fixture)?;
    let replacement = lock(fixture)?;
    unlock(fixture, replacement)?;
    fixture.preference("never")?;
    wait("never preference", || {
        Ok((fixture.snapshot()?["preview"] == "never").then_some(()))
    })?;
    redacted(fixture)?;
    wait("never UI", || Ok((!sensitive(&ui(fixture)?)).then_some(())))?;
    call(
        "grim",
        &[&path(
            &fixture.options.output.join("redacted-notification.png"),
        )],
        &fixture.vars,
    )?;
    fixture.preference("always")?;
    wait("always preference", || {
        Ok((fixture.snapshot()?["previewsVisible"] == true).then_some(()))
    })?;
    let held = lock(fixture)?;
    redacted(fixture)?;
    fixture.children.stop(fixture.server)?;
    wait("server shutdown UI", || {
        Ok((ui(fixture)?["available"] == false).then_some(()))
    })?;
    fixture
        .vars
        .insert("LUMEN_LOCK_PREVIEWS".into(), "1".into());
    fixture.server = fixture.start("notifications-host-allowed.log", &helper, &["serve"], false)?;
    wait("host allowed", || {
        let value = fixture.snapshot()?;
        Ok((value["previewsVisible"] == true && value["lock"]["locked"] == true).then_some(()))
    })?;
    require(
        fixture.snapshot()?.to_string().contains("Private body"),
        "Host override not visible",
    )?;
    fixture.children.stop(fixture.server)?;
    wait("host allowed shutdown", || {
        Ok((ui(fixture)?["available"] == false).then_some(()))
    })?;
    fixture
        .vars
        .insert("LUMEN_LOCK_PREVIEWS".into(), "0".into());
    fixture.server = fixture.start("notifications-host-denied.log", &helper, &["serve"], false)?;
    wait("host denied", || {
        Ok((fixture.snapshot()?["lock"]["locked"] == true).then_some(()))
    })?;
    redacted(fixture)?;
    unlock(fixture, held)?;
    let old_instance = fixture.snapshot()?["instance"].clone();
    let old_transport = ui(fixture)?["transport"].clone();
    fixture.children.stop(fixture.server)?;
    wait("cleared unavailable UI", || {
        let value = ui(fixture)?;
        Ok(
            (value["available"] == false && value["rows"].as_array().is_some_and(Vec::is_empty))
                .then_some(()),
        )
    })?;
    fixture.server = fixture.start("notifications-restarted.log", &helper, &["serve"], false)?;
    wait("fresh service", || {
        let value = fixture.snapshot()?;
        Ok((value["instance"] != old_instance && value["previewsVisible"] == true).then_some(()))
    })?;
    wait("fresh UI service", || {
        Ok((ui(fixture)?["instance"] == fixture.snapshot()?["instance"]).then_some(()))
    })?;
    let current = ui(fixture)?;
    let mut stale = old_frame.clone();
    stale["transport"] = json!(old_transport.to_string().trim_matches('"'));
    stale["revision"] = json!(current["revision"].as_i64().ok_or("Missing revision")? + 100);
    fixture.ipc("notificationAudit", "inject", &[&stale.to_string()])?;
    let after = ui(fixture)?;
    require(
        after["available"] == true
            && after["instance"] == current["instance"]
            && after["revision"] == current["revision"]
            && after["rows"] == current["rows"],
        "Stale transport frame changed current UI",
    )?;
    fixture.children.stop(fixture.compositor)?;
    wait("disconnect fail closed", || {
        Ok((fixture.snapshot()?["lock"] == json!({"known":false,"locked":true})).then_some(()))
    })?;
    redacted(fixture)?;
    fixture.start_compositor("sway-reconnected.log")?;
    let held = lock(fixture)?;
    redacted(fixture)?;
    unlock(fixture, held)?;
    fixture.children.stop(fixture.compositor)?;
    save(
        &fixture.options.output.join("result.json"),
        &json!({"passed":true,"checks":["real native session-lock acquire and unlock","production lock view and fallback client","abandoned lock remains protected","subject body images and action transport redaction","protected original retained history","UI accessible label and painted redacted card","always constrained by host lock policy","never hides while unlocked","transport owner restart clears stale UI","native disconnect fails closed and fresh reconnect restores authority"]}),
    )?;
    Ok(())
}
