//! Software rendered production MenuCard text at every supported text scale.
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::env;
use std::error::Error;
use std::fs::{self, File};
use std::io::BufReader;
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
fn ink(path: &Path, item: &Value) -> Result<usize> {
    let decoder=png::Decoder::new(BufReader::new(File::open(path)?));
    let mut reader=decoder.read_info()?;
    let mut bytes=vec![0;reader.output_buffer_size()];
    let info=reader.next_frame(&mut bytes)?;
    let channels=match info.color_type { png::ColorType::Rgb=>3,png::ColorType::Rgba=>4,_=>return Err("Expected RGB menu capture".into()) };
    let rect=&item["rect"];
    let x=rect["x"].as_f64().ok_or("text x")?.round().max(0.) as usize;
    let y=rect["y"].as_f64().ok_or("text y")?.round().max(0.) as usize;
    let width=rect["width"].as_f64().ok_or("text width")?.round().max(0.) as usize;
    let height=rect["height"].as_f64().ok_or("text height")?.round().max(0.) as usize;
    need(width>0 && height>0 && x+width<=info.width as usize && y+height<=info.height as usize,"Text crop outside menu capture")?;
    let colour=item["color"].as_str().ok_or("text colour")?;
    let rgb=&colour[colour.len()-6..];
    let target=[u8::from_str_radix(&rgb[0..2],16)?,u8::from_str_radix(&rgb[2..4],16)?,u8::from_str_radix(&rgb[4..6],16)?];
    let mut count=0;
    for row in y..y+height { for col in x..x+width {
        let start=(row*info.width as usize+col)*channels;
        let pixel=&bytes[start..start+3];
        if pixel.iter().zip(target).all(|(a,b)|a.abs_diff(b)<30) {count+=1;}
    }}
    need(count>=4 && count<width*height*7/10,"Menu label has no bounded visible ink")?;
    Ok(count)
}
fn run(launcher: &Path, font_config: &Path, output: &Path) -> Result<()> {
    need(!output.exists(),"Output path already exists")?;
    fs::create_dir_all(output)?;
    let vars=launcher_env(launcher)?;
    let assets=PathBuf::from(vars.get("LUMEN_DESKTOP_ASSETS").ok_or("Packaged assets unavailable")?);
    copy_tree(&assets.join("shell"),&output.join("desktop"))?;
    let palette:Value=serde_json::from_slice(&fs::read(assets.join("appearance/palette.json"))?)?;
    let cases=(0..2).flat_map(|mode| ["light","dark"].get(mode).copied()).flat_map(|mode| [380,700].into_iter().flat_map(move |width| [1.,1.15,1.3,1.5].into_iter().map(move |scale|json!({"mode":mode,"width":width,"scale":scale})))).collect::<Vec<_>>();
    let fixture=include_str!("../../menu-typography.qml")
        .replace("@CASES@",&serde_json::to_string(&cases)?)
        .replace("@PALETTES@",&palette.to_string())
        .replace("@OUTPUT@",&Value::String(output.display().to_string()).to_string());
    let qml=output.join("fixture.qml"); fs::write(&qml,fixture)?;
    let home=output.join("home");fs::create_dir(&home)?;
    let runtime=output.join("runtime");fs::create_dir(&runtime)?;fs::set_permissions(&runtime,fs::Permissions::from_mode(0o700))?;
    let mut environment=env::vars().collect::<BTreeMap<_,_>>();environment.extend(vars);
    for key in ["WAYLAND_DISPLAY","DISPLAY","SWAYSOCK","AT_SPI_BUS_ADDRESS"] {environment.remove(key);}
    for (key,value) in [("HOME",home.display().to_string()),("XDG_RUNTIME_DIR",runtime.display().to_string()),("FONTCONFIG_FILE",font_config.display().to_string()),("QT_QPA_PLATFORM","offscreen".into()),("QT_QPA_PLATFORMTHEME","generic".into()),("QT_QUICK_BACKEND","software".into()),("QT_ACCESSIBILITY","0".into()),("QT_LINUX_ACCESSIBILITY_ALWAYS_ON","0".into()),("NO_AT_BRIDGE","1".into()),("GTK_A11Y","none".into())] {environment.insert(key.into(),value);}
    for name in ["CONFIG","CACHE","STATE","DATA"] {let dir=output.join(name.to_lowercase());fs::create_dir(&dir)?;environment.insert(format!("XDG_{name}_HOME"),dir.display().to_string());}
    let state=output.join("state/lumen-desktop");fs::create_dir_all(&state)?;
    fs::write(state.join("appearance.json"),serde_json::to_vec(&json!({"font":palette["font"],"palette":palette["light"],"preferences":{"reduceMotion":true}}))?)?;
    let log=output.join("qt.log");let file=File::create(&log)?;
    let mut child=Command::new("dbus-run-session").args(["--","quickshell","-p",qml.to_str().ok_or("fixture path")?]).env_clear().envs(&environment).process_group(0).stdout(Stdio::from(file.try_clone()?)).stderr(Stdio::from(file)).spawn()?;
    let end=Instant::now()+Duration::from_secs(35);
    let status=loop {if let Some(status)=child.try_wait()? {break status;} if Instant::now()>end {unsafe {libc::kill(-(child.id() as i32),libc::SIGKILL);} child.wait()?;return Err("Menu typography QML timed out".into());}thread::sleep(Duration::from_millis(30));};
    let text=fs::read_to_string(&log)?;
    need(status.success() && !text.contains("MENU_TEXT_FAILURE"),format!("Menu QML failed: {text}"))?;
    for marker in ["WARN","ReferenceError:","TypeError:","Binding loop"] {need(!text.contains(marker),format!("Menu QML warning: {marker}"))?;}
    let line=text.lines().find_map(|line|line.split_once("MENU_TEXT_RESULT ").map(|(_,value)|value)).ok_or("Missing menu text result")?;
    let mut records:Vec<Value>=serde_json::from_str(line)?;need(records.len()==16,"Expected 16 menu cases")?;
    for row in &mut records {let capture=PathBuf::from(row["capture"].as_str().ok_or("capture")?);for name in ["label","shortcut"] {let pixels=ink(&capture,&row[name])?;row[name]["ink"]=json!(pixels);}}
    fs::write(output.join("result.json"),serde_json::to_vec_pretty(&json!({"passed":true,"cases":records}))?)?;
    Ok(())
}
fn main() -> Result<()> {
    let mut launcher=None;let mut font_config=None;let mut output=None;let mut args=env::args_os().skip(1);
    while let Some(arg)=args.next() {match arg.to_str() {
        Some("--launcher")=>launcher=Some(PathBuf::from(args.next().ok_or("--launcher value")?)),
        Some("--font-config")=>font_config=Some(PathBuf::from(args.next().ok_or("--font-config value")?)),
        Some("--output")=>output=Some(PathBuf::from(args.next().ok_or("--output value")?)),
        _=>return Err("Unknown menu typography option".into()),
    }}
    run(&launcher.ok_or("--launcher required")?,&font_config.ok_or("--font-config required")?,&output.ok_or("--output required")?)
}
