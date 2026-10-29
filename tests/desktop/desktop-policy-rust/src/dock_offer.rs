use super::*;

fn ipc(ctx: &Context, qml: &Path, target: &str, method: &str, args: &[&str]) -> Result<String> {
    let qml = path(qml);
    let mut command = vec!["-p",qml.as_str(),"ipc","call",target,method];
    command.extend_from_slice(args);
    ctx.call("quickshell", &command)
}
fn state(ctx: &Context, qml: &Path, target: &str) -> Result<Value> {
    Ok(serde_json::from_str(&ipc(ctx,qml,target,"status",&[])?)?)
}
fn records(path: &Path) -> Result<Vec<Value>> {
    if !path.exists() { return Ok(Vec::new()); }
    Ok(fs::read_to_string(path)?.lines().map(serde_json::from_str).collect::<std::result::Result<_,_>>()?)
}
fn uri(path: &Path) -> Result<String> {
    let mut result=String::from("file://");
    for byte in path.to_str().ok_or("Non-UTF8 offer path")?.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte,b'/'|b'-'|b'.'|b'_'|b'~') {result.push(byte as char)}
        else {result.push_str(&format!("%{byte:02X}"))}
    }
    Ok(result)
}
fn installed(ctx: &Context, name: &str) -> Result<PathBuf> {
    for directory in ctx.vars.get("PATH").ok_or("No PATH")?.split(':') {
        let path=Path::new(directory).join(name);
        if path.is_file() {return Ok(path.canonicalize()?);}
    }
    Err(fail(format!("Missing installed {name}")))
}
fn instrument(ctx: &Context) -> Result<()> {
    let path=ctx.root.0.join("assets/shell/Dock.qml");
    let original=fs::read_to_string(&path)?;
    let alias=r#"
    property alias fixtureDropWorker:dropWorker
    property string fixtureResult:""
    property var fixtureEvents:[]
    function fixtureRecord(kind,key,event){const offered=event.urls;fixtureEvents=fixtureEvents.concat([{kind:kind,key:key,hasUrls:event.hasUrls,length:offered.length,urls:Array.from(offered).map(url=>url.toString()),mapped:offered.map(url=>url.toString()),supportedActions:event.supportedActions}]);}
    function fixtureDrops(){const result=[];function visit(item){if(item.modelData && item.index!==undefined){const area=item.children.find(child=>child.containsDrag!==undefined);if(area)result.push({key:item.modelData.key,contains:area.containsDrag});}for(const child of item.children || [])visit(child);}visit(surface);return result;}
"#;
    let result="dock.fixtureResult=text;";
    let entered="dock.fixtureRecord(\"enter\",tile.modelData.key,drag);";
    let dropped="dock.fixtureRecord(\"drop\",tile.modelData.key,drop);";
    for needle in ["    id: dock","onEntered:drag=>{","onDropped:drop=>{"] {
        need(original.matches(needle).count()==1,format!("Dock instrumentation anchor changed: {needle}"))?;
    }
    let modified=original.replace("    id: dock",&format!("    id: dock{alias}"))
        .replace("onStreamFinished:{try{const result=JSON.parse(text);",&format!("onStreamFinished:{{{result}try{{const result=JSON.parse(text);"))
        .replace("onEntered:drag=>{",&format!("onEntered:drag=>{{{entered}"))
        .replace("onDropped:drop=>{",&format!("onDropped:drop=>{{{dropped}"));
    need(modified.replace(alias,"").replace(result,"").replace(entered,"").replace(dropped,"")==original,"Dock instrumentation is not reversible")?;
    fs::write(&path,&modified)?;
    save(&ctx.opt.output.join("instrumentation.json"),&json!({"originalSha256":format!("{:x}",Sha256::digest(original.as_bytes())),
        "modifiedSha256":format!("{:x}",Sha256::digest(modified.as_bytes())),"roundtrip":true}))?;
    Ok(())
}
fn target(ctx: &Context, qml: &Path, key: &str) -> Result<(f64,f64)> {
    let row=state(ctx,qml,"dockOffer")?;
    let index=row["keys"].as_array().ok_or("No Dock keys")?.iter().position(|item| item==key).ok_or("Target Dock key absent")? as f64;
    let width=row["width"].as_f64().ok_or("No Dock width")?;
    let slot=row["slot"].as_f64().ok_or("No Dock slot")?;
    let shelf=row["shelf"].as_f64().ok_or("No Dock shelf")?;
    Ok(((1280.0-width)/2.0+12.0+(index+0.5)*slot,900.0-7.0-shelf/2.0))
}
fn start_offer(ctx: &mut Context, peer: &Path, dock: &Path, id: &Value, uris: &[String], key: &str, accepted: bool) -> Result<()> {
    need(uris.len()==2,"Expected two offered URIs")?;
    need(ipc(ctx,peer,"offerPeer","configureUris",&[&uris[0],&uris[1]])?=="true","Peer URI configuration refused")?;
    wait("peer URI MIME bytes",||Ok((state(ctx,peer,"offerPeer")?["mimeBytes"]==uris.iter().map(|u|u.len()+2).sum::<usize>()).then_some(())))?;
    ctx.checked(&format!("[con_id={id}] focus"))?;
    let node=wait("focused peer",||Ok(ctx.find(id)?.filter(|row|row["focused"]==true)))?;
    let peer_state=state(ctx,peer,"offerPeer")?;
    need(node["window_rect"]["width"]==peer_state["width"] && node["window_rect"]["height"]==peer_state["height"],"Peer surface dimensions changed")?;
    let x=node["rect"]["x"].as_f64().ok_or("Peer x missing")?+node["window_rect"]["x"].as_f64().ok_or("Peer window x missing")?+peer_state["x"].as_f64().ok_or("Peer target x missing")?;
    let y=node["rect"]["y"].as_f64().ok_or("Peer y missing")?+node["window_rect"]["y"].as_f64().ok_or("Peer window y missing")?+peer_state["y"].as_f64().ok_or("Peer target y missing")?;
    ctx.move_pointer((x,y))?;
    wait("peer hover",||Ok((state(ctx,peer,"offerPeer")?["hovered"]==true).then_some(())))?;
    ctx.press()?;
    wait("peer press",||Ok((state(ctx,peer,"offerPeer")?["pressed"]==true).then_some(())))?;
    ctx.move_pointer((x+16.0,y))?;
    ctx.move_pointer((x+32.0,y))?;
    wait("native drag active",||Ok((state(ctx,peer,"offerPeer")?["active"]==true).then_some(())))?;
    ctx.move_pointer(target(ctx,dock,key)?)?;
    if accepted {
        wait("Dock contains native offer",||{
            let row=state(ctx,dock,"dockOffer")?;
            Ok(row["drops"].as_array().is_some_and(|items|items.iter().any(|item|item["key"]==key && item["contains"]==true)).then_some(()))
        })?;
    } else {
        wait("active refused compositor drag",||{
            let seats=ctx.sway(&["-t","get_seats"])?;
            Ok(seats.as_array().is_some_and(|rows|rows.iter().any(|seat|seat["lumen_drag"]["active"]==true)).then_some(()))
        })?;
    }
    Ok(())
}
pub fn run(ctx: &mut Context) -> Result<()> {
    let busy=ctx.opt.scenario=="dock-offer-busy";
    ctx.checked("output HEADLESS-1 resolution 1280x900 position 0 0; output HEADLESS-2 disable; focus output HEADLESS-1")?;
    ctx.pointer_start()?;
    ctx.pointer_command("m 0 0")?;
    ctx.launch("wtype", &["-s","120000"],"keyboard.log",false)?;
    wait("pointer capability",||{
        let seats=ctx.sway(&["-t","get_seats"])?;
        Ok(seats.as_array().and_then(|rows|rows.iter().find(|seat|seat["capabilities"].as_i64().is_some_and(|bits|bits&1!=0))).cloned())
    })?;
    ctx.vars.insert("LUMEN_LAUNCH_SCOPES".into(),"0".into());
    ctx.vars.insert("XDG_CURRENT_DESKTOP".into(),"sway".into());
    ctx.vars.insert("DBUS_SYSTEM_BUS_ADDRESS".into(),format!("unix:path={}/absent-system",ctx.root.0.display()));
    ctx.vars.insert("PULSE_SERVER".into(),format!("unix:{}/absent-audio",ctx.root.0.display()));
    ctx.vars.insert("XDG_DATA_DIRS".into(),ctx.vars.get("XDG_DATA_HOME").ok_or("No XDG data root")?.clone());
    let docs=ctx.root.0.join("offers");fs::create_dir(&docs)?;
    let alpha=docs.join("Alpha # % ü.txt");let beta=docs.join("Beta.txt");let image=docs.join("pixel.png");
    fs::write(&alpha,"first captured offer")?;fs::write(&beta,"second captured offer")?;
    fs::copy(ctx.repo.join("res/lumen-desktop/shell/wallpaper.png"),&image)?;
    let originals=[(&alpha,hash(&alpha)?),(&beta,hash(&beta)?),(&image,hash(&image)?)];
    let opened=ctx.opt.output.join("opened.jsonl");
    let exe=env::current_exe()?;let bin=exe.parent().ok_or("No fixture bin directory")?;
    let observer=bin.join("dock_offer_observer");need(observer.is_file(),"Missing Rust offer observer")?;
    let apps=Path::new(ctx.vars.get("XDG_DATA_HOME").ok_or("No XDG data home")?).join("applications");fs::create_dir_all(&apps)?;
    for (name,mime) in [("fixture-accept","text/plain"),("fixture-refuse","image/png")] {
        fs::write(apps.join(format!("{name}.desktop")),format!("[Desktop Entry]\nType=Application\nName=Offer Observer\nExec={} {} %U\nMimeType={mime};\nTerminal=false\n",observer.display(),opened.display()))?;
    }
    let helper=installed(ctx,"lumen-dock-drop")?;
    let gate=ctx.root.0.join("release-helper");let gate_record=ctx.opt.output.join("busy-child.json");
    if busy {
        let bindir=ctx.root.0.join("bin");fs::create_dir(&bindir)?;
        symlink(bin.join("dock_offer_gate"),bindir.join("lumen-dock-drop"))?;
        ctx.vars.insert("LUMEN_DOCK_GATE_PATH".into(),path(&gate));
        ctx.vars.insert("LUMEN_DOCK_GATE_RECORD".into(),path(&gate_record));
        ctx.vars.insert("LUMEN_DOCK_GATE_HELPER".into(),path(&helper));
        ctx.vars.insert("PATH".into(),format!("{}:{}",bindir.display(),ctx.vars.get("PATH").ok_or("No PATH")?));
        save(&ctx.opt.output.join("busy-helper.json"),&json!({"helper":helper,"sha256":hash(&helper)?}))?;
    }
    instrument(ctx)?;
    let palette=load(&ctx.repo.join("res/lumen-desktop/appearance/palette.json"))?;
    save(&ctx.root.0.join("appearance.json"),&json!({"font":palette["font"],"palette":palette["dark"],"preferences":{"reduceMotion":true,"reduceTransparency":true}}))?;
    let dock=ctx.root.0.join("dock-offer-dock.qml");fs::write(&dock,include_str!("../dock_offer_dock.qml"))?;
    let peer=ctx.root.0.join("dock-offer-peer.qml");fs::write(&peer,include_str!("../dock_offer_peer.qml"))?;
    ctx.launch("quickshell", &["-p",&path(&dock)],"dock.log",false)?;
    wait("Dock drop areas",||Ok(state(ctx,&dock,"dockOffer").ok().filter(|row|row["drops"].as_array().is_some_and(|items|items.len()==2))))?;
    let index=ctx.launch("quickshell", &["-p",&path(&peer)],"peer.log",false)?;
    let pid=ctx.children[index].0.id();
    let node=wait("native offer peer",||Ok(find_node(&ctx.tree()?,|row|row["name"]=="Private URI offer peer" && row["pid"]==pid && row["window_rect"]["width"].as_i64().unwrap_or(0)>0)))?;
    let id=node["id"].clone();
    let uris=vec![uri(&alpha)?,uri(&beta)?];
    let mut checks=Vec::new();
    if busy {
        start_offer(ctx,&peer,&dock,&id,&uris,"accept",true)?;
        ctx.release()?;
        wait("first drop held in helper",||Ok((state(ctx,&peer,"offerPeer")?["finished"]==1 && state(ctx,&dock,"dockOffer")?["busy"]==true && gate_record.exists()).then_some(())))?;
        let first_command=state(ctx,&dock,"dockOffer")?["command"].clone();
        need(state(ctx,&peer,"offerPeer")?["action"]==1 && records(&opened)?.is_empty(),"First busy offer did not remain captured")?;
        let later=docs.join("Later.txt");fs::write(&later,"later unaccepted offer")?;
        start_offer(ctx,&peer,&dock,&id,&[uri(&beta)?,uri(&later)?],"accept",false)?;
        ctx.release()?;
        wait("second busy drop refused",||Ok((state(ctx,&peer,"offerPeer")?["finished"]==2).then_some(())))?;
        let observed=json!({"peer":state(ctx,&peer,"offerPeer")?,"ui":state(ctx,&dock,"dockOffer")?,"gate":load(&gate_record)?,"launches":records(&opened)?});
        save(&ctx.opt.output.join("busy-observed.json"),&observed)?;
        need(observed["peer"]["action"]==0 && observed["ui"]["error"].as_str().is_some_and(|s|!s.is_empty())
            && observed["ui"]["command"]==first_command && records(&opened)?.is_empty(),"Busy native drop was queued or accepted")?;
        ctx.picture("busy-refused.png")?;
        checks.push("Busy native offer refuses second drop without queuing or replacing helper");
        fs::write(&gate,"")?;
        wait("first helper released",||Ok((state(ctx,&dock,"dockOffer")?["busy"]==false && records(&opened)?.len()==1).then_some(())))?;
        let launched=records(&opened)?;
        need(launched[0].as_array().is_some_and(|rows|rows.iter().map(|row|row["uri"].as_str().unwrap_or("")).eq(uris.iter().map(String::as_str))),"Released helper launched wrong offer")?;
        checks.push("Released owned helper dispatches captured original offer once");
    } else {
        start_offer(ctx,&peer,&dock,&id,&uris,"accept",true)?;
        ctx.release()?;
        wait("accepted offer dispatched",||{
            let peer_state=state(ctx,&peer,"offerPeer")?;
            let dock_state=state(ctx,&dock,"dockOffer")?;
            save(&ctx.opt.output.join("last-offer.json"),&json!({"peer":peer_state,"dock":dock_state,"records":records(&opened)?}))?;
            Ok((peer_state["finished"]==1 && dock_state["busy"]==false && (records(&opened)?.len()==1 || dock_state["error"].as_str().is_some_and(|s|!s.is_empty()))).then_some(()))
        })?;
        let row=state(ctx,&dock,"dockOffer")?;
        need(row["error"].as_str().unwrap_or("").is_empty() && records(&opened)?.len()==1,"Accepted offer did not reach observer")?;
        let launched=records(&opened)?;
        need(launched[0].as_array().is_some_and(|items|items.len()==2 && items.iter().zip([&alpha,&beta]).all(|(item,path)|item["sha256"]==hash(path).unwrap_or_default())),"Observer saw wrong offered bytes")?;
        need(state(ctx,&peer,"offerPeer")?["action"]==1,"Accepted offer action was not CopyAction")?;
        checks.push("Separate Wayland URI offer reaches Dock/helper and exact private observer bytes");
        start_offer(ctx,&peer,&dock,&id,&[uri(&alpha)?,uri(&image)?],"accept",true)?;ctx.release()?;
        wait("mixed MIME refusal",||Ok((state(ctx,&peer,"offerPeer")?["finished"]==2 && state(ctx,&dock,"dockOffer")?["error"].as_str().is_some_and(|s|!s.is_empty()) && state(ctx,&dock,"dockOffer")?["busy"]==false).then_some(())))?;
        need(records(&opened)?.len()==1 && state(ctx,&dock,"dockOffer")?["error"].as_str().unwrap_or("").contains("every dropped file type"),"Mixed MIME offer partly launched")?;
        checks.push("Mixed compatible and incompatible MIME types refuse as a unit");
        start_offer(ctx,&peer,&dock,&id,&uris,"refuse",true)?;ctx.release()?;
        wait("incompatible receiver refusal",||Ok((state(ctx,&peer,"offerPeer")?["finished"]==3 && state(ctx,&dock,"dockOffer")?["error"].as_str().is_some_and(|s|!s.is_empty()) && state(ctx,&dock,"dockOffer")?["busy"]==false).then_some(())))?;
        need(records(&opened)?.len()==1,"Incompatible receiver launched observer")?;
        checks.push("Incompatible app does not fall back to another receiver");
        let unavailable=docs.join("Unavailable.txt");
        start_offer(ctx,&peer,&dock,&id,&[uri(&alpha)?,uri(&unavailable)?],"accept",true)?;ctx.release()?;
        wait("unavailable file refusal",||Ok((state(ctx,&peer,"offerPeer")?["finished"]==4 && state(ctx,&dock,"dockOffer")?["error"].as_str().is_some_and(|s|!s.is_empty()) && state(ctx,&dock,"dockOffer")?["busy"]==false).then_some(())))?;
        need(records(&opened)?.len()==1 && state(ctx,&dock,"dockOffer")?["error"].as_str().unwrap_or("").contains("unavailable or unreadable"),"Missing file partially launched")?;
        checks.push("Unavailable local path refused before app launch");
        let prior=state(ctx,&dock,"dockOffer")?;
        let drops=prior["events"].as_array().ok_or("No Dock events")?.iter().filter(|row|row["kind"]=="drop").count();
        start_offer(ctx,&peer,&dock,&id,&uris,"accept",true)?;
        ctx.key(&["-k","Escape"])?;
        wait("Escape canceled native offer",||Ok((state(ctx,&peer,"offerPeer")?["finished"]==5 && state(ctx,&dock,"dockOffer")?["drops"].as_array().is_some_and(|rows|rows.iter().all(|row|row["contains"]==false))).then_some(())))?;
        ctx.release()?;
        let after=state(ctx,&dock,"dockOffer")?;
        need(records(&opened)?.len()==1 && after["busy"]==false && after["command"]==prior["command"]
            && after["events"].as_array().ok_or("No Dock events")?.iter().filter(|row|row["kind"]=="drop").count()==drops,"Canceled offer dispatched helper")?;
        checks.push("Physical Escape cancels native offer without helper dispatch");
    }
    for (file,digest) in originals {need(hash(file)?==digest,"Offered source bytes changed")?;}
    checks.push("Accepted and refused offers preserve original source bytes");
    ctx.picture("dock-offer.png")?;
    for name in ["dock.log","peer.log"] {
        let log=fs::read_to_string(ctx.opt.output.join(name))?;
        need(!["ReferenceError","TypeError","Binding loop","Failed to load"].iter().any(|term|log.contains(term)),format!("QML warning in {name}: {log}"))?;
    }
    save(&ctx.opt.output.join("result.json"),&json!({"passed":true,"checks":checks,"launches":records(&opened)?,
        "final":state(ctx,&dock,"dockOffer")?,"peer":state(ctx,&peer,"offerPeer")?,"scope":"Actual private Wayland URI offers, installed Dock helper and Rust observer"}))?;
    Ok(())
}
