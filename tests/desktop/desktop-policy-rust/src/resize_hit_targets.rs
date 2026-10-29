//! Pointer resizing at observed native frame edges and corners on three output scales.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Frame { x: i64, y: i64, width: i64, height: i64 }
#[derive(Clone, Copy, Debug)]
struct Edges { left: i64, right: i64, top: i64, bottom: i64 }
fn frame(node: &Value) -> Result<Frame> {
    let rect = &node["rect"];
    let deco = &node["deco_rect"];
    let d = number(&deco["height"])?;
    need(number(&node["current_border_width"])? > 0 && d > 0, format!("Native frame absent: {node}"))?;
    Ok(Frame { x:number(&rect["x"])?, y:number(&rect["y"])?-d,
        width:number(&rect["width"])?, height:number(&rect["height"])?+d })
}
fn edges(f: Frame) -> Edges { Edges {left:f.x,right:f.x+f.width,top:f.y,bottom:f.y+f.height} }
fn state(path: &Path) -> Result<Value> { load(path) }
fn node(ctx: &Context, id: &Value) -> Result<Value> { ctx.find(id)?.ok_or_else(|| fail(format!("Native container missing {id}"))) }
fn mapped(ctx: &Context, pid: u32) -> Result<Option<Value>> {
    Ok(find_node(&ctx.tree()?, |row| row["pid"] == pid && row["app_id"].as_str().is_some_and(|v| !v.is_empty())
        && row["deco_rect"]["height"].as_i64().is_some_and(|v| v > 0)))
}
fn configured(ctx: &Context, id: &Value, path: &Path) -> Result<Option<Value>> {
    let Some(row) = ctx.find(id)? else { return Ok(None) };
    let current = state(path)?;
    Ok((current["width"] == row["window_rect"]["width"] && current["height"] == row["window_rect"]["height"]).then_some(row))
}
fn ack(ctx: &Context, id: &Value, path: &Path) -> Result<Value> {
    wait("GTK configure acknowledgement", || configured(ctx, id, path))
}
fn move_window(ctx: &Context, id: &Value, x: i64, y: i64, width: i64, height: i64) -> Result<()> {
    let id = number(id)?;
    ctx.checked(&format!("[con_id={id}] resize set width {width} px height {height} px, move absolute position {x} px {y} px"))
}
fn focus(ctx: &Context, id: &Value) -> Result<()> {
    ctx.checked(&format!("[con_id={}] focus", number(id)?))?;
    wait("exact native focus", || Ok((node(ctx,id)?["focused"] == true).then_some(())))
}
fn counts(path: &Path) -> Result<(i64, i64)> {
    let row = state(path)?;
    Ok((number(&row["presses"])?, number(&row["releases"])?))
}
fn sample(node: &Value, h: i64, v: i64) -> Result<(f64, f64)> {
    let f = frame(node)?; let e = edges(f);
    let half = number(&node["current_border_width"])? as f64 / 2.0;
    Ok((if h < 0 {e.left as f64 + half} else if h > 0 {e.right as f64 - half} else {(e.left + e.right) as f64 / 2.0},
        if v < 0 {e.top as f64 + half} else if v > 0 {e.bottom as f64 - half} else {(e.top + e.bottom) as f64 / 2.0}))
}
fn drag(ctx: &mut Context, x: f64, y: f64, dx: i64, dy: i64, rapid: bool) -> Result<()> {
    ctx.move_pointer((x,y))?;
    ctx.press()?;
    if rapid {
        ctx.pointer_command(&format!("m {} {}", dx as f64 / 2.0, dy as f64 / 2.0))?;
        ctx.pointer_command(&format!("m {} {}", dx as f64 / 2.0, dy as f64 / 2.0))?;
    } else { ctx.pointer_command(&format!("m {dx} {dy}"))?; }
    ctx.release()
}
fn captured(node: &Value) -> Value { json!({"id":node["id"],"pid":node["pid"],"appId":node["app_id"],
    "rect":node["rect"],"deco":node["deco_rect"],"window":node["window_rect"]}) }
fn still(before: Edges, after: Edges, h: i64, v: i64) -> bool {
    let horizontal = if h < 0 { (after.right - before.right).abs() <= 1 }
        else if h > 0 { (after.left - before.left).abs() <= 1 }
        else { (after.left - before.left).abs() <= 1 && (after.right - before.right).abs() <= 1 };
    let vertical = if v < 0 { (after.bottom - before.bottom).abs() <= 1 }
        else if v > 0 { (after.top - before.top).abs() <= 1 }
        else { (after.top - before.top).abs() <= 1 && (after.bottom - before.bottom).abs() <= 1 };
    horizontal && vertical
}

