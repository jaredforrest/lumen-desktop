//! Normal and reduced motion using production components and private providers.
use super::*;
use crate::physical_keyboard::Keyboard;
use zbus::blocking::{Connection, Proxy};
use std::collections::HashMap;

fn state(ctx:&Context)->Result<Value>{let row:Value=serde_json::from_str(&ctx.ipc("state",&[])?)?;save(&ctx.opt.output.join("last-ui.json"),&row)?;Ok(row)}
fn ready(ctx:&Context,label:&str,predicate:impl Fn(&Value)->bool)->Result<Value>{wait(label,||{let row=state(ctx)?;Ok(predicate(&row).then_some(row))})}
fn n(row:&Value)->f64{row.as_f64().unwrap_or(0.)}
fn settled(row:&Value)->bool{row["dock"]["icons"].as_array().is_some_and(|icons|!icons.is_empty()&&icons.iter().all(|i|(n(&i["width"])-n(&i["target"])).abs()<0.02))}
fn trace(ctx:&Context)->Result<Value>{Ok(serde_json::from_str(&ctx.ipc("endTrace",&[])?)?)}
fn help(ctx:&Context,pid:u32)->Result<Option<Value>>{Ok(find_node(&ctx.tree()?,|r|r["pid"]==pid && r["name"]=="Desktop Help"))}
fn workspace(ctx:&Context)->Result<String>{let rows=ctx.sway(&["-t","get_workspaces"])?;Ok(rows.as_array().ok_or("Workspaces")?.iter().find(|r|r["focused"]==true).and_then(|r|r["name"].as_str()).unwrap_or("").into())}
fn box_at(row:&Value)->(i64,i64,i64,i64){let x=n(&row["x"]).round() as i64;let y=n(&row["y"]).round() as i64;(x,y,x+n(&row["width"]).round() as i64,y+n(&row["height"]).round() as i64)}
fn crop(ctx:&Context,file:&str,rect:(i64,i64,i64,i64))->Result<Vec<u8>>{
 let (x,y,r,b)=rect;let width=r-x;let height=b-y;need(width>0&&height>0,format!("Empty crop {rect:?}"))?;
 let mut decoder=png::Decoder::new(File::open(ctx.opt.output.join(file))?);
 decoder.set_transformations(png::Transformations::EXPAND|png::Transformations::STRIP_16);
 let mut reader=decoder.read_info()?;let mut buffer=vec![0;reader.output_buffer_size()];let info=reader.next_frame(&mut buffer)?;
 need(x>=0&&y>=0&&r<=info.width as i64&&b<=info.height as i64,"Crop outside screenshot")?;
 let channels=match info.color_type {png::ColorType::Rgb=>3,png::ColorType::Rgba=>4,_=>return Err(fail("Unsupported screenshot color type"))};
 let mut output=Vec::with_capacity((width*height*3) as usize);
 for row in y..b {for col in x..r {let offset=(row as usize*info.width as usize+col as usize)*channels;output.extend_from_slice(&buffer[offset..offset+3]);}}
 Ok(output)
}
fn hud_pixels(ctx:&Context,before:&str,after:&str,row:&Value)->Result<()>{let width=n(&row["width"]);let height=n(&row["height"]);let bottom=n(&row["bottom"]);let rect=(((1100.-width)/2.).round() as i64,(800.-bottom-height).round() as i64,((1100.+width)/2.).round() as i64,(800.-bottom).round() as i64);need(crop(ctx,before,rect)?!=crop(ctx,after,rect)?,"Actual HUD must paint")}
fn rgb(text:&str)->Result<[i32;3]>{let clean=text.trim_start_matches('#');need(clean.len()>=6,format!("Invalid color {text}"))?;let clean=if clean.len()==8 {&clean[2..]} else {clean};Ok([i32::from_str_radix(&clean[0..2],16)?,i32::from_str_radix(&clean[2..4],16)?,i32::from_str_radix(&clean[4..6],16)?])}
fn banner_painted(ctx:&Context,arrival:&Value,name:&str)->Result<bool>{
 ctx.picture(name)?;let banner=&arrival["notifications"];let rect=&banner["summary"]["rect"];
 let origin_x=1100.-n(&banner["width"])-n(&banner["right"]);let origin_y=n(&banner["top"]);
 let (x,y,r,b)=box_at(rect);let bytes=crop(ctx,name,((origin_x+x as f64).round() as i64,(origin_y+y as f64).round() as i64,(origin_x+r as f64).round() as i64,(origin_y+b as f64).round() as i64))?;
 let color=rgb(banner["summary"]["color"].as_str().ok_or("Summary color")?)?;
 Ok(bytes.chunks_exact(3).filter(|pixel|pixel.iter().enumerate().all(|(i,v)|(*v as i32-color[i]).abs()<=2)).count()>=4)
}
fn notify(connection:&Connection)->Result<u32>{let proxy=Proxy::new(connection,"org.freedesktop.Notifications","/org/freedesktop/Notifications","org.freedesktop.Notifications")?;let hints:HashMap<&str,zbus::zvariant::Value<'_>>=HashMap::new();Ok(proxy.call("Notify",&("Private motion producer",0u32,"","Motion receipt","Private synthetic body",Vec::<&str>::new(),hints,5000i32))?)}
fn snapshot(connection:&Connection)->Result<Value>{let proxy=Proxy::new(connection,"org.freedesktop.Notifications","/org/freedesktop/Notifications","org.lumendesktop.Notifications")?;let raw:String=proxy.call("Snapshot",&())?;Ok(serde_json::from_str(&raw)?)}
fn preference(ctx:&Context,palette:&Value,reduced:bool)->Result<()>{save(&ctx.root.0.join("appearance.json"),&json!({"font":palette["font"],"palette":palette["light"],"preferences":{"reduceMotion":reduced,"reduceTransparency":true,"dockAutoHide":true,"dockMagnification":true,"dockSize":48}}))}
fn clean_log(ctx:&Context,file:&str)->Result<()>{let log=fs::read_to_string(ctx.opt.output.join(file))?;need(!["ReferenceError","TypeError","binding loop","Failed to load"].iter().any(|m|log.contains(m)),format!("QML error in {file}: {log}"))}
pub fn run(ctx:&mut Context)->Result<()> {
 need(ctx.repo==Path::new("/source"),"Reduced motion requires private source mount")?;
 need(env::var("HOME").ok().as_deref()==Some("/home/fixture"),"Reduced motion requires private HOME")?;
 for dev in ["dri","snd","video0"]{need(!Path::new("/dev").join(dev).exists(),format!("Host device visible: {dev}"))?;}
 ctx.vars.insert("LUMEN_UNIFIED_OWNED_HEADERS".into(),"1".into());for key in ["LUMEN_NATIVE_WINDOW_CONTROLS","LUMEN_NATIVE_WINDOW_ACTIONS","LUMEN_LAUNCH_SCOPES","LUMEN_LOCK_PREVIEWS"]{ctx.vars.insert(key.into(),"0".into());}
 let helper=ctx.vars.get("LUMEN_NOTIFICATION_HELPER").cloned().ok_or("Launcher must provide Rust notification helper")?;need(Path::new(&helper).is_file(),"Rust notification helper unavailable")?;ctx.vars.insert("LUMEN_NOTIFICATION_STATE".into(),path(&ctx.root.0.join("notification-history")));
 save(&ctx.root.0.join("notification-policy.json"),&json!({"notifications":{"preview":"when_unlocked","policies":{}}}))?;ctx.vars.insert("LUMEN_NIX_SETTINGS_ACTIVE".into(),path(&ctx.root.0.join("notification-policy.json")));
 let palette=load(&ctx.repo.join("res/lumen-desktop/appearance/palette.json"))?;preference(ctx,&palette,false)?;
 ctx.launch(&helper,&["serve"],"notifications.log",false)?;
 let connection=zbus::blocking::connection::Builder::address(ctx.vars.get("DBUS_SESSION_BUS_ADDRESS").ok_or("Private bus address")?.as_str())?.build()?;wait("private unlocked observer",||Ok((snapshot(&connection)?["lock"]==json!({"known":true,"locked":false})).then_some(())))?;
 let mut keyboard=Keyboard::start(ctx,"keyboard.log")?;ctx.pointer_start()?;let mut records=Vec::new();
 for reduced in [false,true] {
  let label=if reduced{"reduced"}else{"normal"};preference(ctx,&palette,reduced)?;
  if ctx.opt.effects{ctx.checked(&format!("animation_duration_ms {}",if reduced{0}else{160}))?;}
  let qml=path(&ctx.qml);let shell=ctx.launch("quickshell", &["--no-duplicate","-p",&qml],&format!("{label}-shell.log"),false)?;let pid=ctx.children[shell].0.id();
  ready(ctx,"motion preference and notification transport",|r|r["reduced"]==reduced&&r["notifications"]["available"]==true)?;
  ready(ctx,"initial Dock auto-hide",|r|r["dock"]["revealed"]==false&&r["dock"]["surfaceOpacity"]==0)?;
  let base:Vec<f64>=state(ctx)?["dock"]["icons"].as_array().ok_or("Dock icons")?.iter().map(|i|n(&i["width"])).collect();
  ctx.ipc("beginTrace",&[])?;ctx.ipc("focusDock",&[])?;
  let row=ready(ctx,"Dock reveal and magnification",|r|r["dock"]["active"]==true&&r["dock"]["keyboard"]==true&&r["dock"]["surfaceOpacity"]==1&&settled(r))?;
  let dock_trace=trace(ctx)?;need(dock_trace["overflow"]==false,"Dock trace overflow")?;
  let targets:Vec<f64>=row["dock"]["icons"].as_array().ok_or("Dock targets")?.iter().map(|i|n(&i["target"])).collect();
  let intermediate=dock_trace["samples"].as_array().ok_or("Dock samples")?.iter().any(|s|s["widths"].as_array().is_some_and(|widths|widths.iter().zip(&base).zip(&targets).any(|((w,b),t)|n(w)>b.min(*t)+0.01&&n(w)<b.max(*t)-0.01)));
  need(intermediate!=reduced,format!("Dock motion trajectory mismatch {reduced}: {dock_trace}"))?;
  keyboard.key("End")?;ready(ctx,"physical Dock End",|r|r["dock"]["index"]==1&&settled(r))?;keyboard.key("Escape")?;ready(ctx,"physical Dock cancel",|r|r["dock"]["keyboard"]==false&&settled(r))?;need(state(ctx)?["actions"]==json!([]),"Dock cancel activated action")?;
  ctx.ipc("focusDock",&[])?;ready(ctx,"Dock reopen",|r|r["dock"]["active"]==true&&r["dock"]["keyboard"]==true)?;keyboard.key("Home")?;keyboard.key("Return")?;
  ready(ctx,"physical Help activation",|r|r["help"]["active"]==true&&r["help"]["search"]==true)?;need(help(ctx,pid)?.is_some()&&state(ctx)?["actions"]==json!(["help"]),"Help native window/action missing")?;
  keyboard.text("Dock")?;ready(ctx,"physical Help query",|r|r["help"]["query"]=="Dock")?;keyboard.key("Escape")?;ready(ctx,"physical Help close",|r|r["help"]["visible"]==false)?;need(help(ctx,pid)?.is_none(),"Help native window remained")?;
  ctx.ipc("focusDock",&[])?;ready(ctx,"third Dock open",|r|r["dock"]["active"]==true&&r["dock"]["keyboard"]==true)?;keyboard.key("Home")?;keyboard.key("Return")?;ready(ctx,"Help reopen",|r|r["help"]["active"]==true)?;need(state(ctx)?["help"]["query"]=="Dock","Help query not retained")?;
  let node=help(ctx,pid)?.ok_or("Missing Help native node")?;let id=number(&node["id"])?;ctx.checked(&format!("[con_id={id}] lumen_window minimize"))?;wait("native minimize",||Ok((ctx.find(&json!(id))?.is_some_and(|r|r["lumen_state"]["minimised"]==true)).then_some(())))?;
  ctx.checked(&format!("[con_id={id}] scratchpad show"))?;ready(ctx,"native restore",|r|r["help"]["active"]==true)?;need(help(ctx,pid)?.is_some_and(|r|r["lumen_state"]["minimised"]!=true),"Native restore stayed minimized")?;
  ctx.checked(&format!("[con_id={id}] move container to workspace number 2"))?;keyboard.logo_number(2)?;wait("physical workspace 2",||Ok((workspace(ctx)?=="2").then_some(())))?;keyboard.logo_number(1)?;wait("physical workspace 1",||Ok((workspace(ctx)?=="1").then_some(())))?;ctx.checked(&format!("[con_id={id}] move container to workspace number 1, focus"))?;ready(ctx,"Help focus after workspace",|r|r["help"]["active"]==true)?;
  for action in ["suspend","reboot","poweroff","logout"]{for key in ["Return","Escape"]{ctx.ipc("showSession",&[action])?;ready(ctx,"default session Cancel",|r|r["session"]["active"]==true&&r["session"]["focus"]=="Cancel")?;keyboard.key(key)?;ready(ctx,"physical session Cancel",|r|r["session"]["visible"]==false)?;need(state(ctx)?["session"]["confirmations"]==0,format!("{action} {key} confirmed unexpectedly"))?;}}
  ctx.ipc("showSession",&["suspend"])?;ready(ctx,"Sleep cancellation default",|r|r["session"]["focus"]=="Cancel")?;keyboard.key("Tab")?;ready(ctx,"deliberate Sleep focus",|r|r["session"]["focus"]=="Sleep")?;keyboard.key("Return")?;ready(ctx,"Sleep confirmed",|r|r["session"]["visible"]==false&&r["session"]["confirmations"]==1)?;
  ctx.checked(&format!("[con_id={id}] focus"))?;ready(ctx,"Help focus after cancellation",|r|r["help"]["active"]==true)?;
  let before=format!("{label}-hud-before.png");let shown=format!("{label}-hud-shown.png");ctx.picture(&before)?;ctx.ipc("showHud",&["true"])?;let hud=ready(ctx,"HUD visible",|r|r["hud"]["visible"]==true&&r["hud"]["opacity"]==1)?;ctx.picture(&shown)?;hud_pixels(ctx,&before,&shown,&hud["hud"])?;ctx.ipc("showHud",&["false"])?;ready(ctx,"HUD hidden",|r|r["hud"]["visible"]==false)?;
  ctx.ipc("beginTrace",&[])?;let notification_id=notify(&connection)?;let arrival=ready(ctx,"provider banner",|r|r["notifications"]["banners"]==1&&r["notifications"]["opacity"]==1)?;let banner_trace=trace(ctx)?;need(banner_trace["overflow"]==false,"Banner trace overflow")?;
  let banner_intermediate=banner_trace["samples"].as_array().ok_or("Banner samples")?.iter().any(|s|s["banner"].as_f64().is_some_and(|v|v>0.001&&v<0.999));need(banner_intermediate!=reduced,format!("Banner motion mismatch {reduced}: {banner_trace}"))?;
  need(!arrival["notifications"]["summary"].is_null(),"Summary text absent")?;wait("summary pixels",||Ok(banner_painted(ctx,&arrival,&format!("{label}-banner.png"))?.then_some(())))?;
  let notification=&arrival["notifications"];let dismiss=&notification["dismiss"]["rect"];let x=1100.-n(&notification["width"])-n(&notification["right"])+n(&dismiss["x"])+n(&dismiss["width"])/2.;let y=n(&notification["top"])+n(&dismiss["y"])+n(&dismiss["height"])/2.;ctx.move_pointer((x,y))?;ready(ctx,"dismiss hover",|r|r["notifications"]["dismiss"]["visible"]==true)?;ctx.press()?;ctx.release()?;ready(ctx,"physical provider dismiss",|r|r["notifications"]["visible"]==false&&r["notifications"]["rows"].as_array().is_some_and(|rows|rows.iter().all(|v|v["id"]!=notification_id)))?;
  let settled=ready(ctx,"final Dock settlement",|r|settled(r)&&r["dock"]["keyboard"]==false)?;let node=help(ctx,pid)?.ok_or("Help missing at final state")?;let normalized=json!({"helpGeometry":node["rect"],"helpActive":settled["help"]["active"],"workspace":workspace(ctx)?,"helpVisible":settled["help"]["visible"],"query":settled["help"]["query"],"actions":settled["actions"],"confirmations":settled["session"]["confirmations"],"notifications":settled["notifications"]["rows"],"hudVisible":settled["hud"]["visible"],"dockKeyboard":settled["dock"]["keyboard"]});
  records.push(json!({"reduced":reduced,"normalized":normalized,"dockTrace":dock_trace,"bannerTrace":banner_trace,"checks":15}));
  ctx.children[shell].stop(libc::SIGTERM)?;let restart=ctx.launch("quickshell", &["--no-duplicate","-p",&qml],&format!("{label}-restart.log"),false)?;ready(ctx,"shell reconnect",|r|r["reduced"]==reduced&&r["notifications"]["available"]==true)?;need(state(ctx)?["help"]["visible"]==false&&state(ctx)?["notifications"]["rows"]==json!([]),"Restart retained ephemeral UI")?;ctx.ipc("focusDock",&[])?;ready(ctx,"restart Dock focus",|r|r["dock"]["active"]==true&&r["dock"]["keyboard"]==true)?;keyboard.key("Home")?;keyboard.key("Return")?;ready(ctx,"restart Help activation",|r|r["help"]["active"]==true&&r["help"]["search"]==true)?;keyboard.text("Dock")?;ready(ctx,"restart input",|r|r["help"]["query"]=="Dock")?;keyboard.key("Escape")?;ready(ctx,"restart Help close",|r|r["help"]["visible"]==false)?;ctx.children[restart].stop(libc::SIGTERM)?;
  clean_log(ctx,&format!("{label}-shell.log"))?;clean_log(ctx,&format!("{label}-restart.log"))?;
 }
 need(records[0]["normalized"]==records[1]["normalized"],format!("Final state differs: {records:?}"))?;
 save(&ctx.opt.output.join("result.json"),&json!({"passed":true,"cases":records,"capabilities":{"normalReducedFinalStateParity":true,"physicalDockNavigationCancelActivate":true,"capturedNativeMinimizeRestore":true,"physicalWorkspaceSwitch":true,"physicalDefaultSessionCancel":true,"realNotificationProviderAndPhysicalDismiss":true,"hudPresented":true,"restartInput":true,"menuBarFade":false,"screenSaverMotion":false,"allNavigationScenarios":false},"scope":"Isolated engineering lifecycle differential; no reference timings or live desktop changes"}))?;
 println!("PASS reduced motion lifecycle");Ok(())
}
