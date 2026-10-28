//! Capture and recheck the original Sway container before typing.
use serde_json::Value;
use std::io::Read;
use std::process::{Command,Stdio};
use std::time::{Duration,Instant};

pub fn focused(tree:&Value) -> Option<&Value> {
    if tree["focused"]==true && (tree["type"]=="con" || tree["type"]=="floating_con") {
        return Some(tree);
    }
    for key in ["nodes","floating_nodes"] {
        for child in tree[key].as_array().into_iter().flatten() {
            if let Some(value)=focused(child) { return Some(value); }
        }
    }
    None
}

fn run(command:&str,args:&[&str],timeout:Duration) -> Result<Vec<u8>,String> {
    let mut child=Command::new(command).args(args).stdout(Stdio::piped()).stderr(Stdio::null())
        .spawn().map_err(|error|error.to_string())?;
    let stdout=child.stdout.take().ok_or("Command output is unavailable")?;
    let reader=std::thread::spawn(move || {let mut output=Vec::new();stdout.take(16*1024*1024+1).read_to_end(&mut output).map(|_|output)});
    let deadline=Instant::now()+timeout;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let output=reader.join().map_err(|_|"Command output reader stopped".to_string())?
                    .map_err(|error|error.to_string())?;
                if output.len()>16*1024*1024 {return Err(format!("{command} output is too large"));}
                if status.success() {return Ok(output)}
                return Err(format!("{command} exited with {status}"));
            }
            Ok(None) if Instant::now()<deadline => std::thread::sleep(Duration::from_millis(10)),
            Ok(None) => {let _=child.kill();let _=child.wait();let _=reader.join();return Err(format!("{command} timed out"));}
            Err(error) => {let _=child.kill();let _=child.wait();let _=reader.join();return Err(error.to_string());}
        }
    }
}

fn sway_tree() -> Result<Value,String> {
    let raw=run("swaymsg",&["-r","-t","get_tree"],Duration::from_secs(3))?;
    serde_json::from_slice(&raw).map_err(|error|error.to_string())
}

#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub struct Dimensions {
    pub width:i32,
    pub height:i32,
    pub sidebar:bool,
    pub details:bool,
}

fn dimensions_for(area:&Value,target:Option<&Value>,expanded:bool,native:bool)->Dimensions {
    let area_width=area["width"].as_i64().unwrap_or(740).clamp(1,10_000) as i32;
    let area_height=area["height"].as_i64().unwrap_or(1000).clamp(1,10_000) as i32;
    let mut available_width=area_width;
    if native {
        if let Some(target)=target {
            let target_width=target["rect"]["width"].as_i64().unwrap_or(i64::from(area_width));
            available_width=available_width.min((target_width-24).max(352).min(10_000) as i32);
        }
    }
    let wide=available_width>=680;
    Dimensions {
        width:if expanded {if wide {740.min(available_width)} else {540.min(available_width)}} else {352},
        height:if expanded {490.min(area_height.max(380))} else {402.min(area_height.max(380))},
        sidebar:expanded && available_width>=540,
        details:expanded && wide,
    }
}

pub fn viewer_dimensions(target:Option<&Value>,expanded:bool,native:bool)->Dimensions {
    let area=run("swaymsg",&["-r","-t","get_workspaces"],Duration::from_secs(3))
        .ok().and_then(|raw|serde_json::from_slice::<Value>(&raw).ok())
        .and_then(|workspaces|workspaces.as_array().and_then(|items|
            items.iter().find(|item|item["focused"]==true).map(|item|item["rect"].clone())));
    dimensions_for(area.as_ref().unwrap_or(&serde_json::json!({"width":740,"height":1000})),target,expanded,native)
}

fn app_id() -> &'static str {
    "lumen-character-viewer"
}

pub fn configure_window() -> Result<(),String> {
    let app=app_id();
    for rule in ["floating enable","border none","resize set width 352 px height 402 px",
        "move position mouse","opacity 0"] {
        let command=format!("for_window [app_id=\"{app}\"] {rule}");
        run("swaymsg",&["-r",&command],Duration::from_secs(3))?;
    }
    Ok(())
}