pub fn run(ctx: &mut Context, rapid: bool) -> Result<()> {
    let client = path(ctx.opt.client.as_ref().ok_or("Missing Rust GTK resize client (--client)")?);
    for (key, value) in [("GDK_BACKEND","wayland"),("GTK_CSD","0"),("GTK_A11Y","none"),("NO_AT_BRIDGE","1")] {
        ctx.vars.insert(key.into(), value.into());
    }
    ctx.pointer_start()?;
    let target_path = ctx.root.0.join("target.json");
    let neighbor_path = ctx.root.0.join("neighbor.json");
    let target = ctx.launch(&client, &["org.example.ResizeTarget","Resize target",&path(&target_path)], "resize-target.log", false)?;
    let target_pid = ctx.children[target].0.id();
    let target_node = wait("GTK target mapping", || mapped(ctx, target_pid))?;
    let target_id = target_node["id"].clone();
    let neighbor = ctx.launch(&client, &["org.example.ResizeNeighbor","Resize neighbor",&path(&neighbor_path)], "resize-neighbor.log", false)?;
    let neighbor_pid = ctx.children[neighbor].0.id();
    let neighbor_node = wait("GTK neighbor mapping", || mapped(ctx, neighbor_pid))?;
    let neighbor_id = neighbor_node["id"].clone();
    let directions = [("left",-1,0),("right",1,0),("top",0,-1),("bottom",0,1),
        ("top-left",-1,-1),("top-right",1,-1),("bottom-left",-1,1),("bottom-right",1,1)];
    let mut checks = Vec::new(); let mut receipts = Vec::new();
    for (output, scale) in [("HEADLESS-1",1.0),("HEADLESS-2",1.5),("HEADLESS-2",2.0)] {
        ctx.checked(&format!("output {output} scale {scale}"))?;
        let actual = wait("output scale acknowledgement", || Ok(ctx.sway(&["-t","get_outputs"])?
            .as_array().and_then(|rows| rows.iter().find(|r| r["name"] == output && r["scale"].as_f64() == Some(scale))).cloned()))?;
        let ox = number(&actual["rect"]["x"])?; let oy = number(&actual["rect"]["y"])?;
        ctx.checked(&format!("[con_id={}] move container to output {output}", number(&target_id)?))?;
        ctx.checked(&format!("[con_id={}] move container to output {output}", number(&neighbor_id)?))?;
        for (label, h, v) in directions {
            move_window(ctx,&target_id,ox+220,oy+180,300,230)?;
            let target_node = ack(ctx,&target_id,&target_path)?; let target_frame = frame(&target_node)?;
            move_window(ctx,&neighbor_id,ox+10,oy+20,184,154)?;
            let neighbor_node = ack(ctx,&neighbor_id,&neighbor_path)?; let neighbor_frame = frame(&neighbor_node)?;
            let nx = if h < 0 {target_frame.x-neighbor_frame.width-8} else if h > 0 {target_frame.x+target_frame.width+8} else {target_frame.x};
            let ny = if v < 0 {target_frame.y-neighbor_frame.height-8} else if v > 0 {target_frame.y+target_frame.height+8} else {target_frame.y};
            move_window(ctx,&neighbor_id,nx,ny,184,154)?;
            let neighbor_node = ack(ctx,&neighbor_id,&neighbor_path)?;
            focus(ctx,&target_id)?;
            let target_node = ack(ctx,&target_id,&target_path)?;
            let before = edges(frame(&target_node)?);
            let neighbor_before = frame(&neighbor_node)?;
            let target_counts = counts(&target_path)?; let neighbor_counts = counts(&neighbor_path)?;
            let (x,y) = sample(&target_node,h,v)?;
            save(&ctx.opt.output.join("active-case.json"),&json!({"output":output,"scale":scale,"direction":label,
                "point":{"x":x,"y":y},"before":captured(&target_node),"neighbor":captured(&neighbor_node),
                "inputMode":if rapid {"two-motion-rapid-release"} else {"single-motion"}}))?;
            drag(ctx,x,y,h*32,v*24,rapid)?;
            let after_node = wait("native frame resize", || {
                let Some(row) = configured(ctx,&target_id,&target_path)? else {return Ok(None)};
                let after = edges(frame(&row)?);
                let moved = (h >= 0 || after.left < before.left) && (h <= 0 || after.right > before.right)
                    && (v >= 0 || after.top < before.top) && (v <= 0 || after.bottom > before.bottom);
                Ok((moved && still(before,after,h,v)).then_some(row))
            })?;
            let after = edges(frame(&after_node)?);
            need(still(before,after,h,v), format!("Opposite frame edge moved {label} {before:?} {after:?}"))?;
            need(frame(&node(ctx,&neighbor_id)?)? == neighbor_before && node(ctx,&target_id)?["focused"] == true
                && counts(&target_path)? == target_counts && counts(&neighbor_path)? == neighbor_counts,
                format!("Neighbor or client input leaked during {label}"))?;
            receipts.push(json!({"output":output,"scale":scale,"direction":label,"point":{"x":x,"y":y},
                "before":captured(&target_node),"after":captured(&after_node),
                "neighborBefore":captured(&neighbor_node),"neighborAfter":captured(&node(ctx,&neighbor_id)?),
                "clientCounts":{"target":target_counts,"neighbor":neighbor_counts}}));
            checks.push(format!("{output} scale{scale}: exact {label} frame resize with stationary opposite edges and no input leak"));
        }
        move_window(ctx,&target_id,ox+220,oy+180,300,230)?;
        let target_node = ack(ctx,&target_id,&target_path)?;
        let before = frame(&target_node)?;
        move_window(ctx,&neighbor_id,ox+10,oy+20,184,154)?;
        ack(ctx,&neighbor_id,&neighbor_path)?;
        focus(ctx,&target_id)?;
        let neighbor_before = frame(&node(ctx,&neighbor_id)?)?;
        let counts_before = counts(&target_path)?;
        let x = number(&target_node["rect"]["x"])? + number(&target_node["window_rect"]["x"])? + 40;
        let y = number(&target_node["rect"]["y"])? + 40;
        ctx.move_pointer((x as f64,y as f64))?; ctx.press()?; ctx.release()?;
        wait("GTK target content input", || Ok((counts(&target_path)? == (counts_before.0+1,counts_before.1+1)).then_some(())))?;
        need(frame(&node(ctx,&target_id)?)? == before && frame(&node(ctx,&neighbor_id)?)? == neighbor_before,
            "Content click resized a native frame")?;
        checks.push(format!("{output} scale{scale}: content click reaches GTK target without resize"));
        let (x,y) = sample(&target_node,1,0)?;
        move_window(ctx,&neighbor_id,x as i64-40,y as i64-65,184,154)?;
        ack(ctx,&neighbor_id,&neighbor_path)?;
        focus(ctx,&neighbor_id)?;
        let neighbor_before = frame(&node(ctx,&neighbor_id)?)?;
        let target_before = frame(&node(ctx,&target_id)?)?;
        let counts_before = counts(&neighbor_path)?;
        drag(ctx,x,y,16,0,rapid)?;
        wait("topmost GTK neighbor input", || Ok((counts(&neighbor_path)? == (counts_before.0+1,counts_before.1+1)).then_some(())))?;
        need(frame(&node(ctx,&target_id)?)? == target_before && frame(&node(ctx,&neighbor_id)?)? == neighbor_before
            && node(ctx,&neighbor_id)?["focused"] == true, "Covered target edge resized")?;
        checks.push(format!("{output} scale{scale}: topmost neighbor owns covered target edge"));
        move_window(ctx,&neighbor_id,ox+10,oy+20,184,154)?;
        ack(ctx,&neighbor_id,&neighbor_path)?; focus(ctx,&target_id)?;
        let target_node = ack(ctx,&target_id,&target_path)?;
        let (x,y) = sample(&target_node,1,1)?;
        drag(ctx,x,y,-200,-180,rapid)?;
        let small = wait("GTK minimum-size clamp", || {
            let Some(row) = configured(ctx,&target_id,&target_path)? else { return Ok(None) };
            Ok((row["window_rect"]["width"].as_i64().is_some_and(|v| v <= 200)
                && row["window_rect"]["height"].as_i64().is_some_and(|v| v <= 140)).then_some(row))
        })?;
        need(number(&small["window_rect"]["width"])? >= 180 && number(&small["window_rect"]["height"])? >= 120,
            "GTK minimum-size constraint violated")?;
        checks.push(format!("{output} scale{scale}: corner shrink preserves client minimum"));
        ctx.picture(&format!("scale-{scale}.png"))?;
    }
    if rapid {
        let before = frame(&ack(ctx,&target_id,&target_path)?)?;
        fs::write(target_path.with_extension("resize"), "[400,260]")?;
        let grown = wait("client-authored resize after interactive acknowledgement", || {
            let Some(row) = configured(ctx,&target_id,&target_path)? else {return Ok(None)};
            Ok((row["window_rect"]["width"] == 400 && row["window_rect"]["height"] == 260).then_some(row))
        })?;
        need(frame(&grown)?.x == before.x && frame(&grown)?.y == before.y, "Client resize moved frame origin")?;
        checks.push("Later client-authored resize remains operable".into());
    }
    save(&ctx.opt.output.join("receipts.json"), &json!(receipts))?;
    save(&ctx.opt.output.join("result.json"), &json!({"passed":true,"checks":checks,"effects":ctx.opt.effects,
        "inputMode":if rapid {"two-motion-rapid-release"} else {"single-motion"},
        "scope":"Observed native two-pixel border centres at three output scales"}))?;
    println!("PASS resize hit targets: {} checks", checks.len());
    Ok(())
}
