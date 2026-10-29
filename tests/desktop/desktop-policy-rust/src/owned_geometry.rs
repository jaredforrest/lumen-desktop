use super::*;

fn ui(ctx:&Context)->Result<Value>{Ok(serde_json::from_str(&ctx.ipc("status",&[])?)?)}
fn node(ctx:&Context,title:&str,pid:u32)->Result<Option<Value>>{
    Ok(find_node(&ctx.tree()?,|row|row["name"]==title && row["pid"]==pid))
}
fn open(ctx:&mut Context,kind:&str)->Result<(usize,Value,Value)>{
    ctx.vars.insert("OWNED_FIXTURE_KIND".into(),kind.into());
    let index=ctx.launch("quickshell", &["-p",&path(&ctx.qml)],&format!("owned-{kind}.log"),false)?;
    let title=if kind=="Settings"{"System Settings"}else{"Calendar"};
    let pid=ctx.children[index].0.id();
    let native=wait("owned native window",||node(ctx,title,pid))?;
    let ready=wait("owned geometry fit",||{
        let value=ui(ctx)?;
        Ok((value["completed"].as_i64().is_some_and(|n|n>0)
            && value["busy"]==false).then_some(value))
    })?;
    need(ready["error"]=="",format!("Owned geometry fit failed: {ready}"))?;
    Ok((index,native,ready))
}
fn placement(ctx:&Context,key:&str)->Result<Option<Value>>{
    let path=ctx.root.0.join("XDG_STATE_HOME/lumen-desktop/owned-geometry/placements.json");
    if !path.exists(){return Ok(None);}
    Ok(load(&path)?["owners"].get(key).cloned())
}
pub fn run(ctx:&mut Context,legacy:bool)->Result<()> {
    let palette=load(&ctx.repo.join("res/lumen-desktop/appearance/palette.json"))?;
    save(&ctx.root.0.join("appearance.json"),&json!({"font":palette["font"],"palette":palette["dark"],
        "preferences":{"reduceMotion":true},"windowsPreferences":{"rememberOwnedGeometry":true}}))?;
    ctx.checked("lumen_window margins disable; focus output HEADLESS-1")?;
    if !legacy{ctx.pointer_start()?;}
    let mut checks=Vec::new();
    for (kind,key) in [("Settings","settings.main"),("Calendar","calendar.main")] {
        let (index,native,ready)=open(ctx,kind)?;
        let title=if kind=="Settings"{"System Settings"}else{"Calendar"};
        let pid=ctx.children[index].0.id();
        if legacy {
            need(ready["persistenceSupported"]==false && ready["persistenceStatus"].as_str()
                .is_some_and(|status|status.contains("updated compositor")),
                format!("Legacy persistence was not explicitly unavailable: {ready}"))?;
            need(placement(ctx,key)?.is_none(),"Legacy compositor wrote a placement")?;
            let completed=ready["completed"].as_i64().unwrap();
            ctx.checked(&format!("[con_id={}] move container to output HEADLESS-2",native["id"]))?;
            wait("legacy output refit",||{
                let value=ui(ctx)?;
                Ok((value["completed"].as_i64().is_some_and(|number|number>completed)
                    && value["busy"]==false).then_some(()))
            })?;
            need(placement(ctx,key)?.is_none(),"Legacy output refit wrote a placement")?;
            checks.push(format!("{kind} exposes unsupported persistence but remains usable"));
            ctx.children[index].stop(libc::SIGTERM)?;
            wait("retired native window",||Ok(node(ctx,title,pid)?.is_none().then_some(())))?;
            ctx.checked("focus output HEADLESS-1")?;
            continue;
        }
        need(ready["persistenceSupported"]==true,format!("Owned native capability missing: {ready}"))?;
        let completed=ready["completed"].as_i64().unwrap();
        ctx.checked(&format!("[con_id={}] move container to output HEADLESS-2",native["id"]))?;
        wait("output refit",||{
            let value=ui(ctx)?;
            Ok((value["screen"]=="HEADLESS-2" && value["completed"].as_i64().is_some_and(|n|n>completed)
                && value["busy"]==false).then_some(()))
        })?;
        ctx.checked(&format!("[con_id={}] resize set width 650 px height 433 px, move absolute position 1400 px 110 px",native["id"]))?;
        let row=node(ctx,title,pid)?.ok_or("Owned native window disappeared")?;
        let rect=&row["rect"];
        let x=rect["x"].as_i64().ok_or("No native x")?+200;
        let y=rect["y"].as_i64().ok_or("No native y")?-row["deco_rect"]["height"].as_i64().unwrap_or(0)+16;
        ctx.move_pointer((x as f64,y as f64))?;
        ctx.press()?;
        ctx.move_pointer(((x+60) as f64,(y+40) as f64))?;
        ctx.release()?;
        let saved=wait("committed owned placement",||Ok(placement(ctx,key)?.filter(|record|
            record["output"]=="HEADLESS-2" && record["width"].as_i64().is_some_and(|n|n>0))))?;
        ctx.children[index].stop(libc::SIGTERM)?;
        wait("retired native container",||Ok(node(ctx,title,pid)?.is_none().then_some(())))?;
        let (next,new_native,state)=open(ctx,kind)?;
        need(ctx.children[next].0.id()!=pid && new_native["id"]!=native["id"],
            "Reopened singleton retained stale native identity")?;
        need(state["screen"]=="HEADLESS-2" && state["persistenceSupported"]==true,
            format!("Cross-process placement was not restored: {state}"))?;
        let stale=Command::new("lumen-window-geometry")
            .args(["--app","org.quickshell","--title",title,"--container",&native["id"].to_string()])
            .env_clear().envs(&ctx.vars).output()?;
        need(!stale.status.success() && String::from_utf8_lossy(&stale.stdout).contains("unavailable"),
            "Retired native container was not rejected")?;
        need(node(ctx,title,ctx.children[next].0.id())?.is_some() && placement(ctx,key)?==Some(saved.clone()),
            "Stale container probe changed the replacement window or record")?;
        ctx.checked("output HEADLESS-2 resolution 700x520")?;
        wait("shrunk output refit",||{
            let value=ui(ctx)?;
            Ok((value["width"].as_i64().is_some_and(|n|n<=700)
                && value["height"].as_i64().is_some_and(|n|n<=520) && value["busy"]==false).then_some(()))
        })?;
        need(placement(ctx,key)?==Some(saved.clone()),"Shrinking output overwrote preferred placement")?;
        ctx.children[next].stop(libc::SIGTERM)?;
        ctx.checked("output HEADLESS-2 disable; focus output HEADLESS-1")?;
        let (fallback_index,fallback_native,fallback_state)=open(ctx,kind)?;
        need(fallback_state["screen"]=="HEADLESS-1" && placement(ctx,key)?==Some(saved.clone()),
            "Missing output did not retain a reachable fallback and preferred placement")?;
        let rect=&fallback_native["rect"];
        need(rect["x"].as_i64().is_some_and(|x|x>=0 && x+rect["width"].as_i64().unwrap_or(0)<=1280),
            "Fallback placement is outside connected output")?;
        ctx.children[fallback_index].stop(libc::SIGTERM)?;
        let invalid=Command::new("lumen-window-geometry")
            .args(["--watch","--owner-key","files.1","--app","org.quickshell","--title",title])
            .env_clear().envs(&ctx.vars).output()?;
        need(!invalid.status.success() && String::from_utf8_lossy(&invalid.stdout).contains("Unsupported owned geometry registration")
            && placement(ctx,key)?==Some(saved),"Unsupported owner registration changed private state")?;
        checks.push(format!("{kind} placement survives process restart, topology loss, and stale or unsupported identities"));
        ctx.checked("output HEADLESS-2 enable; output HEADLESS-2 resolution 1000x800")?;
        ctx.checked("focus output HEADLESS-1")?;
    }
    save(&ctx.opt.output.join("result.json"),&json!({"passed":true,"legacy":legacy,"checks":checks}))?;
    Ok(())
}

