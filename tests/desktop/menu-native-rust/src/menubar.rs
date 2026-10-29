use crate::harness::{Fixture, Options, Result, require, wait, wait_true};
use serde_json::{Value, json};
use std::fs;

fn window(state: &Value, app: &str) -> bool {
    state["windows"]
        .as_array()
        .is_some_and(|rows| rows.iter().any(|row| row["app"] == app))
}
fn titled(state: &Value, title: &str) -> bool {
    state["windows"]
        .as_array()
        .is_some_and(|rows| rows.iter().any(|row| row["title"] == title))
}
fn clean_shell(fixture: &Fixture) -> Result<()> {
    let log = fs::read_to_string(fixture.options.output.join("shell.log"))?;
    for error in [
        "TypeError:",
        "ReferenceError:",
        "Binding loop",
        "Failed to load configuration",
        "Cannot assign",
    ] {
        require(!log.contains(error), format!("Shell error {error}: {log}"))?;
    }
    Ok(())
}
pub fn run(options: Options) -> Result<(Vec<&'static str>, Value)> {
    let mut f = Fixture::new(options)?;
    f.client("menu-source")?;
    f.client("menu-bystander")?;
    f.open_menu("Application", "menu-source")?;
    f.key(&["-k", "Down", "-k", "Down"])?;
    f.ipc("retarget", &["menu-bystander"])?;
    f.key(&["-k", "Return"])?;
    wait_true("command dismissal", || Ok(f.state()?["menu"] == ""))?;
    require(
        window(&f.state()?, "menu-bystander"),
        "Bystander was closed",
    )?;
    wait_true("captured close source", || {
        Ok(!window(&f.state()?, "menu-source"))
    })?;
    let mut checks = vec![
        "Installed bridge imports the shared runtime and watches actual native focus",
        "Physical Return closes the captured original window despite mutable selectedWindow replacement; bystander remains",
    ];
    f.client("menu-held")?;
    f.open_menu("Application", "menu-held")?;
    let close = wait("pointer close row", || f.named("Close Window"))?;
    f.point(&close.reference)?;
    f.pointer_command("p")?;
    f.ipc("retarget", &["menu-bystander"])?;
    f.pointer_command("r")?;
    wait_true("pointer captured close", || {
        Ok(!window(&f.state()?, "menu-held"))
    })?;
    require(
        window(&f.state()?, "menu-bystander"),
        "Pointer close hit bystander",
    )?;
    checks.push(
        "Actual pointer press and release retain the original window rather than mutable selection",
    );
    let held = f.client("menu-held")?;
    f.open_menu("Application", "menu-held")?;
    let close = wait("held doomed close row", || f.named("Close Window"))?;
    f.point(&close.reference)?;
    f.pointer_command("p")?;
    f.stop(held)?;
    wait_true("removed held client", || {
        Ok(!window(&f.state()?, "menu-held"))
    })?;
    f.client("menu-held")?;
    f.pointer_command("r")?;
    let state = f.state()?;
    require(
        window(&state, "menu-held") && state["focuses"] == json!([]),
        format!("Destroyed pressed row retargeted: {state}"),
    )?;
    f.key(&["-k", "Escape"])?;
    wait_true("held cancellation Escape", || Ok(f.state()?["menu"] == ""))?;
    checks.push(
        "Destroyed pressed row cancels and cannot close a newly created same-app replacement",
    );
    let doomed_client = f.client("menu-doomed")?;
    f.client("menu-other")?;
    f.open_menu("Window", "menu-bystander")?;
    let doomed = wait("semantic native window row", || f.named("menu-doomed"))?;
    require(
        doomed.role == "menu item" && doomed.enabled,
        "Native window row is not enabled",
    )?;
    for _ in 0..12 {
        f.key(&["-k", "Down"])?;
        if f.named("menu-doomed")?.is_some_and(|row| row.focused) {
            break;
        }
    }
    require(
        f.named("menu-doomed")?.is_some_and(|row| row.focused),
        "Doomed row did not receive keyboard focus",
    )?;
    f.stop(doomed_client)?;
    f.key(&["-k", "Return"])?;
    require(
        f.state()?["focuses"] == json!([]),
        "Removed row acquired new keyboard target",
    )?;
    let _ = f.action(&doomed.reference);
    let state = f.state()?;
    require(
        state["focuses"] == json!([]) && window(&state, "menu-other"),
        "Destroyed AT-SPI reference retargeted",
    )?;
    checks.push(
        "Removed keyboard row and destroyed AT-SPI reference cannot retarget the replacement index",
    );
    f.key(&["-k", "Escape"])?;
    wait_true("Escape dismissal", || Ok(f.state()?["menu"] == ""))?;
    f.open_menu("Window", "menu-bystander")?;
    let other = wait("remaining native row", || f.named("menu-other"))?;
    require(f.focus(&other.reference)?, "Assistive focus rejected")?;
    wait_true("assistive focus selection", || {
        Ok(f.named("menu-other")?.is_some_and(|row| row.focused))
    })?;
    f.key(&["-k", "Return"])?;
    wait_true("Return after actual assistive focus once", || {
        let state = f.state()?;
        Ok(state["focuses"] == json!(["menu-other"])
            && state["active"] == "menu-other"
            && state["bridge"]["appName"] == "menu-other"
            && state["menu"] == "")
    })?;
    f.open_menu("Window", "menu-bystander")?;
    let other = wait("fresh assistive row", || f.named("menu-other"))?;
    require(
        f.action(&other.reference)?,
        "Assistive native action declined",
    )?;
    wait_true("assistive exact native focus once", || {
        let state = f.state()?;
        Ok(state["focuses"] == json!(["menu-other"])
            && state["active"] == "menu-other"
            && state["bridge"]["appName"] == "menu-other")
    })?;
    require(f.state()?["menu"] == "", "Assistive action retained menu")?;
    checks.push("Actual assistive menu-item action focuses its captured live window once");
    remainder(&mut f, &mut checks)?;
    clean_shell(&f)?;
    Ok((
        checks,
        json!({"source":"Installed owned shell components with fixture-only session stub"}),
    ))
}

