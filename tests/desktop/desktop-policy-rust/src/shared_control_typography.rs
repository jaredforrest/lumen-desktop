//! Native shared controls, constrained Calendar editor and hotplugged input at 150% text.
use super::*;
use super::physical_keyboard::Keyboard;

fn state(ctx: &Context) -> Result<Value> { Ok(serde_json::from_str(&ctx.ipc("state", &[])?)?) }
fn active(ctx: &Context) -> Result<Option<()>> { let row=state(ctx)?; Ok((row["active"]==true && row["scale"]==1.5).then_some(())) }
fn focused(ctx: &Context, name: &str) -> Result<()> {
    wait(name, || Ok((state(ctx)?["focus"]==name).then_some(())))
}
fn actions(path: &Path, provider: &str) -> Result<Vec<Value>> {
    if !path.exists() { return Ok(Vec::new()) }
    fs::read_to_string(path)?.lines().map(serde_json::from_str::<Value>)
        .collect::<std::result::Result<Vec<_>,_>>().map(|rows| rows.into_iter().filter(|row|row["provider"]==provider).collect()).map_err(Into::into)
}
fn bounded(row: &Value, rect: &Value) -> Result<()> {
    let (x,y,w,h)=(rect["x"].as_f64().ok_or("Missing x")?,rect["y"].as_f64().ok_or("Missing y")?,
        rect["width"].as_f64().ok_or("Missing width")?,rect["height"].as_f64().ok_or("Missing height")?);
    need(x>=0.0 && y>=0.0 && w>0.0 && h>0.0 && x+w<=row["width"].as_f64().ok_or("Missing owner width")?+1.0
        && y+h<=row["height"].as_f64().ok_or("Missing owner height")?+1.0,format!("Control outside owner: {row}"))
}
fn paint(ctx:&Context,pid:u32,title:&str,subject:&Value,name:&str)->Result<Value>{
    let rect=&subject["rect"]; let node=find_node(&ctx.tree()?,|row|row["pid"]==pid && row["name"]==title)
        .ok_or_else(||fail(format!("Missing native {title} window")))?;
    let x=number(&node["rect"]["x"])?+number(&node["window_rect"]["x"])?+rect["x"].as_f64().ok_or("Paint x")?.round() as i64;
    let y=number(&node["rect"]["y"])?+number(&node["window_rect"]["y"])?+rect["y"].as_f64().ok_or("Paint y")?.round() as i64;
    let w=rect["width"].as_f64().ok_or("Paint width")?.round() as i64;
    let h=rect["height"].as_f64().ok_or("Paint height")?.round() as i64;
    need(w>0 && h>0,"Empty paint target")?;
    let color=subject["color"].as_str().ok_or("Missing paint color")?;
    need(color.len()==7 && color.starts_with('#'),"Invalid paint color")?;
    let rgb=[u8::from_str_radix(&color[1..3],16)?,u8::from_str_radix(&color[3..5],16)?,u8::from_str_radix(&color[5..7],16)?];
    let file=ctx.opt.output.join(format!("{name}.png"));
    ctx.call("grim", &["-o","HEADLESS-1",&path(&file)])?;
    let histogram=ctx.call("magick", &[&path(&file),"-crop",&format!("{w}x{h}+{x}+{y}"),"+repage","-format","%c","histogram:info:"])?;
    let mut ink=0usize;
    for line in histogram.lines(){
        let Some((count,rest))=line.trim().split_once(':') else {continue};
        let Some((_,tuple))=rest.split_once('(') else {continue};
        let Some((tuple,_))=tuple.split_once(')') else {continue};
        let channels=tuple.split(',').take(3).map(|part|part.trim().parse::<i32>()).collect::<std::result::Result<Vec<_>,_>>()?;
        if channels.len()==3 && channels.iter().enumerate().all(|(i,n)|(n-rgb[i] as i32).abs()<=2){ink+=count.trim().parse::<usize>()?}
    }
    need(ink>=4,format!("No actual {name} ink: {ink}"))?;
    Ok(json!({"conId":node["id"],"pid":pid,"box":[x,y,w,h],"inkPixels":ink}))
}
fn protocol_count(log:&str,kind:&str,event:&str)->usize{
    log.lines().filter(|line|line.contains(&format!("wl_{kind}#")) && line.contains(&format!(".{event}("))).count()
}
fn input_absent(ctx:&Context,kind:&str)->Result<Option<()>>{
    let rows=ctx.sway(&["-t","get_inputs"])?;
    Ok(rows.as_array().is_some_and(|rows|rows.iter().all(|row|row["type"]!=kind)).then_some(()))
}

