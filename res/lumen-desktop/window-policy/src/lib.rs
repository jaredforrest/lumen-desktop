use serde_json::Value;
pub mod guard;
pub mod projection;
pub type Result<T> = std::result::Result<T, String>;
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
    fn effective_workspace(&self) -> Result<String> {
        projection::workspace(self.node, self.restored_workspace())
    }
    fn policy_workspace(&self, switch_space: bool) -> Result<String> {
        if switch_space {
            projection::parse(self.node)?;
            Ok(self.restored_workspace().to_owned())
        } else { self.effective_workspace() }
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
pub fn focus_command(
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
pub fn titles(value: &Value) -> Result<Vec<String>> {
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
pub fn app_focus_command(
    tree: &Value,
    app: &str,
    titles: &[String],
    switch_space: bool,
) -> Result<Option<String>> {
    let all = windows(tree);
    let current = all
        .iter()
        .find(|window| window.node["focused"] == true && !window.workspace.is_empty())
        .map(|window|window.policy_workspace(switch_space)).transpose()?;
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
        let current = current.as_deref().ok_or("The active workspace could not be determined")?;
        candidates
            .into_iter()
            .map(|window| Ok((window.effective_workspace()? == current).then_some(window)))
            .collect::<Result<Vec<_>>>()?.into_iter().flatten().collect()
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
            let effective_workspace = chosen.policy_workspace(switch_space)?;
            if !switch_space && Some(effective_workspace.as_str()) != current.as_deref() {
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
pub fn visibility_command(
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

/// Native identity validation belongs to the caller; this shared policy keeps
/// modal redirection, application unhiding and workspace restoration consistent.
pub struct NativePlan {
    pub command: Option<String>,
    pub actions: Vec<Value>,
    pub target: Value,
    pub focus_allowed: bool,
}
pub fn native_focus_plan(
    tree: &Value,
    id: i64,
    unhide_ids: &[i64],
    switch_space: bool,
) -> Result<NativePlan> {
    let all = windows(tree);
    let chosen = all
        .iter()
        .find(|window| window.id().ok() == Some(id))
        .cloned()
        .ok_or("The captured window is no longer available")?;
    let target = modal_target(&all, chosen)?;
    let current = all
        .iter()
        .find(|window| window.node["focused"] == true && !window.workspace.is_empty())
        .map(|window|window.policy_workspace(switch_space)).transpose()?;
    let target_scope = target.policy_workspace(switch_space)?;
    let allowed =
        switch_space || current.as_deref().is_some_and(|workspace| workspace == target_scope);
    if !switch_space && current.is_none() {
        return Err("The active workspace could not be determined".into());
    }
    let mut commands = Vec::new();
    let mut actions = Vec::new();
    for window in &all {
        if (unhide_ids.contains(&window.id().unwrap_or(-1)) || window.id().ok() == target.id().ok())
            && window.state()["hidden"] == true
        {
            if window.state()["version"] != 1 {
                return Err("The compositor cannot restore this hidden application".into());
            }
            commands.push(format!("[con_id={}] lumen_window unhide", window.id()?));
            actions.push(serde_json::json!({"op":"unhide","conId":window.id()?}));
        }
    }
    if allowed && target.node["focused"] != true {
        let workspace = target.restored_workspace();
        let scratch = target.state()["minimised"] == true
            || (workspace == "__i3_scratch" && target.state()["hidden"] != true);
        commands.push(format!(
            "[con_id={}] {}focus",
            target.id()?,
            if scratch { "scratchpad show, " } else { "" }
        ));
        if scratch {actions.push(serde_json::json!({"op":"show","conId":target.id()?}));}
        actions.push(serde_json::json!({"op":"focus","conId":target.id()?}));
    }
    Ok(NativePlan {
        command: (!commands.is_empty()).then(|| commands.join("; ")),
        actions,
        target: target.node.clone(),
        focus_allowed: allowed,
    })
}

#[cfg(test)]
mod native_tests {
    use super::*;
    use serde_json::json;
    fn node(id: i64, workspace: &str, focused: bool) -> Value {
        json!({"id":id,"type":"con","pid":10,"app_id":"app","shell":"xdg_shell","window":null,"name":"identical","focused":focused,"lumen_state":{"version":1,"hidden":false,"minimised":false,"restore_workspace":workspace},"lumen_relationships":{"version":1,"parent_supported":true,"parent":null,"modal":null}})
    }
    fn tree(nodes: Vec<Value>) -> Value {
        json!({"type":"root","nodes":[{"type":"workspace","name":"one","nodes":nodes}]})
    }
    #[test]
    fn projected_picker_scope_does_not_replace_origin_restore() {
        let mut focused=node(1,"origin",true);
        focused["app_id"]=json!("other");
        focused["lumen_all_desktops"]=json!({"version":1,"enabled":true,"supported":true,"reason":null,"revision":"3","projected":true,"origin_workspace":{"id":"1","name":"origin"},"origin_output":{"id":"4","name":"display"},"effective_workspace":{"id":"2","name":"one"},"effective_output":{"id":"4","name":"display"}});
        let mut selected=node(2,"one",false);
        selected["lumen_state"]["minimised"]=json!(true);
        let t=tree(vec![focused.clone(),selected]);
        assert!(native_focus_plan(&t,2,&[],false).unwrap().focus_allowed);
        assert_eq!(app_focus_command(&t,"app",&["identical".into()],false).unwrap(),Some("[con_id=2] scratchpad show, focus".into()));
        focused["lumen_all_desktops"]["version"]=json!(2);
        assert!(native_focus_plan(&tree(vec![focused,node(2,"one",false)]),2,&[],false).is_err());
    }
    #[test]
    fn global_activation_preserves_well_formed_suspended_source_and_target() {
        let unsupported=json!({"version":1,"enabled":true,"supported":false,"reason":"suspended","revision":"9","projected":false,"origin_workspace":null,"origin_output":null,"effective_workspace":null,"effective_output":null});
        let mut focused=node(1,"one",true);focused["app_id"]=json!("other");focused["lumen_all_desktops"]=unsupported.clone();
        let mut target=node(2,"two",false);target["lumen_all_desktops"]=unsupported;target["lumen_state"]["minimised"]=json!(true);
        let t=tree(vec![focused.clone(),target.clone()]);
        assert_eq!(native_focus_plan(&t,2,&[],true).unwrap().command.as_deref(),Some("[con_id=2] scratchpad show, focus"));
        assert_eq!(app_focus_command(&t,"app",&["identical".into()],true).unwrap().as_deref(),Some("[con_id=2] scratchpad show, focus"));
        assert!(native_focus_plan(&t,2,&[],false).is_err());
        assert!(app_focus_command(&t,"app",&["identical".into()],false).is_err());
        target["lumen_all_desktops"]["version"]=json!(2);
        let malformed=tree(vec![focused,target]);
        assert!(native_focus_plan(&malformed,2,&[],true).is_err());
        assert!(app_focus_command(&malformed,"app",&["identical".into()],true).is_err());
    }
    #[test]
    fn exact_native_id_ignores_duplicate_titles_and_refuses_closed_target() {
        let t = tree(vec![node(1, "one", true), node(2, "one", false)]);
        let p = native_focus_plan(&t, 2, &[1, 2], true).unwrap();
        assert_eq!(p.command.as_deref(), Some("[con_id=2] focus"));
        assert_eq!(p.target["id"], 2);
        assert!(native_focus_plan(&t, 3, &[1, 2], true).is_err());
    }
    #[test]
    fn native_hidden_minimized_restore_and_workspace_policy() {
        let mut selected = node(2, "two", false);
        selected["lumen_state"]["hidden"] = json!(true);
        let t = tree(vec![node(1, "one", true), selected]);
        let p = native_focus_plan(&t, 2, &[2], false).unwrap();
        assert!(!p.focus_allowed);
        assert_eq!(p.command.as_deref(), Some("[con_id=2] lumen_window unhide"));
        let mut selected = node(2, "one", false);
        selected["lumen_state"]["minimised"] = json!(true);
        let p =
            native_focus_plan(&tree(vec![node(1, "one", true), selected]), 2, &[2], true).unwrap();
        assert_eq!(
            p.command.as_deref(),
            Some("[con_id=2] scratchpad show, focus")
        );
    }
    #[test]
    fn native_modal_redirect_and_ambiguous_graph_share_existing_policy() {
        let mut child = node(3, "one", false);
        child["lumen_relationships"]["parent"] = json!(2);
        child["lumen_relationships"]["modal"] = json!(true);
        let t = tree(vec![
            node(1, "one", true),
            node(2, "one", false),
            child.clone(),
        ]);
        let p = native_focus_plan(&t, 2, &[2], true).unwrap();
        assert_eq!(p.target["id"], 3);
        assert_eq!(p.command.as_deref(), Some("[con_id=3] focus"));
        child["id"] = json!(4);
        let mut all = t["nodes"][0]["nodes"].as_array().unwrap().clone();
        all.push(child);
        assert!(native_focus_plan(&tree(all), 2, &[2], true).is_err());
    }
}