fn remainder(f: &mut Fixture, checks: &mut Vec<&'static str>) -> Result<()> {
    f.open_menu("Desktop", "menu-bystander")?;
    for (keys, index) in [
        (vec!["-k", "Home"], 0),
        (vec!["-k", "Tab"], 1),
        (vec!["-k", "Tab"], 3),
        (vec!["-k", "ISO_Left_Tab"], 1),
    ] {
        f.key(&keys)?;
        require(
            f.state()?["layout"]["selectedIndex"] == index,
            format!("Wrong keyboard selection after {keys:?}"),
        )?;
    }
    f.key(&["-k", "End"])?;
    f.key(&["-k", "Up"])?;
    let logout = wait("session stub row", || f.named("Log Out…"))?;
    require(
        logout.role == "menu item" && f.action(&logout.reference)?,
        "Session semantic action failed",
    )?;
    wait_true("session fixture action once", || {
        Ok(f.state()?["sessions"] == json!(["logout"]))
    })?;
    checks.push("Home End Tab Backtab and arrows skip separators; session action reaches fixture stub exactly once");

    f.open_menu("Desktop", "menu-bystander")?;
    let logout = wait("physical session stub row", || f.named("Log Out…"))?;
    f.point(&logout.reference)?;
    f.pointer_command("p")?;
    f.pointer_command("r")?;
    wait_true("pointer session stub once", || {
        Ok(f.state()?["sessions"] == json!(["logout"]))
    })?;
    let _ = f.action(&logout.reference);
    require(
        f.state()?["sessions"] == json!(["logout"]),
        "Destroyed session reference repeated action",
    )?;
    checks.push("Physical session activation dispatches once and closed/destroyed menu references cannot repeat it");

    f.ipc("filesStart", &[])?;
    wait_true("first private Files", || {
        Ok(f.files_state()?["ready"] == true)
    })?;
    f.ipc("open", &["File"])?;
    f.ipc("open", &["View"])?;
    wait_true("Files menu context after heading change", || {
        Ok(f.files_state()?["focused"] == true)
    })?;
    let listing = wait("owned Files view command", || {
        Ok(f.named("as List")?.filter(|row| row.enabled))
    })?;
    require(
        f.action(&listing.reference)?,
        "Owned Files List action declined",
    )?;
    wait_true("owned Files list view", || {
        Ok(f.files_state()?["first"] == true)
    })?;
    f.ipc("filesSecond", &[])?;
    wait_true("second private Files", || {
        Ok(f.files_state()?["ready"] == true)
    })?;
    f.ipc("open", &["View"])?;
    let _icons = wait("captured second Files command", || {
        Ok(f.named("as Icons")?.filter(|row| row.enabled))
    })?;
    let before = f.files_state()?;
    f.ipc("clear", &[])?;
    f.key(&["-k", "Home", "-k", "Down"])?;
    f.ipc("filesRetarget", &[])?;
    wait_true("changed Files target", || {
        Ok(f.files_state()?["same"] == false)
    })?;
    f.key(&["-k", "Return"])?;
    let after = f.files_state()?;
    require(
        after["first"] == before["first"]
            && after["second"] == before["second"]
            && f.state()?["fileActions"] == json!([]),
        "Changed Files target activated a view command",
    )?;
    f.key(&["-k", "Escape"])?;
    checks.push("Owned Files legitimate view command works and a changed main-window context cancels captured menu input");

    f.ipc("settingsStart", &[])?;
    wait_true("actual Settings bystander", || {
        Ok(titled(&f.state()?, "System Settings"))
    })?;
    f.call("swaymsg", &["[title=\"System Settings\"]", "focus"])?;
    wait_true("Settings bridge context", || {
        Ok(f.state()?["bridge"]["title"] == "System Settings")
    })?;
    f.ipc("open", &["Application"])?;
    let unknown = wait("unknown owned close-all row", || {
        f.named("Close All Windows")
    })?;
    require(
        !unknown.enabled,
        "Unknown Settings grouping enabled Close All",
    )?;
    let _ = f.action(&unknown.reference);
    let files = f.files_state()?;
    require(
        files["settingsVisible"] == true
            && files["firstVisible"] == true
            && files["secondVisible"] == true,
        "Disabled Settings row closed a window",
    )?;
    f.key(&["-k", "Escape"])?;
    f.ipc("filesFocusFirst", &[])?;
    wait_true("exact owned Files context", || {
        Ok(f.files_state()?["ready"] == true)
    })?;
    f.ipc("open", &["Application"])?;
    let close_all = wait("explicit owned close-all group", || {
        Ok(f.named("Close All Windows")?.filter(|row| row.enabled))
    })?;
    f.point(&close_all.reference)?;
    f.pointer_command("p")?;
    f.ipc("replaceAssociationEpoch", &[])?;
    f.pointer_command("r")?;
    let files = f.files_state()?;
    require(
        files["firstVisible"] == true
            && files["secondVisible"] == true
            && files["settingsVisible"] == true,
        "Old association epoch closed windows",
    )?;
    let close_all = wait("fresh owned group epoch", || {
        Ok(f.named("Close All Windows")?.filter(|row| row.enabled))
    })?;
    f.point(&close_all.reference)?;
    f.pointer_command("p")?;
    f.pointer_command("r")?;
    wait_true("captured Files group close", || {
        let files = f.files_state()?;
        Ok(files["firstVisible"] == false && files["secondVisible"] == false)
    })?;
    require(
        f.files_state()?["settingsVisible"] == true && titled(&f.state()?, "System Settings"),
        "Close All touched Settings",
    )?;
    checks.push("Explicit Files association scopes Close All, replacement association epoch cancels held input, unknown Settings grouping stays disabled and Settings survives");

    f.open_menu("Edit", "menu-bystander")?;
    f.key(&["-k", "Down", "-k", "Return"])?;
    require(
        f.state()?["layout"]["selectedIndex"] == -1,
        "Disabled Edit command selected",
    )?;
    f.key(&["-k", "Escape"])?;
    wait_true("disabled menu Escape", || Ok(f.state()?["menu"] == ""))?;
    require(
        f.state()?["focuses"] == json!([]),
        "Disabled Edit focused a window",
    )?;
    f.key(&["menu-focus-restored", "-k", "Return"])?;
    wait_true("physical ordinary client input after Escape", || {
        Ok(
            fs::read_to_string(f.options.output.join("menu-bystander-input.txt"))
                .is_ok_and(|text| text.contains("menu-focus-restored")),
        )
    })?;
    checks.push(
        "Disabled commands have no keyboard activation and Escape restores ordinary native focus",
    );
    Ok(())
}
