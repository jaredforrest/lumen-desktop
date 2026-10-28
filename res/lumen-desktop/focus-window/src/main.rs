//! Focus and hide exact Sway containers using sampled native identities.
use serde_json::Value;
use std::{
    env,
    io::Read,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

type Result<T> = std::result::Result<T, String>;

#[derive(Clone)]
struct Window<'a> {
    node: &'a Value,
    workspace: String,
}
impl Window<'_> {
    fn id(&self) -> Result<i64> {
        self.node["id"]
            .as_i64()
            .ok_or("Invalid window identity".into())
    }
    fn title(&self) -> &str {
        self.node["name"].as_str().unwrap_or("")
    }
    fn identity(&self) -> &str {
        self.node["app_id"]
            .as_str()
            .filter(|s| !s.is_empty())
            .or_else(|| self.node["window_properties"]["class"].as_str())
            .unwrap_or("")
    }
    fn state(&self) -> &Value {
        &self.node["lumen_state"]
    }
    fn restored_workspace(&self) -> &str {
        self.state()["restore_workspace"]
            .as_str()
            .filter(|s| !s.is_empty())
            .unwrap_or(&self.workspace)
    }
}

fn walk<'a>(node: &'a Value, workspace: &str, output: &mut Vec<Window<'a>>) {
    let workspace = if node["type"] == "workspace" {
        node["name"].as_str().unwrap_or("")
    } else {
        workspace
    };
    output.push(Window {
        node,
        workspace: workspace.to_string(),
    });
    for key in ["nodes", "floating_nodes"] {
        if let Some(children) = node[key].as_array() {
            for child in children {
                walk(child, workspace, output);
            }
        }
    }
}
fn windows(tree: &Value) -> Vec<Window<'_>> {
    let mut output = Vec::new();
    walk(tree, "", &mut output);
    output
}
fn relation(node: &Value) -> Result<(Option<i64>, Option<bool>)> {
    let value = &node["lumen_relationships"];
    if value.is_null() {
        return Ok((None, None));
    }
    let valid = value.is_object()
        && value["version"].as_i64() == Some(1)
        && value["parent_supported"].is_boolean()
        && (value["parent"].is_null() || value["parent"].as_i64().is_some_and(|id| id >= 0))
        && (value["modal"].is_null() || value["modal"].is_boolean());
    if !valid
        || (value["parent_supported"] == false
            && (!value["parent"].is_null() || !value["modal"].is_null()))
    {
        return Err("Invalid native window relationships".into());
    }
    Ok((value["parent"].as_i64(), value["modal"].as_bool()))
}
fn modal_target<'a>(all: &[Window<'a>], chosen: Window<'a>) -> Result<Window<'a>> {
    let mut observed = Vec::new();
    for window in all {
        if window.node["app_id"].is_null()
            && window.node["window"].is_null()
            && window.node["window_properties"].is_null()
        {
            continue;
        }
        let (parent, modal) = relation(window.node)?;
        observed.push((window.clone(), parent, modal));
    }
    let mut target = chosen.id()?;
    let mut seen = std::collections::HashSet::new();
    loop {
        if !seen.insert(target) {
            return Err("Cyclic modal parent graph".into());
        }
        let children = observed
            .iter()
            .filter(|(_, parent, modal)| *parent == Some(target) && *modal == Some(true))
            .collect::<Vec<_>>();
        if children.len() > 1 {
            return Err("Ambiguous modal descendants".into());
        }
        if let Some((child, _, _)) = children.first() {
            target = child.id()?;
        } else {
            break;
        }
    }
    observed
        .into_iter()
        .find(|(window, _, _)| window.id().ok() == Some(target))
        .map(|entry| entry.0)
        .ok_or("Activation target is unavailable".into())
}
fn exact<'a>(all: &'a [Window<'a>], app: &str, title: &str) -> Vec<Window<'a>> {
    all.iter()
        .filter(|window| window.identity() == app && window.title() == title)
        .cloned()
        .collect()
}
fn focus_command(
    tree: &Value,
    app: &str,
    title: &str,
    action: Option<&str>,
) -> Result<Option<String>> {
    let all = windows(tree);
    let candidates = exact(&all, app, title);
    if action.is_none()
        && candidates
            .iter()
            .any(|window| window.node["focused"] == true)
    {
        return Ok(None);
    }
    let chosen = match candidates.as_slice() {
        [] => return Err("The requested window is no longer available".into()),
        [chosen] => chosen.clone(),
        _ => return Err("Window identity is ambiguous; select it in Window Overview".into()),
    };
    if let Some(action) = action {
        if !["close", "minimize", "fill", "fullscreen"].contains(&action) {
            return Err("Unsupported window action".into());
        }
        return Ok(Some(format!(
            "[con_id={}] lumen_window {action}",
            chosen.id()?
        )));
    }
    let chosen = modal_target(&all, chosen)?;
    let restore = if chosen.workspace == "__i3_scratch" {
        "scratchpad show, "
    } else {
        ""
    };
    Ok(Some(format!("[con_id={}] {restore}focus", chosen.id()?)))
}
fn titles(value: &Value) -> Result<Vec<String>> {
    value
        .as_array()
        .ok_or("Application window titles must be a list of strings")?
        .iter()
        .map(|item| {
            item.as_str()
                .map(str::to_owned)
                .ok_or("Application window titles must be a list of strings".into())
        })
        .collect()
}
fn app_focus_command(
    tree: &Value,
    app: &str,
    titles: &[String],
    switch_space: bool,
) -> Result<Option<String>> {
    let all = windows(tree);
    let current = all
        .iter()
        .find(|window| window.node["focused"] == true && !window.workspace.is_empty())
        .map(|window| window.workspace.as_str());
    let candidates = all
        .iter()
        .filter(|window| {
            window.identity() == app && titles.iter().any(|title| title == window.title())
        })
        .cloned()
        .collect::<Vec<_>>();
    let unhide = candidates
        .iter()
        .filter(|window| window.state()["hidden"] == true)
        .map(|window| format!("[con_id={}] lumen_window unhide", window.id().unwrap()))
        .collect::<Vec<_>>()
        .join("; ");
    let candidates = if switch_space {
        candidates
    } else {
        let current = current.ok_or("The active workspace could not be determined")?;
        candidates
            .into_iter()
            .filter(|window| window.restored_workspace() == current)
            .collect()
    };
    if candidates.is_empty() {
        return Ok((!unhide.is_empty()).then_some(unhide));
    }
    for title in titles {
        let matches = candidates
            .iter()
            .filter(|window| window.title() == title)
            .collect::<Vec<_>>();
        if matches.len() > 1 {
            return Err("Window identity is ambiguous; select it in Window Overview".into());
        }
        if let Some(first) = matches.first() {
            let chosen = modal_target(&all, (*first).clone())?;
            let workspace = chosen.restored_workspace();
            if !switch_space && Some(workspace) != current {
                return Ok((!unhide.is_empty()).then_some(unhide));
            }
            if chosen.node["focused"] == true {
                return Ok((!unhide.is_empty()).then_some(unhide));
            }
            let scratch = chosen.state()["minimised"] == true
                || (workspace == "__i3_scratch" && chosen.state()["hidden"] != true);
            let command = format!(
                "[con_id={}] {}focus",
                chosen.id()?,
                if scratch { "scratchpad show, " } else { "" }
            );
            return Ok(Some(if unhide.is_empty() {
                command
            } else {
                format!("{unhide}; {command}")
            }));
        }
    }
    Ok(None)
}
fn visibility_command(
    tree: &Value,
    app: &str,
    titles: &[String],
    action: &str,
) -> Result<Option<String>> {
    if !["hide", "hide-others"].contains(&action) {
        return Err("Unsupported application visibility action".into());
    }
    let mut targets = Vec::new();
    for window in windows(tree) {
        if window.node["pid"].as_i64().is_none_or(|pid| pid == 0)
            || window.node["id"].as_i64().is_none_or(|id| id == 0)
        {
            continue;
        }
        let belongs =
            window.identity() == app && titles.iter().any(|title| title == window.title());
        if belongs == (action == "hide") {
            if window.state()["version"].as_i64() != Some(1) {
                return Err(
                    "This compositor does not support independent application hiding".into(),
                );
            }
            targets.push(format!("[con_id={}] lumen_window hide", window.id()?));
        }
    }
    Ok((!targets.is_empty()).then(|| targets.join("; ")))
}

fn swaymsg(args: &[&str]) -> Result<Value> {
    let mut child = Command::new("swaymsg")
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| e.to_string())?;
    let stdout = child.stdout.take().ok_or("Sway output unavailable")?;
    let stderr = child.stderr.take().ok_or("Sway error output unavailable")?;
    let output = thread::spawn(move || collect_pipe(stdout, 16 * 1024 * 1024));
    let errors = thread::spawn(move || collect_pipe(stderr, 65536));
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let output = output.join().map_err(|_| "Sway output reader failed")??;
                let _ = errors.join();
                if !status.success() {
                    return Err("swaymsg rejected the request".into());
                }
                return serde_json::from_slice(&output).map_err(|e| e.to_string());
            }
            Ok(None) if started.elapsed() < Duration::from_secs(4) => {
                thread::sleep(Duration::from_millis(10))
            }
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("swaymsg timed out".into());
            }
            Err(error) => return Err(error.to_string()),
        }
    }
}
fn collect_pipe(mut pipe: impl Read, limit: usize) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    let mut chunk = [0u8; 8192];
    let mut oversized = false;
    loop {
        let count = pipe.read(&mut chunk).map_err(|e| e.to_string())?;
        if count == 0 {
            break;
        }
        if bytes.len() + count > limit {
            oversized = true;
        }
        if !oversized {
            bytes.extend_from_slice(&chunk[..count]);
        }
    }
    if oversized {
        Err("Sway response is too large".into())
    } else {
        Ok(bytes)
    }
}
fn option_value<'a>(args: &'a [String], name: &str) -> Result<Option<&'a str>> {
    let positions = args
        .iter()
        .enumerate()
        .filter(|(_, arg)| *arg == name)
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    if positions.len() > 1 {
        return Err(format!("Duplicate option {name}"));
    }
    positions
        .first()
        .map(|index| {
            args.get(index + 1)
                .map(String::as_str)
                .ok_or(format!("Missing value for {name}"))
        })
        .transpose()
}
fn validate_options(args: &[String]) -> Result<()> {
    let mut index = 2;
    let mut seen = std::collections::HashSet::new();
    while index < args.len() {
        let name = args[index].as_str();
        if !seen.insert(name.to_string()) {
            return Err(format!("Duplicate option {name}"));
        }
        if name == "--no-switch-space" {
            index += 1;
            continue;
        }
        if ![
            "--action",
            "--activate-titles",
            "--app-action",
            "--previous-app",
            "--previous-title",
        ]
        .contains(&name)
        {
            return Err(format!("Unknown option: {name}"));
        }
        let value = args
            .get(index + 1)
            .filter(|value| !value.starts_with("--"))
            .ok_or(format!("Missing value for {name}"))?;
        if name == "--action"
            && !["close", "minimize", "fill", "fullscreen"].contains(&value.as_str())
        {
            return Err("Unsupported window action".into());
        }
        if name == "--app-action" && !["hide", "hide-others"].contains(&value.as_str()) {
            return Err("Unsupported application visibility action".into());
        }
        index += 2;
    }
    Ok(())
}
fn run(args: &[String]) -> Result<()> {
    if args.len() < 2 {
        return Err("expected an application identity and window title".into());
    }
    validate_options(args)?;
    let (app, title) = (&args[0], &args[1]);
    let action = option_value(args, "--action")?;
    let activate = option_value(args, "--activate-titles")?;
    let app_action = option_value(args, "--app-action")?;
    let previous_app = option_value(args, "--previous-app")?;
    let previous_title = option_value(args, "--previous-title")?.unwrap_or("");
    let no_switch = args.iter().any(|arg| arg == "--no-switch-space");
    let tree = swaymsg(&["-t", "get_tree", "-r"])?;
    let command = if let Some(raw) = activate {
        if action.is_some() {
            return Err("Application activation cannot be combined with a window action".into());
        }
        let titles = titles(&serde_json::from_str::<Value>(raw).map_err(|e| e.to_string())?)?;
        if let Some(app_action) = app_action {
            let hide = visibility_command(&tree, app, &titles, app_action)?;
            let focus = if app_action == "hide-others" {
                app_focus_command(&tree, app, &titles, !no_switch)?
            } else if let Some(previous_app) = previous_app {
                app_focus_command(
                    &tree,
                    previous_app,
                    &[previous_title.to_string()],
                    !no_switch,
                )?
            } else {
                None
            };
            match (hide, focus) {
                (Some(a), Some(b)) => Some(format!("{a}; {b}")),
                (Some(a), None) | (None, Some(a)) => Some(a),
                _ => None,
            }
        } else {
            app_focus_command(&tree, app, &titles, !no_switch)?
        }
    } else {
        if app_action.is_some() || previous_app.is_some() {
            return Err("Application actions require an application window list".into());
        }
        focus_command(&tree, app, title, action)?
    };
    if let Some(command) = command {
        let reply = swaymsg(&["-r", &command])?;
        if !reply.as_array().is_some_and(|items| {
            !items.is_empty() && items.iter().all(|item| item["success"] == true)
        }) {
            return Err("The window manager rejected the focus request".into());
        }
    }
    Ok(())
}
fn main() {
    if let Err(error) = run(&env::args().skip(1).collect::<Vec<_>>()) {
        eprintln!("Unable to focus window: {error}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn window(id: i64, title: &str) -> Value {
        json!({"id": id, "type": "con", "pid": id+100, "app_id":"writer", "name":title, "focused":false})
    }
    fn tree(nodes: Vec<Value>, workspace: &str) -> Value {
        json!({"type":"root","nodes":[{"type":"workspace","name":workspace,"nodes":nodes}]})
    }
    #[test]
    fn exact_identity_actions_and_ambiguity() {
        let hostile = "\"; kill; [app_id=.*]";
        assert_eq!(
            focus_command(
                &tree(vec![window(17, hostile)], "1"),
                "writer",
                hostile,
                None
            )
            .unwrap()
            .as_deref(),
            Some("[con_id=17] focus")
        );
        assert_eq!(
            focus_command(
                &tree(vec![window(17, "Document")], "__i3_scratch"),
                "writer",
                "Document",
                None
            )
            .unwrap()
            .as_deref(),
            Some("[con_id=17] scratchpad show, focus")
        );
        assert_eq!(
            focus_command(
                &tree(vec![window(17, "Document")], "1"),
                "writer",
                "Document",
                Some("close")
            )
            .unwrap()
            .as_deref(),
            Some("[con_id=17] lumen_window close")
        );
        assert!(
            focus_command(
                &tree(vec![window(17, "Document"), window(18, "Document")], "1"),
                "writer",
                "Document",
                None
            )
            .unwrap_err()
            .contains("ambiguous")
        );
    }
    #[test]
    fn modal_child_only_redirects_activation() {
        let parent = window(17, "Document");
        let mut child = window(18, "Find");
        child["lumen_relationships"] =
            json!({"version":1,"parent_supported":true,"parent":17,"modal":true});
        let fixture = tree(vec![parent, child], "1");
        assert_eq!(
            focus_command(&fixture, "writer", "Document", None)
                .unwrap()
                .as_deref(),
            Some("[con_id=18] focus")
        );
        assert_eq!(
            focus_command(&fixture, "writer", "Document", Some("close"))
                .unwrap()
                .as_deref(),
            Some("[con_id=17] lumen_window close")
        );
    }
    #[test]
    fn mru_hide_and_restore() {
        let mut first = window(17, "First");
        first["lumen_state"] = json!({"version":1,"hidden":true,"restore_workspace":"1"});
        let mut second = window(18, "Second");
        second["lumen_state"] =
            json!({"version":1,"hidden":true,"minimised":true,"restore_workspace":"1"});
        let fixture = tree(vec![first, second], "__i3_scratch");
        let order = ["Second".to_string(), "First".to_string()];
        assert_eq!(
            app_focus_command(&fixture, "writer", &order, true)
                .unwrap()
                .as_deref(),
            Some(
                "[con_id=17] lumen_window unhide; [con_id=18] lumen_window unhide; [con_id=18] scratchpad show, focus"
            )
        );
        assert_eq!(
            visibility_command(&fixture, "writer", &order, "hide")
                .unwrap()
                .as_deref(),
            Some("[con_id=17] lumen_window hide; [con_id=18] lumen_window hide")
        );
    }
    #[test]
    fn focused_fallback_and_xwayland_identity() {
        let mut focused = window(17, "Document");
        focused["focused"] = json!(true);
        assert_eq!(
            focus_command(
                &tree(vec![focused, window(18, "Document")], "1"),
                "writer",
                "Document",
                None
            )
            .unwrap(),
            None
        );
        let mut xwayland = window(19, "Document");
        xwayland["app_id"] = Value::Null;
        xwayland["window_properties"] = json!({"class":"writer"});
        assert_eq!(
            focus_command(&tree(vec![xwayland], "1"), "writer", "Document", None)
                .unwrap()
                .as_deref(),
            Some("[con_id=19] focus")
        );
    }
    #[test]
    fn no_space_switch_and_duplicate_titles() {
        let mut current = window(17, "Current");
        current["focused"] = json!(true);
        let fixture = json!({"type":"root","nodes":[{"type":"workspace","name":"1","nodes":[current]},{"type":"workspace","name":"2","nodes":[window(18,"Other")]}]});
        let order = ["Other".to_string(), "Current".to_string()];
        assert_eq!(
            app_focus_command(&fixture, "writer", &order, false).unwrap(),
            None
        );
        assert_eq!(
            app_focus_command(&fixture, "writer", &order, true)
                .unwrap()
                .as_deref(),
            Some("[con_id=18] focus")
        );
        assert_eq!(
            app_focus_command(&fixture, "writer", &["Other".into()], false).unwrap(),
            None
        );
        assert!(
            app_focus_command(
                &tree(vec![window(17, "Document"), window(18, "Document")], "1"),
                "writer",
                &["Document".into()],
                true
            )
            .unwrap_err()
            .contains("ambiguous")
        );
    }
    #[test]
    fn hide_others_rejects_unsupported_compositor() {
        let mut target = window(17, "Document");
        target["lumen_state"] = json!({"version":1});
        let mut other = window(18, "Other");
        other["app_id"] = json!("other");
        other["lumen_state"] = json!({"version":1});
        let fixture = tree(vec![target, other], "1");
        assert_eq!(
            visibility_command(&fixture, "writer", &["Document".into()], "hide-others")
                .unwrap()
                .as_deref(),
            Some("[con_id=18] lumen_window hide")
        );
        assert!(
            visibility_command(
                &tree(vec![window(17, "Document")], "1"),
                "writer",
                &["Document".into()],
                "hide"
            )
            .unwrap_err()
            .contains("independent application hiding")
        );
    }
    #[test]
    fn option_parser_rejects_unsupported_actions_and_missing_values() {
        assert!(
            validate_options(&[
                "writer".into(),
                "Document".into(),
                "--action".into(),
                "kill; exec bad".into()
            ])
            .is_err()
        );
        assert!(
            validate_options(&["writer".into(), "Document".into(), "--action".into()]).is_err()
        );
        assert!(titles(&json!("Document")).is_err());
    }
}
