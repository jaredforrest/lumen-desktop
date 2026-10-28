//! Screenshot, recording and Sway shortcut controls.
use serde_json::Value;
use std::env;
use std::fs::{self,File};
use std::io::{Read,Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixStream;
use std::path::{Path,PathBuf};
use std::process::{Command,Stdio};
use std::time::Duration;

type Result<T> = std::result::Result<T,String>;
const UNIT:&str="lumen-screen-record";
fn run(program:&str,args:&[&str])->Result<()> {
    let status=Command::new(program).args(args).status().map_err(|e|e.to_string())?;
    if status.success(){Ok(())}else{Err(format!("{program} rejected the request"))}
}
fn output(program:&str,args:&[&str])->Result<String> {
    let result=Command::new(program).args(args).output().map_err(|e|e.to_string())?;
    if !result.status.success(){return Err(format!("{program} rejected the request"));}
    Ok(String::from_utf8_lossy(&result.stdout).trim().to_owned())
}
fn menu(title:&str,choices:&[&str])->Result<Option<String>> {
    let mut child=Command::new("rofi").args(["-dmenu","-i","-no-custom","-p",title])
        .stdin(Stdio::piped()).stdout(Stdio::piped()).spawn().map_err(|e|e.to_string())?;
    child.stdin.take().ok_or("Rofi input unavailable")?.write_all(choices.join("\n").as_bytes()).map_err(|e|e.to_string())?;
    let answer=child.wait_with_output().map_err(|e|e.to_string())?;
    let success=answer.status.success();
    let answer=String::from_utf8_lossy(&answer.stdout).trim().to_owned();
    Ok((success && choices.contains(&answer.as_str())).then_some(answer))
}
fn notify(title:&str,body:&str,image:Option<&Path>,persistent:bool,replace:Option<&str>)->Option<String> {
    let mut command=Command::new("notify-send");command.args(["-a","Desktop conveniences"]);
    if let Some(image)=image{command.arg("-i").arg(image);}
    if persistent{command.args(["-t","0"]);}
    if let Some(id)=replace{command.args(["-r",id]);}
    command.arg("-p").arg(title).arg(body).output().ok()
        .map(|result|String::from_utf8_lossy(&result.stdout).trim().to_owned())
}
fn focused(node:&Value)->Option<&Value> {
    if node["focused"]==true{return Some(node);}
    for child in node["nodes"].as_array().into_iter().flatten().chain(node["floating_nodes"].as_array().into_iter().flatten()) {
        if let Some(found)=focused(child){return Some(found);}
    }
    None
}
fn tree()->Result<Value> {serde_json::from_str(&output("swaymsg",&["-t","get_tree"])?).map_err(|e|e.to_string())}
fn geometry(rect:&Value)->Result<String> {
    let (x,y,width,height)=(rect["x"].as_i64().ok_or("Invalid window geometry")?,rect["y"].as_i64().ok_or("Invalid window geometry")?,
        rect["width"].as_i64().ok_or("Invalid window geometry")?,rect["height"].as_i64().ok_or("Invalid window geometry")?);
    if width<=0 || height<=0{return Err("The selected window has no capture area.".into());}
    Ok(format!("{x},{y} {width}x{height}"))
}
fn display()->Result<Option<String>> {
    let outputs:Value=serde_json::from_str(&output("swaymsg",&["-t","get_outputs"])?).map_err(|e|e.to_string())?;
    let names=outputs.as_array().ok_or("Invalid display list")?.iter().filter(|item|item["active"]==true)
        .filter_map(|item|item["name"].as_str()).collect::<Vec<_>>();
    if names.is_empty(){return Err("No active display found.".into());}
    if names.len()==1{Ok(Some(names[0].into()))}else{menu("Display",&names)}
}
fn region()->Result<Option<String>> {
    let result=Command::new("slurp").output().map_err(|e|e.to_string())?;
    let value=String::from_utf8_lossy(&result.stdout).trim().to_owned();
    Ok((result.status.success() && !value.is_empty()).then_some(value))
}
fn unique_file(folder:&Path,suffix:&str)->Result<PathBuf> {
    fs::create_dir_all(folder).map_err(|e|e.to_string())?;
    let prefix=chrono::Local::now().format("%Y-%m-%d_%H-%M-%S-").to_string();
    let file=tempfile::Builder::new().prefix(&prefix).suffix(suffix).tempfile_in(folder).map_err(|e|e.to_string())?;
    file.into_temp_path().keep().map_err(|e|e.to_string())
}
fn private_copy(source:&Path,destination:&Path)->Result<()> {
    let mut input=File::open(source).map_err(|e|e.to_string())?;
    let mut output=File::options().write(true).truncate(true).open(destination).map_err(|e|e.to_string())?;
    std::io::copy(&mut input,&mut output).map_err(|e|e.to_string())?;
    fs::set_permissions(destination,fs::Permissions::from_mode(0o600)).map_err(|e|e.to_string())?;
    Ok(())
}
fn prune(folder:&Path,maximum:usize) {
    let Ok(entries)=fs::read_dir(folder) else{return;};
    let mut files=entries.filter_map(|entry|entry.ok().map(|entry|entry.path()))
        .filter(|path|path.extension().is_some_and(|ext|ext=="png"))
        .filter_map(|path|fs::metadata(&path).ok().and_then(|meta|meta.modified().ok()).map(|time|(time,path)))
        .collect::<Vec<_>>();
    files.sort_by(|a,b|b.0.cmp(&a.0));
    for (_,old) in files.into_iter().skip(maximum){let _=fs::remove_file(old);}
}
#[derive(Default)] struct Options {
    mode:Option<String>,action:Option<String>,delay:Option<u32>,geometry:Option<String>,
    output:Option<String>,directory:Option<PathBuf>,audio:Option<String>,stop:bool,
}
fn parse(args:&[String])->Result<Options> {
    let mut options=Options::default();let mut iter=args.iter();
    while let Some(option)=iter.next() {
        let mut value=||iter.next().cloned().ok_or_else(||format!("Missing value for {option}"));
        match option.as_str() {
            "--mode"=>options.mode=Some(value()?),"--action"=>options.action=Some(value()?),
            "--delay"=>{let delay=value()?.parse::<u32>().map_err(|_|"Invalid delay")?;if delay>30{return Err("Invalid delay".into());}options.delay=Some(delay);},
            "--geometry"=>options.geometry=Some(value()?),"--output"=>options.output=Some(value()?),
            "--directory"=>options.directory=Some(PathBuf::from(value()?)),"--audio"=>options.audio=Some(value()?),
            "--stop"=>options.stop=true,_=>return Err(format!("Unknown option: {option}")),
        }
    }
    Ok(options)
}
fn choose(option:Option<&String>,title:&str,choices:&[&str])->Result<Option<String>> {
    if let Some(value)=option {
        if !choices.contains(&value.as_str()){return Err(format!("Invalid {title} choice"));}
        Ok(Some(value.clone()))
    }else{menu(title,choices)}
}
fn capture(options:Options)->Result<()> {
    // Capture focus before opening any menu.
    let window=focused(&tree()?).cloned();
    let Some(mode)=choose(options.mode.as_ref(),"Screenshot",&["Region","Window","Display"])? else{return Ok(());};
    let delay=if let Some(delay)=options.delay{delay}else{
        let Some(answer)=menu("Delay in seconds",&["0","3","5","10"])? else{return Ok(());};
        answer.parse::<u32>().map_err(|e|e.to_string())?
    };
    let Some(action)=choose(options.action.as_ref(),"Output",&["Preview and annotate","Save file","Copy to clipboard","Save and copy","Recognize text"])? else{return Ok(());};
    let mut command=Vec::new();
    if mode=="Region" {
        let area=if options.geometry.is_some(){options.geometry}else{region()?};
        let Some(area)=area else{return Ok(());};command.extend(["-g".to_owned(),area]);
    }else if mode=="Window" {
        let window=window.ok_or("Focus an application window before capturing it.")?;
        if !["con","floating_con"].contains(&window["type"].as_str().unwrap_or("")){return Err("Focus an application window before capturing it.".into());}
        command.extend(["-g".to_owned(),geometry(&window["rect"])?]);
    }else{
        let name=if options.output.is_some(){options.output}else{display()?};
        let Some(name)=name else{return Ok(());};command.extend(["-o".to_owned(),name]);
    }
    std::thread::sleep(Duration::from_secs_f64((delay as f64).max(0.2)));
    let destination=options.directory.unwrap_or_else(||PathBuf::from(env::var_os("HOME").unwrap_or_default()).join("data/screenshots"));
    let temporary=tempfile::Builder::new().prefix("lumen-capture-").tempdir().map_err(|e|e.to_string())?;
    let source=temporary.path().join("capture.png");command.push(source.to_string_lossy().into_owned());
    run("grim",&command.iter().map(String::as_str).collect::<Vec<_>>())?;
    match action.as_str() {
        "Recognize text"=>{
            let cache=env::var_os("XDG_CACHE_HOME").map(PathBuf::from).unwrap_or_else(||PathBuf::from(env::var_os("HOME").unwrap_or_default()).join(".cache"))
                .join("lumen-desktop/ocr-captures");
            fs::create_dir_all(&cache).map_err(|e|e.to_string())?;
            fs::set_permissions(&cache,fs::Permissions::from_mode(0o700)).map_err(|e|e.to_string())?;
            let saved=unique_file(&cache,".png")?;
            if let Err(error)=private_copy(&source,&saved){let _=fs::remove_file(&saved);return Err(error);}
            if let Err(error)=run("lumen-desktop-action",&["recognize-image",saved.to_str().ok_or("Invalid capture path")?]){let _=fs::remove_file(&saved);return Err(error);}
            prune(&cache,20);
        }
        "Copy to clipboard"=>{
            let source=File::open(&source).map_err(|e|e.to_string())?;
            let status=Command::new("wl-copy").args(["--type","image/png"]).stdin(source).status().map_err(|e|e.to_string())?;
            if !status.success(){return Err("Clipboard rejected the screenshot".into());}
            notify("Screenshot copied","Paste into the destination application.",None,false,None);
        }
        "Preview and annotate"=>{
            let saved=unique_file(&destination,".png")?;fs::remove_file(&saved).map_err(|e|e.to_string())?;
            run("satty",&["--filename",source.to_str().ok_or("Invalid capture path")?,"--output-filename",saved.to_str().ok_or("Invalid screenshot path")?])?;
        }
        _=>{
            let saved=unique_file(&destination,".png")?;
            if let Err(error)=private_copy(&source,&saved){let _=fs::remove_file(&saved);return Err(error);}
            if action=="Save and copy" {
                let file=File::open(&saved).map_err(|e|e.to_string())?;
                let status=Command::new("wl-copy").args(["--type","image/png"]).stdin(file).status().map_err(|e|e.to_string())?;
                if !status.success(){return Err("Clipboard rejected the screenshot".into());}
            }
            notify("Screenshot saved",&saved.to_string_lossy(),Some(&saved),false,None);
            println!("{}",saved.display());
        }
    }
    Ok(())
}
fn recording_active()->bool {
    Command::new("systemctl").args(["--user","show",UNIT,"-p","ActiveState","--value"]).output().ok()
        .is_some_and(|result|["active","activating","deactivating"].contains(&String::from_utf8_lossy(&result.stdout).trim()))
}
fn record(options:Options)->Result<()> {
    if options.stop {if recording_active(){run("systemctl",&["--user","stop",UNIT])?;}return Ok(());}
    if recording_active(){
        if menu("Recording in progress",&["Stop recording","Keep recording"])?.as_deref()==Some("Stop recording"){
            run("systemctl",&["--user","stop",UNIT])?;
        }
        return Ok(());
    }
    let Some(mode)=choose(options.mode.as_ref(),"Record screen",&["Region","Display"])? else{return Ok(());};
    let mut recorder=Vec::new();
    if mode=="Region" {
        let area=if options.geometry.is_some(){options.geometry}else{region()?};
        let Some(area)=area else{return Ok(());};recorder.extend(["-g".to_owned(),area]);
    }else{
        let output=if options.output.is_some(){options.output}else{display()?};
        let Some(output)=output else{return Ok(());};recorder.extend(["-o".to_owned(),output]);
    }
    let Some(audio)=choose(options.audio.as_ref(),"Audio",&["No audio","Default audio source"])? else{return Ok(());};
    if audio=="Default audio source"{recorder.push("--audio".into());}
    let destination=options.directory.unwrap_or_else(||PathBuf::from(env::var_os("HOME").unwrap_or_default()).join("data/recordings"));
    let saved=unique_file(&destination,".mp4")?;fs::remove_file(&saved).map_err(|e|e.to_string())?;
    let wrapper=match env::var("LUMEN_UTILITIES_WRAPPER") {
        Ok(wrapper)=>wrapper,
        Err(_)=>env::current_exe().map_err(|e|e.to_string())?.to_string_lossy().into_owned(),
    };
    let mut command=Command::new("systemd-run");
    command.args(["--user","--collect","--unit",UNIT,"--property=KillSignal=SIGINT","--property=KillMode=mixed","--property=TimeoutStopSec=30"]);
    for key in ["WAYLAND_DISPLAY","SWAYSOCK","XDG_RUNTIME_DIR","DBUS_SESSION_BUS_ADDRESS"] {
        if let Ok(value)=env::var(key){command.args(["--setenv",&format!("{key}={value}")]);}
    }
    let status=command.arg(wrapper).arg("record-worker").arg(saved).args(recorder).status().map_err(|e|e.to_string())?;
    if !status.success(){return Err("Recording service was not started".into());}
    Ok(())
}
fn worker(file:&str,options:&[String])->Result<()> {
    let mut child=Command::new("wf-recorder").arg("-f").arg(file).args(options).spawn().map_err(|e|e.to_string())?;
    let pid=child.id() as i32;
    let mut signals=signal_hook::iterator::Signals::new([libc::SIGINT,libc::SIGTERM]).map_err(|e|e.to_string())?;
    let handle=signals.handle();
    let signal_thread=std::thread::spawn(move ||for _ in signals.forever(){unsafe{libc::kill(pid,libc::SIGINT);}});
    std::thread::sleep(Duration::from_millis(500));
    let notice=if child.try_wait().map_err(|e|e.to_string())?.is_none() {
        notify("Screen recording is active","Super+Shift+R opens the stop control.",None,true,None)
    }else{None};
    let status=child.wait().map_err(|e|e.to_string())?;
    handle.close();let _=signal_thread.join();
    let path=Path::new(file);
    if status.success() && fs::metadata(path).is_ok_and(|meta|meta.len()>0){
        notify("Screen recording stopped",file,None,false,notice.as_deref());return Ok(());
    }
    notify("Screen recording failed","Check the lumen-screen-record user service log.",None,false,notice.as_deref());
    Err(format!("Screen recording failed: {status}"))
}
fn receive(stream:&mut UnixStream)->Result<(u32,Value)> {
    let mut header=[0_u8;14];stream.read_exact(&mut header).map_err(|e|e.to_string())?;
    if &header[..6]!=b"i3-ipc"{return Err("Invalid Sway IPC message".into());}
    let size=u32::from_le_bytes(header[6..10].try_into().unwrap()) as usize;
    let kind=u32::from_le_bytes(header[10..14].try_into().unwrap());
    if size>16*1024*1024{return Err("Invalid Sway IPC message".into());}
    let mut body=vec![0_u8;size];stream.read_exact(&mut body).map_err(|e|e.to_string())?;
    Ok((kind,serde_json::from_slice(&body).map_err(|e|e.to_string())?))
}
fn shortcuts_watch()->Result<()> {
    let wrapper=env::var("LUMEN_UTILITIES_WRAPPER").map_err(|e|e.to_string())?;
    let commands=[("XF86EmojiPicker","characters"),("Control+Mod4+space","characters"),("Mod4+period","characters"),("Print","capture"),("Mod4+Shift+r","record")]
        .map(|(key,action)|format!("bindsym {key} exec {wrapper} {action}")).join("; ");
    let apply=||->Result<()> {
        let replies:Value=serde_json::from_str(&output("swaymsg",&["-r",&commands])?).map_err(|e|e.to_string())?;
        if !replies.as_array().is_some_and(|items|items.iter().all(|item|item["success"]==true)){return Err("Shortcut activation failed".into());}
        println!("Lumen shortcuts applied");Ok(())
    };
    let socket=env::var("SWAYSOCK").map_err(|e|e.to_string())?;
    let mut events=UnixStream::connect(socket).map_err(|e|e.to_string())?;
    let payload=b"[\"workspace\"]";
    events.write_all(b"i3-ipc").map_err(|e|e.to_string())?;
    events.write_all(&(payload.len() as u32).to_le_bytes()).map_err(|e|e.to_string())?;
    events.write_all(&2_u32.to_le_bytes()).map_err(|e|e.to_string())?;
    events.write_all(payload).map_err(|e|e.to_string())?;
    let (kind,first)=receive(&mut events)?;
    if kind!=2 || first["success"]!=true{return Err("Sway subscription failed".into());}
    apply()?;
    loop {let (kind,event)=receive(&mut events)?;if kind==0x8000_0000 && event["change"]=="reload"{apply()?;}}
}
fn main_result(args:&[String])->Result<()> {
    let Some(command)=args.first().map(String::as_str) else{return Err("An action is required".into());};
    match command {
        "characters"=>run("lumen-character-viewer",&[]),
        "shortcuts-watch"=>shortcuts_watch(),
        "capture"=>capture(parse(&args[1..])?),
        "record"=>record(parse(&args[1..])?),
        "record-worker"=>worker(args.get(1).ok_or("Missing recording path")?,&args[2..]),
        _=>Err("Unknown action".into()),
    }
}
fn main() {
    let arguments=env::args().skip(1).collect::<Vec<_>>();
    if let Err(error)=main_result(&arguments) {
        if arguments.first().is_none_or(|action|action!="record-worker") {
            notify("Desktop action failed",&error,None,false,None);
        }
        eprintln!("{error}");std::process::exit(1);
    }
}
#[cfg(test)] mod tests {
    use super::*;
    use serde_json::json;
    #[test] fn geometry_and_options_are_restricted() {
        assert_eq!(geometry(&json!({"x":-3,"y":4,"width":128,"height":96})).unwrap(),"-3,4 128x96");
        assert!(geometry(&json!({"x":0,"y":0,"width":0,"height":3})).is_err());
        assert!(parse(&["--delay".into(),"31".into()]).is_err());
        assert_eq!(parse(&["--mode".into(),"Window".into()]).unwrap().mode.as_deref(),Some("Window"));
    }
    #[test] fn focused_window_is_found_before_menu_focus() {
        let tree=json!({"nodes":[{"nodes":[{"id":3,"focused":false}],"floating_nodes":[{"id":7,"focused":true}]}]});
        assert_eq!(focused(&tree).unwrap()["id"],7);
    }
    #[test] fn saved_capture_remains_private() {
        let root=tempfile::tempdir().unwrap();let source=root.path().join("source.png");
        fs::write(&source,b"capture").unwrap();
        fs::set_permissions(&source,fs::Permissions::from_mode(0o644)).unwrap();
        let saved=unique_file(root.path(),".png").unwrap();
        private_copy(&source,&saved).unwrap();
        assert_eq!(fs::metadata(&saved).unwrap().permissions().mode()&0o777,0o600);
        assert_eq!(fs::read(&saved).unwrap(),b"capture");
    }
    #[test] fn sway_subscription_reply_is_framed_and_bounded() {
        let (mut writer,mut reader)=UnixStream::pair().unwrap();
        let body=b"{\"success\":true}";
        writer.write_all(b"i3-ipc").unwrap();
        writer.write_all(&(body.len() as u32).to_le_bytes()).unwrap();
        writer.write_all(&2_u32.to_le_bytes()).unwrap();
        writer.write_all(body).unwrap();
        let (kind,value)=receive(&mut reader).unwrap();
        assert_eq!(kind,2);assert_eq!(value["success"],true);
    }
}
