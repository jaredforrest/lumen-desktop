//! Software-rendered production essential dialog text at constrained sizes and scales.
#[path = "../private_runtime.rs"]
mod private_runtime;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::env;
use std::error::Error;
use std::fs::{self, File, OpenOptions};
use std::io::{BufReader, Write};
use std::os::unix::fs::{PermissionsExt, symlink};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

type Result<T> = std::result::Result<T, Box<dyn Error>>;
fn need(ok: bool, message: impl Into<String>) -> Result<()> { if ok { Ok(()) } else { Err(message.into().into()) } }
fn copy_tree(source: &Path, target: &Path) -> Result<()> {
    fs::create_dir(target)?;
    for entry in fs::read_dir(source)? {
        let entry=entry?; let from=entry.path(); let to=target.join(entry.file_name());
        let kind=fs::symlink_metadata(&from)?.file_type();
        if kind.is_dir() { copy_tree(&from,&to)?; }
        else if kind.is_symlink() { symlink(fs::read_link(&from)?,&to)?; }
        else if kind.is_file() { fs::copy(&from,&to)?; let mut perms=fs::metadata(&to)?.permissions();perms.set_mode(perms.mode()|0o200);fs::set_permissions(&to,perms)?; }
    }
    Ok(())
}
fn launcher_env(launcher: &Path) -> Result<BTreeMap<String,String>> {
    let output=Command::new(launcher).arg("--print-env0").output()?;
    need(output.status.success(),"Launcher environment unavailable")?;
    let mut vars=BTreeMap::new();
    for part in output.stdout.split(|byte|*byte==0) {
        if let Some((key,value))=std::str::from_utf8(part)?.split_once('=') { vars.insert(key.into(),value.into()); }
    }
    Ok(vars)
}
fn provider() -> Result<()> {
    let mut input=String::new();std::io::stdin().read_line(&mut input)?;
    let request:Value=serde_json::from_str(&input)?;
    let name=env::args_os().next().and_then(|p|PathBuf::from(p).file_name().map(|n|n.to_owned())).ok_or("Missing provider name")?;
    let response=if request["action"]!="status" {
        if let Ok(path)=env::var("LUMEN_ESSENTIAL_MUTATIONS") {writeln!(OpenOptions::new().create(true).append(true).open(path)?,"{request}")?;}
        json!({"error":"Private fixture refuses mutations"})
    } else if name=="lumen-window-pair" {
        json!({"source":{"id":101,"pid":501,"title":"Private captured document with a long descriptive title",
            "output":"HEADLESS-1","pair":{"id":0,"eligible":true,"survivor":false}},"candidates":[]})
    } else if name=="lumen-display-settings" {
        json!({"outputs":[{"name":"HEADLESS-1","label":"Private display","mode":"1280x720@60",
            "modes":["1280x720@60"],"scale":1,"x":0,"y":0}],"pending":"","seconds":0})
    } else {return Err("Unknown provider".into())};
    println!("{response}");Ok(())
}
fn ink(path:&Path,item:&Value)->Result<usize>{
    let mut reader=png::Decoder::new(BufReader::new(File::open(path)?)).read_info()?;
    let mut bytes=vec![0;reader.output_buffer_size()];let info=reader.next_frame(&mut bytes)?;
    let channels=match info.color_type {png::ColorType::Rgb=>3,png::ColorType::Rgba=>4,_=>return Err("Expected RGB capture".into())};
    let rect=&item["rect"];
    let x=rect["x"].as_f64().ok_or("x")?.round().max(0.) as usize;
    let y=rect["y"].as_f64().ok_or("y")?.round().max(0.) as usize;
    let width=rect["width"].as_f64().ok_or("width")?.round().max(0.) as usize;
    let height=rect["height"].as_f64().ok_or("height")?.round().max(0.) as usize;
    need(width>0 && height>0 && x+width<=info.width as usize && y+height<=info.height as usize,"Paint crop outside capture")?;
    let colour=item["color"].as_str().ok_or("colour")?;let rgb=&colour[colour.len()-6..];
    let target=[u8::from_str_radix(&rgb[0..2],16)?,u8::from_str_radix(&rgb[2..4],16)?,u8::from_str_radix(&rgb[4..6],16)?];
    let mut count=0;
    for row in y..y+height {for col in x..x+width {let start=(row*info.width as usize+col)*channels;
        if bytes[start..start+3].iter().zip(target).all(|(a,b)|a.abs_diff(b)<32) {count+=1;}}}
    need(count>=20 && count<width*height*65/100,format!("Final action label has no bounded painted ink: {count}"))?;
    Ok(count)
}
fn run(launcher:&Path,font_config:&Path,output:&Path)->Result<()> {
    need(!output.exists(),"Output path already exists")?;fs::create_dir_all(output)?;
    let vars=launcher_env(launcher)?;
    let assets=PathBuf::from(vars.get("LUMEN_DESKTOP_ASSETS").ok_or("Packaged assets unavailable")?);
    copy_tree(&assets.join("shell"),&output.join("desktop"))?;
    for name in ["ForceQuitDialog.qml","WindowPairPicker.qml"] {
        let file=output.join("desktop").join(name);
        let before=fs::read_to_string(&file)?;
        let after=before.replacen("PanelWindow {","Window {",1).lines()
            .filter(|line|!line.trim().starts_with("anchors {top:true;bottom:true;left:true;right:true}")
                && !line.trim().starts_with("exclusionMode:") && !line.trim().starts_with("WlrLayershell."))
            .collect::<Vec<_>>().join("\n");
        fs::write(file,format!("import QtQuick.Window\n{after}\n"))?;
    }
    let palette:Value=serde_json::from_slice(&fs::read(assets.join("appearance/palette.json"))?)?;
    let mut cases=Vec::new();
    for consumer in ["Force","Pair","Displays"] {for mode in ["light","dark"] {for scale in [1.0,1.15,1.3,1.5] {
        for (width,height) in if consumer=="Displays" {[(340,480),(700,800)]} else {[(700,480),(1100,800)]} {
            cases.push(json!({"consumer":consumer,"mode":mode,"scale":scale,"width":width,"height":height}));
        }
    }}}
    let fixture=include_str!("../../essential_dialog_offscreen.qml")
        .replace("@CASES@",&serde_json::to_string(&cases)?)
        .replace("@PALETTES@",&palette.to_string())
        .replace("@OUTPUT@",&Value::String(output.display().to_string()).to_string());
    let qml=output.join("fixture.qml");fs::write(&qml,fixture)?;
    let home=output.join("home");fs::create_dir(&home)?;
    let private_runtime=private_runtime::PrivateRuntime::new()?;let runtime=private_runtime.path();
    let bin=output.join("bin");fs::create_dir(&bin)?;
    let exe=env::current_exe()?;for name in ["lumen-window-pair","lumen-display-settings"]{symlink(&exe,bin.join(name))?;}
    let mut environment=env::vars().collect::<BTreeMap<_,_>>();environment.extend(vars);
    for key in ["WAYLAND_DISPLAY","DISPLAY","SWAYSOCK","AT_SPI_BUS_ADDRESS"]{environment.remove(key);}
    environment.insert("PATH".into(),format!("{}:{}",bin.display(),environment.get("PATH").cloned().unwrap_or_default()));
    environment.insert("LUMEN_ESSENTIAL_MUTATIONS".into(),output.join("mutations.jsonl").display().to_string());
    for (key,value) in [("HOME",home.display().to_string()),("XDG_RUNTIME_DIR",runtime.display().to_string()),
        ("FONTCONFIG_FILE",font_config.display().to_string()),("QT_QPA_PLATFORM","offscreen".into()),
        ("QT_QPA_PLATFORMTHEME","generic".into()),("QT_QUICK_BACKEND","software".into()),
        ("QT_ACCESSIBILITY","0".into()),("NO_AT_BRIDGE","1".into()),("GTK_A11Y","none".into())]{environment.insert(key.into(),value);}
    for name in ["CONFIG","CACHE","STATE","DATA"]{let dir=output.join(name.to_lowercase());fs::create_dir(&dir)?;environment.insert(format!("XDG_{name}_HOME"),dir.display().to_string());}
    let state=output.join("state/lumen-desktop");fs::create_dir_all(&state)?;
    fs::write(state.join("appearance.json"),serde_json::to_vec(&json!({"font":palette["font"],"palette":palette["light"],"preferences":{"reduceMotion":true}}))?)?;
    let log=output.join("qt.log");let file=File::create(&log)?;
    let mut child=Command::new("dbus-run-session").args(["--","quickshell","-p",qml.to_str().ok_or("fixture path")?])
        .env_clear().envs(&environment).process_group(0).stdout(Stdio::from(file.try_clone()?)).stderr(Stdio::from(file)).spawn()?;
    let end=Instant::now()+Duration::from_secs(45);
    let status=loop {if let Some(status)=child.try_wait()?{break status;}if Instant::now()>end{unsafe{libc::kill(-(child.id() as i32),libc::SIGKILL);}child.wait()?;return Err("Essential dialog QML timed out".into());}thread::sleep(Duration::from_millis(30));};
    let text=fs::read_to_string(&log)?;
    let runtime_ipc_bytes=private_runtime.verify_log(&text)?;
    need(status.success() && !text.contains("ESSENTIAL_DIALOG_FAILURE"),format!("Essential dialog QML failed: {text}"))?;
    for marker in ["WARN","ReferenceError:","TypeError:","Binding loop","Failed to start IPC"]{need(!text.contains(marker),format!("Essential dialog QML warning: {marker}"))?;}
    let line=text.lines().find_map(|line|line.split_once("ESSENTIAL_DIALOG_RESULT ").map(|(_,value)|value)).ok_or("Missing QML result")?;
    let mut records:Vec<Value>=serde_json::from_str(line)?;need(records.len()==48,"Expected 48 dialog cases")?;
    need(!output.join("mutations.jsonl").exists(),"A private provider mutation occurred")?;
    for row in &mut records {let capture=PathBuf::from(row["capture"].as_str().ok_or("Capture")?);let pixels=ink(&capture,&row["painted"])?;row["painted"]["ink"]=json!(pixels);}
    fs::write(output.join("result.json"),serde_json::to_vec_pretty(&json!({"passed":true,"cases":records,"runtimeIpcBytes":runtime_ipc_bytes,"scope":"Production dialog QML painted action labels, text roles and focus reveal; transport adapted"}))?)?;
    Ok(())
}
fn main()->Result<()> {
    let name=env::args_os().next().and_then(|p|PathBuf::from(p).file_name().map(|n|n.to_owned())).ok_or("Name")?;
    if name=="lumen-window-pair" || name=="lumen-display-settings" {return provider();}
    let mut launcher=None;let mut font_config=None;let mut output=None;let mut args=env::args_os().skip(1);
    while let Some(arg)=args.next(){match arg.to_str(){
        Some("--launcher")=>launcher=Some(PathBuf::from(args.next().ok_or("--launcher value")?)),
        Some("--font-config")=>font_config=Some(PathBuf::from(args.next().ok_or("--font-config value")?)),
        Some("--output")=>output=Some(PathBuf::from(args.next().ok_or("--output value")?)),
        _=>return Err("Unknown essential dialog option".into()),
    }}
    run(&launcher.ok_or("--launcher required")?,&font_config.ok_or("--font-config required")?,&output.ok_or("--output required")?)
}