fn picker_workspace<'a>(node:&'a Value,app:&str,workspace:Option<&'a Value>)
    -> Option<(&'a Value,Option<&'a Value>)> {
    let workspace=if node["type"]=="workspace" {node.get("rect")} else {workspace};
    if node["app_id"]==app {return Some((node,workspace));}
    for key in ["nodes","floating_nodes"] {
        for child in node[key].as_array().into_iter().flatten() {
            if let Some(found)=picker_workspace(child,app,workspace) {return Some(found);}
        }
    }
    None
}

pub fn position_window(width:i32,height:i32) -> Result<(),String> {
    let app=app_id();
    let command=format!("[app_id=\"{app}\"] floating enable, border none, resize set width {width} px height {height} px, focus");
    run("swaymsg",&["-r",&command],Duration::from_secs(3)).map(|_|())
}

pub fn reveal_window(width:i32,height:i32) -> Result<bool,String> {
    let tree=sway_tree()?;
    let Some((viewer,area))=picker_workspace(&tree,app_id(),None) else {return Ok(false)};
    let id=viewer["id"].as_u64().ok_or("Viewer has no container identity")?;
    let rect=&viewer["rect"];
    if (rect["width"].as_i64().unwrap_or(0)-i64::from(width)).abs()>2 ||
        (rect["height"].as_i64().unwrap_or(0)-i64::from(height)).abs()>2 {
        let command=format!("[con_id={id}] resize set width {width} px height {height} px");
        run("swaymsg",&["-r",&command],Duration::from_secs(3))?;
        return Ok(false);
    }
    let command=if let Some(area)=area {
        let x=rect["x"].as_i64().unwrap_or(0);
        let y=rect["y"].as_i64().unwrap_or(0);
        let w=rect["width"].as_i64().unwrap_or(0);
        let h=rect["height"].as_i64().unwrap_or(0);
        let left=area["x"].as_i64().unwrap_or(0);
        let top=area["y"].as_i64().unwrap_or(0);
        let right=left+area["width"].as_i64().unwrap_or(0)-w;
        let bottom=top+area["height"].as_i64().unwrap_or(0)-h;
        format!("[con_id={id}] move absolute position {} px {} px, opacity 1, focus",
            x.clamp(left,right.max(left)),y.clamp(top,bottom.max(top)))
    } else {format!("[con_id={id}] opacity 1, focus")};
    run("swaymsg",&["-r",&command],Duration::from_secs(3))?;
    Ok(true)
}

pub fn capture() -> Option<Value> { sway_tree().ok().and_then(|tree|focused(&tree).cloned()) }

pub fn browser_target(target:&Value) -> bool {
    let app=target["app_id"].as_str().or_else(||target["window_properties"]["class"].as_str())
        .unwrap_or("").to_lowercase();
    ["brave","chromium","google-chrome","codex-desktop","chatgpt"].iter().any(|name|app.contains(name))
}

fn copy(text:&str) -> Result<(),String> {run("wl-copy",&["--",text],Duration::from_secs(3)).map(|_|())}

fn insert_with<F>(text:&str,target:Option<&Value>,copy_only:bool,mut command:F) -> Result<(),String>
where F:FnMut(&str,&[&str])->Result<Vec<u8>,String> {
    if copy_only || target.is_none() { return command("wl-copy",&["--",text]).map(|_|()); }
    let target=target.unwrap();
    let id=target["id"].as_u64().filter(|id|*id>0).ok_or("Invalid original window identity")?;
    let selector=format!("[con_id={id}] focus");
    command("swaymsg",&[&selector])?;
    let tree=serde_json::from_slice::<Value>(&command("swaymsg",&["-r","-t","get_tree"])?).map_err(|error|error.to_string())?;
    if focused(&tree).and_then(|value|value["id"].as_u64())!=Some(id) {
        return Err("The original window has closed".into());
    }
    if browser_target(target) {
        command("wl-copy",&["--",text])?;
        command("wtype",&["-s","100","-M","ctrl","-k","v","-m","ctrl"])?;
    } else {
        command("wtype",&["-s","100","-d","10","--",text])?;
    }
    Ok(())
}