pub fn run(ctx:&mut Context)->Result<()> {
    for (key,value) in [("LUMEN_UNIFIED_OWNED_HEADERS","1"),("LUMEN_NATIVE_WINDOW_CONTROLS","0"),
        ("LUMEN_NATIVE_WINDOW_ACTIONS","0"),("LUMEN_FILE_SERVICE","0"),("LUMEN_LAUNCH_SCOPES","0"),
        ("LUMEN_NEARBY_SHARING_PROVIDER","")] {ctx.vars.insert(key.into(),value.into());}
    let palette=load(&ctx.repo.join("res/lumen-desktop/appearance/palette.json"))?;
    save(&ctx.root.0.join("appearance.json"),&json!({"font":palette["font"],"palette":palette["light"],
        "preferences":{"textScale":1.5,"reduceMotion":true,"reduceTransparency":true}}))?;
    let bin=ctx.root.0.join("bin");fs::create_dir(&bin)?;
    let executable=std::env::current_exe()?.parent().ok_or("Fixture executable parent")?.to_path_buf();
    for (source,target) in [("shared_control_audio_provider","wpctl"),("shared_control_calendar_provider","lumen-calendar"),
        ("shared_control_display_provider","lumen-system-controls")] {symlink(executable.join(source),bin.join(target))?;}
    let old=ctx.vars.get("PATH").cloned().unwrap_or_default();ctx.vars.insert("PATH".into(),format!("{}:{old}",bin.display()));
    let action_path=ctx.opt.output.join("provider-actions.jsonl");
    ctx.vars.insert("LUMEN_FIXTURE_SHARED_ACTIONS".into(),path(&action_path));
    ctx.vars.insert("WAYLAND_DEBUG".into(),"client".into());
    let qml=path(&ctx.qml);
    let shell=ctx.launch("quickshell", &["--no-duplicate","-p",&qml],"shared-control-shell.log",false)?;
    let pid=ctx.children[shell].0.id();
    let mut keyboard=Keyboard::start(ctx,"shared-control-keyboard.log")?;
    ctx.pointer_start()?;
    wait("shared control IPC",||Ok((ctx.ipc("ready",&[])?=="true").then_some(())))?;
    let mut cases=Vec::new();
    ctx.ipc("open", &["Audio"])?;wait("audio window",||active(ctx))?;
    ctx.ipc("focus", &["Private playback device volume"])?;focused(ctx,"Private playback device volume")?;
    keyboard.key("Right")?;
    wait("volume write",||Ok((actions(&action_path,"audio")?.len()==1).then_some(())))?;
    need(actions(&action_path,"audio")?[0]["args"].as_array().is_some_and(|a|a.len()>=4 && a[0]=="set-volume" && a[1]=="-l" && a[2]=="1" && a[3]=="1"),"Volume adapter command wrong")?;
    keyboard.key("Tab")?;focused(ctx,"Mute Private playback device")?;keyboard.key("space")?;
    wait("mute write",||Ok((actions(&action_path,"audio")?.len()==2).then_some(())))?;
    need(actions(&action_path,"audio")?[1]["args"]==json!(["set-mute","1","1"]),"Mute adapter command wrong")?;
    cases.push(json!({"kind":"Audio","state":state(ctx)?,"writes":actions(&action_path,"audio")?}));
    ctx.ipc("open", &["Date"])?;wait("date window",||active(ctx))?;
    ctx.ipc("focus", &["Starts year"])?;focused(ctx,"Starts year")?;
    keyboard.key("Up")?;wait("date numeric step",||Ok((state(ctx)?["date"].as_str().is_some_and(|s|s.starts_with("2027-"))).then_some(())))?;
    cases.push(json!({"kind":"Date","state":state(ctx)?}));
    keyboard.key("Escape")?;wait("date dismissal",||Ok((state(ctx)?["visible"]==false).then_some(())))?;
    for (label,focus,expected,child) in [("input","Default input",None,false),("cancel","Cancel",Some("cancel"),false),
        ("cancel-child","Cancel",Some("cancel"),true),("other","Other",Some("other"),false),
        ("primary","Proceed",Some("primary"),false),("primary-child","Proceed",Some("primary"),true),
        ("disabled","Default input",None,false),("hidden","Default input",None,false)] {
        ctx.ipc("returnCase", &[label])?;wait("return window",||active(ctx))?;
        ctx.ipc(if child {"focusChild"} else {"focus"},&[focus])?;focused(ctx,focus)?;
        let row=state(ctx)?;
        if let Some(recipient)=expected {need(row["focusAncestry"].as_array().is_some_and(|a|a.iter().any(|x|x==recipient)),format!("Return ancestry {label}: {row}"))?;}
        keyboard.key("Return")?;
        if let Some(recipient)=expected {
            wait("return action",||Ok((state(ctx)?["returnActions"].as_array().is_some_and(|a|!a.is_empty())).then_some(())))?;
            need(state(ctx)?["returnActions"]==json!([recipient]),format!("Wrong Return action {label}"))?;
        } else {
            keyboard.text("z")?;
            wait("return input",||Ok((state(ctx)?["input"]=="z").then_some(())))?;
            need(state(ctx)?["returnActions"]==json!([]),format!("Unwanted Return action {label}: {}",state(ctx)?))?;
        }
        cases.push(json!({"kind":"ReturnSemantics","case":label,"state":state(ctx)?}));
    }
    ctx.ipc("open", &["CalendarEditor"])?;
    wait("calendar editor parent",||{let row=state(ctx)?;Ok((row["active"]==true && row["loaded"]==true && row["busy"]==false).then_some(()))})?;
    ctx.ipc("edit", &[])?;focused(ctx,"Event title")?;
    ctx.ipc("focus", &["Starts year"])?;focused(ctx,"Starts year")?;keyboard.key("Up")?;
    ctx.ipc("focus", &["Cancel"])?;focused(ctx,"Cancel")?;
    let row=state(ctx)?;bounded(&row,&row["target"])?;
    let painted=paint(ctx,pid,"Calendar",&json!({"rect":row["targetText"]["rect"],"color":row["targetText"]["color"]}),"editor")?;
    keyboard.key("Return")?;wait("Calendar Cancel",||Ok((state(ctx)?["editing"]==false).then_some(())))?;
    need(actions(&action_path,"calendar")?.iter().all(|row|!matches!(row["request"]["action"].as_str(),Some("create"|"update"|"delete"))),"Calendar Cancel attempted mutation")?;
    cases.push(json!({"kind":"CalendarEditor","state":state(ctx)?,"painted":painted}));
    keyboard.key("Escape")?;
    ctx.ipc("open", &["Brightness"])?;wait("brightness window",||active(ctx))?;
    wait("brightness slider",||Ok((state(ctx)?["slider"].is_object()).then_some(())))?;
    ctx.ipc("focus", &["Display brightness"])?;focused(ctx,"Display brightness")?;
    let node=find_node(&ctx.tree()?,|row|row["pid"]==pid && row["name"]=="Shared Controls").ok_or("Shared Controls node missing")?;
    let row=state(ctx)?;let rect=&row["slider"];
    let x=number(&node["rect"]["x"])? as f64+number(&node["window_rect"]["x"])? as f64+rect["x"].as_f64().ok_or("Slider x")?+rect["width"].as_f64().ok_or("Slider width")?/2.0;
    let y=number(&node["rect"]["y"])? as f64+number(&node["window_rect"]["y"])? as f64+rect["y"].as_f64().ok_or("Slider y")?+rect["height"].as_f64().ok_or("Slider height")?/2.0;
    ctx.move_pointer((x,y))?;
    wait("brightness tooltip",||Ok((state(ctx)?["tooltip"]["opened"]==true).then_some(())))?;
    let row=state(ctx)?;bounded(&row,&row["tooltip"]["rect"])?;
    need(row["tooltip"]["pixelSize"]==17,format!("Tooltip text scale missing: {row}"))?;
    let painted=paint(ctx,pid,"Shared Controls",&row["tooltip"],"brightness")?;
    cases.push(json!({"kind":"Brightness","state":row,"painted":painted}));
    keyboard.key("Escape")?;
    let dismissed_tip=wait("brightness supplementary help dismissal",||{
        let row=state(ctx)?;
        Ok((row["tooltip"]["opened"]==false).then_some(row))
    })?;
    save(&ctx.opt.output.join("brightness-tooltip-dismissed.json"),&dismissed_tip)?;
    need(dismissed_tip["visible"]==true && dismissed_tip["active"]==true,"Supplementary help dismissal also closed its owner")?;
    cases.push(json!({"kind":"BrightnessTooltipDismissal","state":dismissed_tip}));
    keyboard.key("Escape")?;wait("brightness owner dismissal after help",||Ok((state(ctx)?["visible"]==false).then_some(())))?;
    ctx.ipc("returnCase", &["reconnect"])?;wait("reconnect window",||active(ctx))?;
    ctx.ipc("focus", &["Default input"])?;focused(ctx,"Default input")?;
    let log_path=ctx.opt.output.join("shared-control-shell.log");
    let before=protocol_count(&fs::read_to_string(&log_path)?,"keyboard","enter");
    drop(keyboard);wait("keyboard capability absent",||input_absent(ctx,"keyboard"))?;
    let mut keyboard=Keyboard::start(ctx,"shared-control-keyboard-reconnected.log")?;
    wait("keyboard re-enter",||{let log=fs::read_to_string(&log_path)?;Ok((protocol_count(&log,"keyboard","enter")>before && state(ctx)?["active"]==true).then_some(()))})?;
    keyboard.text("reconnected")?;wait("reconnected keyboard text",||Ok((state(ctx)?["input"]=="reconnected").then_some(())))?;
    cases.push(json!({"kind":"KeyboardReconnect","state":state(ctx)?}));
    let before=protocol_count(&fs::read_to_string(&log_path)?,"pointer","enter");
    let pointer=ctx.pointer.take().ok_or("Pointer missing")?;
    ctx.children[pointer].stop(libc::SIGTERM)?;
    wait("pointer capability absent",||input_absent(ctx,"pointer"))?;
    ctx.pointer_start()?;
    let node=find_node(&ctx.tree()?,|row|row["pid"]==pid && row["name"]=="Return eligibility").ok_or("Return eligibility node missing")?;
    let row=state(ctx)?;let rect=&row["target"];
    let x=number(&node["rect"]["x"])? as f64+number(&node["window_rect"]["x"])? as f64+rect["x"].as_f64().ok_or("Return x")?+rect["width"].as_f64().ok_or("Return width")?/2.0;
    let y=number(&node["rect"]["y"])? as f64+number(&node["window_rect"]["y"])? as f64+rect["y"].as_f64().ok_or("Return y")?+rect["height"].as_f64().ok_or("Return height")?/2.0;
    ctx.move_pointer((x,y))?;
    wait("pointer re-enter",||{let log=fs::read_to_string(&log_path)?;Ok((protocol_count(&log,"pointer","enter")>before && state(ctx)?["active"]==true).then_some(()))})?;
    ctx.press()?;ctx.release()?;wait("pointer field press",||Ok((state(ctx)?["returnPointerPresses"]==1).then_some(())))?;
    keyboard.text(" pointer")?;wait("pointer preserves focus",||Ok((state(ctx)?["input"]=="reconnected pointer").then_some(())))?;
    need(state(ctx)?["returnActions"]==json!([]),"Pointer reconnect triggered action")?;
    cases.push(json!({"kind":"PointerReconnect","state":state(ctx)?}));
    ctx.ipc("open", &["Brightness"])?;wait("last window",||active(ctx))?;
    keyboard.key("Escape")?;wait("last dismissal",||Ok((state(ctx)?["visible"]==false).then_some(())))?;
    let log=fs::read_to_string(&log_path)?;
    need(log.len()<=8*1024*1024,"Protocol trace exceeded 8 MiB")?;
    need(!["ReferenceError","TypeError","binding loop"].iter().any(|s|log.contains(s)),"Shared control QML diagnostic")?;
    need(protocol_count(&log,"keyboard","enter")>0 && protocol_count(&log,"keyboard","leave")>0
        && protocol_count(&log,"pointer","enter")>0 && protocol_count(&log,"pointer","leave")>0,"Missing protocol enter/leave")?;
    save(&ctx.opt.output.join("result.json"),&json!({"passed":true,"cases":cases,"protocolTraceBytes":log.len(),
        "scope":"Actual shared control input and constrained editor cancellation; private provider and input devices"}))?;
    println!("PASS shared control native typography: {} cases",cases.len());
    Ok(())
}
