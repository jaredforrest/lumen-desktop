//! Focus and hide native containers using the shared activation policy.
use serde_json::Value;
use lumen_window_policy::{focus_command,app_focus_command,visibility_command,titles};
use std::{env,io::Read,process::{Command,Stdio},thread,time::{Duration,Instant}};
type Result<T> = std::result::Result<T,String>;
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