pub fn components(ctx:&mut Context,legacy:bool)->Result<()> {
    let palette=load(&ctx.repo.join("res/lumen-desktop/appearance/palette.json"))?;
    save(&ctx.root.0.join("appearance.json"),&json!({"font":palette["font"],"palette":palette["dark"],
        "preferences":{"reduceMotion":true},"windowsPreferences":{"rememberOwnedGeometry":true}}))?;
    ctx.checked("lumen_window margins disable; focus output HEADLESS-1")?;
    let mut checks=Vec::new();
    for kind in ["Settings","Calendar"] {
        ctx.vars.insert("OWNED_FIXTURE_KIND".into(),kind.into());
        let index=ctx.launch("quickshell", &["-p",&path(&ctx.qml)],&format!("owned-component-{kind}.log"),false)?;
        let state=wait("production owned component fit",||{
            let value=ui(ctx)?;
            Ok((value["fit"]["completed"].as_i64().is_some_and(|n|n>0)
                && value["fit"]["busy"]==false).then_some(value))
        })?;
        need(state["fit"]["error"]=="",format!("Production {kind} fit failed: {state}"))?;
        need(state["fit"]["persistenceSupported"]==!legacy,
            format!("Production {kind} exposed wrong native capability: {state}"))?;
        if legacy {
            need(state["fit"]["persistenceStatus"].as_str().is_some_and(|text|text.contains("updated compositor")),
                format!("Production {kind} did not disclose unavailable persistence: {state}"))?;
        }
        let footer=&state["footer"];
        need((!legacy || footer["height"].as_f64().is_some_and(|h|h>0.0))
            && footer["viewports"].as_array().is_some_and(|rows|rows.iter().all(|row|
                row["bottom"].as_f64().unwrap_or(f64::INFINITY)<=footer["y"].as_f64().unwrap_or(f64::NEG_INFINITY))),
            format!("Production {kind} placement status overlaps content: {state}"))?;
        checks.push(format!("Production {kind} component fits with correct capability and visible status footer"));
        ctx.children[index].stop(libc::SIGTERM)?;
    }
    save(&ctx.opt.output.join("result.json"),&json!({"passed":true,"legacy":legacy,"checks":checks}))?;
    Ok(())
}