pub fn insert(text:&str,target:Option<&Value>,copy_only:bool) -> Result<(),String> {
    insert_with(text,target,copy_only,|command,args|run(command,args,Duration::from_secs(3)))
}

pub fn insert_or_copy(text:&str,target:Option<&Value>,copy_only:bool) {
    if let Err(error)=insert(text,target,copy_only) {
        let _=copy(text);
        let _=run("notify-send",&["Emoji copied","Could not insert into the original app. Paste to insert."],Duration::from_secs(3));
        eprintln!("{error}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn finds_exact_focused_container() {
        let tree=json!({"type":"root","nodes":[{"type":"workspace","nodes":[
            {"type":"con","id":5,"focused":false},{"type":"con","id":8,"focused":true}]}]});
        assert_eq!(focused(&tree).unwrap()["id"],8);
    }
    #[test]
    fn captures_focused_floating_application() {
        let tree=json!({"type":"root","floating_nodes":[{"type":"floating_con","id":12,
            "app_id":"org.quickshell","focused":true}]});
        assert_eq!(focused(&tree).unwrap()["id"],12);
    }
    #[test]
    fn viewer_workspace_uses_own_workspace_rect() {
        let tree=json!({"type":"root","nodes":[{"type":"workspace","rect":{"x":100,"y":20,"width":700,"height":400},
            "floating_nodes":[{"type":"con","app_id":"lumen-character-viewer","id":7,"rect":{"x":900,"y":300,"width":352,"height":402}}]}]});
        let (viewer,area)=picker_workspace(&tree,"lumen-character-viewer",None).unwrap();
        assert_eq!(viewer["id"],7);
        assert_eq!(area.unwrap()["x"],100);
    }
    #[test]
    fn narrow_caret_target_hides_expanded_panes() {
        let area=json!({"width":900,"height":410});
        let target=json!({"rect":{"width":500}});
        assert_eq!(dimensions_for(&area,Some(&target),true,true),Dimensions{
            width:476,height:410,sidebar:false,details:false});
        assert_eq!(dimensions_for(&area,None,true,false),Dimensions{
            width:740,height:410,sidebar:true,details:true});
    }
    #[test]
    fn chromium_policy_is_scoped_to_app_identity() {
        assert!(browser_target(&json!({"app_id":"brave-browser"})));
        assert!(browser_target(&json!({"window_properties":{"class":"ChatGPT"}})));
        assert!(!browser_target(&json!({"app_id":"terminal"})));
    }
    #[test]
    fn exact_recheck_precedes_literal_keyboard_dispatch() {
        let target=json!({"type":"con","id":8,"app_id":"terminal"});
        let mut calls=Vec::new();
        insert_with("🦀",Some(&target),false,|command,args| {
            calls.push((command.to_owned(),args.iter().map(|arg|(*arg).to_owned()).collect::<Vec<_>>()));
            if args==["-r","-t","get_tree"] {Ok(serde_json::to_vec(&json!({"type":"con","id":8,"focused":true})).unwrap())}
            else {Ok(Vec::new())}
        }).unwrap();
        assert_eq!(calls[0],("swaymsg".into(),vec!["[con_id=8] focus".into()]));
        assert_eq!(calls[2],("wtype".into(),vec!["-s","100","-d","10","--","🦀"].into_iter().map(String::from).collect()));
    }
    #[test]
    fn replaced_target_never_receives_text() {
        let target=json!({"type":"con","id":8,"app_id":"brave-browser"});
        let mut calls=Vec::new();
        let result=insert_with("🦀",Some(&target),false,|command,args| {
            calls.push(command.to_owned());
            if args==["-r","-t","get_tree"] {Ok(serde_json::to_vec(&json!({"type":"con","id":9,"focused":true})).unwrap())}
            else {Ok(Vec::new())}
        });
        assert!(result.is_err());
        assert_eq!(calls,["swaymsg","swaymsg"]);
    }
}
