//! Live launcher and switcher typography, physical input and captured intent.
use super::*;
use crate::physical_keyboard::Keyboard;

fn state(ctx: &Context) -> Result<Value> { ctx.ui() }
fn ready(ctx: &Context, predicate: impl Fn(&Value)->bool, label: &str) -> Result<Value> {
    wait(label,|| {let row=state(ctx)?;Ok(predicate(&row).then_some(row))})
}
fn focus(ctx: &Context, label: &str) -> Result<Value> {
    ctx.ipc("focusLabel",&[label])?;
    let row=ready(ctx,|r|r["control"]["focus"]==true,"launcher focused control")?;
    let rect=&row["control"]["rect"];let clip=&row["control"]["clipped"];
    need(clip["width"].as_f64().unwrap_or(0.)>=rect["width"].as_f64().unwrap_or(f64::INFINITY)-1. &&
        clip["height"].as_f64().unwrap_or(0.)>=rect["height"].as_f64().unwrap_or(f64::INFINITY)-1.,
        format!("Launcher focus clipped: {row}"))?;
    Ok(row)
}
fn configure(ctx: &Context,mode:&str,scale:&str) -> Result<()> {ctx.ipc("configure",&[mode,scale])?;Ok(())}
fn open(ctx:&Context,value:&str) -> Result<()> {ctx.ipc("open",&[value])?;ready(ctx,|r|r["visible"]==true && r["windowActive"]==true,"launcher window active")?;Ok(())}
fn type_query(ctx:&Context,k:&mut Keyboard,query:&str,count:i64) -> Result<()> {
    wait("launcher search focus",||Ok((ctx.ipc("focusTarget",&[])?=="true").then_some(())))?;
    k.text(query)?;
    ready(ctx,|r|r["query"]==query && r["launcher"]["count"]==count,"physical launcher query")?;
    Ok(())
}
fn target_point(row:&Value) -> Result<(f64,f64)> {
    let rect=&row["control"]["rect"];
    Ok((rect["x"].as_f64().ok_or("Target x")?+rect["width"].as_f64().ok_or("Target width")?/2.,
        rect["y"].as_f64().ok_or("Target y")?+rect["height"].as_f64().ok_or("Target height")?/2.))
}
fn painted(ctx:&Context,name:&str,selected_only:bool) -> Result<Value> {
    let row=state(ctx)?;
    let capture=ctx.opt.output.join(format!("{name}.png"));ctx.picture(&format!("{name}.png"))?;
    let body=&row["body"];
    let wanted=if row["family"]=="switcher" {row["switcherLabel"].as_str().unwrap_or("")}
        else if row["wanted"]=="Open" || row["wanted"]=="Show Folder" {row["wanted"].as_str().unwrap_or("")}
        else {row["currentLabel"].as_str().unwrap_or("")};
    let texts=row["texts"].as_array().ok_or("Missing launcher texts")?;
    let mut receipts=Vec::new();
    for text in texts {
        if text["text"].as_str()!=Some(wanted) && (row["family"]!="switcher" || selected_only) {continue}
        if row["family"]=="switcher" && selected_only && text["text"]!=row["switcherLabel"] {continue}
        let rect=&text["rect"];
        let x=(body["x"].as_f64().unwrap_or(0.)+rect["x"].as_f64().unwrap_or(0.)).round() as i64;
        let y=(body["y"].as_f64().unwrap_or(0.)+rect["y"].as_f64().unwrap_or(0.)).round() as i64;
        let w=rect["width"].as_f64().unwrap_or(0.).round() as i64;
        let h=rect["height"].as_f64().unwrap_or(0.).round() as i64;
        if w<=0 || h<=0 {continue}
        let color=text["color"].as_str().ok_or("Text color")?;
        if !color.starts_with('#') || color.len()<7 {continue}
        let rgb=[u8::from_str_radix(&color[1..3],16)?,u8::from_str_radix(&color[3..5],16)?,u8::from_str_radix(&color[5..7],16)?];
        let histogram=ctx.call("magick",&[&path(&capture),"-crop",&format!("{w}x{h}+{x}+{y}"),"+repage","-format","%c","histogram:info:"])?;
        let mut pixels=0usize;
        for line in histogram.lines() {
            let Some((count,rest))=line.trim().split_once(':') else {continue};
            let Some((_,tuple))=rest.split_once('(') else {continue};
            let Some((tuple,_))=tuple.split_once(')') else {continue};
            let channels=tuple.split(',').take(3).map(|v|v.trim().parse::<i32>()).collect::<std::result::Result<Vec<_>,_>>()?;
            if channels.len()==3 && channels.iter().enumerate().all(|(i,v)|(v-rgb[i] as i32).abs()<=32) {pixels+=count.trim().parse::<usize>()?}
        }
        receipts.push(json!({"text":text["text"],"inkPixels":pixels,"cropPixels":w*h,"rect":rect}));
    }
    let required:Vec<_>=receipts.iter().filter(|r|r["text"]==wanted).collect();
    need(!required.is_empty() && required.iter().all(|r|r["inkPixels"].as_u64().unwrap_or(0)>=4 &&
        (r["inkPixels"].as_f64().unwrap_or(f64::INFINITY))<(r["cropPixels"].as_f64().unwrap_or(0.)*0.6)),
        format!("Launcher label not painted: {name} {wanted} {receipts:?}"))?;
    save(&ctx.opt.output.join(format!("{name}-paint.json")),&json!({"state":row,"receipts":receipts}))?;
    Ok(json!({"capture":capture,"ink":required}))
}
fn action(ctx:&Context,expected:Value) -> Result<()> {
    ready(ctx,|r|r["actions"]==expected,"captured launcher action")?;Ok(())
}
fn no_action(ctx:&Context,k:&mut Keyboard) -> Result<()> {
    wait("launcher search restored",||Ok((ctx.ipc("focusTarget",&[])?=="true").then_some(())))?;
    k.text("z")?;
    ready(ctx,|r|r["query"]=="applicationz" && r["actions"].as_array().is_some_and(Vec::is_empty),"stale launcher action refused")?;
    k.key("Escape")?;ready(ctx,|r|r["visible"]==false,"launcher escape")?;Ok(())
}
fn long_preview(ctx:&mut Context,k:&mut Keyboard,mode:&str) -> Result<Value> {
    configure(ctx,mode,"1.5")?;open(ctx,"launcher")?;
    ctx.ipc("openFiles",&[])?;ctx.ipc("longPreview",&[])?;type_query(ctx,k,"private",1)?;
    focus(ctx,"Search result preview")?;
    k.key("Home")?;ready(ctx,|r|r["control"]["contentY"]==0,"preview Home")?;
    let initial=state(ctx)?;
    need(initial["control"]["contentHeight"].as_f64().unwrap_or(0.)>1000.,"Long preview content missing")?;
    k.key("Down")?;let arrow=ready(ctx,|r|r["control"]["contentY"].as_f64().unwrap_or(0.)>0.,"preview Down")?;
    k.key("PageDown")?;let page=ready(ctx,|r|r["control"]["contentY"].as_f64().unwrap_or(0.)>arrow["control"]["contentY"].as_f64().unwrap_or(f64::INFINITY),"preview PageDown")?;
    k.key("End")?;
    let end=ready(ctx,|r| {
        let c=&r["control"];
        c["contentY"].as_f64().is_some_and(|y|y>=c["contentHeight"].as_f64().unwrap_or(f64::INFINITY)-c["rect"]["height"].as_f64().unwrap_or(0.)-1.)
    },"preview End")?;
    let body=end["texts"].as_array().ok_or("Long preview text")?.iter()
        .find(|r|r["text"].as_str().is_some_and(|s|s.ends_with("FINAL PRIVATE PREVIEW LINE"))).ok_or("Final preview text missing")?;
    let bottom=end["body"]["y"].as_f64().ok_or("Preview body y")?+
        body["rect"]["y"].as_f64().ok_or("Preview text y")?+
        body["rect"]["height"].as_f64().ok_or("Preview text height")?;
    let viewport=&end["control"]["rect"];
    need(bottom>=viewport["y"].as_f64().unwrap_or(f64::INFINITY) &&
        bottom<=viewport["y"].as_f64().unwrap_or(0.)+viewport["height"].as_f64().unwrap_or(0.) &&
        body["height"].as_f64().unwrap_or(0.)>1000.,"Long preview final line not in viewport")?;
    ctx.picture(&format!("{mode}-long-end.png"))?;
    let x=(end["body"]["x"].as_f64().unwrap_or(0.)+body["rect"]["x"].as_f64().unwrap_or(0.)).round() as i64;
    let width=body["width"].as_f64().unwrap_or(0.).round() as i64;
    let y=(bottom-24.).round() as i64;
    need(width>0 && y>=0,"Invalid final line crop")?;
    let color=body["color"].as_str().ok_or("Preview color")?;
    let rgb=[u8::from_str_radix(&color[1..3],16)?,u8::from_str_radix(&color[3..5],16)?,u8::from_str_radix(&color[5..7],16)?];
    let histogram=ctx.call("magick",&[&path(&ctx.opt.output.join(format!("{mode}-long-end.png"))),
        "-crop",&format!("{width}x24+{x}+{y}"),"+repage","-format","%c","histogram:info:"])?;
    let mut pixels=0usize;
    for line in histogram.lines() {
        let Some((count,rest))=line.trim().split_once(':') else {continue};
        let Some((_,tuple))=rest.split_once('(') else {continue};
        let Some((tuple,_))=tuple.split_once(')') else {continue};
        let channels=tuple.split(',').take(3).map(|v|v.trim().parse::<i32>()).collect::<std::result::Result<Vec<_>,_>>()?;
        if channels.len()==3 && channels.iter().enumerate().all(|(i,v)|(v-rgb[i] as i32).abs()<=32) {pixels+=count.trim().parse::<usize>()?}
    }
    need(pixels>=4 && (pixels as i64)<width*24*6/10,"Final preview text has no bounded ink")?;
    k.key("PageUp")?;ready(ctx,|r|r["control"]["contentY"].as_f64().unwrap_or(f64::INFINITY)<end["control"]["contentY"].as_f64().unwrap_or(0.),"preview PageUp")?;
    k.key("End")?;ready(ctx,|r|r["control"]["contentY"].as_f64().unwrap_or(0.)>=end["control"]["contentY"].as_f64().unwrap_or(f64::INFINITY)-1.,"preview End restored")?;
    k.key("Tab")?;ready(ctx,|r|r["focus"].as_array().is_some_and(|v|v.iter().any(|x|x.as_str().is_some_and(|s|s.contains("Open")))),"preview Open Tab")?;
    k.key("Tab")?;ready(ctx,|r|r["focus"].as_array().is_some_and(|v|v.iter().any(|x|x.as_str().is_some_and(|s|s.contains("Show Folder")))),"preview Show Folder Tab")?;
    k.key("Return")?;action(ctx,json!([{"path":"/home/fixture/document.txt","parent":true,"token":"private-token","browse":true}]))?;
    open(ctx,"launcher")?;ctx.ipc("openFiles",&[])?;ctx.ipc("longPreview",&[])?;type_query(ctx,k,"private",1)?;
    let row=focus(ctx,"Search result preview")?;k.key("Home")?;
    ready(ctx,|r|r["control"]["contentY"]==0,"preview wheel start")?;
    ctx.move_pointer(target_point(&row)?)?;ctx.pointer_command("w 1")?;
    let wheel=ready(ctx,|r|r["control"]["contentY"].as_f64().unwrap_or(0.)>0.,"preview physical wheel")?;
    k.key("Escape")?;ready(ctx,|r|r["visible"]==false,"long preview Escape")?;
    Ok(json!({"mode":mode,"case":"long-preview-keyboard-and-wheel","initial":initial,"arrow":arrow,"page":page,"end":end,"wheel":wheel,"finalInkPixels":pixels}))
}
pub fn run(ctx:&mut Context) -> Result<()> {
    for (key,value) in [("LUMEN_UNIFIED_OWNED_HEADERS","1"),("LUMEN_NATIVE_WINDOW_CONTROLS","0"),
        ("LUMEN_NATIVE_WINDOW_ACTIONS","0"),("LUMEN_LAUNCH_SCOPES","0"),("LUMEN_NEARBY_SHARING_PROVIDER","")] {
        ctx.vars.insert(key.into(),value.into());
    }
    let palette=load(&ctx.repo.join("res/lumen-desktop/appearance/palette.json"))?;
    ctx.vars.insert("LUMEN_FIXTURE_PALETTES".into(),serde_json::to_string(&palette)?);
    save(&ctx.root.0.join("settings.json"),&json!({"search":{"historyEnabled":false},"clipboard":{"enabled":false}}))?;
    ctx.vars.insert("LUMEN_NIX_SETTINGS_ACTIVE".into(),path(&ctx.root.0.join("settings.json")));
    save(&ctx.root.0.join("appearance.json"),&json!({"font":palette["font"],"palette":palette["light"],
        "preferences":{"textScale":1.5,"reduceMotion":true,"reduceTransparency":true},"searchPreferences":{"disabledProviders":["clipboard"]}}))?;
    ctx.checked("output HEADLESS-1 resolution 1200x900")?;
    let qml=path(&ctx.qml);ctx.launch("quickshell",&["-p",&qml],"launcher-shell.log",false)?;
    let mut keyboard=Keyboard::start(ctx,"launcher-keyboard.log")?;
    ctx.pointer_start()?;
    ready(ctx,|r|r["width"].as_i64().unwrap_or(0)>0,"launcher IPC")?;
    let mut checks=Vec::new();
    ctx.checked("output HEADLESS-1 resolution 480x360")?;
    for mode in ["light","dark"] {checks.push(long_preview(ctx,&mut keyboard,mode)?);}
    for small in [false,true] {
        ctx.checked(if small {"output HEADLESS-1 resolution 480x360"} else {"output HEADLESS-1 resolution 1200x900"})?;
        for mode in ["light","dark"] {
            configure(ctx,mode,"1.5")?;
            open(ctx,"launcher")?;ctx.ipc("layout",&["list"])?;type_query(ctx,&mut keyboard,"application",2)?;
            keyboard.key("Down")?;ready(ctx,|r|r["launcher"]["selected"]==1,"launcher list selection")?;
            let list_paint=painted(ctx,&format!("{mode}-{small}-list"),false)?;
            keyboard.key("Return")?;action(ctx,json!(["fixture.second"]))?;
            checks.push(json!({"mode":mode,"small":small,"case":"list-selection-return","paint":list_paint}));
            open(ctx,"launcher")?;ctx.ipc("layout",&["preview"])?;type_query(ctx,&mut keyboard,"application",2)?;
            focus(ctx,"Search result preview")?;keyboard.key("End")?;
            if small {ready(ctx,|r|r["control"]["contentY"].as_f64().unwrap_or(0.)>0.,"preview scroll")?;}
            keyboard.key("Tab")?;ready(ctx,|r|r["focus"].as_array().is_some_and(|items|items.iter().any(|v|v.as_str().is_some_and(|s|s.contains("Open")))),"preview Open focus")?;
            focus(ctx,"Open")?;let preview_paint=painted(ctx,&format!("{mode}-{small}-preview"),false)?;
            keyboard.key("space")?;action(ctx,json!(["fixture.first"]))?;
            checks.push(json!({"mode":mode,"small":small,"case":"preview-space","paint":preview_paint}));
            open(ctx,"launcher")?;ctx.ipc("layout",&["preview"])?;type_query(ctx,&mut keyboard,"application",2)?;
            focus(ctx,"Open")?;let before=state(ctx)?["currentKey"].clone();ctx.ipc("select",&["1"])?;
            ready(ctx,|r|r["currentKey"]!=before,"changed launcher selection")?;
            keyboard.key("Return")?;no_action(ctx,&mut keyboard)?;
            checks.push(json!({"mode":mode,"small":small,"case":"changed-selection-refuses-return"}));
            open(ctx,"launcher")?;ctx.ipc("layout",&["preview"])?;type_query(ctx,&mut keyboard,"application",2)?;
            focus(ctx,"Open")?;let before=state(ctx)?["currentKey"].clone();ctx.ipc("removeSelected",&[])?;
            ready(ctx,|r|r["currentKey"]!=before,"removed launcher result")?;
            keyboard.key("space")?;no_action(ctx,&mut keyboard)?;
            checks.push(json!({"mode":mode,"small":small,"case":"removed-result-refuses-space"}));
            open(ctx,"launcher")?;ctx.ipc("layout",&["preview"])?;type_query(ctx,&mut keyboard,"application",2)?;
            let row=focus(ctx,"Open")?;ctx.move_pointer(target_point(&row)?)?;
            ctx.press()?;ctx.release()?;action(ctx,json!(["fixture.first"]))?;
            checks.push(json!({"mode":mode,"small":small,"case":"preview-pointer-click"}));
            open(ctx,"launcher")?;ctx.ipc("layout",&["preview"])?;type_query(ctx,&mut keyboard,"application",2)?;
            let row=focus(ctx,"Open")?;let before=row["currentKey"].clone();ctx.move_pointer(target_point(&row)?)?;
            ctx.press()?;ready(ctx,|r|r["control"]["pressed"]==true,"held preview action")?;
            ctx.ipc("select",&["1"])?;ready(ctx,|r|r["currentKey"]!=before,"held preview retarget")?;
            ctx.release()?;ready(ctx,|r|r["control"]["pressed"]==false,"preview pointer release")?;
            no_action(ctx,&mut keyboard)?;
            checks.push(json!({"mode":mode,"small":small,"case":"changed-selection-refuses-pointer-release"}));
            open(ctx,"launcher")?;ctx.ipc("openFiles",&[])?;type_query(ctx,&mut keyboard,"private",1)?;
            focus(ctx,"Open")?;keyboard.key("Tab")?;focus(ctx,"Show Folder")?;
            let show_paint=painted(ctx,&format!("{mode}-{small}-show-folder"),false)?;
            keyboard.key("Return")?;
            action(ctx,json!([{"path":"/home/fixture/document.txt","parent":true,"token":"private-token","browse":true}]))?;
            checks.push(json!({"mode":mode,"small":small,"case":"captured-file-show-folder","paint":show_paint}));
            keyboard.key("Escape")?;
            ctx.ipc("resetEntries",&[])?;configure(ctx,mode,"1")?;open(ctx,"switcher")?;
            ready(ctx,|r|r["captionSize"].as_array().is_some_and(|v|!v.is_empty() && v.iter().all(|n|n==12)),"switcher default captions")?;
            let baseline=state(ctx)?;
            need(baseline["tileWidth"]==104 && baseline["captionMetrics"].as_array().is_some_and(|v|v.iter().all(|r|r["resolvedFamily"]==palette["font"])),"Switcher default text metrics")?;
            let default_paint=painted(ctx,&format!("{mode}-{small}-switcher-default"),false)?;
            for scale in ["1.15","1.3","1.5"] {
                configure(ctx,mode,scale)?;
                let expected=(12.*scale.parse::<f64>()?).round() as i64;
                ready(ctx,|r|r["captionSize"].as_array().is_some_and(|v|!v.is_empty()&&v.iter().all(|n|n==expected)),"scaled switcher captions")?;
                let enlarged=state(ctx)?;
                need(enlarged["iconSizes"].as_array().is_some_and(|v|!v.is_empty()&&v.iter().all(|r|r["width"]==66&&r["height"]==66)),"Switcher icons scaled unexpectedly")?;
                for row in enlarged["captionMetrics"].as_array().ok_or("Scaled caption metrics")? {
                    let prior=baseline["captionMetrics"].as_array().ok_or("Default caption metrics")?.iter()
                        .find(|v|v["text"]==row["text"]).ok_or("Caption changed identity")?;
                    let width=row["width"].as_f64().ok_or("Scaled caption width")?;
                    let natural=row["naturalWidth"].as_f64().ok_or("Scaled natural caption")?;
                    let baseline_width=prior["width"].as_f64().ok_or("Default caption width")?;
                    let baseline_natural=prior["naturalWidth"].as_f64().ok_or("Default natural caption")?;
                    need(natural>0. && baseline_natural>0. &&
                        (width+1.)/natural>=baseline_width/baseline_natural && row["resolvedFamily"]==palette["font"],
                        format!("Switcher caption lost capacity: {row} {prior}"))?;
                }
                let _=painted(ctx,&format!("{mode}-{small}-switcher-{scale}"),false)?;
            }
            keyboard.key("Right")?;ready(ctx,|r|r["selected"]==1,"switcher selection")?;
            keyboard.key("Escape")?;ready(ctx,|r|r["visible"]==false && r["actions"].as_array().is_some_and(Vec::is_empty),"switcher Escape")?;
            open(ctx,"switcher")?;keyboard.key("Right")?;ready(ctx,|r|r["selected"]==1,"switcher second selection")?;
            keyboard.key("Return")?;action(ctx,json!(["fixture.second"]))?;
            checks.push(json!({"mode":mode,"small":small,"case":"switcher-selection-cancel-commit","baseline":baseline,"paint":default_paint}));
            if small {
                ctx.ipc("manySwitcher",&[])?;ready(ctx,|r|r["visible"]==true&&r["selected"]==0,"many switcher windows")?;
                for i in 1..8 {keyboard.key("Right")?;ready(ctx,|r|r["selected"]==i,"narrow switcher selection")?;}
                let row=state(ctx)?;
                need(row["tileWidth"].as_i64().unwrap_or(0)>=156 && row["switcherLabel"]=="Application 8","Narrow switcher layout")?;
                let ink=painted(ctx,&format!("{mode}-many-switcher"),true)?;
                keyboard.key("Return")?;action(ctx,json!(["fixture.7"]))?;
                checks.push(json!({"mode":mode,"small":small,"case":"narrow-horizontal-switcher-selection","paint":ink}));
            }
        }
    }
    let log=fs::read_to_string(ctx.opt.output.join("launcher-shell.log"))?;
    need(!["ReferenceError:","TypeError:","Binding loop","Failed to load configuration","Cannot assign"].iter().any(|v|log.contains(v)),format!("Launcher QML errors: {log}"))?;
    save(&ctx.opt.output.join("result.json"),&json!({"passed":true,"checks":checks,
        "scope":"Physical launcher and switcher selection, captured intents and painted labels in basic/effects private compositors"}))?;
    println!("PASS launcher and switcher native typography: {} cases",checks.len());
    Ok(())
}
